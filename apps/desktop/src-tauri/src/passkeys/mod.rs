//! UwULock as the system's passkey provider: the vault's passkeys for
//! browsers and apps outside the extension.
//!
//! - **Linux** ([`linux`]): a virtual FIDO2 security key over `/dev/uhid`.
//!   Browsers talk CTAP2 to it like to a USB key; UwULock asks in its own
//!   window. Off until switched on in the settings.
//! - **Windows 11** ([`windows`]): a plugin passkey manager (webauthn.dll's
//!   plugin API, 24H2+). Experimental, off until switched on.
//! - **Android 14+** ([`android`]): Credential Manager's provider service in
//!   the app (Kotlin, in the mobile plugin) calls in here over JNI; the
//!   system's own sheet asks the person.
//! - **iOS 17+ and macOS 14+** ([`apple`]): an AutoFill extension. Its own
//!   process, so the app leaves it a sealed list of passkeys and takes in
//!   what it made (uwulock_authenticator::apple).
//!
//! Every way in ends here: find the passkeys of a site in the open vault,
//! make one into a login and save it, sign with one. Only the account on
//! screen is used; a locked vault answers nothing. Design and threat model:
//! docs/passkeys.md.

// Asking in UwULock's own dialog (CTAP2 through `DesktopBackend`) is what the
// security key and the Windows plugin do; phones and the Apple extension ask
// in the system's own sheets.
#![cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]

#[cfg(target_os = "android")]
pub(crate) mod android;
#[cfg(target_os = "android")]
mod android_logins;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub(crate) mod apple;
#[cfg(target_os = "linux")]
mod linux;
mod logins;
#[cfg(any(windows, test))]
mod registry;
#[cfg(windows)]
mod windows;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use uwulock_authenticator::ctap2::{self, status, Assertion, GetAssertion, MakeCredential, Status};
use uwulock_authenticator::webauthn::{b64, from_b64};
use uwulock_bitwarden::vault::{Item, ItemKind, LoginUri};
use uwulock_bitwarden::EncString;
use uwulock_core::passkey::{Passkey, BE, BS, UP, UV};
use zeroize::Zeroizing;

use crate::vault::{
    access_token, emit_status, entry_id, host_of, iso_now, patch_cache, prepare, sealed, Failure,
    Patch, Result, VaultState,
};

/// How long a request waits for the person.
const ASK_FOR: Duration = Duration::from_secs(120);

/// Which ways in are on, kept in `passkeys.json` next to the accounts.
/// Nothing secret.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Linux: the virtual security key.
    #[serde(default)]
    pub security_key: bool,
    /// Windows 11: the plugin passkey manager.
    #[serde(default)]
    pub windows_plugin: bool,
    /// iOS and macOS: keep the AutoFill extension's sealed passkey list.
    #[serde(default)]
    pub apple_extension: bool,
}

/// What a request wants from the person.
#[derive(Debug, Clone)]
pub(crate) enum Ask {
    Create(MakeCredential),
    Get(GetAssertion),
    /// "Touch the key you want to use."
    Select,
}

/// What the person decided.
#[derive(Debug, Clone, Default)]
pub(crate) struct Decision {
    /// Create: the login the passkey goes into, `None` for a new one. Get:
    /// the login of the chosen passkey.
    pub item_id: Option<String>,
    /// Get: the chosen passkey.
    pub credential_id: Option<Vec<u8>>,
    /// The master password was typed again.
    pub verified: bool,
}

/// Who asks, as far as UwULock can tell.
#[derive(Debug, Clone, Default)]
pub(crate) struct Client {
    /// "Firefox", "Windows", the programs holding the security key open;
    /// empty when UwULock can't tell.
    pub name: String,
    /// A browser UwULock knows (Linux), or a request Windows signed. The
    /// dialog warns about everything else.
    pub trusted: bool,
}

/// Something the person should know while the setting is on (R7 L-1): on
/// Linux another program of theirs holds the one security key the broker
/// makes per user; on Windows the registry entry that starts UwULock for a
/// request pointed elsewhere. Shown in the settings and as a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    /// `held` (Linux) or `registry` (Windows).
    pub kind: &'static str,
    /// The program holding the key, or what the registry entry started, as
    /// far as UwULock could tell.
    pub holder: Option<String>,
}

struct Pending {
    id: u64,
    ask: Ask,
    client: Client,
    reply: mpsc::Sender<std::result::Result<Decision, Status>>,
}

