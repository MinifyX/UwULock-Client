//! What Bitwarden and Vaultwarden send, as they send it — and what they take
//! back.
//!
//! Bitwarden answers in camelCase, older Vaultwardens in PascalCase, and a few
//! fields changed their case over the years. So every key is lowered first
//! ([`lowercase_keys`]) and the structs here name them in lower case. Every
//! encrypted field stays an encrypted string until [`crate::vault`] opens it.
//!
//! Writing goes the other way: the `…Request` structs at the end are what a
//! save sends, in the camelCase both servers expect.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Lowers every object key, recursively. Values are left alone.
///
/// Except under [`RAW`] keys: what they hold is never read here, only handed
/// back on a save, so it stays exactly as the server spelled it.
pub fn lowercase_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let key = key.to_lowercase();
                    let value = if RAW.contains(&key.as_str()) {
                        value
                    } else {
                        lowercase_keys(value)
                    };
                    (key, value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(lowercase_keys).collect()),
        other => other,
    }
}

/// Keys whose values go back to the server as they came. Vaultwarden stores a
/// login object as it gets it, so a passkey written back with lowered keys
/// (`credentialid`, `rpid`) would stay that way for every other client.
const RAW: &[&str] = &["fido2credentials"];

/// The passkey fields Bitwarden names, in its camelCase.
const PASSKEY_KEYS: &[&str] = &[
    "credentialId",
    "keyType",
    "keyAlgorithm",
    "keyCurve",
    "keyValue",
    "rpId",
    "userHandle",
    "userName",
    "counter",
    "rpName",
    "userDisplayName",
    "discoverable",
    "creationDate",
];

/// A passkey as a save sends it: the fields Bitwarden knows get their
/// camelCase name back if an earlier UwULock stored them lowered, everything
/// else stays as it is. A key that is already there in camelCase wins.
pub fn passkey_for_saving(passkey: &Value) -> Value {
    let Value::Object(map) = passkey else {
        return passkey.clone();
    };
    let mut out = serde_json::Map::new();
    for (key, value) in map {
        let name = PASSKEY_KEYS
            .iter()
            .find(|known| **known != key && known.to_lowercase() == *key)
            .filter(|known| !map.contains_key(**known))
            .map_or(key.as_str(), |known| known);
        out.insert(name.to_string(), value.clone());
    }
    Value::Object(out)
}

/// Numbers that some servers send as strings, and the other way round.
fn flexible_u32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::Number(n)) => n.as_u64().map(|n| n as u32),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    })
}

fn flexible_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::String(s)) => Some(s),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    })
}

