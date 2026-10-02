//! What the AutoFill extension on iOS and macOS reads and writes.
//!
//! The extension is its own process; the app's open vault isn't there. So
//! the app leaves the extension a **sealed list** of the vault's passkeys in
//! the shared App Group folder, and the extension leaves passkeys it made in
//! an **outbox** there, which the app takes into the vault at its next
//! unlock. Both are sealed with the *provider key*: 32 random bytes the app
//! keeps sealed under the user key in its account file, and in the shared
//! Keychain for the extension, readable only after Face ID, Touch ID or the
//! device passcode (`.userPresence`, this device only). Nothing on disk is
//! plain; see docs/passkeys.md.
//!
//! The seal is AES-256-GCM, CryptoKit's `AES.GCM.SealedBox.combined` behind
//! a version byte: `0x01 ‖ nonce (12) ‖ ciphertext ‖ tag (16)`, with
//! [`AAD`] as authenticated data.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use uwulock_core::passkey::Passkey;
use zeroize::Zeroizing;

use crate::webauthn::{b64, from_b64};

/// Authenticated with every seal: a list sealed for something else doesn't
/// open as this one.
pub const AAD: &[u8] = b"uwulock-passkeys-v1";
const VERSION: u8 = 1;

/// A new provider key.
pub fn new_key() -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.fill_bytes(key.as_mut());
    key
}

pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let sealed = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: AAD,
            },
        )
        .expect("AES-GCM seals anything this size");
    let mut out = Vec::with_capacity(1 + 12 + sealed.len());
    out.push(VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    out
}

pub fn open(key: &[u8; 32], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    if sealed.len() < 1 + 12 + 16 || sealed[0] != VERSION {
        return Err("not a sealed passkey list".into());
    }
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(
            Nonce::from_slice(&sealed[1..13]),
            Payload {
                msg: &sealed[13..],
                aad: AAD,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| "the passkey list doesn't open with this key".into())
}

/// One passkey as the extension sees it. All binary values in URL-safe
/// base64; the private key as the raw 32-byte P-256 scalar, which is what
/// CryptoKit's `P256.Signing.PrivateKey(rawRepresentation:)` takes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub credential_id: String,
    pub rp_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rp_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_handle: Option<String>,
    /// The login it is in; `None` in the outbox (a new login).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub private_key: String,
    #[serde(default)]
    pub created: String,
}

impl Drop for Entry {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.private_key.zeroize();
    }
}

/// The list the app leaves for the extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub entries: Vec<Entry>,
}

impl Entry {
    pub fn of(passkey: &Passkey, item_id: Option<&str>) -> Result<Entry, String> {
        let der = Zeroizing::new(from_b64(&passkey.key_value)?);
        let secret = p256::SecretKey::from_pkcs8_der(&der)
            .map_err(|_| "a passkey's private key doesn't parse".to_string())?;
        Ok(Entry {
            credential_id: b64(&passkey.credential_id_bytes().map_err(|e| e.to_string())?),
            rp_id: passkey.rp_id.clone(),
            rp_name: passkey.rp_name.clone(),
            user_name: passkey.user_name.clone(),
            user_display_name: passkey.user_display_name.clone(),
            user_handle: passkey.user_handle.clone(),
            item_id: item_id.map(str::to_string),
            private_key: b64(&secret.to_bytes()),
            created: passkey.creation_date.clone(),
        })
    }

    /// A passkey the extension made, as the vault keeps it.
    pub fn to_passkey(&self) -> Result<Passkey, String> {
        let raw = Zeroizing::new(from_b64(&self.private_key)?);
        let secret = p256::SecretKey::from_slice(&raw)
            .map_err(|_| "the extension's private key isn't a P-256 scalar".to_string())?;
        let der = secret.to_pkcs8_der().map_err(|e| format!("P-256: {e}"))?;
        let id = from_b64(&self.credential_id)?;
        if id.is_empty() || self.rp_id.is_empty() {
            return Err("a passkey from the extension without an id or a site".into());
        }
        Ok(Passkey {
            credential_id: uwulock_core::passkey::credential_id_from_bytes(&id),
            key_value: Zeroizing::new(b64(der.as_bytes())),
            rp_id: self.rp_id.clone(),
            rp_name: self.rp_name.clone(),
            user_handle: self
                .user_handle
                .as_deref()
                .map(|handle| from_b64(handle).map(|bytes| b64(&bytes)))
                .transpose()?,
            user_name: self.user_name.clone(),
            user_display_name: self.user_display_name.clone(),
            counter: 0,
            discoverable: true,
            creation_date: self.created.clone(),
        })
    }
}

