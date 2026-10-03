//! iOS 17+ and macOS 14+: the AutoFill extension's side of the vault.
//!
//! The extension (`apps/desktop/src-tauri/apple/PasskeyProvider`, Swift) is
//! its own process, started by the system while UwULock may not run at all.
//! So while the vault is open, the app keeps a **sealed list** of the
//! account's passkeys in the App Group folder they share, and takes in the
//! passkeys the extension made (its **outbox**) as new logins. Both are
//! sealed with the account's provider key (uwulock_authenticator::apple): 32
//! random bytes, kept here sealed under the account's user key
//! (`passkeys-apple-<account>.key`), and for the extension in the shared
//! Keychain behind Face ID / Touch ID / the device passcode, this device
//! only.
//!
//! There is one list and one Keychain slot: those of the account last open
//! on screen (`passkeys-apple.account` says which). Each sealed file names
//! its account, so switching accounts never loses anything: an outbox entry
//! of another account waits until that account is open here again, and one
//! that doesn't open is set aside (`outbox/unreadable/`), never deleted.
//! Logging out of that account, or switching the setting off, takes the
//! list, the Keychain item and the system's entries away.
//!
//! None of it works before the app and the extension are signed with an
//! Apple developer team (App Group, Keychain group, the AutoFill
//! entitlement): until then the status says so. docs/passkeys.md has the
//! steps.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Listener, Manager};
use uwulock_authenticator::apple::{self as sealed, Snapshot};
use uwulock_bitwarden::EncString;
use uwulock_bitwarden::SymmetricKey;
use zeroize::Zeroizing;

use super::{passkeys_of, Provider};
use crate::vault::{Failure, Result, VaultState};

static REFRESHING: AtomicBool = AtomicBool::new(false);

/// Held while the list is handed to the extension and while an account's
/// list is taken away: a refresh that began before a logout can't put the
/// list back after it.
static HANDING_OVER: parking_lot::Mutex<()> = parking_lot::const_mutex(());

/// How many replaced provider keys of an account are kept.
const OLD_KEYS: usize = 5;

/// How many taken-in outbox entries are remembered, against one put back.
const REMEMBERED: usize = 1000;

/// A passkey as the system's list shows it (ASPasskeyCredentialIdentity):
/// no secret, only what the QuickType bar and the sheet show.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub rp_id: String,
    pub user_name: String,
    pub credential_id: String,
    pub user_handle: String,
    pub record_identifier: Option<String>,
}

/// What the app remembers of an account's list (`passkeys-apple-<account>.json`).
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    /// The last list's generation, or a later one the extension wrote.
    #[serde(default)]
    generation: u64,
    /// Credential ids of outbox entries taken in, newest last.
    #[serde(default)]
    imported: Vec<String>,
}

pub(crate) fn start(app: &AppHandle) {
    for event in ["vault-changed", "vault-status"] {
        let handle = app.clone();
        app.listen(event, move |_| refresh_soon(&handle));
    }
}

fn enabled(app: &AppHandle) -> bool {
    app.state::<Provider>().settings().apple_extension
}

pub(crate) fn active(app: &AppHandle) -> bool {
    enabled(app) && native::problem(app).is_none()
}

pub(crate) fn problem(app: &AppHandle) -> Option<String> {
    native::problem(app)
}

/// Switched off: the list, the Keychain item, the system's entries and the
/// account's provider key go.
pub(crate) fn clear(app: &AppHandle) {
    let _handing_over = HANDING_OVER.lock();
    if let Err(error) = native::clear(app) {
        tracing::warn!(%error, "couldn't remove the extension's passkey list");
    }
    if let Some(account) = recorded(app) {
        let _ = std::fs::remove_file(record_path(app));
        forget_files(app, &account);
    }
}

