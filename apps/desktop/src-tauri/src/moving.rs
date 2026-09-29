//! Moving a vault from Bitwarden (cloud or self-hosted) or Vaultwarden into
//! the UwULock account on screen. The protocol is `uwulock_bitwarden::moving`;
//! this is the window's side of it.
//!
//! The source account is logged in here, in the move dialog, with two-step
//! login and Bitwarden's new-device code like any login — and kept only in
//! memory, for as long as the dialog is open. Nothing of it is written to
//! disk but the journal: which source object became which object here, ids
//! only, sealed under the target account's user key in its own folder
//! (`move-journal.json`).
//!
//! The run goes step by step, one object each, with a `move-progress` event
//! to the page after every step and the journal written right after. It ends
//! with `move-finished` (the summary, and whether it was cancelled) or
//! `move-failed`. While it runs, it counts as activity, so the vault doesn't
//! lock under it; a vault locked by hand ends it.

use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use uwulock_bitwarden::api::{PasswordLogin, TwoFactorAnswer};
use uwulock_bitwarden::crypto::{self, decrypt_user_key};
use uwulock_bitwarden::moving::{Journal, Mover, Preview, Source, Step, Summary, Target};
use uwulock_bitwarden::{Client, EncString, LoginOutcome, Server, SymmetricKey, TwoFactorMethod};
use zeroize::Zeroizing;

use crate::account::Account;
use crate::vault::{
    access_token, derive_off_thread, iso_now, sync_account, Failure, ServerInput, VaultState,
};

type Result<T> = std::result::Result<T, Failure>;

/// The source login between the password and the two-step code.
struct PendingSource {
    client: Arc<Client>,
    server: Server,
    email: String,
    master_key: Zeroizing<[u8; 32]>,
    hash: Zeroizing<String>,
}

#[derive(Default)]
pub(crate) struct MoveState {
    pending: Mutex<Option<PendingSource>>,
    /// The prepared move, and the account it moves into.
    mover: Arc<tokio::sync::Mutex<Option<(String, Mover)>>>,
    running: AtomicBool,
    cancel: AtomicBool,
    /// The dialog closed while a run was going: it drops the move when it stops.
    closing: AtomicBool,
    /// Counts [`MoveState::forget`]s, so a move prepared while the vault
    /// locked isn't kept after all.
    forgotten: AtomicU64,
}

impl MoveState {
    /// Locking, a session the server ended and logging out drop the source
    /// login and the prepared move: the source vault (decrypted), both user
    /// keys and the source's tokens don't stay in memory behind a locked
    /// vault. A running move stops after its current step and drops it then;
    /// the journal keeps what it did, so the next move continues from there.
    pub(crate) fn forget(&self) {
        self.forgotten.fetch_add(1, Ordering::SeqCst);
        *self.pending.lock() = None;
        if self.running.load(Ordering::SeqCst) {
            self.cancel.store(true, Ordering::SeqCst);
            self.closing.store(true, Ordering::SeqCst);
            return;
        }
        match self.mover.try_lock() {
            Ok(mut mover) => *mover = None,
            // Held for a moment (a move being stored, or a run starting):
            // dropped as soon as it is free.
            Err(_) => {
                self.cancel.store(true, Ordering::SeqCst);
                let mover = self.mover.clone();
                tauri::async_runtime::spawn(async move {
                    *mover.lock().await = None;
                });
            }
        }
    }

    /// Like [`MoveState::forget`], for one account that locked or left: only
    /// when the move goes into it, or it is on screen (the dialog's source
    /// login belongs to the account on screen).
    pub(crate) fn forget_for(&self, id: &str, on_screen: bool) {
        let target = self
            .mover
            .try_lock()
            .map(|mover| mover.as_ref().map(|(target, _)| target == id));
        // A mover that is busy is running: better stopped than kept.
        if on_screen || !matches!(target, Ok(None) | Ok(Some(false))) {
            self.forget();
        }
    }
}

/// Whether the account on screen can take a move.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveTarget {
    /// It is on a UwULock Server.
    uwulock: bool,
    /// Its server makes families (whether this account may is for the preview).
    families: bool,
    email: String,
    server: String,
}

