//! The vault, as the page sees it: the accounts, logging in, unlocking,
//! locking, syncing, the items and saving them.
//!
//! Secrets stay here. The list and an item's details carry names, usernames,
//! addresses and notes; a password, a card number, a hidden field or a
//! private key only goes to the page when someone clicks the eye
//! ([`reveal_field`]), and copying ([`copy_field`]) goes from here straight to
//! the clipboard. Editing works the same way round: the page sends back what
//! was typed, and what nobody touched is taken from the item that is already
//! here — so a password nobody looked at is never in the window.
//!
//! Several accounts live here at once — a Vaultwarden at home, one at work.
//! One of them is the open one; the others keep their own keys, their own
//! vault and their own session. Locking drops every decrypted value of every
//! account.

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use uwulock_bitwarden::api::{parse_sync, PasswordLogin, TwoFactorAnswer};
use uwulock_bitwarden::crypto::{self, decrypt_user_key};
use uwulock_bitwarden::vault::{Field, FieldKind, Item, ItemKind, LoginUri, Secret};
use uwulock_bitwarden::wire;
use uwulock_bitwarden::{
    generator, totp, Client, Device, EncString, Error, Kdf, LoginOutcome, Server, Session,
    SymmetricKey, Vault,
};
use zeroize::Zeroizing;

use crate::account::{Account, Storage, Stored};
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
            Error::Conflict => "conflict",
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

/// What went wrong for one account, as the account card shows it.
#[derive(Debug, Default, Clone)]
struct Trouble {
    sync_error: Option<String>,
    /// The server no longer accepts this device's session: log in again.
    session_expired: bool,
}

pub(crate) struct VaultState {
    storage: Storage,
    /// Every account on this device, in the order they are shown.
    accounts: Mutex<Vec<Stored>>,
    /// The one whose vault is on screen.
    active: Mutex<Option<String>>,
    /// The accounts that are open, by id. An account that was switched away
    /// from stays open until something locks it.
    unlocked: RwLock<HashMap<String, Unlocked>>,
    pending: Mutex<Option<PendingLogin>>,
    syncing: Mutex<HashSet<String>>,
    troubles: Mutex<HashMap<String, Trouble>>,
    security: Mutex<Security>,
    last_activity: Mutex<Instant>,
    clipboard: Arc<Clipboard>,
}

impl VaultState {
    pub fn new(storage: Storage) -> Self {
        let accounts = storage.accounts();
        let active = storage
            .active()
            .or_else(|| accounts.first().map(|a| a.id.clone()));
        VaultState {
            storage,
            accounts: Mutex::new(accounts),
            active: Mutex::new(active),
            unlocked: RwLock::new(HashMap::new()),
            pending: Mutex::new(None),
            syncing: Mutex::new(HashSet::new()),
            troubles: Mutex::new(HashMap::new()),
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

    /// The account on screen.
    fn active_id(&self) -> Result<String> {
        self.active
            .lock()
            .clone()
            .ok_or_else(|| Failure::new("logged-out", "No account on this device."))
    }

    fn account(&self, id: &str) -> Result<Account> {
        self.accounts
            .lock()
            .iter()
            .find(|stored| stored.id == id)
            .map(|stored| stored.account.clone())
            .ok_or_else(|| Failure::new("logged-out", "No such account on this device."))
    }

    fn active_account(&self) -> Result<(String, Account)> {
        let id = self.active_id()?;
        let account = self.account(&id)?;
        Ok((id, account))
    }

    /// Changes an account and writes it back to disk.
    fn update_account(&self, id: &str, change: impl FnOnce(&mut Account)) {
        let mut accounts = self.accounts.lock();
        let Some(stored) = accounts.iter_mut().find(|stored| stored.id == id) else {
            return;
        };
        change(&mut stored.account);
        if let Err(error) = self.storage.save_account(id, &stored.account) {
            tracing::warn!(%error, "couldn't save the account");
        }
    }

    fn trouble(&self, id: &str) -> Trouble {
        self.troubles.lock().get(id).cloned().unwrap_or_default()
    }

    fn set_trouble(&self, id: &str, trouble: Trouble) {
        self.troubles.lock().insert(id.to_string(), trouble);
    }

    /// Locks every account: no key, no session, no clipboard.
    fn lock_now(&self) {
        self.unlocked.write().clear();
        *self.pending.lock() = None;
        self.clipboard.clear_now();
    }

    /// Runs `f` on the open vault of the account on screen. Doesn't count as
    /// activity: the page polls (the one-time code, every second) and must not
    /// keep the vault open by that alone. Activity is what the user does —
    /// see [`touch`].
    fn with_unlocked<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> {
        let id = self.active_id().map_err(|_| Failure::locked())?;
        let mut guard = self.unlocked.write();
        let unlocked = guard.get_mut(&id).ok_or_else(Failure::locked)?;
        f(unlocked)
    }
}

// ── Status ─────────────────────────────────────────────────

/// One account in the switcher.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBrief {
    id: String,
    label: String,
    email: String,
    name: Option<String>,
    server: String,
    server_kind: &'static str,
    /// Its vault is open — switching to it asks for nothing.
    unlocked: bool,
    active: bool,
    last_sync: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `logged-out`, `locked` or `unlocked` — for the account on screen.
    state: &'static str,
    account_id: Option<String>,
    label: Option<String>,
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
    /// Every account on this device, the open one included.
    accounts: Vec<AccountBrief>,
}

fn server_kind(server: &Server) -> &'static str {
    match server {
        Server::BitwardenUs => "bitwarden-us",
        Server::BitwardenEu => "bitwarden-eu",
        Server::SelfHosted { .. } => "self-hosted",
    }
}