pub(crate) struct Provider {
    path: PathBuf,
    settings: Mutex<Settings>,
    pending: Mutex<Option<Pending>>,
    next_id: AtomicU64,
    warning: Mutex<Option<Warning>>,
    #[cfg(target_os = "linux")]
    device: Mutex<Option<linux::Device>>,
    #[cfg(windows)]
    plugin: Mutex<Option<windows::Plugin>>,
}

impl Provider {
    pub(crate) fn new(dir: &std::path::Path) -> Self {
        let path = dir.join("passkeys.json");
        let settings = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Provider {
            path,
            settings: Mutex::new(settings),
            pending: Mutex::new(None),
            next_id: AtomicU64::new(1),
            warning: Mutex::new(None),
            #[cfg(target_os = "linux")]
            device: Mutex::new(None),
            #[cfg(windows)]
            plugin: Mutex::new(None),
        }
    }

    pub(crate) fn settings(&self) -> Settings {
        self.settings.lock().clone()
    }

    /// Sets (or with `None` clears) the warning; whether it changed.
    fn set_warning(&self, warning: Option<Warning>) -> bool {
        let mut current = self.warning.lock();
        if *current == warning {
            return false;
        }
        *current = warning;
        true
    }
}

/// The most bytes of a holder (a path, a registry entry) UwULock logs or
/// shows.
const HOLDER_SHOWN: usize = 512;

/// `text` from another program, safe to log and show: control and
/// invisible direction characters escaped, cut to [`HOLDER_SHOWN`] bytes.
pub(crate) fn shown(text: &str) -> String {
    uwulock_authenticator::broker::printable(text, HOLDER_SHOWN)
}

/// Sets the warning, and when it is new, logs it and tells the page
/// (`passkey-provider-warning`, which shows a note and refreshes the
/// settings).
pub(crate) fn warn(app: &AppHandle, mut warning: Option<Warning>) {
    // The holder comes from another program's path or registry entry:
    // escaped and cut before it is logged or shown (R8 C-1).
    if let Some(warning) = &mut warning {
        warning.holder = warning.holder.as_deref().map(shown);
    }
    if !app.state::<Provider>().set_warning(warning.clone()) {
        return;
    }
    if let Some(warning) = &warning {
        tracing::warn!(
            kind = warning.kind,
            holder = warning.holder.as_deref().unwrap_or("unknown"),
            "another program stands in for UwULock's passkeys"
        );
    }
    let _ = app.emit("passkey-provider-warning", warning);
}

/// Starts what the settings switched on.
pub(crate) fn start(app: &AppHandle) {
    #[cfg(target_os = "android")]
    android::init(app);
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    apple::start(app);
    let settings = app.state::<Provider>().settings();
    #[cfg(target_os = "linux")]
    if settings.security_key {
        if let Err(error) = linux::start(app) {
            tracing::warn!(%error, "the virtual security key didn't start");
        }
    }
    #[cfg(windows)]
    if settings.windows_plugin {
        if let Err(error) = windows::start(app) {
            tracing::warn!(%error, "the Windows passkey plugin didn't start");
        }
    }
    let _ = (app, settings);
}

// ── Asking the person ──────────────────────────────────────

fn show_window(app: &AppHandle) {
    #[cfg(desktop)]
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    #[cfg(mobile)]
    let _ = app;
}

/// Puts a request in front of the person and waits for the answer. `tick`
/// runs every 100 ms meanwhile (CTAPHID's keepalives); `cancelled` stops the
/// wait (the browser gave up).
pub(crate) fn ask(
    app: &AppHandle,
    ask: Ask,
    client: &Client,
    cancelled: &AtomicBool,
    tick: &mut dyn FnMut(),
) -> std::result::Result<Decision, Status> {
    let provider = app.state::<Provider>();
    let (reply, answers) = mpsc::channel();
    let id = provider.next_id.fetch_add(1, Ordering::Relaxed);
    {
        let mut pending = provider.pending.lock();
        if pending.is_some() {
            // One question at a time.
            return Err(status::NOT_ALLOWED);
        }
        *pending = Some(Pending {
            id,
            ask,
            client: client.clone(),
            reply,
        });
    }
    show_window(app);
    let _ = app.emit("passkey-request", id);
    let started = Instant::now();
    let outcome = loop {
        match answers.recv_timeout(Duration::from_millis(100)) {
            Ok(answer) => break answer,
            Err(mpsc::RecvTimeoutError::Disconnected) => break Err(status::OPERATION_DENIED),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if cancelled.load(Ordering::Relaxed) {
            break Err(status::KEEPALIVE_CANCEL);
        }
        if started.elapsed() > ASK_FOR {
            break Err(status::USER_ACTION_TIMEOUT);
        }
        tick();
    };
    let mut pending = provider.pending.lock();
    if pending.as_ref().is_some_and(|p| p.id == id) {
        *pending = None;
    }
    drop(pending);
    let _ = app.emit("passkey-request", 0);
    outcome
}

/// A login the new passkey can go into.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginChoice {
    item_id: String,
    name: String,
    user_name: Option<String>,
    /// Bitwarden keeps one passkey per login: this one is replaced.
    has_passkey: bool,
}

