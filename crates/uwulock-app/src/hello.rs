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
//!
//! The keys themselves are the host's ([`Host::biometric_secret`]): Windows
//! Hello, Touch ID's Keychain item and the phone's plugin live with the app
//! around this crate. What is sealed with them, and how it is kept, is here.

#[cfg(not(any(mobile, target_os = "macos")))]
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use parking_lot::RwLock;
#[cfg(not(any(mobile, target_os = "macos")))]
use rand::RngCore as _;
use serde::Deserialize;
use uwulock_bitwarden::{EncString, Error, SymmetricKey};
#[cfg(not(any(mobile, target_os = "macos")))]
use zeroize::Zeroizing;

use crate::host::Host;
use crate::vault::Failure;
use crate::App;

/// What this device unlocks with, once asked: `windowsHello`, or a phone's
/// `fingerprint`, `face`, `iris`, `faceId`, `touchId`, `opticId` or
/// `biometric`. `None` until asked, and where there is nothing.
static KIND: RwLock<Option<String>> = RwLock::new(None);

/// The words of a phone's biometric dialog, in the page's language, and on a
/// Mac what completes "UwULock is trying to …" (`reason`). Windows Hello
/// brings its own.
#[derive(Debug, Clone, Default, Deserialize)]
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

impl Prompt {
    /// The words with what the page left empty filled in: the phone's title
    /// and the way back, and on a Mac the end of macOS's sentence.
    fn filled(&self) -> Prompt {
        let or = |text: &str, fallback: &str| {
            if text.trim().is_empty() {
                fallback.to_string()
            } else if cfg!(target_os = "macos") {
                text.trim().to_string()
            } else {
                text.to_string()
            }
        };
        Prompt {
            title: or(&self.title, "UwULock"),
            subtitle: self.subtitle.clone(),
            cancel: or(&self.cancel, "Master password"),
            reason: or(&self.reason, "unlock your vault"),
        }
    }
}

/// Whether this device can unlock without the master password. `false`
/// until [`probe`] has an answer (it can take a moment).
pub(crate) fn available() -> bool {
    KIND.read().is_some()
}

pub(crate) fn kind() -> Option<String> {
    KIND.read().clone()
}

/// Asks the host, in the back, and tells the page when the answer changed.
/// On a phone again whenever UwULock comes back to the screen: a finger may
/// have been enrolled meanwhile.
pub fn probe(app: &App) {
    let app = app.clone();
    std::thread::spawn(move || {
        let kind = app.host().biometric_kind();
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

/// The name the device keeps the key of an account under.
pub fn name(account_id: &str) -> String {
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
#[cfg_attr(not(mobile), allow(dead_code))]
const PHONE: &str = "m1:";
/// How a Mac's copy is kept: `t1:<the user key under Touch ID's key>`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
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

/// What a failure that only switching it on again mends ends with.
pub const AGAIN: &str = "Unlock with the master password and switch it on again.";

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
    host: &dyn Host,
    account_id: &str,
    user_key: &SymmetricKey,
    prompt: &Prompt,
) -> Result<String, Failure> {
    let mut random = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.fill_bytes(random.as_mut());
    let kept = crate::dpapi::protect(random.as_ref()).map_err(|e| Failure::new("hello", e))?;
    let secret = host.biometric_secret(
        &name(account_id),
        &challenge(account_id, random.as_ref()),
        true,
        &prompt.filled(),
    )?;
    let sealed = EncString::encrypt(&user_key.to_bytes(), &key_from(&secret));
    Ok(format!("{PREFIX}{}:{sealed}", B64.encode(kept)))
}

/// Unlocking: the system asks the person, and the sealed copy opens.
#[cfg(not(any(mobile, target_os = "macos")))]
pub(crate) fn open(
    host: &dyn Host,
    account_id: &str,
    stored: &str,
    prompt: &Prompt,
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
        .and_then(|kept| crate::dpapi::unprotect(&kept))
        .map_err(|error| {
            Failure::new(
                "hello",
                format!("Windows couldn't open Hello's challenge ({error}). {AGAIN}"),
            )
        })?;
    let secret = host.biometric_secret(
        &name(account_id),
        &challenge(account_id, &random),
        false,
        &prompt.filled(),
    )?;
    opened(sealed, &secret)
}

/// Switching it on: a new secret on the device (Touch ID's Keychain item, the
/// phone's Keystore or Keychain), and the user key sealed under it.
#[cfg(any(mobile, target_os = "macos"))]
pub(crate) fn seal(
    host: &dyn Host,
    account_id: &str,
    user_key: &SymmetricKey,
    prompt: &Prompt,
) -> Result<String, Failure> {
    let secret = host.biometric_secret(&name(account_id), &[], true, &prompt.filled())?;
    let sealed = EncString::encrypt(&user_key.to_bytes(), &key_from(&secret));
    Ok(format!("{}{sealed}", local_prefix()))
}

/// Unlocking: the device hands out its secret after the person, and the
/// sealed copy opens.
#[cfg(any(mobile, target_os = "macos"))]
pub(crate) fn open(
    host: &dyn Host,
    account_id: &str,
    stored: &str,
    prompt: &Prompt,
) -> Result<SymmetricKey, Failure> {
    let device = if cfg!(mobile) { "phone" } else { "Mac" };
    let sealed = stored.strip_prefix(local_prefix()).ok_or_else(|| {
        Failure::new(
            "biometric-changed",
            format!("No copy for this {device}. {AGAIN}"),
        )
    })?;
    let secret = host.biometric_secret(&name(account_id), &[], false, &prompt.filled())?;
    opened(sealed, &secret)
}

#[cfg(any(mobile, target_os = "macos"))]
fn local_prefix() -> &'static str {
    if cfg!(mobile) {
        PHONE
    } else {
        MAC
    }
}

/// Switching it off: the device forgets the account's key.
pub(crate) fn forget(host: &dyn Host, account_id: &str) {
    host.biometric_forget(&name(account_id));
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
        assert!(open(&crate::host::NoHost, "a", &old, &Prompt::default())
            .unwrap_err()
            .message()
            .contains("older UwULock"));
    }
}