/// An account leaves this device: when the extension's list is that
/// account's, it goes too (see [`clear`]), and so does the account's
/// provider key.
pub(crate) fn forget(app: &AppHandle, account_id: &str) {
    if !sealed::valid_account(account_id) {
        return;
    }
    let _handing_over = HANDING_OVER.lock();
    if recorded(app).as_deref() == Some(account_id) {
        if let Err(error) = native::clear(app) {
            tracing::warn!(%error, "couldn't remove the extension's passkey list");
        }
        let _ = std::fs::remove_file(record_path(app));
    }
    forget_files(app, account_id);
}

/// The account's provider key and state. Kept while the outbox still holds
/// passkeys of the account: the key is sealed under its user key, so it is
/// of use only after logging in to it again — which then takes them in.
fn forget_files(app: &AppHandle, account_id: &str) {
    let pending = native::outbox(app)
        .unwrap_or_default()
        .iter()
        .any(|(_, bytes)| sealed::account_of(bytes) == Ok(account_id));
    if pending {
        tracing::info!(
            "kept the account's provider key: the extension's outbox has passkeys of it"
        );
        return;
    }
    let _ = std::fs::remove_file(key_path(app, account_id));
    let _ = std::fs::remove_file(state_path(app, account_id));
    for old in old_key_paths(app, account_id) {
        let _ = std::fs::remove_file(old);
    }
}

/// Provider keys of the account that were replaced
/// (`passkeys-apple-<account>.key.old-<ms>`), oldest first.
fn old_key_paths(app: &AppHandle, account_id: &str) -> Vec<PathBuf> {
    let prefix = format!("passkeys-apple-{account_id}.key.old-");
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir(app))
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.starts_with(&prefix))
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// A provider key file opened with the user key.
fn open_key_file(path: &Path, user_key: &SymmetricKey) -> Option<Zeroizing<[u8; 32]>> {
    let text = std::fs::read_to_string(path).ok()?;
    let opened = Zeroizing::new(
        text.trim()
            .parse::<EncString>()
            .and_then(|enc| enc.decrypt(user_key))
            .ok()?,
    );
    let mut key = Zeroizing::new([0u8; 32]);
    (opened.len() == 32).then(|| {
        key.copy_from_slice(&opened);
        key
    })
}

/// The replaced provider keys that still open: outbox entries sealed with
/// one of them are taken in all the same.
fn old_keys(
    app: &AppHandle,
    account_id: &str,
    user_key: &SymmetricKey,
) -> Vec<Zeroizing<[u8; 32]>> {
    old_key_paths(app, account_id)
        .iter()
        .filter_map(|path| open_key_file(path, user_key))
        .collect()
}

pub(crate) fn refresh_soon(app: &AppHandle) {
    if !enabled(app) || REFRESHING.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = refresh(&app).await {
            tracing::debug!(
                error = error.message(),
                "the extension's passkey list wasn't refreshed"
            );
        }
        REFRESHING.store(false, Ordering::Release);
    });
}

fn dir(app: &AppHandle) -> PathBuf {
    app.state::<VaultState>().storage().dir().to_path_buf()
}

fn key_path(app: &AppHandle, account_id: &str) -> PathBuf {
    dir(app).join(format!("passkeys-apple-{account_id}.key"))
}

fn state_path(app: &AppHandle, account_id: &str) -> PathBuf {
    dir(app).join(format!("passkeys-apple-{account_id}.json"))
}

fn record_path(app: &AppHandle) -> PathBuf {
    dir(app).join("passkeys-apple.account")
}

/// The account whose list the extension has.
fn recorded(app: &AppHandle) -> Option<String> {
    std::fs::read_to_string(record_path(app))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|id| sealed::valid_account(id))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".new");
    let partial = PathBuf::from(partial);
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)
}

