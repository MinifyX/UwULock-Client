//! Unlocking with Windows Hello, and on phones with a fingerprint or face.
//!
//! Windows Hello keeps a key pair per app and name in the TPM (or in
//! software, where there is none) and signs with it only after the person
//! showed their face, finger or PIN. UwULock asks it to sign a challenge for
//! the account; the signature (RSA PKCS#1 v1.5, the same every time for the
//! same key and challenge) goes through SHA-256 and is stretched into a key
//! that seals a copy of the user key in the account's file. Without the
//! person at the computer there is no signature, and without it the copy
//! doesn't open. Nothing of it needs a signed app.
//!
//! The challenge has 32 random bytes, new at every switching on, kept next to
//! the copy under DPAPI (this Windows user only). So a signature obtained
//! elsewhere is worth nothing, and one obtained on this computer only until
//! Windows Hello is switched off and on again, which makes a new key pair and
//! a new challenge — the way back after a suspicion. (Up to 0.3.0-beta.1 the
//! challenge was fixed and public; such a copy is dropped at its next use and
//! the person switches Windows Hello on again.)
//!
//! Touch ID only in the Mac App Store build (`touch`): its Keychain items that
//! only open with a finger need the app signed with the developer team and a
//! Keychain group. There it works like a phone: 32 random bytes in the
//! Keychain, the copy kept as `t1:<the user key under their key>`. A Touch ID
//! prompt alone, with the key kept elsewhere on disk, would be a door in front
//! of an open window — so the other Mac builds offer none. Linux has no common
//! equivalent.
//!
//! On Android and iOS the phone keeps the secret itself (`phone`, the
//! `tauri-plugin-uwulock-mobile` plugin): 32 random bytes in the Android
//! Keystore or the iOS Keychain, handed out only after a strong biometric,
//! and gone for good when a finger or face is enrolled. They stretch into the
//! key that seals the copy of the user key, the same way as Hello's
//! signature; the copy is kept as `m1:<the user key under that key>`. No
//! master password and no PIN instead of the biometric: that is what the
//! master password is for.

#[cfg(windows)]
mod platform {
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