fn status_of(state: &VaultState) -> Status {
    let accounts = state.accounts.lock().clone();
    let active = state.active.lock().clone();
    let open = state.unlocked.read();
    let syncing = state.syncing.lock().clone();
    let account = active
        .as_ref()
        .and_then(|id| accounts.iter().find(|stored| &stored.id == id))
        .map(|stored| stored.account.clone());
    let unlocked = active.as_ref().is_some_and(|id| open.contains_key(id));
    let trouble = active
        .as_ref()
        .map(|id| state.trouble(id))
        .unwrap_or_default();

    Status {
        state: match (&account, unlocked) {
            (None, _) => "logged-out",
            (Some(_), false) => "locked",
            (Some(_), true) => "unlocked",
        },
        account_id: account.as_ref().and(active.clone()),
        label: account.as_ref().map(Account::title),
        email: account.as_ref().map(|a| a.email.clone()),
        name: account.as_ref().and_then(|a| a.name.clone()),
        server: account.as_ref().map(|a| a.server.label()),
        server_kind: account.as_ref().map(|a| server_kind(&a.server)),
        server_url: account.as_ref().and_then(|a| match &a.server {
            Server::SelfHosted { url } => Some(url.clone()),
            _ => None,
        }),
        last_sync: account.as_ref().and_then(|a| a.last_sync),
        syncing: active.as_ref().is_some_and(|id| syncing.contains(id)),
        sync_error: trouble.sync_error,
        session_expired: trouble.session_expired,
        accounts: accounts
            .iter()
            .map(|stored| AccountBrief {
                label: stored.account.title(),
                email: stored.account.email.clone(),
                name: stored.account.name.clone(),
                server: stored.account.server.label(),
                server_kind: server_kind(&stored.account.server),
                unlocked: open.contains_key(&stored.id),
                active: active.as_deref() == Some(stored.id.as_str()),
                last_sync: stored.account.last_sync,
                id: stored.id.clone(),
            })
            .collect(),
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
    state.refuse_weaker_kdf(&server, &email, kdf)?;
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

/// The remember token this device already has for that account, if any. It is
/// sealed under the user key, which a fresh login doesn't have yet — so it
/// only helps while that account is unlocked (logging in again after the
/// session expired).
fn remembered_token(state: &VaultState, client: &Client, email: &str) -> Option<Zeroizing<String>> {
    let stored = state.accounts.lock().iter().find_map(|stored| {
        (stored.account.email == email && &stored.account.server == client.server())
            .then(|| stored.clone())
    })?;
    let unlocked = state.unlocked.read();
    Account::unseal(
        &stored.account.protected_remember_token,
        &unlocked.get(&stored.id)?.user_key,
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

    // The same account may already be on this device: logging in again after
    // the session expired keeps its folder, its label and its remember token.
    let id = state.storage.id_for(&pending.server, &pending.email);
    let previous = state.account(&id).ok();
    let previous_remember = previous.as_ref().and_then(|account| {
        let unlocked = state.unlocked.read();
        Account::unseal(
            &account.protected_remember_token,
            &unlocked.get(&id)?.user_key,
        )
    });
    let remember = session.remember_token.clone().or(previous_remember);

    let mut account = Account {
        version: 1,
        server: pending.server.clone(),
        email: pending.email.clone(),
        name: None,
        label: previous.and_then(|account| account.label),
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
        .save_account(&id, &account)
        .map_err(|e| Failure::new("io", format!("Couldn't save the account: {e}")))?;
    if let Err(error) = state.storage.save_cache(&id, &text) {
        tracing::warn!(%error, "couldn't cache the vault");
    }
    let _ = state.storage.set_active(Some(&id));
    {
        let mut accounts = state.accounts.lock();
        match accounts.iter_mut().find(|stored| stored.id == id) {
            Some(stored) => stored.account = account,
            None => accounts.push(Stored {
                id: id.clone(),
                account,
            }),
        }
    }
    state.unlocked.write().insert(
        id.clone(),
        Unlocked {
            user_key,
            vault,
            session: Some(session),
            reprompt_ok: HashSet::new(),
        },
    );
    *state.active.lock() = Some(id.clone());
    state.set_trouble(&id, Trouble::default());
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
    let (id, account) = state.active_account()?;
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
            state.update_account(&id, |account| account.kdf = kdf);
            key
        }
    };

    let vault = match state.storage.load_cache(&id) {
        Some(text) => match parse_sync(&text).and_then(|sync| Vault::open(&sync, &user_key)) {
            Ok(vault) => vault,
            Err(error) => {
                tracing::warn!(%error, "the cached vault didn't open");
                Vault::default()
            }
        },
        None => Vault::default(),
    };
    state.unlocked.write().insert(
        id,
        Unlocked {
            user_key,
            vault,
            session: None,
            reprompt_ok: HashSet::new(),
        },
    );
    state.touch();
    tracing::info!("unlocked");
    emit_status(&app);
    spawn_sync(app.clone());
    Ok(status_of(&state))
}

/// Locks every account at once — one click, nothing left open behind it.
#[tauri::command]
pub(crate) fn lock(app: AppHandle, state: State<'_, VaultState>) {
    state.lock_now();
    tracing::info!("locked");
    emit_status(&app);
}

/// Logging out: that account, its session and its cached vault leave this
/// device. Without an id it's the account on screen; the others stay.
#[tauri::command]
pub(crate) fn logout(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: Option<String>,
) -> Result<Status> {
    let id = match id {
        Some(id) => id,
        None => state.active_id()?,
    };
    state.log_out(&id)?;
    tracing::info!("logged out");
    emit_status(&app);
    Ok(status_of(&state))
}

impl VaultState {
    /// Logging in again to an account this device knows: the server doesn't
    /// get to ask for a cheaper key derivation than the one stored from the
    /// last login, or the hash sent next would be that much easier to guess
    /// the master password from. Whoever lowered it on purpose removes the
    /// account here and adds it again; a first login takes what the server
    /// says, within `Kdf::check` and `Kdf::check_ceilings`.
    fn refuse_weaker_kdf(&self, server: &Server, email: &str, kdf: Kdf) -> Result<()> {
        let accounts = self.accounts.lock();
        let known = accounts
            .iter()
            .find(|stored| &stored.account.server == server && stored.account.email == email);
        match known {
            Some(stored) if kdf.is_weaker_than(&stored.account.kdf) => Err(Failure::new(
                "weaker-kdf",
                format!(
                    "The server asks for a weaker key derivation ({kdf:?}) than this \
                     account's last login used ({:?}).",
                    stored.account.kdf
                ),
            )),
            _ => Ok(()),
        }
    }

    /// Removes one account from this device. The id comes from the page, so
    /// it has to be one of the accounts first: nothing else is ever handed to
    /// `Storage::forget`.
    fn log_out(&self, id: &str) -> Result<()> {
        self.account(id)?;
        self.unlocked.write().remove(id);
        *self.pending.lock() = None;
        self.clipboard.clear_now();
        self.storage
            .forget(id)
            .map_err(|e| Failure::new("io", format!("Couldn't remove the account: {e}")))?;
        self.accounts.lock().retain(|stored| stored.id != id);
        self.troubles.lock().remove(id);
        if self.active.lock().as_deref() == Some(id) {
            let next = self.accounts.lock().first().map(|stored| stored.id.clone());
            let _ = self.storage.set_active(next.as_deref());
            *self.active.lock() = next;
        }
        Ok(())
    }
}

/// Brings another account's vault on screen. One that is still open shows up
/// straight away; one that isn't asks for its master password.
#[tauri::command]
pub(crate) fn switch_account(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<Status> {
    state.account(&id)?;
    state.touch();
    let _ = state.storage.set_active(Some(&id));
    *state.active.lock() = Some(id.clone());
    emit_status(&app);
    if state.unlocked.read().contains_key(&id) {
        // Its vault may be a few minutes old; bring it up to date in the back.
        spawn_sync(app.clone());
    }
    Ok(status_of(&state))
}

/// What an account is called in the switcher. An empty name goes back to the
/// server's address.
#[tauri::command]
pub(crate) fn rename_account(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    label: String,
) -> Result<Status> {
    state.account(&id)?;
    let label = label.trim().to_string();
    state.update_account(&id, |account| {
        account.label = (!label.is_empty()).then_some(label)
    });
    emit_status(&app);
    Ok(status_of(&state))
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
    let id = app.state::<VaultState>().active_id()?;
    sync_account(app, &id).await
}

async fn sync_account(app: &AppHandle, id: &str) -> Result<()> {
    let state = app.state::<VaultState>();
    // One sync per account at a time.
    if !state.syncing.lock().insert(id.to_string()) {
        return Ok(());
    }
    emit_status(app);
    let result = sync_inner(&state, id).await;
    state.syncing.lock().remove(id);
    match &result {
        Ok(()) => state.set_trouble(id, Trouble::default()),
        Err(error) if error.kind == "locked" => {}
        Err(error) => state.set_trouble(
            id,
            Trouble {
                sync_error: Some(error.message.clone()),
                session_expired: error.kind == "session-expired",
            },
        ),
    }
    emit_status(app);
    if result.is_ok() {
        let _ = app.emit("vault-changed", ());
    }
    result
}

/// A token this account can use right now, renewed from the refresh token
/// when the old one is about to run out.
async fn access_token(state: &VaultState, id: &str) -> Result<Zeroizing<String>> {
    let account = state.account(id)?;
    let (user_key, access) = {
        let unlocked = state.unlocked.read();
        let unlocked = unlocked.get(id).ok_or_else(Failure::locked)?;
        let access = unlocked
            .session
            .as_ref()
            .filter(|s| !s.is_expiring())
            .map(|s| s.access_token.clone());
        (unlocked.user_key.clone(), access)
    };
    if let Some(token) = access {
        return Ok(token);
    }
    let client = state.client(account.server.clone())?;
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
        let sealed = Account::seal(new, &user_key);
        state.update_account(id, |account| account.protected_refresh_token = Some(sealed));
    }
    if let Some(unlocked) = state.unlocked.write().get_mut(id) {
        unlocked.session = Some(session);
    }
    Ok(token)
}

async fn sync_inner(state: &VaultState, id: &str) -> Result<()> {
    let account = state.account(id)?;
    let access = access_token(state, id).await?;
    let client = state.client(account.server.clone())?;
    let user_key = {
        let unlocked = state.unlocked.read();
        unlocked
            .get(id)
            .ok_or_else(Failure::locked)?
            .user_key
            .clone()
    };

    let text = client.sync(&access).await?;
    let sync = parse_sync(&text)?;
    let vault = Vault::open(&sync, &user_key)?;
    state
        .storage
        .save_cache(id, &text)
        .map_err(|e| Failure::new("io", format!("Couldn't cache the vault: {e}")))?;
    state.update_account(id, |stored| {
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
    });
    if let Some(unlocked) = state.unlocked.write().get_mut(id) {
        unlocked.vault = vault;
    }
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
            if state.unlocked.read().is_empty() {
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
            // Only the account on screen: the others come up to date when
            // someone switches to them.
            let expired = state
                .active_id()
                .map(|id| state.trouble(&id).session_expired)
                .unwrap_or(true);
            if last_sync.elapsed() >= SYNC_EVERY && !expired {
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

/// The moment, as Bitwarden writes dates: `2026-09-23T12:30:00.000Z`. Saved
/// items carry one, for the password history and the trash.
fn iso_now() -> String {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    iso_from_unix(since.as_secs(), since.subsec_millis())
}

fn iso_from_unix(seconds: u64, millis: u32) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Days since the epoch to a calendar date, counting from March so leap
    // days land at the end of the year (Howard Hinnant's civil_from_days).
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rest / 3600,
        (rest / 60) % 60,
        rest % 60
    )
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
                "passkeys": l.passkey_count(),
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
    let (_, account) = state.active_account()?;
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

// ── Saving ─────────────────────────────────────────────────
//
// The editor sends back what was typed. A value it never had — a password
// nobody looked at, a card number, a hidden field — comes as `null`, and then
// the one already here is kept; `""` clears it. So editing a name doesn't need
// the password to pass through the window.

/// A value the editor may have left alone.
type Keep = Option<String>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    kind: ItemKind,
    name: String,
    #[serde(default)]
    notes: Keep,
    #[serde(default)]
    favorite: bool,
    #[serde(default)]
    reprompt: bool,
    #[serde(default)]
    folder_id: Option<String>,
    #[serde(default)]
    login: Option<LoginDraft>,
    #[serde(default)]
    card: Option<CardDraft>,
    /// The identity's fields by name; a name that isn't in here keeps its value.
    #[serde(default)]
    identity: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    ssh_key: Option<SshKeyDraft>,
    #[serde(default)]
    fields: Vec<FieldDraft>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginDraft {
    #[serde(default)]
    username: Keep,
    #[serde(default)]
    password: Keep,
    #[serde(default)]
    totp: Keep,
    #[serde(default)]
    uris: Vec<UriDraft>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UriDraft {
    uri: String,
    #[serde(default, rename = "match")]
    match_kind: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CardDraft {
    #[serde(default)]
    cardholder_name: Keep,
    #[serde(default)]
    brand: Keep,
    #[serde(default)]
    number: Keep,
    #[serde(default)]
    exp_month: Keep,
    #[serde(default)]
    exp_year: Keep,
    #[serde(default)]
    code: Keep,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SshKeyDraft {
    #[serde(default)]
    private_key: Keep,
    #[serde(default)]
    public_key: Keep,
    #[serde(default)]
    fingerprint: Keep,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldDraft {
    #[serde(default)]
    name: Option<String>,
    kind: FieldKind,
    #[serde(default)]
    value: Keep,
    /// Which field of the item this one was before. Carries the value the
    /// editor never saw, and where a linked field points.
    #[serde(default)]
    from: Option<usize>,
}

fn secret_of(value: String) -> Option<Secret> {
    (!value.is_empty()).then(|| Zeroizing::new(value))
}

/// `null` keeps what is there, a value replaces it, `""` clears it.
fn apply_keep(field: &mut Option<Secret>, value: Keep) {
    if let Some(value) = value {
        *field = secret_of(value);
    }
}

fn apply_draft(item: &mut Item, draft: Draft, now: &str) -> Result<()> {
    item.name = Zeroizing::new(draft.name.trim().to_string());
    apply_keep(&mut item.notes, draft.notes);
    item.favorite = draft.favorite;
    item.reprompt = draft.reprompt;
    item.folder_id = draft.folder_id.filter(|id| !id.is_empty());

    if let Some(login) = draft.login {
        let old_uris = item
            .login
            .as_ref()
            .map(|l| l.uris.clone())
            .unwrap_or_default();
        let current = item
            .login
            .as_mut()
            .ok_or_else(|| Failure::new("invalid", "This item isn't a login."))?;
        apply_keep(&mut current.username, login.username);
        apply_keep(&mut current.totp, login.totp);
        current.uris = login
            .uris
            .into_iter()
            .filter(|u| !u.uri.trim().is_empty())
            .map(|u| {
                let uri = u.uri.trim().to_string();
                LoginUri {
                    // An address that didn't change keeps the checksum the
                    // server made for it; a changed one has none any more.
                    checksum: old_uris
                        .iter()
                        .find(|old| old.uri.as_str() == uri)
                        .and_then(|old| old.checksum.clone()),
                    uri: Zeroizing::new(uri),
                    match_kind: u.match_kind.filter(|m| *m <= 5),
                }
            })
            .collect();
        // Last, because it writes the history.
        if let Some(password) = login.password {
            item.set_password(Zeroizing::new(password), now);
        }
    }

    if let Some(card) = draft.card {
        let current = item
            .card
            .as_mut()
            .ok_or_else(|| Failure::new("invalid", "This item isn't a card."))?;
        apply_keep(&mut current.cardholder_name, card.cardholder_name);
        apply_keep(&mut current.brand, card.brand);
        apply_keep(&mut current.number, card.number);
        apply_keep(&mut current.exp_month, card.exp_month);
        apply_keep(&mut current.exp_year, card.exp_year);
        apply_keep(&mut current.code, card.code);
    }

    if let Some(values) = draft.identity {
        if item.identity.is_none() {
            return Err(Failure::new("invalid", "This item isn't an identity."));
        }
        for (name, _) in IDENTITY_FIELDS {
            let Some(value) = values.get(*name) else {
                continue;
            };
            let value = secret_of(value.trim().to_string());
            let identity = item.identity.as_mut().expect("checked above");
            match *name {
                "title" => identity.title = value,
                "firstName" => identity.first_name = value,
                "middleName" => identity.middle_name = value,
                "lastName" => identity.last_name = value,
                "username" => identity.username = value,
                "company" => identity.company = value,
                "email" => identity.email = value,
                "phone" => identity.phone = value,
                "address1" => identity.address1 = value,
                "address2" => identity.address2 = value,
                "address3" => identity.address3 = value,
                "postalCode" => identity.postal_code = value,
                "city" => identity.city = value,
                "state" => identity.state = value,
                "country" => identity.country = value,
                "ssn" => identity.ssn = value,
                "passportNumber" => identity.passport_number = value,
                "licenseNumber" => identity.license_number = value,
                _ => {}
            }
        }
    }

    if let Some(ssh) = draft.ssh_key {
        let current = item
            .ssh_key
            .as_mut()
            .ok_or_else(|| Failure::new("invalid", "This item isn't an SSH key."))?;
        apply_keep(&mut current.private_key, ssh.private_key);
        apply_keep(&mut current.public_key, ssh.public_key);
        apply_keep(&mut current.fingerprint, ssh.fingerprint);
    }

    let old_fields = item.fields.clone();
    item.fields = draft
        .fields
        .into_iter()
        .map(|field| {
            let old = field.from.and_then(|index| old_fields.get(index));
            Field {
                name: field.name.and_then(secret_of),
                value: match field.value {
                    Some(value) => secret_of(value),
                    None => old.and_then(|old| old.value.clone()),
                },
                kind: field.kind,
                linked_id: old
                    .filter(|_| field.kind == FieldKind::Linked)
                    .and_then(|old| old.linked_id),
            }
        })
        .collect();
    Ok(())
}

/// What a write changed, for the cached vault. Applying it here keeps the
/// list and the details right away, without waiting for the next sync.
enum Patch {
    Cipher(Value),
    Trash(String),
    RemoveCipher(String),
    Folder(Value),
    RemoveFolder(String),
}

/// The value of `name`, whatever case the server spells it in.
fn entry<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    let object = value.as_object()?;
    object
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

fn entry_id(value: &Value) -> Option<&str> {
    entry(value, "id")?.as_str()
}

/// The `ciphers` or `folders` list of a sync, made if it isn't there.
fn list_mut<'a>(sync: &'a mut Value, name: &str) -> Option<&'a mut Vec<Value>> {
    let object = sync.as_object_mut()?;
    let key = object
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned()
        .unwrap_or_else(|| name.to_string());
    object
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
}

fn set_field(value: &mut Value, name: &str, to: Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let key = object
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned()
        .unwrap_or_else(|| name.to_string());
    object.insert(key, to);
}

/// Writes the change into the cached sync and opens the vault again from it.
fn patch_cache(state: &VaultState, account_id: &str, patch: Patch) -> Result<()> {
    let Some(text) = state.storage.load_cache(account_id) else {
        // Nothing cached yet; the next sync brings everything anyway.
        return Ok(());
    };
    let mut sync: Value = serde_json::from_str(&text)
        .map_err(|e| Failure::new("io", format!("The cached vault doesn't parse: {e}")))?;
    match patch {
        Patch::Cipher(cipher) => {
            let Some(id) = entry_id(&cipher).map(str::to_string) else {
                return Ok(());
            };
            let ciphers = list_mut(&mut sync, "ciphers").ok_or_else(broken_cache)?;
            match ciphers
                .iter()
                .position(|c| entry_id(c) == Some(id.as_str()))
            {
                Some(index) => ciphers[index] = cipher,
                None => ciphers.push(cipher),
            }
        }
        Patch::Trash(id) => {
            let ciphers = list_mut(&mut sync, "ciphers").ok_or_else(broken_cache)?;
            if let Some(cipher) = ciphers
                .iter_mut()
                .find(|c| entry_id(c) == Some(id.as_str()))
            {
                set_field(cipher, "deletedDate", json!(iso_now()));
            }
        }
        Patch::RemoveCipher(id) => {
            let ciphers = list_mut(&mut sync, "ciphers").ok_or_else(broken_cache)?;
            ciphers.retain(|c| entry_id(c) != Some(id.as_str()));
        }
        Patch::Folder(folder) => {
            let Some(id) = entry_id(&folder).map(str::to_string) else {
                return Ok(());
            };
            let folders = list_mut(&mut sync, "folders").ok_or_else(broken_cache)?;
            match folders
                .iter()
                .position(|f| entry_id(f) == Some(id.as_str()))
            {
                Some(index) => folders[index] = folder,
                None => folders.push(folder),
            }
        }
        Patch::RemoveFolder(id) => {
            let folders = list_mut(&mut sync, "folders").ok_or_else(broken_cache)?;
            folders.retain(|f| entry_id(f) != Some(id.as_str()));
            // The items in it stay, without a folder — as the server does it.
            let ciphers = list_mut(&mut sync, "ciphers").ok_or_else(broken_cache)?;
            for cipher in ciphers.iter_mut() {
                if entry(cipher, "folderId").and_then(Value::as_str) == Some(id.as_str()) {
                    set_field(cipher, "folderId", Value::Null);
                }
            }
        }
    }
    let text = sync.to_string();
    state
        .storage
        .save_cache(account_id, &text)
        .map_err(|e| Failure::new("io", format!("Couldn't cache the vault: {e}")))?;
    let parsed = parse_sync(&text)?;
    let mut guard = state.unlocked.write();
    let unlocked = guard.get_mut(account_id).ok_or_else(Failure::locked)?;
    unlocked.vault = Vault::open(&parsed, &unlocked.user_key)?;
    Ok(())
}

fn broken_cache() -> Failure {
    Failure::new("io", "The cached vault isn't a sync.")
}

/// The item as it is here, ready to be sent — with the reprompt honoured: an
/// item that asks for the master password can't be changed without it either.
fn prepare(state: &VaultState, account_id: &str, id: &str) -> Result<Item> {
    let guard = state.unlocked.read();
    let unlocked = guard.get(account_id).ok_or_else(Failure::locked)?;
    let item = unlocked
        .vault
        .item(id)
        .ok_or_else(|| Failure::new("not-found", "This item isn't in the vault any more."))?;
    if item.reprompt && !unlocked.reprompt_ok.contains(id) {
        return Err(Failure::new(
            "reprompt",
            "This item asks for the master password first.",
        ));
    }
    Ok(item.clone())
}

fn sealed(state: &VaultState, account_id: &str, item: &Item) -> Result<wire::CipherRequest> {
    item.can_save()?;
    let guard = state.unlocked.read();
    let unlocked = guard.get(account_id).ok_or_else(Failure::locked)?;
    let outer = unlocked
        .vault
        .outer_key(item.organization_id.as_deref(), &unlocked.user_key)?;
    Ok(item.seal(outer)?)
}

/// Saves an item: a new one when `id` is empty, otherwise the one it names.
/// Returns the item's id.
#[tauri::command]
pub(crate) async fn save_item(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: Option<String>,
    draft: Draft,
) -> Result<String> {
    state.touch();
    let (account_id, account) = state.active_account()?;
    let mut item = match &id {
        Some(id) => {
            let item = prepare(&state, &account_id, id)?;
            if item.kind != draft.kind {
                return Err(Failure::new("invalid", "An item can't change its type."));
            }
            if item.deleted {
                return Err(Failure::new(
                    "invalid",
                    "An item in the trash can't be changed. Restore it first.",
                ));
            }
            item
        }
        None => Item::new(draft.kind),
    };
    let collection_ids = item.collection_ids.clone();
    apply_draft(&mut item, draft, &iso_now())?;
    let request = sealed(&state, &account_id, &item)?;

    let client = state.client(account.server.clone())?;
    let access = access_token(&state, &account_id).await?;
    let answer = match &id {
        Some(id) => client.update_cipher(&access, id, request).await?,
        None => {
            client
                .create_cipher(&access, request, &collection_ids)
                .await?
        }
    };
    let saved_id = entry_id(&answer)
        .map(str::to_string)
        .or_else(|| id.clone())
        .unwrap_or_default();
    patch_cache(&state, &account_id, Patch::Cipher(answer))?;
    tracing::info!(new = id.is_none(), "item saved");
    let _ = app.emit("vault-changed", ());
    emit_status(&app);
    Ok(saved_id)
}

/// Changes one thing about an item that is already there, and saves it.
async fn change_item(
    app: &AppHandle,
    state: &VaultState,
    id: &str,
    change: impl FnOnce(&mut Item),
) -> Result<()> {
    state.touch();
    let (account_id, account) = state.active_account()?;
    let mut item = prepare(state, &account_id, id)?;
    change(&mut item);
    let request = sealed(state, &account_id, &item)?;
    let client = state.client(account.server.clone())?;
    let access = access_token(state, &account_id).await?;
    let answer = client.update_cipher(&access, id, request).await?;
    patch_cache(state, &account_id, Patch::Cipher(answer))?;
    let _ = app.emit("vault-changed", ());
    emit_status(app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn set_favorite(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    favorite: bool,
) -> Result<()> {
    change_item(&app, &state, &id, |item| item.favorite = favorite).await
}

#[tauri::command]
pub(crate) async fn set_item_folder(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    folder_id: Option<String>,
) -> Result<()> {
    let folder_id = folder_id.filter(|id| !id.is_empty());
    change_item(&app, &state, &id, move |item| item.folder_id = folder_id).await
}

/// Into the trash, or — with `permanent` — gone for good.
#[tauri::command]
pub(crate) async fn delete_item(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    permanent: bool,
) -> Result<()> {
    state.touch();
    let (account_id, account) = state.active_account()?;
    prepare(&state, &account_id, &id)?;
    let client = state.client(account.server.clone())?;
    let access = access_token(&state, &account_id).await?;
    if permanent {
        client.delete_cipher(&access, &id).await?;
        patch_cache(&state, &account_id, Patch::RemoveCipher(id))?;
    } else {
        client.trash_cipher(&access, &id).await?;
        patch_cache(&state, &account_id, Patch::Trash(id))?;
    }
    tracing::info!(permanent, "item deleted");
    let _ = app.emit("vault-changed", ());
    emit_status(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn restore_item(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<()> {
    state.touch();
    let (account_id, account) = state.active_account()?;
    prepare(&state, &account_id, &id)?;
    let client = state.client(account.server.clone())?;
    let access = access_token(&state, &account_id).await?;
    let answer = client.restore_cipher(&access, &id).await?;
    if entry_id(&answer).is_some() {
        patch_cache(&state, &account_id, Patch::Cipher(answer))?;
    } else {
        sync_account(&app, &account_id).await?;
    }
    let _ = app.emit("vault-changed", ());
    emit_status(&app);
    Ok(())
}

/// A new folder when `id` is empty, otherwise a new name for that one.
#[tauri::command]
pub(crate) async fn save_folder(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: Option<String>,
    name: String,
) -> Result<String> {
    state.touch();
    let (account_id, account) = state.active_account()?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(Failure::new("invalid", "A folder needs a name."));
    }
    let sealed = {
        let guard = state.unlocked.read();
        let unlocked = guard.get(&account_id).ok_or_else(Failure::locked)?;
        EncString::encrypt(name.as_bytes(), &unlocked.user_key).to_string()
    };
    let client = state.client(account.server.clone())?;
    let access = access_token(&state, &account_id).await?;
    let answer = match &id {
        Some(id) => client.rename_folder(&access, id, sealed).await?,
        None => client.create_folder(&access, sealed).await?,
    };
    let saved_id = entry_id(&answer)
        .map(str::to_string)
        .or_else(|| id.clone())
        .unwrap_or_default();
    patch_cache(&state, &account_id, Patch::Folder(answer))?;
    let _ = app.emit("vault-changed", ());
    emit_status(&app);
    Ok(saved_id)
}

/// Removes a folder. The items in it stay, without a folder.
#[tauri::command]
pub(crate) async fn delete_folder(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<()> {
    state.touch();
    let (account_id, account) = state.active_account()?;
    let client = state.client(account.server.clone())?;
    let access = access_token(&state, &account_id).await?;
    client.delete_folder(&access, &id).await?;
    patch_cache(&state, &account_id, Patch::RemoveFolder(id))?;
    let _ = app.emit("vault-changed", ());
    emit_status(&app);
    Ok(())
}

impl VaultState {
    /// A login's address, for opening in the browser.
    pub(crate) fn item_uri(&self, id: &str, index: usize) -> Option<String> {
        let active = self.active.lock().clone()?;
        let unlocked = self.unlocked.read();
        let unlocked = unlocked.get(&active)?;
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
        let (_, account) = self.active_account().ok()?;
        Some(account.server.web())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_look_like_bitwardens() {
        assert_eq!(iso_from_unix(0, 0), "1970-01-01T00:00:00.000Z");
        // A leap day, and the last second of a year.
        assert_eq!(iso_from_unix(951_782_400, 0), "2000-02-29T00:00:00.000Z");
        assert_eq!(iso_from_unix(1_609_459_199, 7), "2020-12-31T23:59:59.007Z");
        assert_eq!(
            iso_from_unix(1_758_629_400, 250),
            "2025-09-23T12:10:00.250Z"
        );
    }

    #[test]
    fn a_patch_finds_its_list_whatever_the_case() {
        let mut sync = json!({ "Ciphers": [{ "Id": "a" }], "profile": {} });
        let ciphers = list_mut(&mut sync, "ciphers").unwrap();
        assert_eq!(ciphers.len(), 1);
        assert_eq!(entry_id(&ciphers[0]), Some("a"));
        ciphers.push(json!({ "id": "b" }));
        // A sync without folders gets the list it was missing.
        assert!(list_mut(&mut sync, "folders").unwrap().is_empty());
        set_field(&mut sync["Ciphers"][0], "deletedDate", json!("now"));
        assert_eq!(sync["Ciphers"][0]["deletedDate"], json!("now"));
        assert_eq!(sync["Ciphers"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn logout_takes_only_an_account_on_this_device() {
        let dir = std::env::temp_dir().join(format!("uwulock-test-{}", uuid::Uuid::new_v4()));
        let storage = Storage::new(dir.join("data")).unwrap();
        let account = Account {
            version: 1,
            server: Server::self_hosted("vault.example.org").unwrap(),
            email: "nyu@example.org".into(),
            name: None,
            label: None,
            kdf: Kdf::Pbkdf2 {
                iterations: 600_000,
            },
            protected_user_key: "2.x|y|z".into(),
            protected_refresh_token: None,
            protected_remember_token: None,
            last_sync: None,
        };
        let id = storage.id_for(&account.server, &account.email);
        storage.save_account(&id, &account).unwrap();
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let state = VaultState::new(storage);

        let absolute = outside.to_string_lossy().to_string();
        for bad in ["", "..", "nonexistent", absolute.as_str()] {
            let failure = state.log_out(bad).unwrap_err();
            assert_eq!(failure.kind, "logged-out", "{bad:?}");
        }
        assert!(outside.exists());
        assert!(dir.join("data").exists());
        assert_eq!(state.accounts.lock().len(), 1);

        state.log_out(&id).unwrap();
        assert!(state.accounts.lock().is_empty());
        assert_eq!(*state.active.lock(), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_known_account_keeps_its_kdf_strength() {
        let dir = std::env::temp_dir().join(format!("uwulock-test-{}", uuid::Uuid::new_v4()));
        let storage = Storage::new(dir.clone()).unwrap();
        let server = Server::self_hosted("vault.example.org").unwrap();
        let account = Account {
            version: 1,
            server: server.clone(),
            email: "nyu@example.org".into(),
            name: None,
            label: None,
            kdf: Kdf::Pbkdf2 {
                iterations: 600_000,
            },
            protected_user_key: "2.x|y|z".into(),
            protected_refresh_token: None,
            protected_remember_token: None,
            last_sync: None,
        };
        let id = storage.id_for(&account.server, &account.email);
        storage.save_account(&id, &account).unwrap();
        let state = VaultState::new(storage);
        let pbkdf2 = |iterations| Kdf::Pbkdf2 { iterations };

        let failure = state
            .refuse_weaker_kdf(&server, "nyu@example.org", pbkdf2(5_000))
            .unwrap_err();
        assert_eq!(failure.kind, "weaker-kdf");
        for same_or_stronger in [
            pbkdf2(600_000),
            pbkdf2(2_000_000),
            Kdf::Argon2id {
                iterations: 3,
                memory_mib: 64,
                parallelism: 4,
            },
        ] {
            state
                .refuse_weaker_kdf(&server, "nyu@example.org", same_or_stronger)
                .unwrap();
        }
        // A first login takes Bitwarden's old defaults as they are.
        state
            .refuse_weaker_kdf(&server, "new@example.org", pbkdf2(5_000))
            .unwrap();
        let other = Server::self_hosted("other.example.org").unwrap();
        state
            .refuse_weaker_kdf(&other, "nyu@example.org", pbkdf2(100_000))
            .unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

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