/// A passkey that can sign in.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyChoice {
    item_id: String,
    item_name: String,
    credential_id: String,
    user_name: Option<String>,
    user_display_name: Option<String>,
}

/// The open request, as the dialog shows it. Worked out anew each time it is
/// asked for: the vault may have been unlocked meanwhile.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestView {
    id: u64,
    /// `create`, `get` or `select`.
    kind: &'static str,
    /// Who asks; empty when UwULock can't tell.
    client: String,
    /// `client` is a browser UwULock knows, or Windows itself.
    trusted: bool,
    rp_id: Option<String>,
    rp_name: Option<String>,
    user_name: Option<String>,
    user_display_name: Option<String>,
    /// Unlock first.
    locked: bool,
    /// The site wants the master password typed again.
    verify: bool,
    /// Create: the vault has a passkey the site already knows.
    excluded: bool,
    logins: Vec<LoginChoice>,
    passkeys: Vec<PasskeyChoice>,
}

#[tauri::command]
pub(crate) fn passkey_request(
    provider: State<'_, Provider>,
    vault: State<'_, VaultState>,
) -> Option<RequestView> {
    let pending = provider.pending.lock();
    let pending = pending.as_ref()?;
    let locked = vault.active_account().is_err() || !is_open(&vault);
    let mut view = RequestView {
        id: pending.id,
        kind: "select",
        client: pending.client.name.clone(),
        trusted: pending.client.trusted,
        rp_id: None,
        rp_name: None,
        user_name: None,
        user_display_name: None,
        locked,
        verify: false,
        excluded: false,
        logins: Vec::new(),
        passkeys: Vec::new(),
    };
    match &pending.ask {
        Ask::Select => {}
        Ask::Create(request) => {
            view.kind = "create";
            view.rp_id = Some(request.rp.id.clone());
            view.rp_name = request.rp.name.clone();
            view.user_name = request.user.name.clone();
            view.user_display_name = request.user.display_name.clone();
            view.verify = request.user_verification;
            if !locked {
                view.excluded = matching(&vault, &request.rp.id, &request.exclude_list)
                    .is_ok_and(|found| !found.is_empty() && !request.exclude_list.is_empty());
                view.logins = logins_for_site(&vault, &request.rp.id);
            }
        }
        Ask::Get(request) => {
            view.kind = "get";
            view.rp_id = Some(request.rp_id.clone());
            view.verify = request.user_verification;
            if !locked {
                view.passkeys = matching(&vault, &request.rp_id, &request.allow_list)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|found| PasskeyChoice {
                        credential_id: found
                            .passkey
                            .credential_id_bytes()
                            .map(|id| b64(&id))
                            .unwrap_or_default(),
                        item_id: found.item_id,
                        item_name: found.item_name,
                        user_name: found.passkey.user_name.clone(),
                        user_display_name: found.passkey.user_display_name.clone(),
                    })
                    .collect();
            }
        }
    }
    Some(view)
}

/// The person's answer. `allow` false declines. A request that wants
/// verification takes the master password, checked here.
#[tauri::command]
pub(crate) async fn passkey_answer(
    provider: State<'_, Provider>,
    vault: State<'_, VaultState>,
    id: u64,
    allow: bool,
    item_id: Option<String>,
    credential_id: Option<String>,
    password: Option<String>,
) -> Result<()> {
    let verify = {
        let pending = provider.pending.lock();
        match pending.as_ref() {
            Some(p) if p.id == id => match &p.ask {
                Ask::Create(r) => r.user_verification,
                Ask::Get(r) => r.user_verification,
                Ask::Select => false,
            },
            _ => return Err(Failure::new("not-found", "This request is over.")),
        }
    };
    let mut verified = false;
    if allow && verify {
        let password = Zeroizing::new(password.unwrap_or_default());
        check_master_password(&vault, password).await?;
        verified = true;
    }
    let credential_id = credential_id
        .map(|id| from_b64(&id).map_err(|e| Failure::new("invalid", e)))
        .transpose()?;
    let pending = provider.pending.lock();
    if let Some(pending) = pending.as_ref().filter(|p| p.id == id) {
        let answer = if allow {
            Ok(Decision {
                item_id: item_id.filter(|id| !id.is_empty()),
                credential_id,
                verified,
            })
        } else {
            Err(status::OPERATION_DENIED)
        };
        let _ = pending.reply.send(answer);
    }
    Ok(())
}

