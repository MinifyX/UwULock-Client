//! The vault, as the page sees it: logging in, unlocking, locking, syncing,
//! and the items.
//!
//! Secrets stay here. The list and an item's details carry names, usernames,
//! addresses and notes; a password, a card number, a hidden field or a
//! private key only goes to the page when someone clicks the eye
//! ([`reveal_field`]), and copying ([`copy_field`]) goes from here straight to
//! the clipboard. Locking drops every decrypted value and the session.

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use uwulock_bitwarden::api::{parse_sync, PasswordLogin, TwoFactorAnswer};
use uwulock_bitwarden::crypto::{self, decrypt_user_key};
use uwulock_bitwarden::vault::{FieldKind, Item, ItemKind, Secret};
use uwulock_bitwarden::{
    generator, totp, Client, Device, EncString, Error, Kdf, LoginOutcome, Server, Session,
    SymmetricKey, Vault,
};
use zeroize::Zeroizing;

use crate::account::{Account, Storage};
use crate::clipboard::Clipboard;

const SYNC_EVERY: Duration = Duration::from_secs(5 * 60);

/// An error as the page gets it: a kind to branch on, a message to show.
#[derive(Debug, Serialize)]
pub struct Failure {
    kind: &'static str,
    message: String,
}

impl Failure {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Failure {
            kind,
            message: message.into(),
        }
    }

    fn locked() -> Self {
        Failure::new("locked", "The vault is locked.")
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        let kind = match &error {
            Error::Network(_) => "network",
            Error::Server { .. } => "server",
            Error::Refused(_) => "refused",
            Error::SessionExpired => "session-expired",
            Error::WrongKey => "wrong-password",
            Error::Crypto(_) => "crypto",
            Error::Unsupported(_) => "unsupported",
        };
        Failure::new(kind, error.to_string())
    }
}

type Result<T> = std::result::Result<T, Failure>;

struct Unlocked {
    user_key: SymmetricKey,
    vault: Vault,
    session: Option<Session>,
    /// Items whose master password re-prompt was answered in this unlock.
    reprompt_ok: HashSet<String>,
}

/// A login between the password and the two-step code.
struct PendingLogin {
    client: Arc<Client>,
    server: Server,
    email: String,
    kdf: Kdf,
    master_key: Zeroizing<[u8; 32]>,
    hash: Zeroizing<String>,
}

#[derive(Clone, Copy, Default)]
struct Security {
    auto_lock: Option<Duration>,
    clipboard: Option<Duration>,
}

pub(crate) struct VaultState {
    storage: Storage,
    account: Mutex<Option<Account>>,
    unlocked: RwLock<Option<Unlocked>>,
    pending: Mutex<Option<PendingLogin>>,
    syncing: AtomicBool,
    session_expired: AtomicBool,
    sync_error: Mutex<Option<String>>,
    security: Mutex<Security>,
    last_activity: Mutex<Instant>,
    clipboard: Arc<Clipboard>,
}

impl VaultState {
    pub fn new(storage: Storage) -> Self {
        let account = storage.load_account();
        VaultState {
            storage,
            account: Mutex::new(account),
            unlocked: RwLock::new(None),
            pending: Mutex::new(None),
            syncing: AtomicBool::new(false),
            session_expired: AtomicBool::new(false),
            sync_error: Mutex::new(None),
            security: Mutex::new(Security {
                auto_lock: Some(Duration::from_secs(15 * 60)),
                clipboard: Some(Duration::from_secs(30)),
            }),
            last_activity: Mutex::new(Instant::now()),
            clipboard: Arc::new(Clipboard::default()),
        }
    }

    fn touch(&self) {
        *self.last_activity.lock() = Instant::now();
    }

    fn device(&self) -> Device {
        Device::this_system(self.storage.device_id())
    }

    fn client(&self, server: Server) -> Result<Client> {
        Ok(Client::new(server, self.device())?)
    }

    fn lock_now(&self) {
        *self.unlocked.write() = None;
        *self.pending.lock() = None;
        self.clipboard.clear_now();
    }

    /// Runs `f` on the open vault. Doesn't count as activity: the page polls
    /// (the one-time code, every second) and must not keep the vault open by
    /// that alone. Activity is what the user does — see [`touch`].
    fn with_unlocked<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> {
        let mut guard = self.unlocked.write();
        let unlocked = guard.as_mut().ok_or_else(Failure::locked)?;
        f(unlocked)
    }
}

// ── Status ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `logged-out`, `locked` or `unlocked`.
    state: &'static str,
    email: Option<String>,
    name: Option<String>,
    server: Option<String>,
    server_kind: Option<&'static str>,
    server_url: Option<String>,
    last_sync: Option<u64>,
    syncing: bool,
    sync_error: Option<String>,
    /// The server no longer accepts this device's session: log in again.
    session_expired: bool,
}

