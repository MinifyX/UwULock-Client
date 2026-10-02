//! The extension as a passkey provider: making a passkey when a site calls
//! `navigator.credentials.create()`, signing with one for
//! `navigator.credentials.get()`.
//!
//! Passkeys are kept like Bitwarden keeps them, one per login, so the ones
//! made here work in Bitwarden's apps and theirs work here. Asking the person
//! and talking to the page is the extension's part; this is the
//! authenticator.
//!
//! Every answer that changed an item carries it as `cipher`, the way the
//! server takes it. The item in here changes along, but keeps its old
//! revision date: sync after saving.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::json;
use uwulock_core::passkey::{attestation_object, Passkey, AT, BE, BS, UP, UV};
use uwulock_core::vault::{Item, ItemKind, LoginUri};
use zeroize::Zeroizing;

use crate::autofill::passkeys_of;
use crate::view::{awaits_reprompt, find, reprompt_first};
use crate::{json, with_unlocked, Failure, Result, Unlocked};

fn b64_url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// URL-safe base64 from the page, padded or not.
fn from_b64_url(text: &str, what: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(text.trim().trim_end_matches('='))
        .map_err(|_| Failure::new("invalid", format!("The {what} is not URL-safe base64.")))
}

fn flags(user_verified: bool) -> u8 {
    UP | BE | BS | if user_verified { UV } else { 0 }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRequest {
    /// The login the passkey goes to; a new login when there is none.
    #[serde(default)]
    item_id: Option<String>,
    /// The new login's name.
    #[serde(default)]
    name: String,
    #[serde(default)]
    folder_id: Option<String>,
    rp_id: String,
    #[serde(default)]
    rp_name: Option<String>,
    #[serde(default)]
    user_handle: Option<String>,
    #[serde(default)]
    user_name: Option<String>,
    #[serde(default)]
    user_display_name: Option<String>,
    #[serde(default)]
    discoverable: bool,
    #[serde(default)]
    user_verified: bool,
    now: String,
}

/// A new passkey, in a login: the one `itemId` names, whose passkey it
/// replaces (Bitwarden keeps one a login), or a new login for the site.
pub fn create(request: &str) -> Result<String> {
    let request: CreateRequest = serde_json::from_str(request)?;
    let rp_id = request.rp_id.trim();
    if rp_id.is_empty() {
        return Err(Failure::new("invalid", "A passkey needs the site's RP id."));
    }
    let user_handle = request
        .user_handle
        .as_deref()
        .map(|handle| from_b64_url(handle, "user handle"))
        .transpose()?;
    let passkey = Passkey::generate(
        rp_id,
        request.rp_name.as_deref(),
        user_handle.as_deref(),
        request.user_name.as_deref(),
        request.user_display_name.as_deref(),
        request.discoverable,
        &request.now,
    )?;
    let item_id = request.item_id.clone().filter(|id| !id.is_empty());

    with_unlocked(|unlocked| {
        let mut item = match &item_id {
            Some(id) => existing_login(unlocked, id)?,
            None => new_login(&request, rp_id),
        };
        let outer = unlocked
            .vault
            .outer_key(item.organization_id.as_deref(), &unlocked.user_key)?
            .clone();
        let sealed = passkey.seal(item.key.as_ref().unwrap_or(&outer));
        if let Some(login) = item.login.as_mut() {
            login.passkeys = Some(vec![sealed]);
        }
        item.can_save()?;
        let cipher = item.seal(&outer)?;

        let auth_data = passkey.authenticator_data(flags(request.user_verified) | AT, true)?;
        let answer = json!({
            "credentialId": b64_url(&passkey.credential_id_bytes()?),
            "attestationObject": b64_url(&attestation_object(&auth_data)),
            "authenticatorData": b64_url(&auth_data),
            "publicKey": b64_url(&passkey.public_key_spki()?),
            "publicKeyAlgorithm": -7,
            "transports": ["internal", "hybrid"],
            "cipher": cipher,
            "itemId": item_id,
        });
        if item_id.is_some() {
            replace(unlocked, item);
        }
        json(&answer)
    })
}

fn existing_login(unlocked: &Unlocked, id: &str) -> Result<Item> {
    let item = find(unlocked, id)?;
    if item.kind != ItemKind::Login {
        return Err(Failure::new("invalid", "Passkeys go into logins."));
    }
    // Like a save from the editor: an item behind the re-prompt isn't changed
    // before it was answered.
    if awaits_reprompt(unlocked, item) {
        return Err(reprompt_first());
    }
    Ok(item.clone())
}

fn new_login(request: &CreateRequest, rp_id: &str) -> Item {
    let mut item = Item::new(ItemKind::Login);
    let name = [Some(&request.name), request.rp_name.as_ref()]
        .into_iter()
        .flatten()
        .map(|name| name.trim())
        .find(|name| !name.is_empty())
        .unwrap_or(rp_id);
    item.name = Zeroizing::new(name.to_string());
    item.folder_id = request.folder_id.clone().filter(|id| !id.is_empty());
    if let Some(login) = item.login.as_mut() {
        login.username = request
            .user_name
            .clone()
            .filter(|name| !name.is_empty())
            .map(Zeroizing::new);
        login.uris = vec![LoginUri {
            uri: Zeroizing::new(format!("https://{rp_id}")),
            match_kind: None,
            checksum: None,
        }];
    }
    item
}

/// Puts a changed item in place of the one with its id.
fn replace(unlocked: &mut Unlocked, item: Item) {
    if let Some(slot) = unlocked.vault.items.iter_mut().find(|i| i.id == item.id) {
        *slot = item;
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssertRequest {
    item_id: String,
    /// The id the site asked for, or the one the person picked, in URL-safe
    /// base64.
    credential_id: String,
    rp_id: String,
    /// SHA-256 of the client data, in URL-safe base64.
    client_data_hash: String,
    #[serde(default)]
    user_verified: bool,
}

/// A signature from the passkey of the item `itemId` with that credential id
/// and RP id. A passkey that counts has its counter raised, and comes back in
/// `cipher` to be saved; Bitwarden's stay at 0, and `cipher` is `null`.
pub fn assert(request: &str) -> Result<String> {
    let request: AssertRequest = serde_json::from_str(request)?;
    let wanted = from_b64_url(&request.credential_id, "credential id")?;
    let client_data_hash = from_b64_url(&request.client_data_hash, "client data hash")?;
    if client_data_hash.len() != 32 {
        return Err(Failure::new(
            "invalid",
            "The client data hash is a SHA-256.",
        ));
    }
    with_unlocked(|unlocked| {
        let item = find(unlocked, &request.item_id)?;
        if awaits_reprompt(unlocked, item) {
            return Err(reprompt_first());
        }
        let (index, mut passkey) = passkeys_of(unlocked, item)
            .into_iter()
            .find(|(_, passkey)| {
                passkey.rp_id.eq_ignore_ascii_case(request.rp_id.trim())
                    && passkey.credential_id_bytes().is_ok_and(|id| id == wanted)
            })
            .ok_or_else(|| {
                Failure::new("not-found", "This item has no such passkey for this site.")
            })?;

        let mut changed = None;
        if passkey.counter > 0 {
            passkey.counter = passkey.counter.saturating_add(1);
            let mut item = item.clone();
            let outer = unlocked
                .vault
                .outer_key(item.organization_id.as_deref(), &unlocked.user_key)?
                .clone();
            let key = item.key.clone().unwrap_or_else(|| outer.clone());
            if let Some(stored) = item
                .login
                .as_mut()
                .and_then(|l| l.passkeys.as_mut())
                .and_then(|passkeys| passkeys.get_mut(index))
            {
                *stored = passkey.reseal(stored, &key);
            }
            let cipher = item.seal(&outer)?;
            changed = Some((item, cipher));
        }

        let auth_data = passkey.authenticator_data(flags(request.user_verified), false)?;
        let signature = passkey.sign(&auth_data, &client_data_hash)?;
        let answer = json!({
            "credentialId": b64_url(&wanted),
            "authenticatorData": b64_url(&auth_data),
            "signature": b64_url(&signature),
            "userHandle": passkey.user_handle_bytes()?.map(|handle| b64_url(&handle)),
            "cipher": changed.as_ref().map(|(_, cipher)| cipher),
        });
        if let Some((item, _)) = changed {
            replace(unlocked, item);
        }
        json(&answer)
    })
}

// ── Managing an item's passkeys ───────────────────────────

/// The passkeys of an item for its details:
/// `[{index, readable, credentialId, rpId, rpName, userName,
/// userDisplayName, creationDate, discoverable}]`. Nothing secret.
pub fn list(id: &str) -> Result<String> {
    with_unlocked(|unlocked| {
        let item = find(unlocked, id)?;
        if awaits_reprompt(unlocked, item) {
            return Err(reprompt_first());
        }
        let key = unlocked.vault.item_key(item, &unlocked.user_key)?;
        json(&uwulock_core::passkey::list(item, key))
    })
}

/// Deletes the passkey at `index` (with `credential_id`, only if it still is
/// that one: else a `conflict`). Answers the item as the server takes it,
/// `{cipher}`, for `PUT /api/ciphers/<id>`; the item in here changes along.
pub fn delete(id: &str, index: usize, credential_id: Option<String>) -> Result<String> {
    with_unlocked(|unlocked| {
        let mut item = existing_login(unlocked, id)?;
        let key = unlocked.vault.item_key(&item, &unlocked.user_key)?.clone();
        uwulock_core::passkey::remove(&mut item, &key, index, credential_id.as_deref()).map_err(
            |error| match error {
                uwulock_core::Error::Conflict => Failure::new(
                    "conflict",
                    "This passkey changed in the meantime. Look again.",
                ),
                uwulock_core::Error::Refused(_) => {
                    Failure::new("not-found", "This item has no such passkey.")
                }
                other => other.into(),
            },
        )?;
        let outer = unlocked
            .vault
            .outer_key(item.organization_id.as_deref(), &unlocked.user_key)?
            .clone();
        item.can_save()?;
        let cipher = item.seal(&outer)?;
        replace(unlocked, item);
        json(&json!({ "cipher": cipher }))
    })
}
