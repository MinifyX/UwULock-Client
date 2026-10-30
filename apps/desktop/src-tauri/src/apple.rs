//! The Apple builds' AutoFill extension, the system's half: what the host
//! (host.rs) hands the app for passkeys/apple.rs and autofill.rs in
//! uwulock-app, which have the story.
//!
//! - `extension` — the extension's sealed list, its key and its outbox: on
//!   iOS through the phone's plugin (Passkeys.swift), on macOS from Rust
//! - `autofill` — macOS: whether UwULock is the AutoFill provider, and asking
//!   for it

/// iOS: Passkeys.swift in the phone's plugin does it.
#[cfg(target_os = "ios")]
pub(crate) mod extension {
    use uwulock_app::passkeys::apple::Identity;
    use uwulock_authenticator::webauthn::{b64, from_b64};
    use zeroize::Zeroizing;

    fn plugin(
    ) -> Result<tauri::State<'static, tauri_plugin_uwulock_mobile::Mobile<tauri::Wry>>, String>
    {
        crate::phone::plugin().ok_or_else(|| "the phone's plugin isn't there".to_string())
    }

    pub(crate) fn note(text: &str) {
        if let Ok(plugin) = plugin() {
            let _ = plugin.autofill_log_note(text);
        }
    }

    pub(crate) fn problem() -> Option<String> {
        match plugin().and_then(|p| p.passkeys_status().map_err(|e| e.message)) {
            Ok(status) if status.ready => None,
            Ok(status) => Some(status.reason.unwrap_or_else(|| "not ready".into())),
            Err(error) => Some(error),
        }
    }

    /// The key goes along every time: Passkeys.swift (re)writes the
    /// Keychain item when the one there isn't this key (by its label,
    /// `uwulock_authenticator::apple::key_id`) or is gone.
    pub(crate) fn store(
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
    pub(crate) fn outbox() -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(plugin()?
            .passkeys_outbox()
            .map_err(|e| e.message)?
            .into_iter()
            .filter_map(|(name, sealed)| Some((name, from_b64(&sealed).ok()?)))
            .collect())
    }

    pub(crate) fn clear_outbox(names: &[String]) -> Result<(), String> {
        plugin()?
            .passkeys_clear_outbox(names)
            .map_err(|e| e.message)
    }

    /// Moves outbox files to `outbox/unreadable/`: `passkeysClearOutbox`
    /// moves a name given as `aside:<name>` instead of removing it.
    pub(crate) fn set_aside(names: &[String]) -> Result<(), String> {
        let names: Vec<String> = names.iter().map(|name| format!("aside:{name}")).collect();
        plugin()?
            .passkeys_clear_outbox(&names)
            .map_err(|e| e.message)
    }

    pub(crate) fn clear() -> Result<(), String> {
        plugin()?.passkeys_clear().map_err(|e| e.message)
    }
}

/// macOS: the same as iOS, done from Rust. The App Group folder and the Keychain
/// group carry the developer team's id, which a build only knows when it is
/// signed (`UWULOCK_APPLE_TEAM_ID` at build time).
#[cfg(target_os = "macos")]
pub(crate) mod extension {
    use crate::hello::touch;
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::item::{ItemClass, ItemSearchOptions};
    use security_framework::passwords::{
        delete_generic_password_options, set_generic_password_options, AccessControlOptions,
        PasswordOptions,
    };
    use std::path::PathBuf;
    use uwulock_app::passkeys::apple::{outbox_name, Identity};

    const TEAM: Option<&str> = option_env!("UWULOCK_APPLE_TEAM_ID");
    const SERVICE: &str = "app.uwulock.passkeys";
    const ACCOUNT: &str = "provider-key";
    const INTERACTION_NOT_ALLOWED: i32 = -25308;