fn load_state(app: &AppHandle, account_id: &str) -> State {
    std::fs::read(state_path(app, account_id))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// The provider key of an open account: the one kept, or a new one. A key
/// that no longer opens (the user key changed) is replaced; what the
/// extension sealed with it is set aside, not lost.
fn provider_key(
    app: &AppHandle,
    account_id: &str,
    user_key: &SymmetricKey,
) -> Result<Zeroizing<[u8; 32]>> {
    let path = key_path(app, account_id);
    if path.exists() {
        if let Some(key) = open_key_file(&path, user_key) {
            return Ok(key);
        }
        // Kept, not overwritten: should it open again (the user key back as
        // it was), what the extension sealed with it is taken in then.
        tracing::warn!(
            "the account's provider key doesn't open any more: a new one, the old one kept"
        );
        let mut old = path.as_os_str().to_owned();
        old.push(format!(".old-{:013}", now_ms()));
        std::fs::rename(&path, PathBuf::from(old))
            .map_err(|e| Failure::new("io", e.to_string()))?;
        let olds = old_key_paths(app, account_id);
        for gone in &olds[..olds.len().saturating_sub(OLD_KEYS)] {
            let _ = std::fs::remove_file(gone);
        }
    }
    let key = sealed::new_key();
    write_atomic(
        &path,
        EncString::encrypt(key.as_ref(), user_key)
            .to_string()
            .as_bytes(),
    )
    .map_err(|e| Failure::new("io", e.to_string()))?;
    Ok(key)
}

/// Whether the vault has this passkey already.
fn in_vault(vault: &VaultState, passkey: &uwulock_core::passkey::Passkey) -> bool {
    passkey.credential_id_bytes().is_ok_and(|id| {
        super::matching(vault, &passkey.rp_id, &[id]).is_ok_and(|found| !found.is_empty())
    })
}

async fn refresh(app: &AppHandle) -> Result<()> {
    if native::problem(app).is_some() {
        return Ok(());
    }
    let vault = app.state::<VaultState>();
    let (account_id, _) = vault.active_account()?;
    if !sealed::valid_account(&account_id) {
        return Err(Failure::new(
            "invalid",
            "an account id the extension can't take",
        ));
    }
    let user_key = vault.user_key_of(&account_id)?;
    let key = provider_key(app, &account_id, &user_key)?;
    let old_keys = old_keys(app, &account_id, &user_key);
    let mut state = load_state(app, &account_id);

    // What the extension made: into the vault, as new logins. Never
    // deleted unless taken in (or already there).
    let mut taken = Vec::new();
    let mut aside = Vec::new();
    for (name, bytes) in native::outbox(app).unwrap_or_default() {
        let set_aside = name.starts_with("unreadable/");
        match sealed::account_of(&bytes) {
            // Another account's: it waits until that one is open here.
            Ok(account) if account != account_id => continue,
            Ok(_) => {}
            Err(error) => {
                if !set_aside {
                    tracing::warn!(%error, "an outbox entry doesn't read: set aside");
                    aside.push(name);
                }
                continue;
            }
        }
        let opened = std::iter::once(&key)
            .chain(&old_keys)
            .map(|key| sealed::open_outbox(key, &bytes))
            .find(|opened| opened.is_ok())
            .unwrap_or_else(|| sealed::open_outbox(&key, &bytes))
            .and_then(|(_, entry)| {
                state.generation = state.generation.max(entry.generation);
                Ok((entry.credential_id.clone(), entry.to_passkey()?))
            });
        let (id, passkey) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                if set_aside {
                    tracing::debug!(%error, "a set-aside outbox entry still doesn't open");
                } else {
                    tracing::warn!(%error, "an outbox entry doesn't open: set aside");
                    aside.push(name);
                }
                continue;
            }
        };
        if state.imported.contains(&id) || in_vault(&vault, &passkey) {
            // Put back, or taken in before: not twice.
            taken.push(name);
            continue;
        }
        match super::save_passkey(app, passkey, None).await {
            Ok(_) => {
                state.imported.push(id);
                taken.push(name);
            }
            Err(error) => {
                tracing::warn!(
                    error = error.message(),
                    "couldn't save a passkey from the extension"
                );
            }
        }
    }
    let excess = state.imported.len().saturating_sub(REMEMBERED);
    state.imported.drain(..excess);
    if !aside.is_empty() {
        let _ = native::set_aside(app, &aside);
    }
    if !taken.is_empty() {
        let _ = native::clear_outbox(app, &taken);
    }

    // The list for the extension.
    let items: Vec<_> = {
        let guard = vault.unlocked.read();
        let unlocked = guard.get(&account_id).ok_or_else(Failure::locked)?;
        unlocked
            .vault
            .items
            .iter()
            // Not in the trash, and no login that asks for the master
            // password again (R7 L-2): the extension only has Face ID or the
            // device passcode.
            .filter(|item| sealed::listed(item))
            .cloned()
            .collect()
    };
    state.generation = sealed::next_generation(state.generation, now_ms());
    let mut snapshot = Snapshot::new(&account_id, state.generation);
    let mut identities = Vec::new();
    for item in &items {
        for (_, passkey) in passkeys_of(&vault, &account_id, item) {
            // The extension can't count up a signature counter in the vault:
            // passkeys that use one stay with the app and the browser extension.
            if !sealed::listed_passkey(&passkey) {
                continue;
            }
            let Ok(entry) = snapshot.push(&key, &passkey, Some(&item.id)) else {
                continue;
            };
            identities.push(Identity {
                rp_id: entry.rp_id.clone(),
                user_name: entry
                    .user_name
                    .clone()
                    .or_else(|| entry.user_display_name.clone())
                    .unwrap_or_else(|| item.name.to_string()),
                credential_id: entry.credential_id.clone(),
                user_handle: entry.user_handle.clone().unwrap_or_default(),
                record_identifier: Some(item.id.clone()),
            });
        }
    }
    let sealed_list = snapshot
        .seal(&key)
        .map_err(|e| Failure::new("invalid", e))?;
    // Only for an account still open: a logout since this refresh began has
    // taken the list away (under the same lock), and it stays away.
    let _handing_over = HANDING_OVER.lock();
    if vault.user_key_of(&account_id).is_err() {
        return Err(Failure::locked());
    }
    // The state first: a list the extension saw is never followed by an
    // older generation.
    let json = serde_json::to_vec(&state).map_err(|e| Failure::new("io", e.to_string()))?;
    write_atomic(&state_path(app, &account_id), &json)
        .map_err(|e| Failure::new("io", e.to_string()))?;
    // Which account's list it is, before it is there: a logout in between
    // still finds it.
    write_atomic(&record_path(app), account_id.as_bytes())
        .map_err(|e| Failure::new("io", e.to_string()))?;
    native::store(app, &sealed_list, &key, &identities)
        .map_err(|e| Failure::new("unsupported", e))?;
    tracing::debug!(
        passkeys = identities.len(),
        "the extension's passkey list is up to date"
    );
    Ok(())
}

