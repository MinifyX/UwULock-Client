//! What the AutoFill extension on iOS and macOS reads and writes.
//!
//! The extension is its own process; the app's open vault isn't there. So
//! the app leaves the extension a **sealed list** of one account's passkeys
//! in the shared App Group folder, and the extension leaves passkeys it made
//! in an **outbox** there, which the app takes into that account's vault at
//! its next unlock. Both are sealed with that account's *provider key*: 32
//! random bytes the app keeps sealed under the account's user key, and in
//! the shared Keychain for the extension (one slot: the key of the account
//! whose list is there), readable only after Face ID, Touch ID or the device
//! passcode (`.userPresence`, this device only). Nothing on disk is plain;
//! see docs/passkeys.md.
//!
//! A sealed file is AES-256-GCM behind a small header naming the account:
//!
//! ```text
//! 0x02 ‖ n (1 byte) ‖ account id (n bytes) ‖ nonce (12) ‖ ciphertext ‖ tag (16)
//! ```
//!
//! — the tail is CryptoKit's `AES.GCM.SealedBox.combined`. The authenticated
//! data is `uwulock-passkeys-v2:<kind>:<account id>`, with the kind `list`
//! or `outbox`: a list doesn't open as an outbox entry, nor one account's
//! file as another's. In the list each passkey's private key is sealed once
//! more on its own (`…:key:<account id>:<credential id>`), so the extension
//! only ever opens the one it signs with.
//!
//! The list carries a `generation` (milliseconds, only ever going up): the
//! extension remembers the highest it has seen per account and refuses an
//! older list put back in its place. Every outbox entry has the credential
//! id the app remembers once it took it in, so an old outbox file put back
//! doesn't come in twice.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uwulock_core::passkey::Passkey;
use zeroize::{Zeroize, Zeroizing};

use crate::webauthn::{b64, from_b64};

const VERSION: u8 = 2;
const DOMAIN: &str = "uwulock-passkeys-v2";
/// What [`Snapshot::version`] says.
pub const LIST_VERSION: u32 = 2;

/// What a sealed file is: authenticated with it, so one never opens as the
/// other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The list the app leaves the extension.
    List,
    /// A passkey the extension made, for the app.
    Outbox,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::List => "list",
            Kind::Outbox => "outbox",
        }
    }
}

/// A new provider key.
pub fn new_key() -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.fill_bytes(key.as_mut());
    key
}

/// Names a provider key without giving it away: the label of the Keychain
/// item, so the app can tell whether the one there is the account's
/// without reading it (which would take Face ID). Hex of the first 8 bytes
/// of SHA-256(`uwulock-provider-key-id-v1` ‖ key); Passkeys.swift does the
/// same.
pub fn key_id(key: &[u8; 32]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"uwulock-provider-key-id-v1");
    hash.update(key);
    hash.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// An account id as it may stand in a header: what UwULock makes (UUIDs),
