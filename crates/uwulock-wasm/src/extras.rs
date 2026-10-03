//! UwULock Server's extras, for the extension: the extras key opened (never
//! made or wrapped again — the web vault and the desktop app do that), own
//! icons opened, the owner's file requests read, and an item shared as a text
//! Send.
//!
//! The extras key stays in here with the user key, and goes with it when the
//! vault locks. Without it (none made yet, or lost in a rotation) everything
//! that needs it is simply off.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uwulock_core::crypto::{EncString, PrivateKey};
use uwulock_core::entry_send;
use uwulock_core::extras::{self, Keys, Resolved};
use uwulock_core::file_request::{self, LinkSecret, PublicInfo};
use uwulock_core::send::{self, TextSend};
use uwulock_core::vault::{FieldKind, Item};
use zeroize::Zeroizing;

use crate::view::{self, find, IDENTITY_FIELDS};
use crate::{json, with_unlocked, Failure, Result, Unlocked};

// ── The extras key ────────────────────────────────────────

/// Opens the extras key from `GET /uwu/v1/keys`. The answer's `state`:
/// `open`; `none` (no client made one yet); `lost` (the key pair changed, or
/// only the private key's wrap is left and this account has no private key).
/// After an official client rotated the user key, the key opens with the
/// account's private key; wrapping it again for the new user key, and adding
/// the private key's wrap to a key from before it existed, is left to the
/// web vault or the desktop app. A wrap that doesn't fit — the two wraps
/// disagree, or an RSA wrap the server could have made — is an error.
pub fn open_extras(keys: &str) -> Result<String> {
    let keys: Keys = serde_json::from_str(keys)?;
    with_unlocked(|unlocked| {
        unlocked.extras = None;
        if keys.lost {
            return json(&json!({ "state": "lost" }));
        }
        if keys.extras_key.is_none() {
            return json(&json!({ "state": "none" }));
        }
        // Also with a user wrap: the private key's wrap is checked against it.
        let private = own_private_key(unlocked)?;
        // A wrong key here is not a wrong password: say what didn't open.
        let resolved =
            extras::resolve(&keys, &unlocked.user_key, private.as_ref()).map_err(|error| {
                Failure::new("crypto", format!("The extras key didn't open: {error}"))
            })?;
        let state = match resolved {
            Resolved::Open { key, .. } => {
                unlocked.extras = Some(key);
                "open"
            }
            Resolved::Lost => "lost",
            // Only when there is no key at all, which was answered above.
            Resolved::Create(_) => "none",
        };
        json(&json!({ "state": state }))
    })
}

/// The account's private key, from the sync (under the user key).
fn own_private_key(unlocked: &Unlocked) -> Result<Option<PrivateKey>> {
    let Some(sealed) = &unlocked.private_key else {
        return Ok(None);
    };
    let der = sealed.parse::<EncString>()?.decrypt(&unlocked.user_key)?;
    Ok(Some(PrivateKey::from_der(&der)?))
}

fn no_extras() -> Failure {
    Failure::new("unsupported", "The extras key isn't open.")
}

// ── Own icons ─────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OwnIcon {
    cipher_id: String,
    key_type: String,
    data: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenedIcon {
    cipher_id: String,
    /// The PNG, base64.
    png: String,
}

/// Opens own icons (`POST /uwu/v1/icons/own/get`'s list): a personal item's
/// under the extras key, an organisation's item's under that organisation's
/// key. Icons of items not in the vault, with a key type that doesn't fit the
/// item, or that don't open, are left out: those items show the automatic
/// icon or the glyph.
pub fn open_icons(icons: &str) -> Result<String> {
    let icons: Vec<OwnIcon> = serde_json::from_str(icons)?;
    with_unlocked(|unlocked| {
        let opened: Vec<OpenedIcon> = icons
            .iter()
            .filter_map(|icon| {
                let item = unlocked.vault.item(&icon.cipher_id)?;
                let key = match (icon.key_type.as_str(), item.organization_id.as_deref()) {
                    ("extras", None) => unlocked.extras.as_ref()?,
                    ("organization", Some(org)) => unlocked
                        .vault
                        .outer_key(Some(org), &unlocked.user_key)
                        .ok()?,
                    _ => return None,
                };
                let png = extras::open_icon(&icon.data, key).ok()?;
                Some(OpenedIcon {
                    cipher_id: icon.cipher_id.clone(),
                    png: STANDARD.encode(png.as_slice()),
                })
            })
            .collect();
        json(&opened)
    })
}