impl Snapshot {
    pub fn seal(&self, key: &[u8; 32]) -> Vec<u8> {
        let json = Zeroizing::new(serde_json::to_vec(self).expect("a snapshot serialises"));
        seal(key, &json)
    }

    pub fn open(key: &[u8; 32], sealed: &[u8]) -> Result<Snapshot, String> {
        let json = open(key, sealed)?;
        serde_json::from_slice(&json).map_err(|e| format!("the passkey list doesn't read: {e}"))
    }
}

/// One outbox file: a passkey the extension made.
pub fn open_outbox(key: &[u8; 32], sealed: &[u8]) -> Result<Entry, String> {
    let json = open(key, sealed)?;
    serde_json::from_slice(&json).map_err(|e| format!("an outbox entry doesn't read: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uwulock_core::passkey::{UP, UV};

    fn passkey() -> Passkey {
        Passkey::generate(
            "example.com",
            Some("Example"),
            Some(b"user-1234"),
            Some("nyu@example.com"),
            None,
            true,
            "2026-10-02T10:00:00.000Z",
        )
        .unwrap()
    }

    #[test]
    fn seal_and_open() {
        let key = new_key();
        let sealed = seal(&key, b"hello");
        assert_eq!(sealed[0], 1);
        assert_eq!(sealed.len(), 1 + 12 + 5 + 16);
        assert_eq!(&open(&key, &sealed).unwrap()[..], b"hello");
        assert!(open(&new_key(), &sealed).is_err());
        let mut flipped = sealed.clone();
        flipped[20] ^= 1;
        assert!(open(&key, &flipped).is_err());
        assert!(open(&key, &sealed[..10]).is_err());
    }

    #[test]
    fn a_snapshot_round_trip_signs_the_same() {
        let key = new_key();
        let passkey = passkey();
        let snapshot = Snapshot {
            version: 1,
            entries: vec![Entry::of(&passkey, Some("item-1")).unwrap()],
        };
        let sealed = snapshot.seal(&key);
        // Nothing readable in the file.
        assert!(!sealed.windows(11).any(|w| w == b"example.com"));
        let opened = Snapshot::open(&key, &sealed).unwrap();
        let entry = &opened.entries[0];
        assert_eq!(entry.item_id.as_deref(), Some("item-1"));
        assert_eq!(from_b64(&entry.private_key).unwrap().len(), 32);

        // What the extension would make of it is the same passkey.
        let back = entry.to_passkey().unwrap();
        assert_eq!(back.credential_id, passkey.credential_id);
        assert_eq!(back.user_handle, passkey.user_handle);
        assert_eq!(
            back.public_key_cose().unwrap(),
            passkey.public_key_cose().unwrap()
        );
        let data = passkey.authenticator_data(UP | UV, false).unwrap();
        assert!(back.sign(&data, &[0; 32]).is_ok());
    }

    #[test]
    fn an_outbox_entry_from_swift() {
        // As the extension writes it: camelCase, no item id.
        let key = new_key();
        let entry = r#"{"credentialId":"AQIDBAUGBwgJCgsMDQ4PEA","rpId":"example.com",
            "userName":"nyu","userHandle":"dXNlcg","privateKey":"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
            "created":"2026-10-02T10:00:00Z"}"#;
        let sealed = seal(&key, entry.as_bytes());
        let entry = open_outbox(&key, &sealed).unwrap();
        let passkey = entry.to_passkey().unwrap();
        assert_eq!(
            passkey.credential_id,
            "01020304-0506-0708-090a-0b0c0d0e0f10"
        );
        assert_eq!(passkey.user_handle_bytes().unwrap().unwrap(), b"user");
        assert!(passkey.discoverable);
        assert!(passkey.public_key_spki().is_ok());
    }
}