fn status_of(state: &VaultState) -> Status {
    let account = state.account.lock().clone();
    let unlocked = state.unlocked.read().is_some();
    Status {
        state: match (&account, unlocked) {
            (None, _) => "logged-out",
            (Some(_), false) => "locked",
            (Some(_), true) => "unlocked",
        },
        email: account.as_ref().map(|a| a.email.clone()),
        name: account.as_ref().and_then(|a| a.name.clone()),
        server: account.as_ref().map(|a| a.server.label()),
        server_kind: account.as_ref().map(|a| match a.server {
            Server::BitwardenUs => "bitwarden-us",
            Server::BitwardenEu => "bitwarden-eu",
            Server::SelfHosted { .. } => "self-hosted",
        }),
        server_url: account.as_ref().and_then(|a| match &a.server {
            Server::SelfHosted { url } => Some(url.clone()),
            _ => None,
        }),
        last_sync: account.as_ref().and_then(|a| a.last_sync),
        syncing: state.syncing.load(Ordering::SeqCst),
        sync_error: state.sync_error.lock().clone(),
        session_expired: state.session_expired.load(Ordering::SeqCst),
    }
}

fn emit_status(app: &AppHandle) {
    let state = app.state::<VaultState>();
    let _ = app.emit("vault-status", status_of(&state));
}

#[tauri::command]
pub(crate) fn vault_status(state: State<'_, VaultState>) -> Status {
    status_of(&state)
}

// ── Logging in ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInput {
    kind: String,
    #[serde(default)]
    url: Option<String>,
}

impl ServerInput {
    fn resolve(&self) -> Result<Server> {
        match self.kind.as_str() {
            "bitwarden-us" => Ok(Server::BitwardenUs),
            "bitwarden-eu" => Ok(Server::BitwardenEu),
            "self-hosted" => Ok(Server::self_hosted(
                self.url.as_deref().unwrap_or_default(),
            )?),
            other => Err(Failure::new(
                "invalid",
                format!("unknown server kind {other}"),
            )),
        }
    }
}

/// Where a login stands after a step.
#[derive(Debug, Serialize)]
#[serde(tag = "step", rename_all = "kebab-case")]
pub enum LoginStep {
    Done {
        status: Status,
    },
    #[serde(rename_all = "camelCase")]
    TwoFactor {
        methods: Vec<uwulock_bitwarden::TwoFactorMethod>,
        message: Option<String>,
    },
    NewDevice,
}

fn derive(password: &str, email: &str, kdf: Kdf) -> Result<Zeroizing<[u8; 32]>> {
    Ok(crypto::master_key(password, email, kdf)?)
}

async fn derive_off_thread(
    password: Zeroizing<String>,
    email: String,
    kdf: Kdf,
) -> Result<Zeroizing<[u8; 32]>> {
    // Argon2 and 600 000 rounds of PBKDF2 take a moment; not on the IPC thread.
    tauri::async_runtime::spawn_blocking(move || derive(&password, &email, kdf))
        .await
        .map_err(|e| Failure::new("crypto", e.to_string()))?
}

#[tauri::command]
pub(crate) async fn login(
    app: AppHandle,
    state: State<'_, VaultState>,
    server: ServerInput,
    email: String,
    password: String,
) -> Result<LoginStep> {
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
    *state.pending.lock() = Some(PendingLogin {
        client,
        server,
        email,
        kdf,
        master_key,
        hash,
    });
    login_step(&app, &state, None, None).await
}

#[tauri::command]
pub(crate) async fn login_two_factor(
    app: AppHandle,
    state: State<'_, VaultState>,
    provider: u8,
    code: String,
    remember: bool,
) -> Result<LoginStep> {
    let answer = TwoFactorAnswer {
        provider,
        code,
        remember,
    };
    login_step(&app, &state, Some(answer), None).await
}

#[tauri::command]
pub(crate) async fn login_new_device(
    app: AppHandle,
    state: State<'_, VaultState>,
    code: String,
) -> Result<LoginStep> {
    login_step(&app, &state, None, Some(code)).await
}

#[tauri::command]
pub(crate) async fn login_send_email(state: State<'_, VaultState>) -> Result<()> {
    let (client, email, hash) = {
        let pending = state.pending.lock();
        let pending = pending
            .as_ref()
            .ok_or_else(|| Failure::new("invalid", "No login in progress."))?;
        (
            pending.client.clone(),
            pending.email.clone(),
            pending.hash.clone(),
        )
    };
    client.send_email_code(&email, &hash).await?;
    Ok(())
}