/// nothing that could be a path.
pub fn valid_account(account: &str) -> bool {
    !account.is_empty()
        && account.len() <= 64
        && account
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn aad(kind: Kind, account: &str) -> Vec<u8> {
    format!("{DOMAIN}:{}:{account}", kind.name()).into_bytes()
}

fn key_aad(account: &str, credential_id: &str) -> Vec<u8> {
    format!("{DOMAIN}:key:{account}:{credential_id}").into_bytes()
}

fn encrypt(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let sealed = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("AES-GCM seals anything this size");
    let mut out = Vec::with_capacity(12 + sealed.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    out
}

fn decrypt(key: &[u8; 32], aad: &[u8], sealed: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    if sealed.len() < 12 + 16 {
        return None;
    }
    Aes256Gcm::new(key.into())
        .decrypt(
            Nonce::from_slice(&sealed[..12]),
            Payload {
                msg: &sealed[12..],
                aad,
            },
        )
        .ok()
        .map(Zeroizing::new)
}

/// Seals `plaintext` as a `kind` file of `account`.
pub fn seal(
    key: &[u8; 32],
    kind: Kind,
    account: &str,
    plaintext: &[u8],
) -> Result<Vec<u8>, String> {
    if !valid_account(account) {
        return Err("not an account id".into());
    }
    let body = encrypt(key, &aad(kind, account), plaintext);
    let mut out = Vec::with_capacity(2 + account.len() + body.len());
    out.push(VERSION);
    out.push(account.len() as u8);
    out.extend_from_slice(account.as_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// The account a sealed file says it belongs to — before opening it, to pick
/// the key. Only [`open`] proves it.
pub fn account_of(sealed: &[u8]) -> Result<&str, String> {
    let (&version, rest) = sealed.split_first().ok_or("an empty file")?;
    if version != VERSION {
        return Err("not a sealed passkey file of this version".into());
    }
    let (&length, rest) = rest.split_first().ok_or("a cut-off file")?;
    let account = rest
        .get(..usize::from(length))
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .filter(|account| valid_account(account))
        .ok_or("the file names no account")?;
    Ok(account)
}

/// Opens a `kind` file: its account and what was sealed.
pub fn open(
    key: &[u8; 32],
    kind: Kind,
    sealed: &[u8],
) -> Result<(String, Zeroizing<Vec<u8>>), String> {
    let account = account_of(sealed)?.to_string();
    let body = &sealed[2 + account.len()..];
    let plain = decrypt(key, &aad(kind, &account), body)
        .ok_or("the passkey file doesn't open with this key")?;
    Ok((account, plain))
}

/// One passkey in the list. All binary values in URL-safe base64. The
/// private key (the raw 32-byte P-256 scalar, what CryptoKit's
/// `P256.Signing.PrivateKey(rawRepresentation:)` takes) is sealed on its own
/// in `sealed_key`: `nonce ‖ ciphertext ‖ tag`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListEntry {
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
    /// The login it is in; `None` for one the extension made since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub sealed_key: String,
    #[serde(default)]
    pub created: String,
}

impl ListEntry {
    /// A vault passkey for the list of `account`.
    pub fn of(
        key: &[u8; 32],
        account: &str,
        passkey: &Passkey,
        item_id: Option<&str>,
    ) -> Result<ListEntry, String> {
        let der = Zeroizing::new(from_b64(&passkey.key_value)?);
        let secret = p256::SecretKey::from_pkcs8_der(&der)
            .map_err(|_| "a passkey's private key doesn't parse".to_string())?;
        let mut raw = secret.to_bytes();
        let credential_id = b64(&passkey.credential_id_bytes().map_err(|e| e.to_string())?);
        let sealed_key = b64(&encrypt(key, &key_aad(account, &credential_id), &raw));
        raw.zeroize();
        Ok(ListEntry {
            credential_id,
            rp_id: passkey.rp_id.clone(),
            rp_name: passkey.rp_name.clone(),
            user_name: passkey.user_name.clone(),
            user_display_name: passkey.user_display_name.clone(),
            user_handle: passkey.user_handle.clone(),
            item_id: item_id.map(str::to_string),
            sealed_key,
            created: passkey.creation_date.clone(),
        })
    }

    /// The raw P-256 scalar, as the extension opens it to sign.
    pub fn private_key(&self, key: &[u8; 32], account: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let sealed = from_b64(&self.sealed_key)?;
        decrypt(key, &key_aad(account, &self.credential_id), &sealed)
            .ok_or_else(|| "a passkey's key doesn't open".to_string())
    }
}

/// One outbox file: a passkey the extension made, the private key as the
/// raw P-256 scalar in URL-safe base64.
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
    pub private_key: String,
    #[serde(default)]
    pub created: String,
    /// The generation of the list the extension wrote with it: the app's
    /// next list counts on from there.
    #[serde(default)]
    pub generation: u64,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}

impl Entry {
    /// A passkey the extension made, as the vault keeps it. Its rpId has to
    /// be one UwULock takes ([`crate::rpid::valid`]).
    pub fn to_passkey(&self) -> Result<Passkey, String> {
        if !crate::rpid::valid(&self.rp_id) {
            return Err("a passkey from the extension for something that isn't a site".into());
        }
        let raw = Zeroizing::new(from_b64(&self.private_key)?);
        let secret = p256::SecretKey::from_slice(&raw)
            .map_err(|_| "the extension's private key isn't a P-256 scalar".to_string())?;
        let der = secret.to_pkcs8_der().map_err(|e| format!("P-256: {e}"))?;
        let id = from_b64(&self.credential_id)?;
        if id.is_empty() {
            return Err("a passkey from the extension without an id".into());
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

fn password_aad(account: &str, item_id: &str) -> Vec<u8> {
    format!("{DOMAIN}:password:{account}:{item_id}").into_bytes()
}

/// One login for password AutoFill. Its password is sealed on its own
/// (`…:password:<account id>:<item id>`), like a passkey's private key: the
/// extension opens only the one the person picked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoginEntry {
    pub item_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    /// How the extension matches the login's addresses (no regular
    /// expressions; [`crate::autofill::uri_hint`]).
    #[serde(default)]
    pub uris: Vec<crate::autofill::UriHint>,
    /// Shown under the name: the first address's host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub sealed_password: String,
}

impl LoginEntry {
    /// A vault login for the list of `account`; `None` when it has no
    /// password.
    pub fn of(
        key: &[u8; 32],
        account: &str,
        item: &uwulock_core::vault::Item,
    ) -> Option<LoginEntry> {
        let login = item.login.as_ref()?;
        let password = login.password.as_ref().filter(|p| !p.is_empty())?;
        let uris = login
            .uris
            .iter()
            .filter_map(|u| crate::autofill::uri_hint(&u.uri, u.match_kind))
            .collect();
        let subtitle = login
            .uris
            .iter()
            .find_map(|u| crate::autofill::Target::web(&u.uri).host)
            .map(|h| h.strip_prefix("www.").unwrap_or(&h).to_string());
        Some(LoginEntry {
            item_id: item.id.clone(),
            name: item.name.to_string(),
            user_name: login
                .username
                .as_ref()
                .filter(|u| !u.is_empty())
                .map(|u| u.to_string()),
            uris,
            subtitle,
            sealed_password: b64(&encrypt(
                key,
                &password_aad(account, &item.id),
                password.as_bytes(),
            )),
        })
    }

    /// The password, as the extension opens it to fill.
    pub fn password(&self, key: &[u8; 32], account: &str) -> Result<Zeroizing<String>, String> {
        let sealed = from_b64(&self.sealed_password)?;
        let plain = decrypt(key, &password_aad(account, &self.item_id), &sealed)
            .ok_or_else(|| "a password doesn't open".to_string())?;
        String::from_utf8(plain.to_vec())
            .map(Zeroizing::new)
            .map_err(|_| "a password isn't text".to_string())
    }
}

/// Whether a login goes into the list for password AutoFill: a login with a
/// password, not in the trash or archived, and not marked "ask for the master
/// password again" — the extension only has Face ID, Touch ID or the device
/// passcode.
pub fn listed_login(item: &uwulock_core::vault::Item) -> bool {
    item.kind == uwulock_core::vault::ItemKind::Login
        && !item.deleted
        && !item.reprompt
        && item.archived_date.is_none()
        && item
            .login
            .as_ref()
            .and_then(|l| l.password.as_ref())
            .is_some_and(|p| !p.is_empty())
}

/// The list the app leaves for the extension: one account's passkeys and
/// logins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    /// Only ever goes up; the extension refuses a list older than one it saw.
    pub generation: u64,
    pub entries: Vec<ListEntry>,
    /// Logins with a password, for password AutoFill. Older extensions skip
    /// the field.
    #[serde(default)]
    pub logins: Vec<LoginEntry>,
    /// From the header, not the JSON.
    #[serde(skip)]
    pub account: String,
}

impl Snapshot {
    pub fn new(account: &str, generation: u64) -> Snapshot {
        Snapshot {
            version: LIST_VERSION,
            generation,
            entries: Vec::new(),
            logins: Vec::new(),
            account: account.to_string(),
        }
    }

    /// Adds a vault passkey, its private key sealed on its own.
    pub fn push(
        &mut self,
        key: &[u8; 32],
        passkey: &Passkey,
        item_id: Option<&str>,
    ) -> Result<&ListEntry, String> {
        let entry = ListEntry::of(key, &self.account, passkey, item_id)?;
        self.entries.push(entry);
        Ok(self.entries.last().expect("just pushed"))
    }

    /// Adds a login for password AutoFill, its password sealed on its own;
    /// `None` when it has no password.
    pub fn push_login(
        &mut self,
        key: &[u8; 32],
        item: &uwulock_core::vault::Item,
    ) -> Option<&LoginEntry> {
        let entry = LoginEntry::of(key, &self.account, item)?;
        self.logins.push(entry);
        self.logins.last()
    }

    pub fn seal(&self, key: &[u8; 32]) -> Result<Vec<u8>, String> {
        let json = Zeroizing::new(serde_json::to_vec(self).expect("a snapshot serialises"));
        seal(key, Kind::List, &self.account, &json)
    }

    pub fn open(key: &[u8; 32], sealed: &[u8]) -> Result<Snapshot, String> {
        let (account, json) = open(key, Kind::List, sealed)?;
        let mut snapshot: Snapshot = serde_json::from_slice(&json)
            .map_err(|e| format!("the passkey list doesn't read: {e}"))?;
        snapshot.account = account;
        Ok(snapshot)
    }
}

/// Whether `item`'s passkeys go into the extension's list (and the
/// system's credential identity store) at all: not in the trash, and not
/// marked "ask for the master password again". The extension only has Face
/// ID, Touch ID or the device passcode, so such a login stays with the app
/// and the browser extension, which ask for the master password like for
/// any other secret of it.
pub fn listed(item: &uwulock_core::vault::Item) -> bool {
    !item.deleted && !item.reprompt && item.login.as_ref().is_some_and(|l| l.passkey_count() > 0)
}

/// Whether one passkey goes into the list: the extension can't count up a
/// signature counter in the vault, so passkeys that use one stay with the
/// app and the browser extension.
pub fn listed_passkey(passkey: &Passkey) -> bool {
    passkey.counter == 0
}

/// The next generation of a list: later than `last` and, normally, the
/// time in milliseconds — so it keeps going up even when the clock doesn't.
pub fn next_generation(last: u64, now_ms: u64) -> u64 {
    now_ms.max(last.saturating_add(1))
}

/// One outbox file: its account and the passkey in it.
pub fn open_outbox(key: &[u8; 32], sealed: &[u8]) -> Result<(String, Entry), String> {
    let (account, json) = open(key, Kind::Outbox, sealed)?;
    let entry =
        serde_json::from_slice(&json).map_err(|e| format!("an outbox entry doesn't read: {e}"))?;
    Ok((account, entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uwulock_core::passkey::{UP, UV};

    const ACCOUNT: &str = "0b5c3a7e-4f1d-4c2a-9e8b-1d2c3b4a5f60";

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
    fn reprompt_logins_stay_out_of_the_list() {
        use uwulock_core::vault::{Item, ItemKind};
        let mut item = Item::new(ItemKind::Login);
        assert!(!listed(&item), "no passkey");
        item.login.as_mut().unwrap().passkeys = Some(vec![serde_json::json!({})]);
        assert!(listed(&item));
        item.reprompt = true;
        assert!(!listed(&item), "asks for the master password");
        item.reprompt = false;
        item.deleted = true;
        assert!(!listed(&item), "in the trash");
        let mut counting = passkey();
        assert!(listed_passkey(&counting));
        counting.counter = 3;
        assert!(!listed_passkey(&counting));
    }

    #[test]
    fn seal_and_open() {
        let key = new_key();
        let sealed = seal(&key, Kind::List, ACCOUNT, b"hello").unwrap();
        assert_eq!(sealed[0], 2);
        assert_eq!(sealed.len(), 2 + ACCOUNT.len() + 12 + 5 + 16);
        assert_eq!(account_of(&sealed).unwrap(), ACCOUNT);
        let (account, plain) = open(&key, Kind::List, &sealed).unwrap();
        assert_eq!(account, ACCOUNT);
        assert_eq!(&plain[..], b"hello");
        assert!(open(&new_key(), Kind::List, &sealed).is_err());
        let mut flipped = sealed.clone();
        flipped[50] ^= 1;
        assert!(open(&key, Kind::List, &flipped).is_err());
        assert!(open(&key, Kind::List, &sealed[..10]).is_err());
        assert!(open(&key, Kind::List, &[]).is_err());
    }

    #[test]
    fn a_list_isnt_an_outbox_entry_nor_another_accounts() {
        let key = new_key();
        let list = seal(&key, Kind::List, ACCOUNT, b"{}").unwrap();
        assert!(open(&key, Kind::Outbox, &list).is_err());
        let outbox = seal(&key, Kind::Outbox, ACCOUNT, b"{}").unwrap();
        assert!(open(&key, Kind::List, &outbox).is_err());

        // The same key under another account's name: the header is
        // authenticated, so swapping it doesn't open.
        let other = "a".repeat(ACCOUNT.len());
        let mut swapped = list.clone();
        swapped[2..2 + ACCOUNT.len()].copy_from_slice(other.as_bytes());
        assert_eq!(account_of(&swapped).unwrap(), other);
        assert!(open(&key, Kind::List, &swapped).is_err());
    }

    #[test]
    fn account_ids_in_headers() {
        assert!(valid_account(ACCOUNT));
        for bad in ["", "../x", "a/b", "a b", &"a".repeat(65)] {
            assert!(!valid_account(bad), "{bad}");
            assert!(seal(&new_key(), Kind::List, bad, b"x").is_err());
        }
        assert!(account_of(&[2, 40, b'a']).is_err());
        assert!(account_of(&[1, 1, b'a']).is_err());
    }

    #[test]
    fn key_ids() {
        let key = new_key();
        assert_eq!(key_id(&key).len(), 16);
        assert_eq!(key_id(&key), key_id(&key));
        assert_ne!(key_id(&key), key_id(&new_key()));
        // As Passkeys.swift computes it.
        assert_eq!(key_id(&[0; 32]), {
            let mut hash = Sha256::new();
            hash.update(b"uwulock-provider-key-id-v1");
            hash.update([0u8; 32]);
            hash.finalize()[..8]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        });
    }

    #[test]
    fn a_snapshot_round_trip_signs_the_same() {
        let key = new_key();
        let passkey = passkey();
        let mut snapshot = Snapshot::new(ACCOUNT, 7);
        snapshot.push(&key, &passkey, Some("item-1")).unwrap();
        let sealed = snapshot.seal(&key).unwrap();
        // Nothing readable in the file.
        assert!(!sealed.windows(11).any(|w| w == b"example.com"));
        let opened = Snapshot::open(&key, &sealed).unwrap();
        assert_eq!(opened.account, ACCOUNT);
        assert_eq!(opened.generation, 7);
        assert_eq!(opened.version, LIST_VERSION);
        let entry = &opened.entries[0];
        assert_eq!(entry.item_id.as_deref(), Some("item-1"));
        // The private key isn't in the list's JSON as such.
        let json = serde_json::to_string(entry).unwrap();
        assert!(!json.contains("privateKey"));
        let raw = entry.private_key(&key, ACCOUNT).unwrap();
        assert_eq!(raw.len(), 32);
        // Bound to its account and credential id.
        assert!(entry.private_key(&key, "someone-else").is_err());
        let mut moved = entry.clone();
        moved.credential_id = "AQID".into();
        assert!(moved.private_key(&key, ACCOUNT).is_err());

        // What the extension would sign with is the same passkey.
        let back = Entry {
            credential_id: entry.credential_id.clone(),
            rp_id: entry.rp_id.clone(),
            rp_name: None,
            user_name: None,
            user_display_name: None,
            user_handle: entry.user_handle.clone(),
            private_key: b64(&raw),
            created: entry.created.clone(),
            generation: 0,
        }
        .to_passkey()
        .unwrap();
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
            "created":"2026-10-02T10:00:00Z","generation":1759399200000}"#;
        let sealed = seal(&key, Kind::Outbox, ACCOUNT, entry.as_bytes()).unwrap();
        let (account, entry) = open_outbox(&key, &sealed).unwrap();
        assert_eq!(account, ACCOUNT);
        assert_eq!(entry.generation, 1_759_399_200_000);
        let passkey = entry.to_passkey().unwrap();
        assert_eq!(
            passkey.credential_id,
            "01020304-0506-0708-090a-0b0c0d0e0f10"
        );
        assert_eq!(passkey.user_handle_bytes().unwrap().unwrap(), b"user");
        assert!(passkey.discoverable);
        assert!(passkey.public_key_spki().is_ok());
    }

    #[test]
    fn an_outbox_entry_for_no_site_doesnt_come_in() {
        let mut entry: Entry = serde_json::from_str(
            r#"{"credentialId":"AQIDBAUGBwgJCgsMDQ4PEA","rpId":"com",
            "privateKey":"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc"}"#,
        )
        .unwrap();
        assert!(entry.to_passkey().is_err());
        entry.rp_id = "bank.example@evil.example".into();
        assert!(entry.to_passkey().is_err());
        entry.rp_id = "login.example.com".into();
        assert!(entry.to_passkey().is_ok());
    }

    #[test]
    fn logins_for_password_autofill() {
        use uwulock_core::vault::{Item, ItemKind, LoginUri};
        let key = new_key();
        let mut item = Item::new(ItemKind::Login);
        item.id = "item-7".into();
        item.name = "Example".to_string().into();
        assert!(!listed_login(&item), "no password");
        {
            let login = item.login.as_mut().unwrap();
            login.username = Some("nyu@example.com".to_string().into());
            login.password = Some("hunter2-but-longer".to_string().into());
            login.uris = vec![
                LoginUri {
                    uri: "https://login.example.com/x".to_string().into(),
                    match_kind: None,
                    checksum: None,
                },
                LoginUri {
                    uri: "^https://".to_string().into(),
                    match_kind: Some(4),
                    checksum: None,
                },
            ];
        }
        assert!(listed_login(&item));
        let mut snapshot = Snapshot::new(ACCOUNT, 1);
        let entry = snapshot.push_login(&key, &item).unwrap().clone();
        assert_eq!(entry.user_name.as_deref(), Some("nyu@example.com"));
        assert_eq!(entry.subtitle.as_deref(), Some("login.example.com"));
        assert_eq!(entry.uris.len(), 1, "no regular expression");
        assert_eq!(entry.uris[0].value, "example.com");
        let sealed = snapshot.seal(&key).unwrap();
        assert!(!sealed.windows(6).any(|w| w == b"hunter"));
        let opened = Snapshot::open(&key, &sealed).unwrap();
        let json = serde_json::to_string(&opened.logins[0]).unwrap();
        assert!(
            !json.contains("hunter2"),
            "the password is sealed in the list too"
        );
        assert_eq!(
            opened.logins[0].password(&key, ACCOUNT).unwrap().as_str(),
            "hunter2-but-longer"
        );
        // Bound to its account and item.
        assert!(opened.logins[0].password(&key, "someone-else").is_err());
        let mut moved = opened.logins[0].clone();
        moved.item_id = "item-8".into();
        assert!(moved.password(&key, ACCOUNT).is_err());
        // An old list without logins still reads.
        let old = seal(
            &key,
            Kind::List,
            ACCOUNT,
            br#"{"version":2,"generation":3,"entries":[]}"#,
        )
        .unwrap();
        assert!(Snapshot::open(&key, &old).unwrap().logins.is_empty());

        item.reprompt = true;
        assert!(!listed_login(&item), "asks for the master password");
        item.reprompt = false;
        item.archived_date = Some("2026-10-01T00:00:00Z".into());
        assert!(!listed_login(&item), "archived");
    }

    #[test]
    fn generations_only_go_up() {
        assert_eq!(next_generation(0, 1000), 1000);
        assert_eq!(next_generation(1000, 1000), 1001);
        // The clock went back: still later than the last.
        assert_eq!(next_generation(5000, 1000), 5001);
        assert_eq!(next_generation(u64::MAX, 1), u64::MAX);
    }
}
