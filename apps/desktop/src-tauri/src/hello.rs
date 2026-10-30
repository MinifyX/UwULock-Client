//! The system's half of unlocking with biometrics: Windows Hello's
//! signature (`platform`) and the Mac App Store build's Touch ID Keychain
//! item (`touch`). The host (host.rs) hands them to the app, which keeps the
//! rest — what is sealed with them and where (uwulock-app's hello.rs, which
//! has the whole story). The phones' secret is the mobile plugin's.

#[cfg(windows)]
pub(crate) mod platform {
    use sha2::{Digest, Sha256};
    use windows::core::{Array, HSTRING};
    use windows::Security::Credentials::{
        KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
    };
    use windows::Security::Cryptography::CryptographicBuffer;

    pub fn available() -> bool {
        KeyCredentialManager::IsSupportedAsync()
            .and_then(|op| op.get())
            .unwrap_or(false)
    }

    /// Windows Hello's signature over `challenge` with the key called
    /// `name`, hashed. `create` makes the key (replacing an old one) and asks
    /// the person to confirm; otherwise the existing key is used.
    pub fn secret(name: &str, challenge: &[u8], create: bool) -> Result<[u8; 32], String> {
        let name = HSTRING::from(name);
        bring_dialog_forward();
        let result = if create {
            KeyCredentialManager::RequestCreateAsync(
                &name,
                KeyCredentialCreationOption::ReplaceExisting,
            )
        } else {
            KeyCredentialManager::OpenAsync(&name)
        }
        .and_then(|op| op.get())
        .map_err(|e| e.message())?;
        let status = result.Status().map_err(|e| e.message())?;
        if status != KeyCredentialStatus::Success {
            return Err(refusal(status));
        }
        let credential = result.Credential().map_err(|e| e.message())?;
        let buffer =
            CryptographicBuffer::CreateFromByteArray(challenge).map_err(|e| e.message())?;
        let signed = credential
            .RequestSignAsync(&buffer)
            .and_then(|op| op.get())
            .map_err(|e| e.message())?;
        let status = signed.Status().map_err(|e| e.message())?;
        if status != KeyCredentialStatus::Success {
            return Err(refusal(status));
        }
        let signature = signed.Result().map_err(|e| e.message())?;
        let mut bytes = Array::<u8>::new();
        CryptographicBuffer::CopyToByteArray(&signature, &mut bytes).map_err(|e| e.message())?;
        Ok(Sha256::digest(&bytes[..]).into())
    }

    pub fn delete(name: &str) {
        let _ = KeyCredentialManager::DeleteAsync(&HSTRING::from(name)).and_then(|op| op.get());
    }

    fn refusal(status: KeyCredentialStatus) -> String {
        if status == KeyCredentialStatus::UserCanceled {
            "Windows Hello was cancelled.".into()
        } else if status == KeyCredentialStatus::NotFound {
            "Windows Hello doesn't know this account any more. Unlock with the master \
             password and switch Windows Hello on again."
                .into()
        } else {
            format!("Windows Hello said no ({status:?}).")
        }
    }

    /// The Hello dialog sometimes opens behind the app's window; it is
    /// looked for a few times and brought to the front.
    fn bring_dialog_forward() {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{
            BringWindowToTop, FindWindowW, SetForegroundWindow,
        };
        std::thread::spawn(|| {
            let class: Vec<u16> = "Credential Dialog Xaml Host\0".encode_utf16().collect();
            for _ in 0..8 {
                std::thread::sleep(std::time::Duration::from_millis(250));
                // SAFETY: a NUL-terminated class name and no window name.
                let found = unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) };
                if let Ok(window) = found {
                    unsafe {
                        let _ = BringWindowToTop(window);
                        let _ = SetForegroundWindow(window);
                    }
                    return;
                }
            }
        });
    }
}

/// Touch ID on a Mac, in the Mac App Store build: 32 random bytes in the
/// data protection keychain, in a Keychain group of the app's alone (no app
/// group: those are Keychain groups too, and the AutoFill extension has
/// `app.uwulock`), handed out only after a finger
/// enrolled right now (`biometryCurrentSet`), on this Mac only, only while
/// it has a login password. Like a phone's secret. A build without the
/// developer team has no Keychain group and offers nothing.
#[cfg(target_os = "macos")]
pub(crate) mod touch {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use rand::RngCore as _;
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
    use security_framework::passwords::{
        set_generic_password_options, AccessControlOptions, PasswordOptions,
    };
    use zeroize::Zeroizing;