pub(crate) async fn check_master_password(
    vault: &VaultState,
    password: Zeroizing<String>,
) -> Result<()> {
    let (_, account) = vault.active_account()?;
    let master_key =
        crate::vault::derive_off_thread(password, account.email.clone(), account.kdf).await?;
    let protected: EncString = account.protected_user_key.parse()?;
    uwulock_bitwarden::crypto::decrypt_user_key(&master_key, &protected)
        .map_err(|_| Failure::new("wrong-password", "The master password is wrong."))?;
    Ok(())
}

/// What the settings show: what is on, and whether this system can.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    settings: Settings,
    /// `linux`, `windows`, `android`, `apple` or `none`.
    platform: &'static str,
    /// Running right now.
    active: bool,
    /// Why it isn't, when it should be.
    problem: Option<String>,
    /// Another program stands in for UwULock ([`Warning`]).
    warning: Option<Warning>,
}

#[tauri::command]
pub(crate) fn passkey_provider_status(app: AppHandle) -> ProviderStatus {
    status_of(&app)
}

fn status_of(app: &AppHandle) -> ProviderStatus {
    let provider = app.state::<Provider>();
    let settings = provider.settings();
    #[cfg(target_os = "linux")]
    let (platform, active, problem) = ("linux", provider.device.lock().is_some(), linux::problem());
    #[cfg(windows)]
    let (platform, active, problem) = (
        "windows",
        provider.plugin.lock().is_some(),
        windows::problem(),
    );
    #[cfg(target_os = "android")]
    let (platform, active, problem) = ("android", true, None::<String>);
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    let (platform, active, problem) = ("apple", apple::active(app), apple::problem(app));
    #[cfg(not(any(
        target_os = "linux",
        windows,
        target_os = "android",
        target_os = "ios",
        target_os = "macos"
    )))]
    let (platform, active, problem) = ("none", false, None::<String>);
    let warning = provider.warning.lock().clone();
    ProviderStatus {
        settings,
        platform,
        active,
        problem: if active { None } else { problem },
        warning,
    }
}

#[tauri::command]
pub(crate) fn set_passkey_provider(app: AppHandle, settings: Settings) -> Result<ProviderStatus> {
    let provider = app.state::<Provider>();
    *provider.settings.lock() = settings.clone();
    let text = serde_json::to_vec_pretty(&settings).expect("settings serialise");
    std::fs::write(&provider.path, text)
        .map_err(|e| Failure::new("io", format!("couldn't save the setting: {e}")))?;
    #[cfg(target_os = "linux")]
    {
        let result = if settings.security_key {
            linux::start(&app)
        } else {
            linux::stop(&app);
            Ok(())
        };
        if let Err(error) = result {
            tracing::warn!(%error, "the virtual security key didn't start");
        }
    }
    #[cfg(windows)]
    {
        let result = if settings.windows_plugin {
            windows::start(&app)
        } else {
            windows::stop(&app);
            Ok(())
        };
        if let Err(error) = result {
            tracing::warn!(%error, "the Windows passkey plugin didn't start");
        }
    }
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    if settings.apple_extension {
        apple::refresh_soon(&app);
    } else {
        apple::clear(&app);
    }
    Ok(status_of(&app))
}

// ── The vault ──────────────────────────────────────────────

fn is_open(vault: &VaultState) -> bool {
    vault
        .active_account()
        .is_ok_and(|(id, _)| vault.unlocked.read().contains_key(&id))
}

/// A passkey found in the vault.
pub(crate) struct Found {
    pub item_id: String,
    pub item_name: String,
    pub passkey: Passkey,
}

/// The passkeys of a login that open.
fn passkeys_of(vault: &VaultState, account_id: &str, item: &Item) -> Vec<(usize, Passkey)> {
    let Some(raw) = item.login.as_ref().and_then(|l| l.passkeys.as_ref()) else {
        return Vec::new();
    };
    let guard = vault.unlocked.read();
    let Some(unlocked) = guard.get(account_id) else {
        return Vec::new();
    };
    let Ok(outer) = unlocked
        .vault
        .outer_key(item.organization_id.as_deref(), &unlocked.user_key)
    else {
        return Vec::new();
    };
    let key = item.key.as_ref().unwrap_or(outer);
    raw.iter()
        .enumerate()
        .filter_map(|(index, raw)| Some((index, Passkey::open(raw, key).ok()?)))
        .collect()
}