/// A list, which some servers send as `null` when it is empty.
fn null_as_empty<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
pub struct Prelogin {
    #[serde(default, deserialize_with = "flexible_u32")]
    pub kdf: Option<u32>,
    #[serde(default, rename = "kdfiterations", deserialize_with = "flexible_u32")]
    pub kdf_iterations: Option<u32>,
    #[serde(default, rename = "kdfmemory", deserialize_with = "flexible_u32")]
    pub kdf_memory: Option<u32>,
    #[serde(default, rename = "kdfparallelism", deserialize_with = "flexible_u32")]
    pub kdf_parallelism: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Token {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// The user key, wrapped under the stretched master key.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default, rename = "privatekey")]
    pub private_key: Option<String>,
    /// Handed out when "remember this device" was asked for with two-step login.
    #[serde(default, rename = "twofactortoken")]
    pub two_factor_token: Option<String>,
}

/// A refused token request.
#[derive(Debug, Default, Deserialize)]
pub struct TokenError {
    #[serde(default)]
    pub error_description: Option<String>,
    /// Provider number → its details (the masked email for email codes).
    #[serde(default, rename = "twofactorproviders2")]
    pub two_factor_providers2: Option<serde_json::Map<String, Value>>,
    #[serde(default, rename = "twofactorproviders")]
    pub two_factor_providers: Option<Vec<Value>>,
    #[serde(default, rename = "errormodel")]
    pub error_model: Option<ErrorModel>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ErrorModel {
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Sync {
    pub profile: Profile,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub folders: Vec<Folder>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub collections: Vec<Collection>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub ciphers: Vec<Cipher>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub sends: Vec<Send>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default, rename = "privatekey")]
    pub private_key: Option<String>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub organizations: Vec<Organization>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Organization {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Folder {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Collection {
    pub id: String,
    #[serde(rename = "organizationid")]
    pub organization_id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Cipher {
    pub id: String,
    #[serde(default, rename = "organizationid")]
    pub organization_id: Option<String>,
    #[serde(default, rename = "folderid")]
    pub folder_id: Option<String>,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default, deserialize_with = "flexible_u32")]
    pub reprompt: Option<u32>,
    /// The item's own key, under the user or organisation key. Newer items only.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub login: Option<Login>,
    #[serde(default)]
    pub card: Option<Card>,
    #[serde(default)]
    pub identity: Option<Identity>,
    #[serde(default, rename = "sshkey")]
    pub ssh_key: Option<SshKey>,
    /// Notes carry their own little object; the server insists on getting it back.
    #[serde(default, rename = "securenote")]
    pub secure_note: Option<SecureNote>,
    #[serde(default)]
    pub fields: Option<Vec<Field>>,
    #[serde(default, rename = "passwordhistory")]
    pub password_history: Option<Vec<PasswordHistory>>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, rename = "collectionids", deserialize_with = "null_as_empty")]
    pub collection_ids: Vec<String>,
    #[serde(default, rename = "revisiondate")]
    pub revision_date: Option<String>,
    #[serde(default, rename = "creationdate")]
    pub creation_date: Option<String>,
    #[serde(default, rename = "deleteddate")]
    pub deleted_date: Option<String>,
    /// Newer servers can archive an item. A save that leaves this out
    /// un-archives it, so it is carried along.
    #[serde(default, rename = "archiveddate")]
    pub archived_date: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SecureNote {
    #[serde(default, rename = "type", deserialize_with = "flexible_u32")]
    pub kind: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Login {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub totp: Option<String>,
    #[serde(default)]
    pub uris: Option<Vec<LoginUri>>,
    #[serde(default, rename = "passwordrevisiondate")]
    pub password_revision_date: Option<String>,
    /// Passkeys. UwULock can't use them, but a save must hand them back
    /// untouched, or the server drops them.
    #[serde(default, rename = "fido2credentials")]
    pub fido2_credentials: Option<Vec<Value>>,
    #[serde(default, rename = "autofillonpageload")]
    pub autofill_on_page_load: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LoginUri {
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default, rename = "match", deserialize_with = "flexible_u32")]
    pub match_kind: Option<u32>,
    /// Bitwarden's check that an address wasn't tampered with. Only valid for
    /// the address it was made for, so it travels with an unchanged one.
    #[serde(default, rename = "urichecksum")]
    pub checksum: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Card {
    #[serde(default, rename = "cardholdername")]
    pub cardholder_name: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default, rename = "expmonth")]
    pub exp_month: Option<String>,
    #[serde(default, rename = "expyear")]
    pub exp_year: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Identity {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, rename = "firstname")]
    pub first_name: Option<String>,
    #[serde(default, rename = "middlename")]
    pub middle_name: Option<String>,
    #[serde(default, rename = "lastname")]
    pub last_name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub address1: Option<String>,
    #[serde(default)]
    pub address2: Option<String>,
    #[serde(default)]
    pub address3: Option<String>,
    #[serde(default, rename = "postalcode")]
    pub postal_code: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub ssn: Option<String>,
    #[serde(default, rename = "passportnumber")]
    pub passport_number: Option<String>,
    #[serde(default, rename = "licensenumber")]
    pub license_number: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SshKey {
    #[serde(default, rename = "privatekey")]
    pub private_key: Option<String>,
    #[serde(default, rename = "publickey")]
    pub public_key: Option<String>,
    /// `keyFingerprint` at Bitwarden, `fingerprint` at some Vaultwardens.
    #[serde(default, rename = "keyfingerprint", alias = "fingerprint")]
    pub fingerprint: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Field {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "flexible_string")]
    pub value: Option<String>,
    #[serde(default, rename = "type", deserialize_with = "flexible_u32")]
    pub kind: Option<u32>,
    /// Which field of the item a linked field points at.
    #[serde(default, rename = "linkedid", deserialize_with = "flexible_u32")]
    pub linked_id: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PasswordHistory {
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default, rename = "lastuseddate")]
    pub last_used_date: Option<String>,
}