/// Where the source login stands.
#[derive(Debug, Serialize)]
#[serde(tag = "step", rename_all = "kebab-case")]
pub enum MoveStep {
    Done {
        preview: Box<Preview>,
    },
    #[serde(rename_all = "camelCase")]
    TwoFactor {
        methods: Vec<TwoFactorMethod>,
        message: Option<String>,
    },
    NewDevice,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Finished {
    summary: Summary,
    cancelled: bool,
}

#[tauri::command]
pub(crate) async fn move_target(state: State<'_, VaultState>) -> Result<MoveTarget> {
    let (id, account) = state.active_account()?;
    state.user_key_of(&id)?;
    let client = state.client(account.server.clone())?;
    let uwulock = client.uwu_info().await.unwrap_or(None).is_some();
    Ok(MoveTarget {
        uwulock,
        families: state.offers(&id, "families"),
        email: account.email.clone(),
        server: account.server.label(),
    })
}

#[tauri::command]
pub(crate) async fn move_login(
    app: AppHandle,
    state: State<'_, VaultState>,
    moves: State<'_, Arc<MoveState>>,
    server: ServerInput,
    email: String,
    password: String,
) -> Result<MoveStep> {
    if moves.running.load(Ordering::SeqCst) {
        return Err(Failure::new("invalid", "A move is running."));
    }
    let forgotten = moves.forgotten.load(Ordering::SeqCst);
    let password = Zeroizing::new(password);
    let email = crypto::normalize_email(&email);
    if email.is_empty() || !email.contains('@') {
        return Err(Failure::new(
            "invalid",
            "That doesn't look like an email address.",
        ));
    }
    let server = server.resolve()?;
    let client = Arc::new(state.client(server.clone())?);
    let kdf = client.prelogin(&email).await?;
    let master_key = derive_off_thread(password.clone(), email.clone(), kdf).await?;
    let hash = Zeroizing::new(crypto::master_password_hash(&master_key, &password));
    // The vault locked while the key was derived: nothing is kept.
    if moves.forgotten.load(Ordering::SeqCst) != forgotten {
        return Err(Failure::locked());
    }
    *moves.pending.lock() = Some(PendingSource {
        client,
        server,
        email,
        master_key,
        hash,
    });
    login_step(&app, None, None).await
}

#[tauri::command]
pub(crate) async fn move_login_two_factor(
    app: AppHandle,
    provider: u8,
    code: String,
) -> Result<MoveStep> {
    // No "remember this device": nothing of the source account is kept.
    let answer = TwoFactorAnswer {
        provider,
        code,
        remember: false,
    };
    login_step(&app, Some(answer), None).await
}

#[tauri::command]
pub(crate) async fn move_login_new_device(app: AppHandle, code: String) -> Result<MoveStep> {
    login_step(&app, None, Some(code)).await
}

#[tauri::command]
pub(crate) async fn move_login_send_email(moves: State<'_, Arc<MoveState>>) -> Result<()> {
    let (client, email, hash) = pending_login(&moves)?;
    client.send_email_code(&email, &hash).await?;
    Ok(())
}

fn pending_login(moves: &MoveState) -> Result<(Arc<Client>, String, Zeroizing<String>)> {
    let pending = moves.pending.lock();
    let pending = pending
        .as_ref()
        .ok_or_else(|| Failure::new("invalid", "No login in progress."))?;
    Ok((
        pending.client.clone(),
        pending.email.clone(),
        pending.hash.clone(),
    ))
}

async fn login_step(
    app: &AppHandle,
    two_factor: Option<TwoFactorAnswer>,
    new_device_code: Option<String>,
) -> Result<MoveStep> {
    let moves = app.state::<Arc<MoveState>>();
    let forgotten = moves.forgotten.load(Ordering::SeqCst);
    let (client, email, hash) = pending_login(&moves)?;
    let outcome = client
        .login(PasswordLogin {
            email: &email,
            password_hash: &hash,
            two_factor,
            remember_token: None,
            new_device_code: new_device_code.as_deref(),
        })
        .await?;
    let session = match outcome {
        LoginOutcome::LoggedIn(session) => session,
        LoginOutcome::TwoFactor { methods, message } => {
            return Ok(MoveStep::TwoFactor { methods, message })
        }
        LoginOutcome::NewDeviceCode => return Ok(MoveStep::NewDevice),
    };
    let pending = moves
        .pending
        .lock()
        .take()
        .ok_or_else(|| Failure::new("invalid", "No login in progress."))?;
    let protected = session
        .protected_user_key
        .clone()
        .ok_or_else(|| Failure::new("server", "The server didn't send the account's key."))?;
    let user_key = decrypt_user_key(&pending.master_key, &protected.parse::<EncString>()?)
        .map_err(|_| {
            Failure::new(
                "crypto",
                "The account's key didn't open with this master password.",
            )
        })?;
    drop(pending.master_key);

    let state = app.state::<VaultState>();
    let (target_id, account) = state.active_account()?;
    let target_key = state.user_key_of(&target_id)?;
    let token = access_token(&state, &target_id).await?;
    let journal = load_journal(&state, &target_id, &target_key);
    let source = Source::new(
        state.client(pending.server.clone())?,
        session,
        user_key,
        &pending.email,
    );
    let target = Target {
        client: state.client(account.server.clone())?,
        user_key: target_key,
        email: account.email.clone(),
    };
    let mover = Mover::prepare(source, target, &token, journal, &iso_now()).await?;
    let preview = Box::new(mover.preview().clone());
    {
        let mut slot = moves.mover.lock().await;
        // Locked, logged out or ended while the source vault came in: it is
        // dropped here instead of kept behind the lock.
        if moves.forgotten.load(Ordering::SeqCst) != forgotten {
            return Err(Failure::locked());
        }
        moves.cancel.store(false, Ordering::SeqCst);
        moves.closing.store(false, Ordering::SeqCst);
        *slot = Some((target_id, mover));
    }
    tracing::info!(from = %pending.server.label(), "a move is ready");
    Ok(MoveStep::Done { preview })
}

/// The journal of earlier moves into this account. One that doesn't open
/// is started over: at worst, a second move copies things twice.
fn load_journal(state: &VaultState, id: &str, user_key: &SymmetricKey) -> Journal {
    let sealed = state.storage().load_move_journal(id);
    Account::unseal(&sealed, user_key)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Written after every step, with the key the run started with: a vault
/// locked in the middle of a step still gets that step written down.
fn save_journal(state: &VaultState, id: &str, user_key: &SymmetricKey, mover: &Mover) {
    let text = serde_json::to_string(mover.journal()).unwrap_or_default();
    if let Err(error) = state
        .storage()
        .save_move_journal(id, &Account::seal(&text, user_key))
    {
        tracing::warn!(%error, "couldn't keep the move journal");
    }
}

/// Starts (or continues) the prepared move in the background.
#[tauri::command]
pub(crate) async fn move_start(app: AppHandle, moves: State<'_, Arc<MoveState>>) -> Result<()> {
    if moves.mover.lock().await.is_none() {
        return Err(Failure::new("invalid", "Nothing to move yet."));
    }
    if moves.running.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    moves.cancel.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        let result = run(&app).await;
        let moves = app.state::<Arc<MoveState>>();
        let target = moves.mover.lock().await.as_ref().map(|(id, _)| id.clone());
        match result {
            Ok(finished) => {
                let _ = app.emit("move-finished", finished);
            }
            Err(failure) => {
                let failure = serde_json::to_value(&failure).unwrap_or_default();
                let _ = app.emit("move-failed", failure);
            }
        }
        if moves.closing.load(Ordering::SeqCst) {
            *moves.mover.lock().await = None;
        }
        moves.running.store(false, Ordering::SeqCst);
        // The account's vault shows what came in.
        if let Some(id) = target {
            if let Err(error) = sync_account(&app, &id).await {
                tracing::warn!(?error, "the sync after the move failed");
            }
        }
    });
    Ok(())
}