/// The passkeys for `rp_id`, those in `allow` only when it lists any. Not in
/// the trash.
pub(crate) fn matching(vault: &VaultState, rp_id: &str, allow: &[Vec<u8>]) -> Result<Vec<Found>> {
    let (account_id, _) = vault.active_account()?;
    let items: Vec<Item> = {
        let guard = vault.unlocked.read();
        let unlocked = guard.get(&account_id).ok_or_else(Failure::locked)?;
        unlocked
            .vault
            .items
            .iter()
            .filter(|item| {
                item.kind == ItemKind::Login
                    && !item.deleted
                    && item.login.as_ref().is_some_and(|l| l.passkey_count() > 0)
            })
            .cloned()
            .collect()
    };
    let rp_id = rp_id.trim();
    Ok(items
        .iter()
        .flat_map(|item| {
            passkeys_of(vault, &account_id, item)
                .into_iter()
                .filter(|(_, passkey)| {
                    passkey.rp_id.eq_ignore_ascii_case(rp_id)
                        && (allow.is_empty()
                            || passkey
                                .credential_id_bytes()
                                .is_ok_and(|id| allow.contains(&id)))
                })
                .map(|(_, passkey)| Found {
                    item_id: item.id.clone(),
                    item_name: item.name.to_string(),
                    passkey,
                })
                .collect::<Vec<_>>()
        })
        .collect())
}

/// Whether a login's addresses belong to the site.
fn for_site(item: &Item, rp_id: &str) -> bool {
    let rp_id = rp_id.trim().trim_start_matches("www.").to_ascii_lowercase();
    item.login.as_ref().is_some_and(|login| {
        login.uris.iter().any(|uri| {
            host_of(&uri.uri).is_some_and(|host| {
                let host = host.to_ascii_lowercase();
                host == rp_id || host.ends_with(&format!(".{rp_id}"))
            })
        })
    })
}

fn logins_for_site(vault: &VaultState, rp_id: &str) -> Vec<LoginChoice> {
    let Ok((account_id, _)) = vault.active_account() else {
        return Vec::new();
    };
    let guard = vault.unlocked.read();
    let Some(unlocked) = guard.get(&account_id) else {
        return Vec::new();
    };
    unlocked
        .vault
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Login && !item.deleted && for_site(item, rp_id))
        .map(|item| LoginChoice {
            item_id: item.id.clone(),
            name: item.name.to_string(),
            user_name: item
                .login
                .as_ref()
                .and_then(|l| l.username.as_ref())
                .map(|u| u.to_string()),
            has_passkey: item.login.as_ref().is_some_and(|l| l.passkey_count() > 0),
        })
        .collect()
}

fn flags(user_present: bool, verified: bool) -> u8 {
    BE | BS | if user_present { UP } else { 0 } | if verified { UV } else { 0 }
}

/// Saves an item: changed when `id` is there, new otherwise. Returns its id.
async fn save(app: &AppHandle, item: &Item, id: Option<&str>) -> Result<String> {
    let vault = app.state::<VaultState>();
    let (account_id, account) = vault.active_account()?;
    let request = sealed(&vault, &account_id, item)?;
    let client = vault.client(account.server.clone())?;
    let access = access_token(&vault, &account_id).await?;
    let answer = match id {
        Some(id) => client.update_cipher(&access, id, request).await?,
        None => {
            client
                .create_cipher(&access, request, &item.collection_ids)
                .await?
        }
    };
    let saved = entry_id(&answer)
        .map(str::to_string)
        .or_else(|| id.map(str::to_string))
        .unwrap_or_default();
    patch_cache(&vault, &account_id, Patch::Cipher(answer))?;
    let _ = app.emit("vault-changed", ());
    emit_status(app);
    Ok(saved)
}

fn new_login(rp_id: &str, rp_name: Option<&str>, user_name: Option<&str>) -> Item {
    let mut item = Item::new(ItemKind::Login);
    let name = rp_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(rp_id);
    item.name = Zeroizing::new(name.to_string());
    if let Some(login) = item.login.as_mut() {
        login.username = user_name
            .filter(|name| !name.is_empty())
            .map(|name| Zeroizing::new(name.to_string()));
        login.uris = vec![LoginUri {
            uri: Zeroizing::new(format!("https://{rp_id}")),
            match_kind: None,
            checksum: None,
        }];
    }
    item
}