#[tauri::command]
pub(crate) fn login_cancel(state: State<'_, VaultState>) {
    *state.pending.lock() = None;
}

async fn login_step(
    app: &AppHandle,
    state: &VaultState,
    two_factor: Option<TwoFactorAnswer>,
    new_device_code: Option<String>,
) -> Result<LoginStep> {
    let (client, email, hash) = {
        let pending = state.pending.lock();
        let pending = pending
            .as_ref()
            .ok_or_else(|| Failure::new("invalid", "No login in progress."))?;
        (
            pending.client.clone(),
            pending.email.clone(),
            pending.hash.clone(),
        )
    };
    // A device remembered at an earlier two-step login skips the code.
    let remember_token = remembered_token(state, &client, &email);
    let outcome = client
        .login(PasswordLogin {
            email: &email,
            password_hash: &hash,
            two_factor,
            remember_token: remember_token.as_deref().map(|s| s.as_str()),
            new_device_code: new_device_code.as_deref(),
        })
        .await?;
    match outcome {
        LoginOutcome::LoggedIn(session) => {
            let pending = state
                .pending
                .lock()
                .take()
                .ok_or_else(|| Failure::new("invalid", "No login in progress."))?;
            finish_login(app, state, pending, session).await?;
            Ok(LoginStep::Done {
                status: status_of(state),
            })
        }
        LoginOutcome::TwoFactor { methods, message } => {
            Ok(LoginStep::TwoFactor { methods, message })
        }
        LoginOutcome::NewDeviceCode => Ok(LoginStep::NewDevice),
    }
}

/// The remember token of the account this device had, if it's the same one.
/// It is sealed under the user key, which a fresh login doesn't have yet — so
/// it only helps while the vault is unlocked (logging in again after the
/// session expired).
fn remembered_token(state: &VaultState, client: &Client, email: &str) -> Option<Zeroizing<String>> {
    let account = state.account.lock().clone()?;
    if account.email != email || &account.server != client.server() {
        return None;
    }
    let unlocked = state.unlocked.read();
    Account::unseal(
        &account.protected_remember_token,
        &unlocked.as_ref()?.user_key,
    )
}

async fn finish_login(
    app: &AppHandle,
    state: &VaultState,
    pending: PendingLogin,
    session: Session,
) -> Result<()> {
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

    // Keep the old remember token unless the server handed out a new one.
    let previous_remember = {
        let account = state.account.lock();
        let unlocked = state.unlocked.read();
        match (account.as_ref(), unlocked.as_ref()) {
            (Some(a), Some(u)) if a.email == pending.email && a.server == pending.server => {
                Account::unseal(&a.protected_remember_token, &u.user_key)
            }
            _ => None,
        }
    };
    let remember = session.remember_token.clone().or(previous_remember);

    let mut account = Account {
        version: 1,
        server: pending.server.clone(),
        email: pending.email.clone(),
        name: None,
        kdf: pending.kdf,
        protected_user_key: protected,
        protected_refresh_token: session
            .refresh_token
            .as_ref()
            .map(|t| Account::seal(t, &user_key)),
        protected_remember_token: remember.as_ref().map(|t| Account::seal(t, &user_key)),
        last_sync: None,
    };

    // The first sync right away, so the vault isn't empty on arrival.
    let text = pending.client.sync(&session.access_token).await?;
    let sync = parse_sync(&text)?;
    let vault = Vault::open(&sync, &user_key)?;
    account.name = sync.profile.name.clone().filter(|n| !n.is_empty());
    account.last_sync = Some(now());

    state
        .storage
        .save_account(&account)
        .map_err(|e| Failure::new("io", format!("Couldn't save the account: {e}")))?;
    if let Err(error) = state.storage.save_cache(&text) {
        tracing::warn!(%error, "couldn't cache the vault");
    }
    *state.account.lock() = Some(account);
    *state.unlocked.write() = Some(Unlocked {
        user_key,
        vault,
        session: Some(session),
        reprompt_ok: HashSet::new(),
    });
    state.session_expired.store(false, Ordering::SeqCst);
    *state.sync_error.lock() = None;
    state.touch();
    tracing::info!(server = %pending.server.label(), "logged in");
    emit_status(app);
    Ok(())
}

// ── Unlocking and locking ──────────────────────────────────