    use uwulock_app::Failure;

    #[link(name = "LocalAuthentication", kind = "framework")]
    unsafe extern "C" {}

    const TEAM: Option<&str> = option_env!("UWULOCK_APPLE_TEAM_ID");
    const SERVICE: &str = "app.uwulock.unlock";
    /// `LAPolicyDeviceOwnerAuthenticationWithBiometrics`.
    const BIOMETRICS: isize = 1;
    const CANCELLED: i32 = -128;
    const NOT_FOUND: i32 = -25300;
    const AUTH_FAILED: i32 = -25293;
    const INTERACTION_NOT_ALLOWED: i32 = -25308;

    /// The Touch ID group in the app's entitlements (Entitlements.mas.plist).
    fn group() -> Option<String> {
        TEAM.filter(|t| !t.is_empty())
            .map(|team| format!("{team}.app.uwulock.unlock"))
    }

    fn nsstring(text: &str) -> Option<Retained<AnyObject>> {
        let text = std::ffi::CString::new(text.replace('\0', "")).ok()?;
        let class = AnyClass::get(c"NSString")?;
        // SAFETY: a NUL-terminated UTF-8 string; NSString copies it.
        unsafe { msg_send![class, stringWithUTF8String: text.as_ptr()] }
    }

    /// A fresh `LAContext`. `quiet`: a Keychain call with it fails instead of
    /// asking for Touch ID (`interactionNotAllowed`).
    pub(crate) fn context(quiet: bool) -> Option<Retained<AnyObject>> {
        let class = AnyClass::get(c"LAContext")?;
        // SAFETY: `new` returns an owned LAContext (or nil).
        let context: Option<Retained<AnyObject>> = unsafe { msg_send![class, new] };
        let context = context?;
        if quiet {
            // SAFETY: `interactionNotAllowed` is a BOOL property of LAContext.
            unsafe {
                let _: () = msg_send![&*context, setInteractionNotAllowed: Bool::YES];
            }
        }
        Some(context)
    }

    /// Hands `context` to a Keychain call (`kSecUseAuthenticationContext`).
    pub(crate) fn with_context(search: &mut ItemSearchOptions, context: &Retained<AnyObject>) {
        let raw = Retained::into_raw(context.clone());
        // SAFETY: `raw` is an owned (+1) LAContext, an NSObject and so a CFType;
        // the search takes that reference over and releases it.
        #[allow(deprecated)]
        unsafe {
            search.authentication_context(raw.cast());
        }
    }

    /// Whether this build has its Keychain group and the Mac a finger enrolled.
    pub fn available() -> bool {
        objc2::rc::autoreleasepool(|_| available_in_pool())
    }

    fn available_in_pool() -> bool {
        if group().is_none() {
            return false;
        }
        let Some(context) = context(false) else {
            return false;
        };
        // SAFETY: `canEvaluatePolicy:error:` takes an LAPolicy (NSInteger) and
        // an optional `NSError **`; nil for the latter.
        let can: Bool = unsafe {
            msg_send![
                &*context,
                canEvaluatePolicy: BIOMETRICS,
                error: std::ptr::null_mut::<*mut AnyObject>()
            ]
        };
        can.as_bool()
    }

    fn search(name: &str, group: &str) -> ItemSearchOptions {
        let mut search = ItemSearchOptions::new();
        search
            .class(ItemClass::generic_password())
            .service(SERVICE)
            .account(name)
            .access_group(group)
            .ignore_legacy_keychains();
        search
    }

    fn missing_group() -> Failure {
        Failure::new(
            "unsupported",
            "Touch ID needs the Mac App Store build of UwULock.",
        )
    }

    /// The secret, after Touch ID; `reason` completes macOS's sentence
    /// "UwULock is trying to …", `cancel` names the way back.
    pub fn read(name: &str, reason: &str, cancel: &str) -> Result<Zeroizing<[u8; 32]>, Failure> {
        // The NSStrings and what the Keychain hands back autoreleased go with
        // this pool, not with the blocking thread that lives on.
        objc2::rc::autoreleasepool(|_| read_in_pool(name, reason, cancel))
    }