    /// Seals `data` with DPAPI for this Windows user.
    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        dpapi(data, true).map(|out| out.to_vec())
    }

    /// Opens what [`protect`] sealed.
    pub fn unprotect(data: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
        dpapi(data, false)
    }

    fn dpapi(data: &[u8], seal: bool) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).map_err(|_| "too much for DPAPI".to_string())?,
            pbData: data.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: `input` points at `data` for its length; DPAPI only reads
        // it. `output` is filled by DPAPI and freed with LocalFree below.
        let ok = unsafe {
            if seal {
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if ok == 0 || output.pbData.is_null() {
            return Err(format!(
                "DPAPI said no ({})",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: DPAPI returned `cbData` bytes at `pbData`, freed right after
        // they are copied (and wiped).
        let out = unsafe {
            let bytes = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
            let copy = zeroize::Zeroizing::new(bytes.to_vec());
            zeroize::Zeroize::zeroize(bytes);
            LocalFree(output.pbData.cast());
            copy
        };
        Ok(out)
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
/// data protection keychain, in the app's own Keychain group (not the one
/// shared with the AutoFill extension), handed out only after a finger
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

    use crate::vault::Failure;

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

    /// The app's own Keychain group, first in its entitlements.
    fn group() -> Option<String> {
        TEAM.filter(|t| !t.is_empty())
            .map(|team| format!("{team}.app.uwulock"))
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
                format!("Nothing is kept for Touch ID on this Mac. {}", super::AGAIN),
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

#[cfg(not(any(windows, mobile, target_os = "macos")))]
mod platform {
    pub fn available() -> bool {
        false
    }

    pub fn secret(_name: &str, _challenge: &[u8], _create: bool) -> Result<[u8; 32], String> {
        Err("Windows Hello is only on Windows.".into())
    }

    pub fn delete(_name: &str) {}

    pub fn protect(_data: &[u8]) -> Result<Vec<u8>, String> {
        Err("DPAPI is only on Windows.".into())
    }

    pub fn unprotect(_data: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
        Err("DPAPI is only on Windows.".into())
    }
}

#[cfg(not(any(mobile, target_os = "macos")))]
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use parking_lot::RwLock;
#[cfg(not(any(mobile, target_os = "macos")))]
use rand::RngCore as _;
use serde::Deserialize;
use tauri::AppHandle;
use uwulock_bitwarden::{EncString, Error, SymmetricKey};
#[cfg(not(any(mobile, target_os = "macos")))]
use zeroize::Zeroizing;

use crate::vault::Failure;

/// What this device unlocks with, once asked: `windowsHello`, or a phone's
/// `fingerprint`, `face`, `iris`, `faceId`, `touchId`, `opticId` or
/// `biometric`. `None` until asked, and where there is nothing.
static KIND: RwLock<Option<String>> = RwLock::new(None);

/// The words of a phone's biometric dialog, in the page's language, and on a
/// Mac what completes "UwULock is trying to …" (`reason`). Windows Hello
/// brings its own.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(not(any(mobile, target_os = "macos")), allow(dead_code))]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub cancel: String,
    #[serde(default)]
    pub reason: String,
}

/// Seals `data` with DPAPI for this Windows user: what `account.rs` keeps
/// the remember token of two-step login under.
#[cfg(windows)]
pub(crate) fn protect_at_rest(data: &[u8]) -> Result<Vec<u8>, String> {
    platform::protect(data)
}

/// Opens what [`protect_at_rest`] sealed.
#[cfg(windows)]
pub(crate) fn unprotect_at_rest(data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    platform::unprotect(data)
}

/// Whether this device can unlock without the master password. `false`
/// until [`probe`] has an answer (it can take a moment).
pub(crate) fn available() -> bool {
    KIND.read().is_some()
}

pub(crate) fn kind() -> Option<String> {
    KIND.read().clone()
}

/// Asks the system, in the back, and tells the page when the answer changed.
/// On a phone again whenever UwULock comes back to the screen: a finger may
/// have been enrolled meanwhile.
pub(crate) fn probe(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let kind = ask();
        let changed = {
            let mut current = KIND.write();
            let changed = *current != kind;
            *current = kind;
            changed
        };
        if changed {
            crate::vault::emit_status(&app);
        }
    });
}

#[cfg(mobile)]
fn ask() -> Option<String> {
    let status = crate::phone::plugin()?.unlock_status().ok()?;
    status
        .available
        .then(|| status.kind.unwrap_or_else(|| "biometric".into()))
}

#[cfg(target_os = "macos")]
fn ask() -> Option<String> {
    touch::available().then(|| "touchId".to_string())
}

#[cfg(not(any(mobile, target_os = "macos")))]
fn ask() -> Option<String> {
    platform::available().then(|| "windowsHello".to_string())
}

fn name(account_id: &str) -> String {
    format!("UwULock-{account_id}")
}

/// What Hello signs: the account and the enrolment's random bytes.
#[cfg_attr(any(mobile, target_os = "macos"), allow(dead_code))]
fn challenge(account_id: &str, random: &[u8]) -> Vec<u8> {
    let mut out = format!("uwulock-hello-v2:{account_id}:").into_bytes();
    out.extend_from_slice(random);
    out
}

/// The key a Windows Hello signature, or a phone's secret, stretches into.
fn key_from(secret: &[u8; 32]) -> SymmetricKey {
    SymmetricKey::stretch(secret)
}

/// How Windows Hello's copy is kept: `v2:<the random bytes under DPAPI,
/// base64>:<the user key under the signature's key>`.
const PREFIX: &str = "v2:";
/// How a phone's copy is kept: `m1:<the user key under the phone's key>`.
const PHONE: &str = "m1:";
/// How a Mac's copy is kept: `t1:<the user key under Touch ID's key>`.
const MAC: &str = "t1:";

fn split(stored: &str) -> Option<(&str, &str)> {
    stored.strip_prefix(PREFIX)?.split_once(':')
}

/// A copy this build can't open: Windows Hello's from before 0.3.0-beta.2,
/// with a fixed challenge — or one from another kind of device.
pub(crate) fn is_outdated(stored: &str) -> bool {
    if cfg!(mobile) {
        !stored.starts_with(PHONE)
    } else if cfg!(target_os = "macos") {
        !stored.starts_with(MAC)
    } else {
        split(stored).is_none()
    }
}

const AGAIN: &str = "Unlock with the master password and switch it on again.";

fn opened(sealed: &str, secret: &[u8; 32]) -> Result<SymmetricKey, Failure> {
    sealed
        .parse::<EncString>()
        .and_then(|e| e.decrypt_key(&key_from(secret)))
        .map_err(|error| match error {
            Error::WrongKey => {
                Failure::new("hello", format!("The device's key has changed. {AGAIN}"))
            }
            other => Failure::new("hello", other.to_string()),
        })
}

/// Switching it on: a new key on the device, and the user key sealed under
/// what it gives. Blocks while the system asks the person.
#[cfg(not(any(mobile, target_os = "macos")))]
pub(crate) fn seal(
    account_id: &str,
    user_key: &SymmetricKey,
    _prompt: &Prompt,
) -> Result<String, Failure> {
    let refused = |message: String| Failure::new("hello", message);
    let mut random = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.fill_bytes(random.as_mut());
    let kept = platform::protect(random.as_ref()).map_err(refused)?;
    let secret = platform::secret(
        &name(account_id),
        &challenge(account_id, random.as_ref()),
        true,
    )
    .map_err(refused)?;
    let sealed = EncString::encrypt(&user_key.to_bytes(), &key_from(&secret));
    Ok(format!("{PREFIX}{}:{sealed}", B64.encode(kept)))
}

/// Unlocking: the system asks the person, and the sealed copy opens.
#[cfg(not(any(mobile, target_os = "macos")))]
pub(crate) fn open(
    account_id: &str,
    stored: &str,
    _prompt: &Prompt,
) -> Result<SymmetricKey, Failure> {
    let (kept, sealed) = split(stored).ok_or_else(|| {
        Failure::new(
            "hello",
            format!("Windows Hello's copy is from an older UwULock and no longer used. {AGAIN}"),
        )
    })?;
    let random = B64
        .decode(kept)
        .map_err(|e| e.to_string())
        .and_then(|kept| platform::unprotect(&kept))
        .map_err(|error| {
            Failure::new(
                "hello",
                format!("Windows couldn't open Hello's challenge ({error}). {AGAIN}"),
            )
        })?;
    let secret = platform::secret(&name(account_id), &challenge(account_id, &random), false)
        .map_err(|message| Failure::new("hello", message))?;
    opened(sealed, &secret)
}

#[cfg(not(any(mobile, target_os = "macos")))]
pub(crate) fn forget(account_id: &str) {
    platform::delete(&name(account_id));
}

/// The sentence and button of the Touch ID sheet.
#[cfg(target_os = "macos")]
fn mac_prompt(prompt: &Prompt) -> (String, String) {
    let or = |text: &str, fallback: &str| {
        let text = text.trim();
        if text.is_empty() {
            fallback.to_string()
        } else {
            text.to_string()
        }
    };
    (
        or(&prompt.reason, "unlock your vault"),
        or(&prompt.cancel, "Master password"),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn seal(
    account_id: &str,
    user_key: &SymmetricKey,
    prompt: &Prompt,
) -> Result<String, Failure> {
    let (reason, cancel) = mac_prompt(prompt);
    let secret = touch::create(&name(account_id), &reason, &cancel)?;
    let sealed = EncString::encrypt(&user_key.to_bytes(), &key_from(&secret));
    Ok(format!("{MAC}{sealed}"))
}

#[cfg(target_os = "macos")]
pub(crate) fn open(
    account_id: &str,
    stored: &str,
    prompt: &Prompt,
) -> Result<SymmetricKey, Failure> {
    let sealed = stored.strip_prefix(MAC).ok_or_else(|| {
        Failure::new(
            "biometric-changed",
            format!("No copy for this Mac. {AGAIN}"),
        )
    })?;
    let (reason, cancel) = mac_prompt(prompt);
    let secret = touch::read(&name(account_id), &reason, &cancel)?;
    opened(sealed, &secret)
}

#[cfg(target_os = "macos")]
pub(crate) fn forget(account_id: &str) {
    touch::delete(&name(account_id), false);
}

#[cfg(mobile)]
fn phone_failure(error: tauri_plugin_uwulock_mobile::Error) -> Failure {
    let kind = match error.code.as_deref() {
        Some("cancelled") => "biometric-cancelled",
        Some("lockout") => "biometric-lockout",
        Some("invalidated") | Some("missing") => "biometric-changed",
        _ => "hello",
    };
    Failure::new(kind, error.message)
}

#[cfg(mobile)]
fn phone() -> Result<tauri::State<'static, tauri_plugin_uwulock_mobile::Mobile<tauri::Wry>>, Failure>
{
    crate::phone::plugin().ok_or_else(|| Failure::new("hello", "The phone isn't ready yet."))
}

#[cfg(mobile)]
fn phone_prompt(prompt: &Prompt) -> tauri_plugin_uwulock_mobile::Prompt {
    let or = |text: &str, fallback: &str| {
        if text.trim().is_empty() {
            fallback.to_string()
        } else {
            text.to_string()
        }
    };
    tauri_plugin_uwulock_mobile::Prompt {
        title: or(&prompt.title, "UwULock"),
        subtitle: prompt.subtitle.clone(),
        cancel: or(&prompt.cancel, "Master password"),
    }
}

#[cfg(mobile)]
pub(crate) fn seal(
    account_id: &str,
    user_key: &SymmetricKey,
    prompt: &Prompt,
) -> Result<String, Failure> {
    let secret = phone()?
        .unlock_create(&name(account_id), &phone_prompt(prompt))
        .map_err(phone_failure)?;
    let sealed = EncString::encrypt(&user_key.to_bytes(), &key_from(&secret));
    Ok(format!("{PHONE}{sealed}"))
}

#[cfg(mobile)]
pub(crate) fn open(
    account_id: &str,
    stored: &str,
    prompt: &Prompt,
) -> Result<SymmetricKey, Failure> {
    let sealed = stored.strip_prefix(PHONE).ok_or_else(|| {
        Failure::new(
            "biometric-changed",
            format!("No copy for this phone. {AGAIN}"),
        )
    })?;
    let secret = phone()?
        .unlock_open(&name(account_id), &phone_prompt(prompt))
        .map_err(phone_failure)?;
    opened(sealed, &secret)
}

#[cfg(mobile)]
pub(crate) fn forget(account_id: &str) {
    if let Ok(plugin) = phone() {
        let _ = plugin.unlock_delete(&name(account_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_opens_only_with_the_same_signature() {
        let user_key = SymmetricKey::generate();
        let sealed = EncString::encrypt(&user_key.to_bytes(), &key_from(&[7; 32])).to_string();
        let opened = sealed
            .parse::<EncString>()
            .unwrap()
            .decrypt_key(&key_from(&[7; 32]))
            .unwrap();
        assert_eq!(opened.to_bytes(), user_key.to_bytes());
        assert!(sealed
            .parse::<EncString>()
            .unwrap()
            .decrypt_key(&key_from(&[8; 32]))
            .is_err());
        assert_ne!(challenge("a", &[1; 32]), challenge("b", &[1; 32]));
        assert_ne!(challenge("a", &[1; 32]), challenge("a", &[2; 32]));
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn copies_from_before_the_random_challenge_are_outdated() {
        let old = EncString::encrypt(&[1; 64], &key_from(&[7; 32])).to_string();
        assert!(is_outdated(&old));
        let new = format!("v2:AAAA:{old}");
        assert!(!is_outdated(&new));
        assert_eq!(split(&new), Some(("AAAA", old.as_str())));
        assert!(open("a", &old, &Prompt::default())
            .unwrap_err()
            .message()
            .contains("older UwULock"));
    }
}