/// A passkey made, and saved.
pub(crate) struct Made {
    /// Android answers with its id and public key.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub passkey: Passkey,
    pub auth_data: Vec<u8>,
}

/// Puts a passkey into the login `item_id` (whose passkey it replaces, as
/// Bitwarden keeps one a login) or a new login for its site, and saves it.
/// Returns the login's id.
pub(crate) async fn save_passkey(
    app: &AppHandle,
    passkey: Passkey,
    item_id: Option<&str>,
) -> Result<String> {
    let vault = app.state::<VaultState>();
    let (account_id, _) = vault.active_account()?;
    let mut item = match item_id {
        Some(id) => {
            let item = prepare(&vault, &account_id, id)?;
            if item.kind != ItemKind::Login || item.deleted {
                return Err(Failure::new("invalid", "Passkeys go into logins."));
            }
            item
        }
        None => new_login(
            &passkey.rp_id,
            passkey.rp_name.as_deref(),
            passkey.user_name.as_deref(),
        ),
    };
    {
        let guard = vault.unlocked.read();
        let unlocked = guard.get(&account_id).ok_or_else(Failure::locked)?;
        let outer = unlocked
            .vault
            .outer_key(item.organization_id.as_deref(), &unlocked.user_key)?;
        let sealed = passkey.seal(item.key.as_ref().unwrap_or(outer));
        if let Some(login) = item.login.as_mut() {
            login.passkeys = Some(vec![sealed]);
        }
    }
    let saved = save(app, &item, item_id).await?;
    tracing::info!(new_login = item_id.is_none(), "passkey saved");
    Ok(saved)
}

/// Makes a passkey for the site into a login (see [`save_passkey`]), and
/// saves it before anything goes back to the site: a passkey the site knows
/// but the vault lost would lock somebody out.
pub(crate) async fn create(
    app: &AppHandle,
    request: &MakeCredential,
    item_id: Option<&str>,
    verified: bool,
) -> Result<Made> {
    let passkey = Passkey::generate(
        &request.rp.id,
        request.rp.name.as_deref(),
        Some(&request.user.id),
        request.user.name.as_deref(),
        request.user.display_name.as_deref(),
        true,
        &iso_now(),
    )?;
    save_passkey(app, passkey.clone(), item_id).await?;
    let auth_data = passkey.authenticator_data(flags(true, verified), true)?;
    Ok(Made { passkey, auth_data })
}

/// A signature from one passkey.
pub(crate) struct Signed {
    pub passkey: Passkey,
    pub auth_data: Vec<u8>,
    pub signature: Vec<u8>,
}

/// One signature at a time: a passkey that counts is read, counted up and
/// saved before the next one reads it, so no two signatures carry the same
/// counter.
static SIGNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Signs with the passkey `credential_id` of the login `item_id`. A passkey
/// that counts (one from elsewhere; UwULock's stay at 0) counts up and is
/// saved again — no signature when that save fails, or the next one would
/// carry the same counter.
pub(crate) async fn sign(
    app: &AppHandle,
    rp_id: &str,
    item_id: &str,
    credential_id: &[u8],
    client_data_hash: &[u8],
    user_present: bool,
    verified: bool,
) -> Result<Signed> {
    let _one_at_a_time = SIGNING.lock().await;
    let vault = app.state::<VaultState>();
    let (account_id, _) = vault.active_account()?;
    let mut item = prepare(&vault, &account_id, item_id)?;
    let (index, mut passkey) = passkeys_of(&vault, &account_id, &item)
        .into_iter()
        .find(|(_, passkey)| {
            passkey.rp_id.eq_ignore_ascii_case(rp_id.trim())
                && passkey
                    .credential_id_bytes()
                    .is_ok_and(|id| id == credential_id)
        })
        .ok_or_else(|| Failure::new("not-found", "This login has no such passkey."))?;
    if passkey.counter > 0 && user_present {
        passkey.counter = passkey.counter.saturating_add(1);
        let resealed = {
            let guard = vault.unlocked.read();
            let unlocked = guard.get(&account_id).ok_or_else(Failure::locked)?;
            let outer = unlocked
                .vault
                .outer_key(item.organization_id.as_deref(), &unlocked.user_key)?;
            let key = item.key.as_ref().unwrap_or(outer);
            item.login
                .as_ref()
                .and_then(|l| l.passkeys.as_ref())
                .and_then(|list| list.get(index))
                .map(|stored| passkey.reseal(stored, key))
        };
        if let (Some(resealed), Some(list)) = (
            resealed,
            item.login.as_mut().and_then(|l| l.passkeys.as_mut()),
        ) {
            list[index] = resealed;
            if let Err(error) = save(app, &item, Some(item_id)).await {
                tracing::warn!(
                    error = error.message(),
                    "couldn't save the passkey's counter, so no signature"
                );
                return Err(error);
            }
        } else {
            return Err(Failure::new("invalid", "couldn't count the passkey up"));
        }
    }
    let auth_data = passkey.authenticator_data(flags(user_present, verified), false)?;
    let signature = passkey.sign(&auth_data, client_data_hash)?;
    Ok(Signed {
        passkey,
        auth_data,
        signature,
    })
}