#[tauri::command]
pub(crate) async fn unlock(
    app: AppHandle,
    state: State<'_, VaultState>,
    password: String,
) -> Result<Status> {
    let password = Zeroizing::new(password);
    let account = state
        .account
        .lock()
        .clone()
        .ok_or_else(|| Failure::new("logged-out", "No account on this device."))?;
    let protected: EncString = account.protected_user_key.parse()?;

    let master_key =
        derive_off_thread(password.clone(), account.email.clone(), account.kdf).await?;
    let user_key = match decrypt_user_key(&master_key, &protected) {
        Ok(key) => key,
        Err(_) => {
            // The key derivation may have changed on another device since the
            // last sync. Ask the server, briefly, and try once more.
            let retry = match state.client(account.server.clone()) {
                Ok(client) => {
                    tokio::time::timeout(Duration::from_secs(8), client.prelogin(&account.email))
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .filter(|kdf| *kdf != account.kdf)
                }
                Err(_) => None,
            };
            let Some(kdf) = retry else {
                return Err(Failure::new(
                    "wrong-password",
                    "The master password is wrong.",
                ));
            };
            let master_key =
                derive_off_thread(password.clone(), account.email.clone(), kdf).await?;
            let key = decrypt_user_key(&master_key, &protected)
                .map_err(|_| Failure::new("wrong-password", "The master password is wrong."))?;
            if let Some(stored) = state.account.lock().as_mut() {
                stored.kdf = kdf;
                let _ = state.storage.save_account(stored);
            }
            key
        }
    };

    let vault = match state.storage.load_cache() {
        Some(text) => match parse_sync(&text).and_then(|sync| Vault::open(&sync, &user_key)) {
            Ok(vault) => vault,
            Err(error) => {
                tracing::warn!(%error, "the cached vault didn't open");
                Vault::default()
            }
        },
        None => Vault::default(),
    };
    *state.unlocked.write() = Some(Unlocked {
        user_key,
        vault,
        session: None,
        reprompt_ok: HashSet::new(),
    });
    state.touch();
    tracing::info!("unlocked");
    emit_status(&app);
    spawn_sync(app.clone());
    Ok(status_of(&state))
}

#[tauri::command]
pub(crate) fn lock(app: AppHandle, state: State<'_, VaultState>) {
    state.lock_now();
    tracing::info!("locked");
    emit_status(&app);
}

/// Logging out: the account, the session and the cached vault leave this device.
#[tauri::command]
pub(crate) fn logout(app: AppHandle, state: State<'_, VaultState>) -> Result<()> {
    state.lock_now();
    state
        .storage
        .forget()
        .map_err(|e| Failure::new("io", format!("Couldn't remove the account: {e}")))?;
    *state.account.lock() = None;
    state.session_expired.store(false, Ordering::SeqCst);
    *state.sync_error.lock() = None;
    tracing::info!("logged out");
    emit_status(&app);
    Ok(())
}

#[tauri::command]
pub(crate) fn touch(state: State<'_, VaultState>) {
    state.touch();
}

#[tauri::command]
pub(crate) fn set_security(
    state: State<'_, VaultState>,
    auto_lock_minutes: Option<u32>,
    clipboard_seconds: Option<u32>,
) {
    *state.security.lock() = Security {
        auto_lock: auto_lock_minutes
            .filter(|m| *m > 0)
            .map(|m| Duration::from_secs(u64::from(m.min(24 * 60)) * 60)),
        clipboard: clipboard_seconds
            .filter(|s| *s > 0)
            .map(|s| Duration::from_secs(u64::from(s.min(600)))),
    };
}

// ── Sync ───────────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn sync_now(app: AppHandle, state: State<'_, VaultState>) -> Result<Status> {
    state.touch();
    sync(&app).await?;
    Ok(status_of(&state))
}

fn spawn_sync(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = sync(&app).await {
            tracing::warn!(kind = error.kind, message = %error.message, "sync failed");
        }
    });
}

async fn sync(app: &AppHandle) -> Result<()> {
    let state = app.state::<VaultState>();
    if state.syncing.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    emit_status(app);
    let result = sync_inner(&state).await;
    state.syncing.store(false, Ordering::SeqCst);
    match &result {
        Ok(()) => *state.sync_error.lock() = None,
        Err(error) if error.kind == "locked" => {}
        Err(error) => {
            if error.kind == "session-expired" {
                state.session_expired.store(true, Ordering::SeqCst);
            }
            *state.sync_error.lock() = Some(error.message.clone());
        }
    }
    emit_status(app);
    if result.is_ok() {
        let _ = app.emit("vault-changed", ());
    }
    result
}

