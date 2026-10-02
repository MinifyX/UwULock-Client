//! iOS 17+ and macOS 14+: the AutoFill extension's side of the vault.
//!
//! The extension (`apps/desktop/src-tauri/apple/PasskeyProvider`, Swift) is
//! its own process, started by the system while UwULock may not run at all.
//! So while the vault is open, the app keeps a **sealed list** of the
//! account's passkeys in the App Group folder they share, and takes in the
//! passkeys the extension made (its **outbox**) as new logins. Both are
//! sealed with the provider key (uwulock_authenticator::apple): 32 random
//! bytes, kept here sealed under the user key (`passkeys-apple.key`), and for
//! the extension in the shared Keychain behind Face ID / Touch ID / the
//! device passcode, this device only.
//!
//! None of it works before the app and the extension are signed with an
//! Apple developer team (App Group, Keychain group, the AutoFill
//! entitlement): until then the status says so. docs/passkeys.md has the
//! steps.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Listener, Manager};
use uwulock_authenticator::apple::{self as sealed, Entry, Snapshot};
use uwulock_bitwarden::EncString;
use uwulock_bitwarden::SymmetricKey;
use zeroize::Zeroizing;

use super::{passkeys_of, Provider};
use crate::vault::{Failure, Result, VaultState};

static REFRESHING: AtomicBool = AtomicBool::new(false);

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

/// Switched off: the list, the key and the system's entries go.
pub(crate) fn clear(app: &AppHandle) {
    if let Err(error) = native::clear(app) {
        tracing::warn!(%error, "couldn't remove the extension's passkey list");
    }
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

/// The provider key of an open account: the one kept, or a new one.
fn provider_key(
    app: &AppHandle,
    account_id: &str,
    user_key: &SymmetricKey,
) -> Result<(Zeroizing<[u8; 32]>, bool)> {
    let vault = app.state::<VaultState>();
    let path = vault
        .storage()
        .dir()
        .join(format!("passkeys-apple-{account_id}.key"));
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(opened) = text
            .trim()
            .parse::<EncString>()
            .and_then(|enc| enc.decrypt(user_key))
        {
            if opened.len() == 32 {
                let mut key = Zeroizing::new([0u8; 32]);
                key.copy_from_slice(&opened);
                return Ok((key, false));
            }
        }
    }
    let key = sealed::new_key();
    std::fs::write(
        &path,
        EncString::encrypt(key.as_ref(), user_key).to_string(),
    )
    .map_err(|e| Failure::new("io", e.to_string()))?;
    Ok((key, true))
}

async fn refresh(app: &AppHandle) -> Result<()> {
    if native::problem(app).is_some() {
        return Ok(());
    }
    let vault = app.state::<VaultState>();
    let (account_id, _) = vault.active_account()?;
    let user_key = vault.user_key_of(&account_id)?;
    let (key, new) = provider_key(app, &account_id, &user_key)?;

    // What the extension made: into the vault, as new logins.
    let mut taken = Vec::new();
    for (name, bytes) in native::outbox(app).unwrap_or_default() {
        let passkey = sealed::open_outbox(&key, &bytes).and_then(|entry| entry.to_passkey());
        match passkey {
            Ok(passkey) => match super::save_passkey(app, passkey, None).await {
                Ok(_) => taken.push(name),
                Err(error) => {
                    tracing::warn!(
                        error = error.message(),
                        "couldn't save a passkey from the extension"
                    );
                }
            },
            Err(error) => {
                // Sealed with an older key: nobody can open it any more.
                tracing::warn!(%error, "an outbox entry doesn't open");
                taken.push(name);
            }
        }
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
            .filter(|item| {
                !item.deleted && item.login.as_ref().is_some_and(|l| l.passkey_count() > 0)
            })
            .cloned()
            .collect()
    };
    let mut snapshot = Snapshot {
        version: 1,
        entries: Vec::new(),
    };
    let mut identities = Vec::new();
    for item in &items {
        for (_, passkey) in passkeys_of(&vault, &account_id, item) {
            // The extension can't count up a signature counter in the vault:
            // passkeys that use one stay with the app and the browser extension.
            if passkey.counter > 0 {
                continue;
            }
            let Ok(entry) = Entry::of(&passkey, Some(&item.id)) else {
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
            snapshot.entries.push(entry);
        }
    }
    let sealed_list = snapshot.seal(&key);
    native::store(app, &sealed_list, new.then_some(&*key), &identities)
        .map_err(|e| Failure::new("unsupported", e))?;
    tracing::debug!(
        passkeys = identities.len(),
        "the extension's passkey list is up to date"
    );
    Ok(())
}

#[cfg(target_os = "ios")]
mod native {
    use super::Identity;
    use tauri::AppHandle;
    use uwulock_authenticator::webauthn::{b64, from_b64};

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

    pub(super) fn store(
        _app: &AppHandle,
        list: &[u8],
        key: Option<&[u8; 32]>,
        identities: &[Identity],
    ) -> Result<(), String> {
        let identities = serde_json::to_value(identities).map_err(|e| e.to_string())?;
        plugin()?
            .passkeys_store(&b64(list), key.map(|k| b64(k)).as_deref(), identities)
            .map_err(|e| e.message)
    }

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

    pub(super) fn clear(_app: &AppHandle) -> Result<(), String> {
        plugin()?.passkeys_clear().map_err(|e| e.message)
    }
}

/// macOS: the same, done from Rust. The App Group folder and the Keychain
/// group carry the developer team's id, which a build only knows when it is
/// signed (`UWULOCK_APPLE_TEAM_ID` at build time).
#[cfg(target_os = "macos")]
mod native {
    use super::Identity;
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

    fn options() -> Result<PasswordOptions, String> {
        let mut options = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
        options.set_access_group(&format!("{}.app.uwulock.passkeys", team()?));
        options.use_protected_keychain();
        Ok(options)
    }

    pub(super) fn problem(_app: &AppHandle) -> Option<String> {
        team().err()
    }

    pub(super) fn store(
        _app: &AppHandle,
        list: &[u8],
        key: Option<&[u8; 32]>,
        _identities: &[Identity],
    ) -> Result<(), String> {
        let folder = folder()?;
        std::fs::create_dir_all(folder.join("outbox")).map_err(|e| e.to_string())?;
        if let Some(key) = key {
            let _ = delete_generic_password_options(options()?);
            let mut options = options()?;
            options.set_access_control_options(AccessControlOptions::USER_PRESENCE);
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

    pub(super) fn outbox(_app: &AppHandle) -> Result<Vec<(String, Vec<u8>)>, String> {
        let dir = folder()?.join("outbox");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        Ok(entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".sealed"))
            .filter_map(|e| {
                Some((
                    e.file_name().to_string_lossy().to_string(),
                    std::fs::read(e.path()).ok()?,
                ))
            })
            .collect())
    }

    pub(super) fn clear_outbox(_app: &AppHandle, names: &[String]) -> Result<(), String> {
        let dir = folder()?.join("outbox");
        for name in names {
            if !name.contains(['/', '\\']) && !name.starts_with('.') {
                let _ = std::fs::remove_file(dir.join(name));
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