// ── CTAP2 for the security key and Windows ─────────────────

/// The authenticator's backend on the desktop: asks in UwULock's window.
pub(crate) struct DesktopBackend<'a> {
    pub app: AppHandle,
    pub client: Client,
    pub cancelled: &'a AtomicBool,
    pub tick: &'a mut dyn FnMut(),
}

fn ctap_status(failure: &Failure) -> Status {
    match failure.kind() {
        "not-found" => status::NO_CREDENTIALS,
        "locked" | "logged-out" | "reprompt" => status::OPERATION_DENIED,
        _ => status::OTHER,
    }
}

impl DesktopBackend<'_> {
    fn ask(&mut self, request: Ask) -> std::result::Result<Decision, Status> {
        ask(
            &self.app,
            request,
            &self.client,
            self.cancelled,
            &mut *self.tick,
        )
    }
}

impl ctap2::Backend for DesktopBackend<'_> {
    fn make_credential(
        &mut self,
        request: &MakeCredential,
    ) -> std::result::Result<Vec<u8>, Status> {
        let decision = self.ask(Ask::Create(request.clone()))?;
        let app = self.app.clone();
        let vault = app.state::<VaultState>();
        if !request.exclude_list.is_empty()
            && matching(&vault, &request.rp.id, &request.exclude_list)
                .is_ok_and(|found| !found.is_empty())
        {
            return Err(status::CREDENTIAL_EXCLUDED);
        }
        if request.user_verification && !decision.verified {
            return Err(status::OPERATION_DENIED);
        }
        let made = tauri::async_runtime::block_on(create(
            &self.app,
            request,
            decision.item_id.as_deref(),
            decision.verified,
        ))
        .map_err(|failure| {
            tracing::warn!(error = failure.message(), "couldn't make the passkey");
            ctap_status(&failure)
        })?;
        Ok(made.auth_data)
    }

    fn get_assertion(&mut self, request: &GetAssertion) -> std::result::Result<Assertion, Status> {
        let app = self.app.clone();
        let vault = app.state::<VaultState>();
        // A browser's silent check before the real request: is one of the
        // passkeys the site named here? The authenticator already refused
        // it without an allow list and drops the account from the answer;
        // here it is throttled and gets no signature from the passkey.
        if !request.user_presence {
            if !PROBES.lock().allow(Instant::now()) {
                return Err(status::NOT_ALLOWED);
            }
            let found = matching(&vault, &request.rp_id, &request.allow_list)
                .map_err(|_| status::NO_CREDENTIALS)?;
            let found = found.into_iter().next().ok_or(status::NO_CREDENTIALS)?;
            return probe(request, &found.passkey);
        }
        // None of the passkeys the site named is in an open vault: say so at
        // once (only a caller that knows the credential ids learns this).
        // Without an allow list the person is asked even then, so nobody
        // learns silently which sites have passkeys.
        if !request.allow_list.is_empty()
            && is_open(&vault)
            && matching(&vault, &request.rp_id, &request.allow_list)
                .is_ok_and(|found| found.is_empty())
        {
            return Err(status::NO_CREDENTIALS);
        }
        let decision = self.ask(Ask::Get(request.clone()))?;
        if request.user_verification && !decision.verified {
            return Err(status::OPERATION_DENIED);
        }
        let (Some(item_id), Some(credential_id)) = (&decision.item_id, &decision.credential_id)
        else {
            return Err(status::NO_CREDENTIALS);
        };
        let found = matching(&vault, &request.rp_id, &request.allow_list)
            .map_err(|failure| ctap_status(&failure))?
            .into_iter()
            .find(|found| {
                &found.item_id == item_id
                    && found
                        .passkey
                        .credential_id_bytes()
                        .is_ok_and(|id| &id == credential_id)
            })
            .ok_or(status::NO_CREDENTIALS)?;
        self.signed(request, item_id, &found.passkey, true, decision.verified)
    }

    fn select(&mut self) -> std::result::Result<(), Status> {
        self.ask(Ask::Select).map(|_| ())
    }
}