// ── File requests ─────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRequest {
    id: String,
    #[serde(default)]
    access_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    link_secret: Option<String>,
    #[serde(default)]
    public_info: Option<String>,
}

/// The owner's labels of file requests (`GET /uwu/v1/file-requests`'s list):
/// `[{id, label}]`, `label` null where there is none or it doesn't open (the
/// extras key was reset: "unnamed").
pub fn file_request_labels(requests: &str) -> Result<String> {
    let requests: Vec<FileRequest> = serde_json::from_str(requests)?;
    with_unlocked(|unlocked| {
        let extras = unlocked.extras.as_ref().ok_or_else(no_extras)?;
        let labels: Vec<Value> = requests
            .iter()
            .map(|request| {
                let label = request
                    .name
                    .as_deref()
                    .and_then(|name| file_request::open_label(name, extras).ok())
                    .map(|label| label.to_string());
                json!({ "id": request.id, "label": label })
            })
            .collect();
        json(&labels)
    })
}

/// A file request's link, from its object: on the main host (`base` the web
/// vault) or, with `send_domain`, on a send domain (`base` its address).
pub fn file_request_link(request: &str, base: &str, send_domain: bool) -> Result<String> {
    let request: FileRequest = serde_json::from_str(request)?;
    with_unlocked(|unlocked| {
        let extras = unlocked.extras.as_ref().ok_or_else(no_extras)?;
        let sealed = request
            .link_secret
            .as_deref()
            .ok_or_else(|| Failure::new("invalid", "This file request has no link secret."))?;
        let secret = LinkSecret::open(sealed, extras)?;
        // What the uploader's page encrypts for has to be this account's key:
        // details naming another were made by someone else who knows the
        // secret, and what is uploaded to them isn't for us.
        let own = own_private_key(unlocked)?
            .ok_or_else(|| Failure::new("crypto", "This account has no key pair."))?;
        let info = request
            .public_info
            .as_deref()
            .ok_or_else(|| Failure::new("invalid", "This file request has no details."))?;
        if !PublicInfo::open(info, &secret)?.is_for(&own.public()) {
            return Err(Failure::new(
                "crypto",
                "This file request's link encrypts for a key that isn't yours. Don't share \
                 it; edit or delete the request in the web vault.",
            ));
        }
        let access_id = match request.access_id.filter(|a| !a.is_empty()) {
            Some(access_id) => access_id,
            None => file_request::access_id(&request.id)?,
        };
        Ok(file_request::link(base, &access_id, &secret, send_domain))
    })
}

// ── Sharing an item as a Send ─────────────────────────────

#[derive(Serialize)]
struct Shareable {
    name: String,
    /// A custom field's own name; the others the popup labels itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

fn candidates(item: &Item) -> Vec<(String, Option<String>)> {
    let mut names: Vec<(String, Option<String>)> = ["username", "password"]
        .iter()
        .map(|n| (n.to_string(), None))
        .collect();
    if let Some(login) = &item.login {
        names.extend((0..login.uris.len()).map(|i| (format!("uri:{i}"), None)));
    }
    names.extend(
        ["card-name", "card-number", "card-expiry", "card-code"]
            .iter()
            .map(|n| (n.to_string(), None)),
    );
    names.extend(
        IDENTITY_FIELDS
            .iter()
            .map(|(n, _)| (format!("identity:{n}"), None)),
    );
    names.extend(
        ["ssh-public", "ssh-fingerprint", "ssh-private"]
            .iter()
            .map(|n| (n.to_string(), None)),
    );
    names.push(("notes".into(), None));
    names.extend(
        item.fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.kind != FieldKind::Linked)
            .map(|(i, f)| {
                let label = f
                    .name
                    .as_ref()
                    .map(|n| n.to_string())
                    .filter(|n| !n.is_empty());
                (format!("field:{i}"), label)
            }),
    );
    names
}

fn shareable_item<'a>(unlocked: &'a Unlocked, id: &str) -> Result<&'a Item> {
    let item = find(unlocked, id)?;
    if view::awaits_reprompt(unlocked, item) {
        return Err(view::reprompt_first());
    }
    Ok(item)
}