/// Whether an outbox name from [`native::outbox`] is one of ours: a file
/// name in the outbox or in its `unreadable/` folder, nothing else.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn outbox_name(name: &str) -> Option<(bool, &str)> {
    let (aside, file) = match name.strip_prefix("unreadable/") {
        Some(file) => (true, file),
        None => (false, name),
    };
    let fine = file.ends_with(".sealed")
        && !file.starts_with('.')
        && !file.contains(['/', '\\'])
        && file != ".sealed";
    fine.then_some((aside, file))
}

#[cfg(target_os = "ios")]
mod native {
    use super::Identity;
    use tauri::AppHandle;
    use uwulock_authenticator::webauthn::{b64, from_b64};
    use zeroize::Zeroizing;

    fn plugin(
    ) -> Result<tauri::State<'static, tauri_plugin_uwulock_mobile::Mobile<tauri::Wry>>, String>
    {
        crate::phone::plugin().ok_or_else(|| "the phone's plugin isn't there".to_string())
    }

    pub(super) fn problem(_app: &AppHandle) -> Option<String> {
        match plugin().and_then(|p| p.passkeys_status().map_err(|e| e.message)) {
            Ok(status) if status.ready => None,
            Ok(status) => Some(status.reason.unwrap_or_else(|| "not ready".into())),
            Err(error) => Some(error),
        }
    }