async fn run(app: &AppHandle) -> Result<Finished> {
    let state = app.state::<VaultState>();
    let moves = app.state::<Arc<MoveState>>();
    let mut guard = moves.mover.lock().await;
    let (id, mover) = guard
        .as_mut()
        .ok_or_else(|| Failure::new("invalid", "Nothing to move yet."))?;
    let user_key = state.user_key_of(id)?;
    loop {
        if moves.cancel.load(Ordering::SeqCst) {
            return Ok(Finished {
                summary: mover.summary().clone(),
                cancelled: true,
            });
        }
        let token = access_token(&state, id).await?;
        let step = mover.step(&token).await;
        save_journal(&state, id, &user_key, mover);
        match step? {
            Step::Working(progress) => {
                // Moving is what the person is doing: no auto-lock under it.
                state.touch();
                let _ = app.emit("move-progress", progress);
            }
            Step::Finished(summary) => {
                tracing::info!(
                    items = summary.moved.items,
                    failed = summary.failed.len(),
                    "the move is done"
                );
                return Ok(Finished {
                    summary,
                    cancelled: false,
                });
            }
        }
    }
}

#[tauri::command]
pub(crate) fn move_cancel(moves: State<'_, Arc<MoveState>>) {
    moves.cancel.store(true, Ordering::SeqCst);
}

/// The dialog closed: the source session and the prepared move are dropped
/// (a running move first stops after its current step).
#[tauri::command]
pub(crate) async fn move_close(moves: State<'_, Arc<MoveState>>) -> Result<()> {
    *moves.pending.lock() = None;
    if moves.running.load(Ordering::SeqCst) {
        moves.cancel.store(true, Ordering::SeqCst);
        moves.closing.store(true, Ordering::SeqCst);
        return Ok(());
    }
    *moves.mover.lock().await = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgetting_counts_and_stops_a_running_move() {
        let moves = MoveState::default();
        moves.forget();
        assert_eq!(moves.forgotten.load(Ordering::SeqCst), 1);
        assert!(!moves.cancel.load(Ordering::SeqCst));

        moves.running.store(true, Ordering::SeqCst);
        moves.forget();
        assert!(moves.cancel.load(Ordering::SeqCst) && moves.closing.load(Ordering::SeqCst));
    }

    #[test]
    fn another_account_leaving_keeps_the_move() {
        let moves = MoveState::default();
        moves.forget_for("other", false);
        assert_eq!(moves.forgotten.load(Ordering::SeqCst), 0);
        moves.forget_for("on-screen", true);
        assert_eq!(moves.forgotten.load(Ordering::SeqCst), 1);
    }
}