    fn read_in_pool(
        name: &str,
        reason: &str,
        cancel: &str,
    ) -> Result<Zeroizing<[u8; 32]>, Failure> {
        let group = group().ok_or_else(missing_group)?;
        let context = context(false).ok_or_else(|| Failure::new("hello", "No LAContext."))?;
        // SAFETY: both are NSString properties of LAContext.
        unsafe {
            if let Some(reason) = nsstring(reason) {
                let _: () = msg_send![&*context, setLocalizedReason: &*reason];
            }
            if let Some(cancel) = nsstring(cancel) {
                let _: () = msg_send![&*context, setLocalizedCancelTitle: &*cancel];
            }
            // No "Enter password" after a finger that didn't fit: the
            // master password is the way back, behind the cancel button.
            if let Some(none) = nsstring("") {
                let _: () = msg_send![&*context, setLocalizedFallbackTitle: &*none];
            }
        }
        let mut search = search(name, &group);
        search.load_data(true).limit(1);
        with_context(&mut search, &context);
        let found = search.search().map_err(|error| match error.code() {
            CANCELLED => Failure::new("biometric-cancelled", "Cancelled."),
            // With `biometryCurrentSet` the item no longer opens once a finger
            // is enrolled or removed.
            NOT_FOUND => Failure::new(
                "biometric-changed",
                format!(
                    "Nothing is kept for Touch ID on this Mac. {}",
                    uwulock_app::hello::AGAIN
                ),
            ),
            AUTH_FAILED => Failure::new("hello", "Touch ID didn't recognise you."),
            code => Failure::new("hello", format!("The Keychain said no ({code}).")),
        })?;
        let mut secret = Zeroizing::new([0u8; 32]);
        match found.into_iter().next() {
            Some(SearchResult::Data(mut data)) if data.len() == 32 => {
                secret.copy_from_slice(&data);
                zeroize::Zeroize::zeroize(&mut data);
                Ok(secret)
            }
            _ => Err(Failure::new(
                "hello",
                "The Keychain gave back something else.",
            )),
        }
    }

    /// A new secret, and Touch ID once to confirm it opens.
    pub fn create(name: &str, reason: &str, cancel: &str) -> Result<Zeroizing<[u8; 32]>, Failure> {
        let group = group().ok_or_else(missing_group)?;
        delete(name, true);
        let mut secret = Zeroizing::new([0u8; 32]);
        rand::rngs::OsRng.fill_bytes(secret.as_mut());
        let refused = |error: security_framework::base::Error| {
            Failure::new("hello", format!("The Keychain said no ({}).", error.code()))
        };
        let access = SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
            AccessControlOptions::BIOMETRY_CURRENT_SET.bits(),
        )
        .map_err(refused)?;
        let mut options = PasswordOptions::new_generic_password(SERVICE, name);
        options.set_access_group(&group);
        options.use_protected_keychain();
        options.set_access_synchronized(Some(false));
        options.set_access_control(access);
        options.set_label("UwULock unlock");
        set_generic_password_options(secret.as_ref(), options).map_err(refused)?;
        let back = read(name, reason, cancel).inspect_err(|_| delete(name, false))?;
        if *back != *secret {
            delete(name, false);
            return Err(Failure::new(
                "hello",
                "The Keychain gave back something else.",
            ));
        }
        Ok(secret)
    }

    /// Removes the secret without asking; `or_ask`: with Touch ID, should
    /// macOS want it for removing.
    pub fn delete(name: &str, or_ask: bool) {
        objc2::rc::autoreleasepool(|_| delete_in_pool(name, or_ask))
    }

    fn delete_in_pool(name: &str, or_ask: bool) {
        let Some(group) = group() else {
            return;
        };
        let mut quiet = search(name, &group);
        if let Some(context) = context(true) {
            with_context(&mut quiet, &context);
        }
        if let Err(error) = quiet.delete() {
            if or_ask && error.code() == INTERACTION_NOT_ALLOWED {
                let _ = search(name, &group).delete();
            }
        }
    }
}
