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
pub fn lowercase_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key.to_lowercase(), lowercase_keys(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(lowercase_keys).collect()),
        other => other,
    }
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
    #[serde(default, rename = "kdf", deserialize_with = "flexible_u32")]
    pub kdf: Option<u32>,
    #[serde(default, rename = "kdfiterations", deserialize_with = "flexible_u32")]
    pub kdf_iterations: Option<u32>,
    #[serde(default, rename = "kdfmemory", deserialize_with = "flexible_u32")]
    pub kdf_memory: Option<u32>,
    #[serde(default, rename = "kdfparallelism", deserialize_with = "flexible_u32")]
    pub kdf_parallelism: Option<u32>,
}

/// A refused token request.
#[derive(Debug, Default, Deserialize)]
pub struct TokenError {
    #[serde(default)]
    pub error: Option<String>,
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
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub ciphers: Vec<Cipher>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default, rename = "privatekey")]
    pub private_key: Option<String>,
    #[serde(default)]
    pub organizations: Vec<Organization>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Organization {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
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
    #[serde(default)]
    pub attachments: Option<Vec<Value>>,
    #[serde(default, rename = "collectionids")]
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
}