/// A file attached to an item. The file itself is fetched on its own
/// (`url`, or `/ciphers/<id>/attachment/<id>`), encrypted in binary
/// ([`crate::crypto::decrypt_file`]).
#[derive(Debug, Default, Deserialize)]
pub struct Attachment {
    pub id: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, rename = "filename")]
    pub file_name: Option<String>,
    /// The attachment's own key, under the item key (or the user or
    /// organisation key). Old attachments have none: their contents are
    /// under that key directly.
    #[serde(default)]
    pub key: Option<String>,
    /// In bytes. A string at Bitwarden, a number at some Vaultwardens.
    #[serde(default, deserialize_with = "flexible_string")]
    pub size: Option<String>,
    #[serde(default, rename = "sizename")]
    pub size_name: Option<String>,
}

/// A Send: a text or a file shared by link. Its values are under the Send's
/// own key, which comes from `key` ([`crate::crypto::send_key`]).
#[derive(Debug, Default, Deserialize)]
pub struct Send {
    pub id: String,
    #[serde(default, rename = "accessid")]
    pub access_id: Option<String>,
    /// 0 a text, 1 a file.
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// The 16-byte seed of the Send's key, under the user key.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub text: Option<SendText>,
    #[serde(default)]
    pub file: Option<SendFile>,
    #[serde(default, rename = "maxaccesscount", deserialize_with = "flexible_u32")]
    pub max_access_count: Option<u32>,
    #[serde(default, rename = "accesscount", deserialize_with = "flexible_u32")]
    pub access_count: Option<u32>,
    /// The hash of the Send's password ([`crate::crypto::send_password_hash`]),
    /// if it has one.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default, rename = "hideemail")]
    pub hide_email: Option<bool>,
    #[serde(default, rename = "revisiondate")]
    pub revision_date: Option<String>,
    #[serde(default, rename = "expirationdate")]
    pub expiration_date: Option<String>,
    #[serde(default, rename = "deletiondate")]
    pub deletion_date: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SendText {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SendFile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "filename")]
    pub file_name: Option<String>,
    #[serde(default, deserialize_with = "flexible_string")]
    pub size: Option<String>,
    #[serde(default, rename = "sizename")]
    pub size_name: Option<String>,
}