async fn sync_inner(state: &VaultState) -> Result<()> {
    let account = state.account.lock().clone().ok_or_else(Failure::locked)?;
    let (user_key, access) = {
        let unlocked = state.unlocked.read();
        let unlocked = unlocked.as_ref().ok_or_else(Failure::locked)?;
        let access = unlocked
            .session
            .as_ref()
            .filter(|s| !s.is_expiring())
            .map(|s| s.access_token.clone());
        (unlocked.user_key.clone(), access)
    };
    let client = state.client(account.server.clone())?;

    let access = match access {
        Some(token) => token,
        None => {
            let refresh = Account::unseal(&account.protected_refresh_token, &user_key)
                .ok_or(Error::SessionExpired)?;
            let session = client.refresh(&refresh).await?;
            let token = session.access_token.clone();
            // Keep a new refresh token, if the server rotated it.
            if let Some(new) = session
                .refresh_token
                .as_ref()
                .filter(|t| t.as_str() != refresh.as_str())
            {
                if let Some(stored) = state.account.lock().as_mut() {
                    stored.protected_refresh_token = Some(Account::seal(new, &user_key));
                    let _ = state.storage.save_account(stored);
                }
            }
            if let Some(unlocked) = state.unlocked.write().as_mut() {
                unlocked.session = Some(session);
            }
            token
        }
    };

    let text = client.sync(&access).await?;
    let sync = parse_sync(&text)?;
    let vault = Vault::open(&sync, &user_key)?;
    state
        .storage
        .save_cache(&text)
        .map_err(|e| Failure::new("io", format!("Couldn't cache the vault: {e}")))?;
    if let Some(stored) = state.account.lock().as_mut() {
        stored.last_sync = Some(now());
        stored.name = sync.profile.name.clone().filter(|n| !n.is_empty());
        // The master password changed elsewhere: the next unlock takes the new one.
        if let Some(key) = sync
            .profile
            .key
            .clone()
            .filter(|k| *k != stored.protected_user_key)
        {
            stored.protected_user_key = key;
        }
        let _ = state.storage.save_account(stored);
    }
    if let Some(unlocked) = state.unlocked.write().as_mut() {
        unlocked.vault = vault;
    }
    state.session_expired.store(false, Ordering::SeqCst);
    Ok(())
}

/// Auto-lock and the periodic sync.
pub(crate) fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_sync = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let state = app.state::<VaultState>();
            if state.unlocked.read().is_none() {
                continue;
            }
            let security = *state.security.lock();
            let idle = state.last_activity.lock().elapsed();
            if security.auto_lock.is_some_and(|after| idle >= after) {
                state.lock_now();
                tracing::info!("locked after {} idle minutes", idle.as_secs() / 60);
                emit_status(&app);
                continue;
            }
            if last_sync.elapsed() >= SYNC_EVERY && !state.session_expired.load(Ordering::SeqCst) {
                last_sync = Instant::now();
                spawn_sync(app.clone());
            }
        }
    });
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// ── Items ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    folders: Vec<uwulock_bitwarden::vault::Folder>,
    collections: Vec<uwulock_bitwarden::vault::Collection>,
    organizations: Vec<uwulock_bitwarden::vault::Organization>,
    skipped: usize,
}

#[tauri::command]
pub(crate) fn vault_overview(state: State<'_, VaultState>) -> Result<Overview> {
    state.with_unlocked(|u| {
        Ok(Overview {
            folders: u.vault.folders.clone(),
            collections: u.vault.collections.clone(),
            organizations: u.vault.organizations.clone(),
            skipped: u.vault.skipped,
        })
    })
}

fn text(value: &Option<Secret>) -> Option<String> {
    value
        .as_ref()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
}

/// The host of an address, for the list: `github.com` from `https://github.com/login`.
fn host_of(uri: &str) -> Option<String> {
    let with_scheme = if uri.contains("://") {
        uri.to_string()
    } else {
        format!("https://{uri}")
    };
    url::Url::parse(&with_scheme)
        .ok()?
        .host_str()
        .map(|h| h.trim_start_matches("www.").to_string())
}

fn last_four(number: &str) -> String {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    digits[digits.len().saturating_sub(4)..].to_string()
}

fn subtitle(item: &Item) -> Option<String> {
    match item.kind {
        ItemKind::Login => item.login.as_ref().and_then(|l| {
            text(&l.username).or_else(|| l.uris.first().and_then(|u| host_of(&u.uri)))
        }),
        ItemKind::Card => item.card.as_ref().map(|c| {
            let brand = text(&c.brand).unwrap_or_default();
            match c.number.as_ref() {
                Some(n) if !n.is_empty() => format!("{brand} *{}", last_four(n)).trim().to_string(),
                _ => brand,
            }
        }),
        ItemKind::Identity => item.identity.as_ref().and_then(|i| {
            let name = [text(&i.first_name), text(&i.last_name)]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            (!name.is_empty())
                .then_some(name)
                .or_else(|| text(&i.email))
        }),
        ItemKind::SshKey => item.ssh_key.as_ref().and_then(|s| text(&s.fingerprint)),
        ItemKind::Note => None,
    }
    .filter(|s| !s.is_empty())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    id: String,
    kind: ItemKind,
    name: String,
    subtitle: Option<String>,
    host: Option<String>,
    favorite: bool,
    folder_id: Option<String>,
    organization_id: Option<String>,
    collection_ids: Vec<String>,
    deleted: bool,
    reprompt: bool,
    has_totp: bool,
    has_password: bool,
    has_username: bool,
    broken: bool,
    revision_date: Option<String>,
}