/// What of an item can go into a Send: the names of its values that have
/// one. Never the authenticator key, nor the password history, nor what the
/// organisation hides from this member (`send::withheld`).
pub fn shareable_fields(id: &str) -> Result<String> {
    with_unlocked(|unlocked| {
        let item = shareable_item(unlocked, id)?;
        let fields: Vec<Shareable> = candidates(item)
            .into_iter()
            .filter(|(name, _)| {
                !send::withheld(item, name) && send::shareable_value(item, name).is_some()
            })
            .map(|(name, label)| Shareable { name, label })
            .collect();
        json(&fields)
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareOptions {
    /// `[name, label]`: the value and what the recipient reads before it.
    fields: Vec<(String, String)>,
    #[serde(default)]
    name: Option<String>,
    deletion_date: String,
    #[serde(default)]
    expiration_date: Option<String>,
    #[serde(default)]
    max_access_count: Option<u32>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    hidden: bool,
    /// An entry Send (`uwulock_core::entry_send`): the readable lines plus
    /// the `uwulock-entry:v1:` line UwULock's Send page shows as an entry.
    /// Only then may `fields` name `totp`: the page makes live codes from
    /// it, the readable lines never hold it.
    #[serde(default)]
    entry: bool,
}

/// A text Send with the chosen values of an item, sealed for
/// `POST /api/sends`: the request's body. Its `key` (the seed under the user
/// key) makes the link afterwards ([`send_link`]).
pub fn seal_share(id: &str, options: &str) -> Result<String> {
    let options: ShareOptions = serde_json::from_str(options)?;
    let password = options.password.map(Zeroizing::new);
    with_unlocked(|unlocked| {
        let item = shareable_item(unlocked, id)?;
        let chosen = options
            .fields
            .iter()
            .filter(|(name, _)| {
                !send::withheld(item, name)
                    && (send::shareable_value(item, name).is_some()
                        || (options.entry && name == "totp" && has_totp(item)))
            })
            .count();
        if chosen == 0 {
            return Err(Failure::new(
                "invalid",
                "Choose at least one value to share.",
            ));
        }
        let name = options
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| item.name.to_string());
        // An entry Send's marker is tagged with the Send's seed.
        let seed = send::generate_send_seed();
        let sealed = TextSend {
            name,
            notes: None,
            text: if options.entry {
                entry_send::share_entry_text(item, &options.fields, seed.as_ref())
            } else {
                send::share_text(item, &options.fields)
            },
            hidden: options.hidden,
            max_access_count: options.max_access_count,
            deletion_date: options.deletion_date.clone(),
            expiration_date: options.expiration_date.clone(),
            password,
            emails: Vec::new(),
            hide_email: false,
        }
        .seal_with_seed(&unlocked.user_key, seed)?;
        json(&sealed.request)
    })
}

fn has_totp(item: &Item) -> bool {
    item.login
        .as_ref()
        .and_then(|l| l.totp.as_ref())
        .is_some_and(|t| !t.trim().is_empty())
}

/// The length of a Send's seed, the link's `#` part decoded.
const SEND_SEED_LEN: usize = 16;

/// The entry in a Send's text (`uwulock_core::entry_send`), as JSON
/// `{entry, readable, openable}`, or `null` when the text is plain (no
/// marker, another version, a tag that doesn't fit the Send's `key`,
/// garbled): then the page shows the text as it is. `key` is the part of the
/// link after the `#` (the 16-byte seed, URL-safe base64); any other length
/// is `invalid`. `openable[i]` says whether
/// `entry.websites[i]` may be a link (http/https). Needs no unlocked vault:
/// the Send page of a recipient uses it.
pub fn decode_entry_send(text: &str, key: &str) -> Result<String> {
    use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
    let seed = zeroize::Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(key.trim().trim_end_matches('='))
            .or_else(|_| URL_SAFE.decode(key.trim()))
            .map_err(|_| Failure::new("invalid", "The link's key isn't one."))?,
    );
    // A Send's seed is 16 bytes (`send::generate_send_seed`); an empty or cut
    // key would verify markers made with that same wrong key (R6 F2).
    if seed.len() != SEND_SEED_LEN {
        return Err(Failure::new("invalid", "The link's key isn't one."));
    }
    match entry_send::decode(text, &seed) {
        Some(entry) => json(&json!({
            "openable": entry.websites.iter().map(|w| entry_send::openable(w)).collect::<Vec<_>>(),
            "entry": entry,
            "readable": entry_send::readable_part(text, &seed),
        })),
        None => Ok("null".into()),
    }
}

/// A Send's link from its `key` and `accessId` as the server answered: on the
/// web vault (`base`), or with `send_domain` on a send domain (`base` its
/// address).
pub fn send_link(key: &str, access_id: &str, base: &str, send_domain: bool) -> Result<String> {
    with_unlocked(|unlocked| {
        let seed = send::open_seed(key, &unlocked.user_key)?;
        Ok(send::link(base, access_id, &seed, send_domain))
    })
}