/// Silent checks answered: 20 at once, then one every 3 s.
static PROBES: Mutex<ctap2::Throttle> =
    parking_lot::const_mutex(ctap2::Throttle::new(20, Duration::from_secs(3)));

/// The answer to a silent check: the passkey's id and authenticator data
/// without UP, signed with a throwaway key ([`ctap2::probe_signature`]).
fn probe(request: &GetAssertion, passkey: &Passkey) -> std::result::Result<Assertion, Status> {
    let credential_id = passkey.credential_id_bytes().map_err(|_| status::OTHER)?;
    let auth_data = passkey
        .authenticator_data(flags(false, false), false)
        .map_err(|_| status::OTHER)?;
    let signature = ctap2::probe_signature(&auth_data, &request.client_data_hash);
    Ok(Assertion {
        credential_id,
        auth_data,
        signature,
        user: None,
    })
}

impl DesktopBackend<'_> {
    fn signed(
        &mut self,
        request: &GetAssertion,
        item_id: &str,
        passkey: &Passkey,
        user_present: bool,
        verified: bool,
    ) -> std::result::Result<Assertion, Status> {
        let credential_id = passkey.credential_id_bytes().map_err(|_| status::OTHER)?;
        let signed = tauri::async_runtime::block_on(sign(
            &self.app,
            &request.rp_id,
            item_id,
            &credential_id,
            &request.client_data_hash,
            user_present,
            verified,
        ))
        .map_err(|failure| ctap_status(&failure))?;
        let user = request.allow_list.is_empty().then(|| ctap2::User {
            id: signed
                .passkey
                .user_handle_bytes()
                .ok()
                .flatten()
                .unwrap_or_default(),
            name: signed.passkey.user_name.clone(),
            display_name: signed.passkey.user_display_name.clone(),
        });
        Ok(Assertion {
            credential_id,
            auth_data: signed.auth_data,
            signature: signed.signature,
            user,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_say_what_happened() {
        assert_eq!(flags(true, true), UP | UV | BE | BS);
        assert_eq!(flags(true, false), UP | BE | BS);
        assert_eq!(flags(false, false), BE | BS);
    }

    #[test]
    fn a_new_login_for_a_passkey() {
        let item = new_login("example.com", Some(" "), Some("nyu"));
        assert_eq!(item.name.as_str(), "example.com");
        let login = item.login.as_ref().unwrap();
        assert_eq!(login.username.as_deref().map(String::as_str), Some("nyu"));
        assert_eq!(login.uris[0].uri.as_str(), "https://example.com");
        assert!(for_site(&item, "example.com"));
        assert!(!for_site(&item, "other.example.com"));
        let named = new_login("login.example.com", Some("Example"), None);
        assert_eq!(named.name.as_str(), "Example");
        assert!(for_site(&named, "example.com"));
    }

    #[test]
    fn a_warning_is_news_once() {
        let dir = std::env::temp_dir().join(format!("uwulock-warning-{}", std::process::id()));
        let provider = Provider::new(&dir);
        let held = Warning {
            kind: "held",
            holder: Some("/home/nyu/.local/bin/thing (pid 42)".into()),
        };
        assert!(provider.set_warning(Some(held.clone())));
        // The keeper trying again finds the same: no second note.
        assert!(!provider.set_warning(Some(held)));
        assert!(provider.set_warning(None));
        assert!(!provider.set_warning(None));
    }

    #[test]
    fn holders_are_shown_escaped_and_cut() {
        let forged = format!("/tmp/x\n<0>fine\u{202e}{}", "a".repeat(1000));
        let text = shown(&forged);
        assert!(text.starts_with("/tmp/x\\n<0>fine\\u{202e}"), "{text}");
        assert!(text.len() <= HOLDER_SHOWN && text.ends_with('…'));
        assert!(!text.chars().any(char::is_control));
    }

    #[test]
    fn settings_default_off() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(!settings.security_key && !settings.windows_plugin);
        let dir = std::env::temp_dir().join(format!("uwulock-passkeys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("passkeys.json"), r#"{"securityKey": true}"#).unwrap();
        assert!(Provider::new(&dir).settings().security_key);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