fn summary(item: &Item) -> ItemSummary {
    let login = item.login.as_ref();
    ItemSummary {
        id: item.id.clone(),
        kind: item.kind,
        name: item.name.to_string(),
        subtitle: subtitle(item),
        host: login.and_then(|l| l.uris.iter().find_map(|u| host_of(&u.uri))),
        favorite: item.favorite,
        folder_id: item.folder_id.clone(),
        organization_id: item.organization_id.clone(),
        collection_ids: item.collection_ids.clone(),
        deleted: item.deleted,
        reprompt: item.reprompt,
        has_totp: login.is_some_and(|l| l.totp.as_ref().is_some_and(|t| !t.is_empty())),
        has_password: login.is_some_and(|l| l.password.as_ref().is_some_and(|p| !p.is_empty())),
        has_username: login.is_some_and(|l| l.username.as_ref().is_some_and(|u| !u.is_empty())),
        broken: item.broken,
        revision_date: item.revision_date.clone(),
    }
}

#[tauri::command]
pub(crate) fn vault_items(state: State<'_, VaultState>) -> Result<Vec<ItemSummary>> {
    state.with_unlocked(|u| Ok(u.vault.items.iter().map(summary).collect()))
}

fn find<'a>(unlocked: &'a Unlocked, id: &str) -> Result<&'a Item> {
    unlocked
        .vault
        .item(id)
        .ok_or_else(|| Failure::new("not-found", "This item isn't in the vault any more."))
}

/// The identity fields, in the order they're shown, and whether each is sensitive.
const IDENTITY_FIELDS: &[(&str, bool)] = &[
    ("title", false),
    ("firstName", false),
    ("middleName", false),
    ("lastName", false),
    ("username", false),
    ("company", false),
    ("email", false),
    ("phone", false),
    ("address1", false),
    ("address2", false),
    ("address3", false),
    ("postalCode", false),
    ("city", false),
    ("state", false),
    ("country", false),
    ("ssn", true),
    ("passportNumber", true),
    ("licenseNumber", true),
];

fn identity_value<'a>(item: &'a Item, name: &str) -> Option<&'a Secret> {
    let i = item.identity.as_ref()?;
    match name {
        "title" => i.title.as_ref(),
        "firstName" => i.first_name.as_ref(),
        "middleName" => i.middle_name.as_ref(),
        "lastName" => i.last_name.as_ref(),
        "username" => i.username.as_ref(),
        "company" => i.company.as_ref(),
        "email" => i.email.as_ref(),
        "phone" => i.phone.as_ref(),
        "address1" => i.address1.as_ref(),
        "address2" => i.address2.as_ref(),
        "address3" => i.address3.as_ref(),
        "postalCode" => i.postal_code.as_ref(),
        "city" => i.city.as_ref(),
        "state" => i.state.as_ref(),
        "country" => i.country.as_ref(),
        "ssn" => i.ssn.as_ref(),
        "passportNumber" => i.passport_number.as_ref(),
        "licenseNumber" => i.license_number.as_ref(),
        _ => None,
    }
}

fn present(value: &Option<Secret>) -> bool {
    value.as_ref().is_some_and(|v| !v.is_empty())
}