    /// The key goes along every time: Passkeys.swift (re)writes the
    /// Keychain item when the one there isn't this key (by its label,
    /// `uwulock_authenticator::apple::key_id`) or is gone.
    pub(super) fn store(
        _app: &AppHandle,
        list: &[u8],
        key: &[u8; 32],
        identities: &[Identity],
    ) -> Result<(), String> {
        let identities = serde_json::to_value(identities).map_err(|e| e.to_string())?;
        let key = Zeroizing::new(b64(key));
        plugin()?
            .passkeys_store(&b64(list), Some(key.as_str()), identities)
            .map_err(|e| e.message)
    }

    /// The outbox, and what was set aside as `unreadable/<name>`.
    pub(super) fn outbox(_app: &AppHandle) -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(plugin()?
            .passkeys_outbox()
            .map_err(|e| e.message)?
            .into_iter()
            .filter_map(|(name, sealed)| Some((name, from_b64(&sealed).ok()?)))
            .collect())
    }

    pub(super) fn clear_outbox(_app: &AppHandle, names: &[String]) -> Result<(), String> {
        plugin()?
            .passkeys_clear_outbox(names)
            .map_err(|e| e.message)
    }

    /// Moves outbox files to `outbox/unreadable/`: `passkeysClearOutbox`
    /// moves a name given as `aside:<name>` instead of removing it.
    pub(super) fn set_aside(_app: &AppHandle, names: &[String]) -> Result<(), String> {
        let names: Vec<String> = names.iter().map(|name| format!("aside:{name}")).collect();
        plugin()?
            .passkeys_clear_outbox(&names)
            .map_err(|e| e.message)
    }

    pub(super) fn clear(_app: &AppHandle) -> Result<(), String> {
        plugin()?.passkeys_clear().map_err(|e| e.message)
    }
}

/// macOS: the same, done from Rust. The App Group folder and the Keychain
/// group carry the developer team's id, which a build only knows when it is
/// signed (`UWULOCK_APPLE_TEAM_ID` at build time).
#[cfg(target_os = "macos")]
mod native {
    use super::{outbox_name, Identity};
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::item::{ItemClass, ItemSearchOptions};
    use security_framework::passwords::{
        delete_generic_password_options, set_generic_password_options, AccessControlOptions,
        PasswordOptions,
    };
    use std::path::PathBuf;
    use tauri::AppHandle;

    const TEAM: Option<&str> = option_env!("UWULOCK_APPLE_TEAM_ID");
    const SERVICE: &str = "app.uwulock.passkeys";
    const ACCOUNT: &str = "provider-key";

