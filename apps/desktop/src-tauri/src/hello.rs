//! Unlocking with Windows Hello.
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
//! Touch ID is not offered: its keychain items that only open with a finger
//! need the app signed with an Apple Developer ID and a keychain entitlement.
//! A Touch ID prompt alone, with the key kept elsewhere on disk, would be a
//! door in front of an open window. Linux has no common equivalent.

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

#[cfg(not(windows))]
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

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use rand::RngCore as _;
use std::sync::OnceLock;
use uwulock_bitwarden::{EncString, Error, SymmetricKey};
use zeroize::Zeroizing;

static AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Whether this computer has Windows Hello set up. Asked once, in the back
/// (the answer can take a moment); `false` until then.
pub(crate) fn available() -> bool {
    AVAILABLE.get().copied().unwrap_or(false)
}

pub(crate) fn probe() {
    if cfg!(windows) {
        std::thread::spawn(|| {
            let _ = AVAILABLE.set(platform::available());
        });
    } else {
        let _ = AVAILABLE.set(false);
    }
}

fn name(account_id: &str) -> String {
    format!("UwULock-{account_id}")
}

/// What Hello signs: the account and the enrolment's random bytes.
fn challenge(account_id: &str, random: &[u8]) -> Vec<u8> {
    let mut out = format!("uwulock-hello-v2:{account_id}:").into_bytes();
    out.extend_from_slice(random);
    out
}

/// The key a Windows Hello signature stretches into.
fn key_from(secret: &[u8; 32]) -> SymmetricKey {
    SymmetricKey::stretch(secret)
}

/// How the copy is kept: `v2:<the random bytes under DPAPI, base64>:<the
/// user key under the signature's key>`.
const PREFIX: &str = "v2:";

fn split(stored: &str) -> Option<(&str, &str)> {
    stored.strip_prefix(PREFIX)?.split_once(':')
}

/// A copy made before 0.3.0-beta.2, with a fixed challenge.
pub(crate) fn is_outdated(stored: &str) -> bool {
    split(stored).is_none()
}

/// Switching it on: a new Hello key for the account, a new random challenge,
/// and the user key sealed under what Hello signs. Blocks while Windows asks
/// the person.
pub(crate) fn seal(account_id: &str, user_key: &SymmetricKey) -> Result<String, String> {
    let mut random = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.fill_bytes(random.as_mut());
    let kept = platform::protect(random.as_ref())?;
    let secret = platform::secret(
        &name(account_id),
        &challenge(account_id, random.as_ref()),
        true,
    )?;
    let sealed = EncString::encrypt(&user_key.to_bytes(), &key_from(&secret));
    Ok(format!("{PREFIX}{}:{sealed}", B64.encode(kept)))
}

/// Unlocking: Windows asks the person, and the sealed copy opens.
pub(crate) fn open(account_id: &str, stored: &str) -> Result<SymmetricKey, String> {
    let again = "Unlock with the master password and switch Windows Hello on again.";
    let (kept, sealed) = split(stored).ok_or_else(|| {
        format!("Windows Hello's copy is from an older UwULock and no longer used. {again}")
    })?;
    let random = B64
        .decode(kept)
        .map_err(|e| e.to_string())
        .and_then(|kept| platform::unprotect(&kept))
        .map_err(|error| format!("Windows couldn't open Hello's challenge ({error}). {again}"))?;
    let secret = platform::secret(&name(account_id), &challenge(account_id, &random), false)?;
    sealed
        .parse::<EncString>()
        .and_then(|e| e.decrypt_key(&key_from(&secret)))
        .map_err(|error| match error {
            Error::WrongKey => format!("Windows Hello's key has changed. {again}"),
            other => other.to_string(),
        })
}

pub(crate) fn forget(account_id: &str) {
    platform::delete(&name(account_id));
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
    fn copies_from_before_the_random_challenge_are_outdated() {
        let old = EncString::encrypt(&[1; 64], &key_from(&[7; 32])).to_string();
        assert!(is_outdated(&old));
        let new = format!("v2:AAAA:{old}");
        assert!(!is_outdated(&new));
        assert_eq!(split(&new), Some(("AAAA", old.as_str())));
        assert!(open("a", &old).unwrap_err().contains("older UwULock"));
    }
}
