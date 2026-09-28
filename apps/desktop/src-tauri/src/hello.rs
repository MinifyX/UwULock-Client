//! Unlocking with Windows Hello.
//!
//! Windows Hello keeps a key pair per app and name in the TPM (or in
//! software, where there is none) and signs with it only after the person
//! showed their face, finger or PIN. UwULock asks it to sign a fixed challenge
//! for the account; the signature (RSA PKCS#1 v1.5, the same every time for
//! the same key and challenge) goes through SHA-256 and is stretched into a
//! key that seals a copy of the user key in the account's file. Without the
//! person at the computer there is no signature, and without it the copy
//! doesn't open. Nothing of it needs a signed app.
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
}

use std::sync::OnceLock;
use uwulock_bitwarden::{EncString, Error, SymmetricKey};

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

fn challenge(account_id: &str) -> Vec<u8> {
    format!("uwulock-hello-v1:{account_id}").into_bytes()
}

/// The key a Windows Hello signature stretches into.
fn key_from(secret: &[u8; 32]) -> SymmetricKey {
    SymmetricKey::stretch(secret)
}

/// Switching it on: a new Hello key for the account, and the user key sealed
/// under what it signs. Blocks while Windows asks the person.
pub(crate) fn seal(account_id: &str, user_key: &SymmetricKey) -> Result<String, String> {
    let secret = platform::secret(&name(account_id), &challenge(account_id), true)?;
    Ok(EncString::encrypt(&user_key.to_bytes(), &key_from(&secret)).to_string())
}

/// Unlocking: Windows asks the person, and the sealed copy opens.
pub(crate) fn open(account_id: &str, sealed: &str) -> Result<SymmetricKey, String> {
    let secret = platform::secret(&name(account_id), &challenge(account_id), false)?;
    sealed
        .parse::<EncString>()
        .and_then(|e| e.decrypt_key(&key_from(&secret)))
        .map_err(|error| match error {
            Error::WrongKey => "Windows Hello's key has changed. Unlock with the master \
                                password and switch Windows Hello on again."
                .to_string(),
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
        assert_ne!(challenge("a"), challenge("b"));
    }
}