    fn team() -> Result<&'static str, String> {
        TEAM.filter(|t| !t.is_empty())
            .ok_or_else(|| "This build isn't signed with an Apple developer team: the AutoFill extension stays off (docs/passkeys.md).".to_string())
    }

    fn folder() -> Result<PathBuf, String> {
        let home = std::env::var_os("HOME").ok_or("no home folder")?;
        Ok(PathBuf::from(home)
            .join("Library/Group Containers")
            .join(format!("{}.app.uwulock", team()?))
            .join("Passkeys"))
    }

    fn group() -> Result<String, String> {
        Ok(format!("{}.app.uwulock.passkeys", team()?))
    }

    /// The Keychain item, in the data protection keychain, never synced.
    fn options() -> Result<PasswordOptions, String> {
        let mut options = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
        options.set_access_group(&group()?);
        options.use_protected_keychain();
        options.set_access_synchronized(Some(false));
        Ok(options)
    }

    /// The label names the key (`key_id`), so the app sees whether the item
    /// there is this key without reading it — attributes take no Touch ID.
    fn label(key: &[u8; 32]) -> String {
        format!(
            "UwULock passkeys {}",
            uwulock_authenticator::apple::key_id(key)
        )
    }

    fn key_there(key: &[u8; 32]) -> bool {
        let Ok(group) = group() else {
            return false;
        };
        ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(SERVICE)
            .account(ACCOUNT)
            .access_group(&group)
            .label(&label(key))
            .ignore_legacy_keychains()
            .load_attributes(true)
            .search()
            .is_ok_and(|found| !found.is_empty())
    }

    pub(super) fn problem(_app: &AppHandle) -> Option<String> {
        team().err()
    }

    pub(super) fn store(
        _app: &AppHandle,
        list: &[u8],
        key: &[u8; 32],
        _identities: &[Identity],
    ) -> Result<(), String> {
        let folder = folder()?;
        std::fs::create_dir_all(folder.join("outbox").join("unreadable"))
            .map_err(|e| e.to_string())?;
        if !key_there(key) {
            // Missing, or another account's key, or one from before: as on
            // iOS, only after Touch ID or the login password, this Mac only.
            let _ = delete_generic_password_options(options()?);
            let access = SecAccessControl::create_with_protection(
                Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
                AccessControlOptions::USER_PRESENCE.bits(),
            )
            .map_err(|e| e.to_string())?;
            let mut options = options()?;
            options.set_access_control(access);
            options.set_label(&label(key));
            set_generic_password_options(key, options).map_err(|e| e.to_string())?;
        }
        let path = folder.join("passkeys.sealed");
        let partial = folder.join("passkeys.sealed.new");
        std::fs::write(&partial, list).map_err(|e| e.to_string())?;
        std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
        // The system's list of passkeys (ASCredentialIdentityStore) is only
        // reachable from Swift: on macOS the extension fills it itself.
        Ok(())
    }

    fn read_dir(dir: PathBuf, prefix: &str) -> Vec<(String, Vec<u8>)> {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| {
                let name = format!("{prefix}{}", e.file_name().to_str()?);
                outbox_name(&name)?;
                Some((name, std::fs::read(e.path()).ok()?))
            })
            .collect()
    }

    /// The outbox, and what was set aside as `unreadable/<name>`.
    pub(super) fn outbox(_app: &AppHandle) -> Result<Vec<(String, Vec<u8>)>, String> {
        let dir = folder()?.join("outbox");
        let mut found = read_dir(dir.clone(), "");
        found.extend(read_dir(dir.join("unreadable"), "unreadable/"));
        Ok(found)
    }

    pub(super) fn clear_outbox(_app: &AppHandle, names: &[String]) -> Result<(), String> {
        let dir = folder()?.join("outbox");
        for name in names {
            if let Some((aside, file)) = outbox_name(name) {
                let folder = if aside {
                    dir.join("unreadable")
                } else {
                    dir.clone()
                };
                let _ = std::fs::remove_file(folder.join(file));
            }
        }
        Ok(())
    }

    pub(super) fn set_aside(_app: &AppHandle, names: &[String]) -> Result<(), String> {
        let dir = folder()?.join("outbox");
        std::fs::create_dir_all(dir.join("unreadable")).map_err(|e| e.to_string())?;
        for name in names {
            if let Some((false, file)) = outbox_name(name) {
                let _ = std::fs::rename(dir.join(file), dir.join("unreadable").join(file));
            }
        }
        Ok(())
    }

    pub(super) fn clear(_app: &AppHandle) -> Result<(), String> {
        let folder = folder()?;
        let _ = std::fs::remove_file(folder.join("passkeys.sealed"));
        let _ = delete_generic_password_options(options()?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::outbox_name;

    #[test]
    fn outbox_names() {
        assert_eq!(outbox_name("A1.sealed"), Some((false, "A1.sealed")));
        assert_eq!(
            outbox_name("unreadable/A1.sealed"),
            Some((true, "A1.sealed"))
        );
        for bad in [
            "../x.sealed",
            "unreadable/../x.sealed",
            "unreadable/unreadable/x.sealed",
            ".sealed",
            ".hidden.sealed",
            "x.txt",
            "a\\b.sealed",
            "aside:x.sealed/",
        ] {
            assert_eq!(outbox_name(bad), None, "{bad}");
        }
    }
}