/// An item's details, secrets left out: they come one by one through
/// [`reveal_field`]. An item with a master password re-prompt only says so
/// until the prompt was answered.
#[tauri::command]
pub(crate) fn vault_item(state: State<'_, VaultState>, id: String) -> Result<Value> {
    state.with_unlocked(|u| {
        let item = find(u, &id)?;
        let summary = summary(item);
        if item.reprompt && !u.reprompt_ok.contains(&id) {
            return Ok(json!({ "summary": summary, "locked": true }));
        }
        let login = item.login.as_ref().map(|l| {
            json!({
                "username": text(&l.username),
                "hasPassword": present(&l.password),
                "hasTotp": present(&l.totp),
                "passwordRevisionDate": l.password_revision_date,
                "uris": l.uris.iter().map(|u| json!({
                    "uri": u.uri.to_string(),
                    "match": u.match_kind,
                    "host": host_of(&u.uri),
                    "openable": u.uri.starts_with("http://") || u.uri.starts_with("https://"),
                })).collect::<Vec<_>>(),
                "passkeys": l.passkeys,
            })
        });
        let card = item.card.as_ref().map(|c| {
            json!({
                "cardholderName": text(&c.cardholder_name),
                "brand": text(&c.brand),
                "numberEnding": c.number.as_ref().filter(|n| !n.is_empty()).map(|n| last_four(n)),
                "expMonth": text(&c.exp_month),
                "expYear": text(&c.exp_year),
                "hasCode": present(&c.code),
            })
        });
        let identity = item.identity.as_ref().map(|_| {
            IDENTITY_FIELDS
                .iter()
                .filter_map(|(name, sensitive)| {
                    let value = identity_value(item, name).filter(|v| !v.is_empty())?;
                    Some(json!({
                        "name": name,
                        "sensitive": sensitive,
                        "value": if *sensitive { None } else { Some(value.to_string()) },
                    }))
                })
                .collect::<Vec<_>>()
        });
        let ssh = item.ssh_key.as_ref().map(|s| {
            json!({
                "publicKey": text(&s.public_key),
                "fingerprint": text(&s.fingerprint),
                "hasPrivateKey": present(&s.private_key),
            })
        });
        let fields = item
            .fields
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let kind = f.kind;
                json!({
                    "index": index,
                    "name": text(&f.name),
                    "kind": kind,
                    "value": match kind {
                        FieldKind::Text | FieldKind::Boolean => text(&f.value),
                        _ => None,
                    },
                    "hasValue": present(&f.value),
                })
            })
            .collect::<Vec<_>>();
        let history = item
            .password_history
            .iter()
            .enumerate()
            .map(|(index, h)| json!({ "index": index, "lastUsed": h.last_used }))
            .collect::<Vec<_>>();
        Ok(json!({
            "summary": summary,
            "locked": false,
            "notes": text(&item.notes),
            "login": login,
            "card": card,
            "identity": identity,
            "sshKey": ssh,
            "fields": fields,
            "passwordHistory": history,
            "attachments": item.attachments,
            "creationDate": item.creation_date,
        }))
    })
}

#[tauri::command]
pub(crate) async fn verify_reprompt(
    state: State<'_, VaultState>,
    id: String,
    password: String,
) -> Result<()> {
    let password = Zeroizing::new(password);
    let account = state.account.lock().clone().ok_or_else(Failure::locked)?;
    let master_key = derive_off_thread(password, account.email.clone(), account.kdf).await?;
    let protected: EncString = account.protected_user_key.parse()?;
    decrypt_user_key(&master_key, &protected)
        .map_err(|_| Failure::new("wrong-password", "The master password is wrong."))?;
    state.with_unlocked(|u| {
        u.reprompt_ok.insert(id);
        Ok(())
    })
}

/// A single value of an item, by name: `password`, `username`, `notes`,
/// `uri:<n>`, `card-number`, `card-code`, `card-name`, `card-expiry`,
/// `identity:<name>`, `ssh-private`, `ssh-public`, `ssh-fingerprint`,
/// `field:<n>`, `history:<n>`, `totp` (the current code).
fn value_of(unlocked: &Unlocked, id: &str, field: &str) -> Result<Zeroizing<String>> {
    let item = find(unlocked, id)?;
    if item.reprompt && !unlocked.reprompt_ok.contains(id) {
        return Err(Failure::new(
            "reprompt",
            "This item asks for the master password first.",
        ));
    }
    let missing = || Failure::new("not-found", "This item has no such value.");
    let clone = |v: Option<&Secret>| v.filter(|v| !v.is_empty()).cloned().ok_or_else(missing);
    let index = |prefix: &str| -> Result<usize> {
        field
            .strip_prefix(prefix)
            .and_then(|n| n.parse().ok())
            .ok_or_else(missing)
    };
    let login = item.login.as_ref();
    let card = item.card.as_ref();
    let ssh = item.ssh_key.as_ref();
    match field {
        "username" => clone(login.and_then(|l| l.username.as_ref())),
        "password" => clone(login.and_then(|l| l.password.as_ref())),
        "totp" => {
            let secret = clone(login.and_then(|l| l.totp.as_ref()))?;
            Ok(totp::Totp::parse(&secret)?.now().0)
        }
        "notes" => clone(item.notes.as_ref()),
        "card-number" => clone(card.and_then(|c| c.number.as_ref())),
        "card-code" => clone(card.and_then(|c| c.code.as_ref())),
        "card-name" => clone(card.and_then(|c| c.cardholder_name.as_ref())),
        "card-expiry" => {
            let c = card.ok_or_else(missing)?;
            let month = text(&c.exp_month).unwrap_or_default();
            let year = text(&c.exp_year).unwrap_or_default();
            if month.is_empty() && year.is_empty() {
                return Err(missing());
            }
            Ok(Zeroizing::new(format!("{:0>2}/{year}", month)))
        }
        "ssh-private" => clone(ssh.and_then(|s| s.private_key.as_ref())),
        "ssh-public" => clone(ssh.and_then(|s| s.public_key.as_ref())),
        "ssh-fingerprint" => clone(ssh.and_then(|s| s.fingerprint.as_ref())),
        _ if field.starts_with("uri:") => clone(
            login
                .and_then(|l| l.uris.get(index("uri:").ok()?))
                .map(|u| &u.uri),
        ),
        _ if field.starts_with("field:") => clone(
            item.fields
                .get(index("field:")?)
                .and_then(|f| f.value.as_ref()),
        ),
        _ if field.starts_with("history:") => clone(
            item.password_history
                .get(index("history:")?)
                .map(|h| &h.password),
        ),
        _ if field.starts_with("identity:") => {
            clone(identity_value(item, field.trim_start_matches("identity:")))
        }
        _ => Err(missing()),
    }
}

