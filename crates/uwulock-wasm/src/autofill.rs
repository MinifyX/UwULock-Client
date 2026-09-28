//! Autofill: what the background matches pages against, and what it fills in
//! once somebody picked an item.
//!
//! The index carries no secrets: names, addresses, and which passkeys there
//! are. Values come only through [`fill_values`], one item at a time, after
//! somebody chose it — and not before the master password re-prompt, for an
//! item that asks for it.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::Serialize;
use serde_json::{json, Value};
use uwulock_core::passkey::Passkey;
use uwulock_core::totp::Totp;
use uwulock_core::vault::{Item, ItemKind, Secret};

use crate::view::{awaits_reprompt, find, present, reprompt_first, subtitle};
use crate::{json, with_unlocked, Failure, Result, Unlocked};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    kind: ItemKind,
    name: String,
    subtitle: Option<String>,
    favorite: bool,
    reprompt: bool,
    /// Archived items stay out of Bitwarden's suggestions; the background
    /// decides.
    archived: bool,
    has_totp: bool,
    has_password: bool,
    has_username: bool,
    uris: Vec<Uri>,
    passkeys: Vec<PasskeyEntry>,
}

#[derive(Debug, Serialize)]
struct Uri {
    uri: String,
    /// Bitwarden's match detection, `null` for the account's default.
    #[serde(rename = "match")]
    match_kind: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PasskeyEntry {
    /// The id the site knows, in URL-safe base64.
    credential_id: String,
    rp_id: String,
    user_name: Option<String>,
    user_display_name: Option<String>,
    /// The site's `user.id`, in URL-safe base64.
    user_handle: Option<String>,
    discoverable: bool,
    counter: u32,
}

/// The passkeys of a login that open, each with where it was in the list.
/// One that doesn't open is left out rather than failing the rest.
pub fn passkeys_of(unlocked: &Unlocked, item: &Item) -> Vec<(usize, Passkey)> {
    let Some(raw) = item.login.as_ref().and_then(|l| l.passkeys.as_ref()) else {
        return Vec::new();
    };
    let Ok(outer) = unlocked
        .vault
        .outer_key(item.organization_id.as_deref(), &unlocked.user_key)
    else {
        return Vec::new();
    };
    let key = item.key.as_ref().unwrap_or(outer);
    raw.iter()
        .enumerate()
        .filter_map(|(index, raw)| Some((index, Passkey::open(raw, key).ok()?)))
        .collect()
}

fn entry(unlocked: &Unlocked, item: &Item) -> Entry {
    let login = item.login.as_ref();
    let passkeys = passkeys_of(unlocked, item)
        .into_iter()
        .filter_map(|(_, passkey)| {
            Some(PasskeyEntry {
                credential_id: URL_SAFE_NO_PAD.encode(passkey.credential_id_bytes().ok()?),
                user_handle: passkey
                    .user_handle_bytes()
                    .ok()?
                    .map(|handle| URL_SAFE_NO_PAD.encode(handle)),
                rp_id: passkey.rp_id.clone(),
                user_name: passkey.user_name.clone(),
                user_display_name: passkey.user_display_name.clone(),
                discoverable: passkey.discoverable,
                counter: passkey.counter,
            })
        })
        .collect();
    Entry {
        id: item.id.clone(),
        kind: item.kind,
        name: item.name.to_string(),
        subtitle: subtitle(item),
        favorite: item.favorite,
        reprompt: item.reprompt,
        archived: item.archived_date.is_some(),
        has_totp: login.is_some_and(|l| present(&l.totp)),
        has_password: login.is_some_and(|l| present(&l.password)),
        has_username: login.is_some_and(|l| present(&l.username)),
        uris: login
            .map(|l| {
                l.uris
                    .iter()
                    .map(|u| Uri {
                        uri: u.uri.to_string(),
                        match_kind: u.match_kind,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        passkeys,
    }
}

/// Every item that isn't in the trash and opened completely.
pub fn index() -> Result<String> {
    with_unlocked(|unlocked| {
        let entries: Vec<Entry> = unlocked
            .vault
            .items
            .iter()
            .filter(|item| !item.deleted && !item.broken)
            .map(|item| entry(unlocked, item))
            .collect();
        json(&entries)
    })
}

/// What to fill from the item `id`: a login's username, password and current
/// TOTP code, a card, or an identity without its ID numbers. Absent values
/// are `null`. `now` is Unix time in seconds, for the code.
pub fn fill_values(id: &str, now: u64) -> Result<String> {
    with_unlocked(|unlocked| {
        let item = find(unlocked, id)?;
        if awaits_reprompt(unlocked, item) {
            return Err(reprompt_first());
        }
        json(&values(item, now)?)
    })
}

/// A value as it is, spaces and all: what gets typed into a page.
fn value(value: &Option<Secret>) -> Option<String> {
    value
        .as_ref()
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

fn values(item: &Item, now: u64) -> Result<Value> {
    if let Some(login) = &item.login {
        let totp = login
            .totp
            .as_ref()
            .filter(|t| !t.is_empty())
            .and_then(|secret| Totp::parse(secret).ok())
            .map(|totp| totp.code_at(now).0.to_string());
        return Ok(json!({
            "kind": "login",
            "username": value(&login.username),
            "password": value(&login.password),
            "totp": totp,
        }));
    }
    if let Some(card) = &item.card {
        return Ok(json!({
            "kind": "card",
            "cardholderName": value(&card.cardholder_name),
            "brand": value(&card.brand),
            "number": value(&card.number),
            "expMonth": value(&card.exp_month),
            "expYear": value(&card.exp_year),
            "code": value(&card.code),
        }));
    }
    if let Some(i) = &item.identity {
        return Ok(json!({
            "kind": "identity",
            "title": value(&i.title),
            "firstName": value(&i.first_name),
            "middleName": value(&i.middle_name),
            "lastName": value(&i.last_name),
            "username": value(&i.username),
            "company": value(&i.company),
            "email": value(&i.email),
            "phone": value(&i.phone),
            "address1": value(&i.address1),
            "address2": value(&i.address2),
            "address3": value(&i.address3),
            "postalCode": value(&i.postal_code),
            "city": value(&i.city),
            "state": value(&i.state),
            "country": value(&i.country),
        }));
    }
    Err(Failure::new(
        "invalid",
        "There is nothing to fill in from this item.",
    ))
}