// ── What a save sends ──────────────────────────────────────
//
// Both servers read these in camelCase. Vaultwarden keeps the login, card,
// identity, note and SSH object as it gets them, so anything left out here is
// gone from the item afterwards — which is why every one of them carries what
// the sync brought along, not just what UwULock shows.

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CipherRequest {
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    pub notes: Option<String>,
    pub favorite: bool,
    pub reprompt: u8,
    pub folder_id: Option<String>,
    pub organization_id: Option<String>,
    /// The item's own key, still wrapped as it came.
    pub key: Option<String>,
    pub login: Option<LoginRequest>,
    pub card: Option<CardRequest>,
    pub identity: Option<IdentityRequest>,
    pub secure_note: Option<SecureNoteRequest>,
    pub ssh_key: Option<SshKeyRequest>,
    pub fields: Option<Vec<FieldRequest>>,
    pub password_history: Option<Vec<PasswordHistoryRequest>>,
    /// The revision UwULock last saw. The server refuses the save if the item
    /// has changed since, instead of overwriting the newer copy.
    pub last_known_revision_date: Option<String>,
    pub archived_date: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub totp: Option<String>,
    pub uris: Vec<LoginUriRequest>,
    pub password_revision_date: Option<String>,
    pub fido2_credentials: Option<Vec<Value>>,
    pub autofill_on_page_load: Option<bool>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginUriRequest {
    pub uri: Option<String>,
    #[serde(rename = "match")]
    pub match_kind: Option<u32>,
    pub uri_checksum: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardRequest {
    pub cardholder_name: Option<String>,
    pub brand: Option<String>,
    pub number: Option<String>,
    pub exp_month: Option<String>,
    pub exp_year: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityRequest {
    pub title: Option<String>,
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
    pub company: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub address3: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub ssn: Option<String>,
    pub passport_number: Option<String>,
    pub license_number: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureNoteRequest {
    #[serde(rename = "type")]
    pub kind: u32,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyRequest {
    pub private_key: Option<String>,
    pub public_key: Option<String>,
    pub key_fingerprint: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldRequest {
    pub name: Option<String>,
    pub value: Option<String>,
    #[serde(rename = "type")]
    pub kind: u32,
    pub linked_id: Option<u32>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordHistoryRequest {
    pub password: String,
    pub last_used_date: String,
}

/// A new item in an organisation goes to `/ciphers/create`, wrapped like this.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRequest {
    pub cipher: CipherRequest,
    pub collection_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRequest {
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_and_camel_case_read_the_same() {
        let camel = serde_json::json!({"profile": {"email": "a@b.c", "privateKey": "x"},
            "ciphers": [{"id": "1", "type": 1, "organizationId": null, "collectionIds": ["c"]}]});
        let pascal = serde_json::json!({"Profile": {"Email": "a@b.c", "PrivateKey": "x"},
            "Ciphers": [{"Id": "1", "Type": 1, "OrganizationId": null, "CollectionIds": ["c"]}]});
        for value in [camel, pascal] {
            let sync: Sync = serde_json::from_value(lowercase_keys(value)).unwrap();
            assert_eq!(sync.profile.private_key.as_deref(), Some("x"));
            assert_eq!(sync.ciphers[0].collection_ids, ["c"]);
        }
    }

    #[test]
    fn lists_sent_as_null_are_empty() {
        let sync = serde_json::json!({
            "Profile": { "Email": "nyu@example.com", "Organizations": null },
            "Folders": null, "Collections": null, "Ciphers": null, "Sends": null,
        });
        let sync: Sync = serde_json::from_value(lowercase_keys(sync)).unwrap();
        assert!(sync.profile.organizations.is_empty());
        assert!(sync.folders.is_empty() && sync.collections.is_empty());
        assert!(sync.ciphers.is_empty() && sync.sends.is_empty());

        // And a sync without them at all.
        let sync: Sync = serde_json::from_value(serde_json::json!({ "profile": {} })).unwrap();
        assert!(sync.ciphers.is_empty() && sync.sends.is_empty());
    }

    #[test]
    fn a_sync_with_organisations_attachments_and_sends() {
        let sync = serde_json::json!({
            "profile": {
                "email": "nyu@example.com",
                "organizations": [{ "id": "o1", "name": "Cats", "key": "4.AAAA" }],
            },
            "folders": null,
            "collections": [{ "id": "c1", "organizationId": "o1", "name": "2.x|y|z" }],
            "ciphers": [
                {
                    "id": "1", "type": 1, "organizationId": "o1", "collectionIds": null,
                    "fields": null, "passwordHistory": null,
                    "attachments": [
                        { "id": "a1", "fileName": "2.a|b|c", "key": "2.d|e|f",
                          "size": "161", "sizeName": "161 Bytes",
                          "url": "https://vault.example.com/attachments/1/a1" },
                        { "id": "a2", "fileName": "2.g|h|i", "key": null, "size": 42 },
                    ],
                },
                { "Id": "2", "Type": 2, "Attachments": null, "CollectionIds": ["c1"] },
            ],
            "sends": [
                { "id": "s1", "accessId": "ct2APRQtJk-BLLDwAYqhRA", "type": 0,
                  "name": "2.j|k|l", "key": "2.m|n|o", "notes": null,
                  "text": { "text": "2.p|q|r", "hidden": false }, "file": null,
                  "maxAccessCount": null, "accessCount": 0, "password": null,
                  "disabled": false, "hideEmail": null,
                  "revisionDate": "2026-09-27T12:00:00Z", "expirationDate": null,
                  "deletionDate": "2026-10-04T12:00:00Z" },
                { "Id": "s2", "Type": 1, "Key": "2.s|t|u", "Text": null,
                  "File": { "Id": "f1", "FileName": "2.v|w|x", "Size": "1024",
                            "SizeName": "1 KB" },
                  "Password": "vTIDfdj3FTDbejmMf+mJWpYdMXsxfeSd1Sma3sjCtiQ=",
                  "MaxAccessCount": "3" },
            ],
        });
        let sync: Sync = serde_json::from_value(lowercase_keys(sync)).unwrap();
        assert_eq!(sync.profile.organizations[0].key.as_deref(), Some("4.AAAA"));
        assert!(sync.folders.is_empty());
        assert_eq!(sync.collections[0].organization_id, "o1");

        let [first, second] = sync.ciphers.as_slice() else {
            panic!("two ciphers")
        };
        assert!(first.collection_ids.is_empty());
        assert_eq!(first.attachments.len(), 2);
        assert_eq!(first.attachments[0].key.as_deref(), Some("2.d|e|f"));
        assert_eq!(first.attachments[0].size.as_deref(), Some("161"));
        assert_eq!(first.attachments[1].key, None);
        assert_eq!(first.attachments[1].size.as_deref(), Some("42"));
        assert!(second.attachments.is_empty());
        assert_eq!(second.collection_ids, ["c1"]);

        let [text, file] = sync.sends.as_slice() else {
            panic!("two sends")
        };
        assert_eq!(text.kind, 0);
        assert_eq!(text.access_id.as_deref(), Some("ct2APRQtJk-BLLDwAYqhRA"));
        assert_eq!(
            text.text.as_ref().and_then(|t| t.text.as_deref()),
            Some("2.p|q|r")
        );
        assert_eq!(text.max_access_count, None);
        assert_eq!(file.kind, 1);
        assert_eq!(
            file.file.as_ref().and_then(|f| f.file_name.as_deref()),
            Some("2.v|w|x")
        );
        assert_eq!(file.max_access_count, Some(3));
        assert!(file.password.is_some());
    }

    #[test]
    fn passkeys_keep_the_servers_spelling() {
        let passkey = serde_json::json!({ "credentialId": "c", "rpId": "example.com",
            "SomethingNewer": { "NestedKey": [1, "Two"] } });
        let sync =
            serde_json::json!({ "Ciphers": [{ "Login": { "Fido2Credentials": [passkey] } }] });
        let lowered = lowercase_keys(sync);
        assert_eq!(
            lowered["ciphers"][0]["login"]["fido2credentials"][0],
            passkey
        );
    }

    #[test]
    fn lowered_passkeys_get_their_names_back() {
        let stored = serde_json::json!({ "credentialid": "c", "rpid": "example.com",
            "username": "u", "counter": "0", "somethingnewer": 1,
            "userName": "camel wins", "UnknownPascal": true });
        let sent = passkey_for_saving(&stored);
        assert_eq!(
            sent,
            serde_json::json!({ "credentialId": "c", "rpId": "example.com",
                "username": "u", "counter": "0", "somethingnewer": 1,
                "userName": "camel wins", "UnknownPascal": true })
        );
        // One that is right already goes back unchanged.
        assert_eq!(passkey_for_saving(&sent), sent);
    }
}