#[tauri::command]
pub(crate) fn reveal_field(
    state: State<'_, VaultState>,
    id: String,
    field: String,
) -> Result<String> {
    state.touch();
    state.with_unlocked(|u| Ok(value_of(u, &id, &field)?.to_string()))
}

#[tauri::command]
pub(crate) fn copy_field(state: State<'_, VaultState>, id: String, field: String) -> Result<()> {
    state.touch();
    let value = state.with_unlocked(|u| value_of(u, &id, &field))?;
    let clear = state.security.lock().clipboard;
    state
        .clipboard
        .copy(&value, clear)
        .map_err(|e| Failure::new("clipboard", e))
}

/// A generated password, copied the same way as a vault value.
#[tauri::command]
pub(crate) fn copy_generated(state: State<'_, VaultState>, text: String) -> Result<()> {
    let text = Zeroizing::new(text);
    let clear = state.security.lock().clipboard;
    state
        .clipboard
        .copy(&text, clear)
        .map_err(|e| Failure::new("clipboard", e))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpCode {
    code: String,
    remaining: u64,
    period: u64,
}

#[tauri::command]
pub(crate) fn totp_code(state: State<'_, VaultState>, id: String) -> Result<TotpCode> {
    state.with_unlocked(|u| {
        let item = find(u, &id)?;
        if item.reprompt && !u.reprompt_ok.contains(&id) {
            return Err(Failure::new(
                "reprompt",
                "This item asks for the master password first.",
            ));
        }
        let secret = item
            .login
            .as_ref()
            .and_then(|l| l.totp.as_ref())
            .ok_or_else(|| Failure::new("not-found", "No authenticator key."))?;
        let totp = totp::Totp::parse(secret)?;
        let (code, remaining) = totp.now();
        Ok(TotpCode {
            code: code.to_string(),
            remaining,
            period: totp.period,
        })
    })
}

#[derive(Debug, Serialize)]
pub struct Generated {
    password: String,
    bits: u32,
}

#[tauri::command]
pub(crate) fn generate_password(options: generator::Options) -> Generated {
    let password = generator::password(&options);
    Generated {
        bits: generator::entropy_bits(&password),
        password: password.to_string(),
    }
}

impl VaultState {
    /// A login's address, for opening in the browser.
    pub(crate) fn item_uri(&self, id: &str, index: usize) -> Option<String> {
        let unlocked = self.unlocked.read();
        let unlocked = unlocked.as_ref()?;
        let item = unlocked.vault.item(id)?;
        if item.reprompt && !unlocked.reprompt_ok.contains(id) {
            return None;
        }
        let uri = item.login.as_ref()?.uris.get(index)?.uri.to_string();
        // Bitwarden keeps addresses without a scheme too; the browser needs one.
        Some(if uri.contains("://") {
            uri
        } else {
            format!("https://{uri}")
        })
    }

    pub(crate) fn web_vault(&self) -> Option<String> {
        self.account.lock().as_ref().map(|a| a.server.web())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_card_endings() {
        assert_eq!(
            host_of("https://www.github.com/login").as_deref(),
            Some("github.com")
        );
        assert_eq!(host_of("nas.local:5001").as_deref(), Some("nas.local"));
        assert_eq!(last_four("4111 1111 1111 1234"), "1234");
        assert_eq!(last_four("12"), "12");
    }
}