    fn team() -> Result<&'static str, String> {
        TEAM.filter(|t| !t.is_empty())
            .ok_or_else(|| "This build isn't signed with an Apple developer team: the AutoFill extension stays off (docs/passkeys.md).".to_string())
    }

    fn folder() -> Result<PathBuf, String> {
        Ok(home()?
            .join("Library/Group Containers")
            .join(format!("{}.app.uwulock", team()?))
            .join("Passkeys"))
    }

    pub(crate) fn log_path() -> Result<PathBuf, String> {
        Ok(folder()?.join("autofill.log"))
    }

    /// The user's real home folder. In the Mac App Store build `HOME` is the
    /// app's own sandbox container, and the group's folder isn't in there.
    fn home() -> Result<PathBuf, String> {
        use std::ffi::{CStr, OsStr};
        use std::os::unix::ffi::OsStrExt;

        let mut buffer = vec![0 as libc::c_char; 4096];
        // SAFETY: an all-zero passwd is a valid value to be overwritten.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        // SAFETY: every pointer is to memory of the size given, alive for the call;
        // `entry`'s strings point into `buffer`, read before it goes.
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                &mut entry,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut found,
            )
        };
        if status == 0 && !found.is_null() && !entry.pw_dir.is_null() {
            // SAFETY: getpwuid_r succeeded, so pw_dir is a NUL-terminated string in `buffer`.
            let dir = unsafe { CStr::from_ptr(entry.pw_dir) };
            return Ok(PathBuf::from(OsStr::from_bytes(dir.to_bytes())));
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "no home folder".to_string())
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

    fn search(group: &str) -> ItemSearchOptions {
        let mut search = ItemSearchOptions::new();
        search
            .class(ItemClass::generic_password())
            .service(SERVICE)
            .account(ACCOUNT)
            .access_group(group)
            .ignore_legacy_keychains();
        // Never Touch ID for the app's side: what would need it fails instead.
        if let Some(context) = touch::context(true) {
            touch::with_context(&mut search, &context);
        }
        search
    }

    /// Whether the item there is this key. An error while the Keychain is
    /// locked (the screen locked, the Mac asleep): then nothing is written,
    /// the list neither, and the next sync tries again.
    fn key_there(key: &[u8; 32]) -> Result<bool, String> {
        let group = group()?;
        objc2::rc::autoreleasepool(|_| {
            match search(&group)
                .label(&label(key))
                .load_attributes(true)
                .search()
            {
                Ok(found) => Ok(!found.is_empty()),
                Err(error) if error.code() == INTERACTION_NOT_ALLOWED => {
                    Err("The Keychain is locked.".to_string())
                }
                Err(_) => Ok(false),
            }
        })
    }

    /// Removes the item, with Touch ID only if macOS insists on it for that.
    fn delete() -> Result<(), String> {
        let group = group()?;
        objc2::rc::autoreleasepool(|_| match search(&group).delete() {
            Err(error) if error.code() == INTERACTION_NOT_ALLOWED => {
                delete_generic_password_options(options()?).map_err(|e| e.to_string())
            }
            _ => Ok(()),
        })
    }

    pub(crate) fn problem() -> Option<String> {
        team().err()
    }

    /// The Mac has Console.app for the app's side (`tracing`): nothing into
    /// the extension's protocol.
    pub(crate) fn note(_text: &str) {}

    pub(crate) fn store(
        list: &[u8],
        key: &[u8; 32],
        _identities: &[Identity],
    ) -> Result<(), String> {
        let folder = folder()?;
        std::fs::create_dir_all(folder.join("outbox").join("unreadable"))
            .map_err(|e| e.to_string())?;
        if !key_there(key)? {
            // Missing, or another account's key, or one from before: as on
            // iOS, only after Touch ID or the login password, this Mac only.
            let _ = delete();
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
    pub(crate) fn outbox() -> Result<Vec<(String, Vec<u8>)>, String> {
        let dir = folder()?.join("outbox");
        let mut found = read_dir(dir.clone(), "");
        found.extend(read_dir(dir.join("unreadable"), "unreadable/"));
        Ok(found)
    }

    pub(crate) fn clear_outbox(names: &[String]) -> Result<(), String> {
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

    pub(crate) fn set_aside(names: &[String]) -> Result<(), String> {
        let dir = folder()?.join("outbox");
        std::fs::create_dir_all(dir.join("unreadable")).map_err(|e| e.to_string())?;
        for name in names {
            if let Some((false, file)) = outbox_name(name) {
                let _ = std::fs::rename(dir.join(file), dir.join("unreadable").join(file));
            }
        }
        Ok(())
    }

    pub(crate) fn clear() -> Result<(), String> {
        let folder = folder()?;
        let _ = std::fs::remove_file(folder.join("passkeys.sealed"));
        let _ = delete();
        // The system's list keeps names until someone empties it; on macOS only the extension
        // fills it, so it goes here too.
        super::autofill::remove_identities();
        Ok(())
    }
}

/// macOS: AuthenticationServices through the Objective-C runtime. The calls
/// answer through a block; they are waited for here (never on the main
/// thread), with a time limit.
#[cfg(target_os = "macos")]
pub(crate) mod autofill {
    use crate::blocks;
    use block2::RcBlock;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
    use objc2::{msg_send, sel};
    use std::sync::mpsc;
    use std::time::Duration;
    use tauri::AppHandle;

    #[link(name = "AuthenticationServices", kind = "framework")]
    unsafe extern "C" {}

    fn class(name: &std::ffi::CStr) -> Option<&'static AnyClass> {
        AnyClass::get(name)
    }

    fn responds(class: &AnyClass, selector: Sel) -> bool {
        // SAFETY: asking a class whether it answers a selector has no side effects.
        let answers: Bool = unsafe { msg_send![class, respondsToSelector: selector] };
        answers.as_bool()
    }

    /// Whether UwULock is switched on in System Settings; `None` when the
    /// system doesn't answer in time.
    pub(crate) fn enabled() -> Option<bool> {
        let store_class = class(c"ASCredentialIdentityStore")?;
        let (tx, rx) = mpsc::channel();
        let block =
            RcBlock::with_encoding::<_, _, _, blocks::Object>(move |state: *mut AnyObject| {
                let enabled = if state.is_null() {
                    false
                } else {
                    // SAFETY: `state` is the ASCredentialIdentityStoreState the system hands the block.
                    let on: Bool = unsafe { msg_send![state, isEnabled] };
                    on.as_bool()
                };
                let _ = tx.send(enabled);
            });
        // SAFETY: `sharedStore` returns the shared instance; the Objective-C name of Swift's
        // `getState(_:)` is `getCredentialIdentityStoreStateWithCompletion:`, taking a block of
        // `void (^)(ASCredentialIdentityStoreState *)`, as `block` is. Checked before it is sent:
        // an unknown selector would raise and abort the app.
        let store = shared_store(store_class)?;
        blocks::guarded("AutoFill state", || unsafe {
            let selector = sel!(getCredentialIdentityStoreStateWithCompletion:);
            let answers: Bool = msg_send![store, respondsToSelector: selector];
            if answers.as_bool() {
                let _: () =
                    msg_send![store, getCredentialIdentityStoreStateWithCompletion: &*block];
            }
            answers.as_bool()
        })
        .filter(|sent| *sent)?;
        rx.recv_timeout(Duration::from_secs(5)).ok()
    }

    /// `ASCredentialIdentityStore.sharedStore`; `store_class` is that class.
    fn shared_store(store_class: &AnyClass) -> Option<&'static AnyObject> {
        // SAFETY: `sharedStore` returns the shared instance (or nil), which lives as long as the
        // process.
        unsafe {
            let store: *mut AnyObject = msg_send![store_class, sharedStore];
            store.as_ref()
        }
    }

    /// Empties the system's list of UwULock's passwords and passkeys (logout, switched off): on
    /// macOS only the extension fills it, so nothing else would. Doesn't wait.
    pub(crate) fn remove_identities() {
        let Some(store_class) = class(c"ASCredentialIdentityStore") else {
            return;
        };
        let block = RcBlock::with_encoding::<_, _, _, blocks::FlagError>(
            |_done: Bool, _error: *mut AnyObject| {},
        );
        // SAFETY: `removeAllCredentialIdentitiesWithCompletion:` takes a block of
        // `void (^)(BOOL, NSError *)`, as `block` is; checked before it is sent.
        let Some(store) = shared_store(store_class) else {
            return;
        };
        blocks::guarded("AutoFill removal", || unsafe {
            let selector = sel!(removeAllCredentialIdentitiesWithCompletion:);
            let answers: Bool = msg_send![store, respondsToSelector: selector];
            if answers.as_bool() {
                let _: () = msg_send![store, removeAllCredentialIdentitiesWithCompletion: &*block];
            }
        });
    }

    /// macOS 15+: the system asks the person itself.
    pub(crate) fn can_ask() -> bool {
        class(c"ASSettingsHelper").is_some_and(|c| {
            responds(
                c,
                sel!(requestToTurnOnCredentialProviderExtensionWithCompletionHandler:),
            )
        })
    }

    /// Asks the system to switch UwULock on (macOS 15+), or opens the
    /// AutoFill settings (macOS 14). Waits for the person, at most two
    /// minutes.
    pub(crate) fn ask(app: &AppHandle) {
        let Some(helper) = class(c"ASSettingsHelper") else {
            return;
        };
        let (tx, rx) = mpsc::channel::<()>();
        let direct = can_ask();
        let opens = responds(
            helper,
            sel!(openCredentialProviderAppSettingsWithCompletionHandler:),
        );
        if !direct && !opens {
            return;
        }
        let started = app.run_on_main_thread(move || {
            if direct {
                let tx = tx.clone();
                let block = RcBlock::with_encoding::<_, _, _, blocks::Flag>(move |_on: Bool| {
                    let _ = tx.send(());
                });
                // SAFETY: the class method takes a block of `void (^)(BOOL)`, as `block` is.
                blocks::guarded("AutoFill request", || unsafe {
                    let _: () = msg_send![
                        helper,
                        requestToTurnOnCredentialProviderExtensionWithCompletionHandler: &*block
                    ];
                });
            } else {
                let tx = tx.clone();
                let block = RcBlock::with_encoding::<_, _, _, blocks::Object>(
                    move |_error: *mut AnyObject| {
                        let _ = tx.send(());
                    },
                );
                // SAFETY: the class method takes a block of `void (^)(NSError *)`, as `block` is.
                blocks::guarded("AutoFill settings", || unsafe {
                    let _: () = msg_send![
                        helper,
                        openCredentialProviderAppSettingsWithCompletionHandler: &*block
                    ];
                });
            }
        });
        if started.is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(120));
        }
    }
}
