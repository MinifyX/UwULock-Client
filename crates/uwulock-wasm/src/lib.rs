//! uwulock-core compiled to WebAssembly, for the browser extension.
//!
//! The extension's background worker never holds a key. It hands this module
//! the master password (or the PIN, or the user key it kept while the browser
//! runs) and from then on asks for what it needs: the list of items, what to
//! fill into a page after somebody picked an item, a passkey's signature.
//! Encrypting what is saved happens in here too. The keys live in this
//! module's memory until [`lock`] wipes them.
//!
//! Everything goes in and out as JSON text, and every error as
//! `{"kind", "message"}`, with the kinds `wrong-password`, `locked`,
//! `reprompt`, `not-found`, `invalid`, `unsupported`, `refused` and `crypto`.
//! The shapes of items are the ones the web vault of UwULock-Server gets
//! (`web/wasm`), so the same components can show them.
//!
//! Every function the extension calls is a thin wrapper here over one in the
//! modules, which returns a [`Failure`] rather than a `JsValue`: those can be
//! tested natively, where a `JsValue` can't be made.

mod autofill;
mod draft;
mod extras;
mod generator;
mod passkeys;
mod session;
mod view;

#[cfg(test)]
mod tests;

use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashSet;
use uwulock_core::crypto::{Kdf, SymmetricKey};
use uwulock_core::vault::Vault;
use wasm_bindgen::prelude::*;
use zeroize::Zeroizing;

/// An error for the extension: what kind, and a message a person can read.
#[derive(Debug, Serialize)]
pub struct Failure {
    kind: &'static str,
    message: String,
}

impl Failure {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Failure {
            kind,
            message: message.into(),
        }
    }
}

impl From<uwulock_core::Error> for Failure {
    fn from(error: uwulock_core::Error) -> Self {
        let kind = match &error {
            uwulock_core::Error::WrongKey => "wrong-password",
            uwulock_core::Error::Refused(_) => "refused",
            uwulock_core::Error::Unsupported(_) => "unsupported",
            _ => "crypto",
        };
        Failure::new(kind, error.to_string())
    }
}

impl From<serde_json::Error> for Failure {
    fn from(error: serde_json::Error) -> Self {
        Failure::new("invalid", error.to_string())
    }
}

impl From<Failure> for JsValue {
    fn from(failure: Failure) -> Self {
        JsValue::from_str(&serde_json::to_string(&failure).unwrap_or_default())
    }
}

pub type Result<T, E = Failure> = std::result::Result<T, E>;

/// The account, unlocked.
pub struct Unlocked {
    pub email: String,
    pub kdf: Kdf,
    /// The user key as the server keeps it, wrapped under the master key.
    pub protected_key: String,
    pub user_key: SymmetricKey,
    pub vault: Vault,
    /// Items with a master password re-prompt whose prompt was answered.
    pub reprompt_ok: HashSet<String>,
    /// The account's private key as the sync has it (under the user key):
    /// what opens the extras key after an official client rotated the user
    /// key.
    pub private_key: Option<String>,
    /// UwULock Server's extras key, once [`open_extras`] opened it.
    pub extras: Option<SymmetricKey>,
}

thread_local! {
    /// The master key between the login's first step and the unlock.
    static PENDING: RefCell<Option<Zeroizing<[u8; 32]>>> = const { RefCell::new(None) };
    static UNLOCKED: RefCell<Option<Unlocked>> = const { RefCell::new(None) };
}

pub fn with_unlocked<T>(work: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> {
    UNLOCKED.with(|cell| match cell.borrow_mut().as_mut() {
        Some(unlocked) => work(unlocked),
        None => Err(Failure::new("locked", "The vault is locked.")),
    })
}

fn json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

/// A [`Failure`] as the JSON text the extension gets as the error.
fn js<T>(result: Result<T>) -> Result<T, JsValue> {
    result.map_err(JsValue::from)
}

/// A KDF as the server's prelogin gives it: `{"kdf", "kdfIterations",
/// "kdfMemory", "kdfParallelism"}`.
pub fn kdf_from(text: &str) -> Result<Kdf> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let number = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as u32)
    };
    let kdf = match number("kdf").unwrap_or(0) {
        0 => Kdf::Pbkdf2 {
            iterations: number("kdfIterations").unwrap_or(600_000),
        },
        1 => Kdf::Argon2id {
            iterations: number("kdfIterations").unwrap_or(3),
            memory_mib: number("kdfMemory").unwrap_or(64),
            parallelism: number("kdfParallelism").unwrap_or(4),
        },
        other => {
            return Err(Failure::new(
                "unsupported",
                format!("key derivation type {other}"),
            ))
        }
    };
    kdf.check()?;
    kdf.check_ceilings()?;
    Ok(kdf)
}

/// Whether the KDF a prelogin asks for is cheaper to guess than `stored`,
/// the one this browser accepted at the account's last login: fewer rounds,
/// less memory, or PBKDF2 where it was Argon2id (`Kdf::is_weaker_than`, the
/// rule the desktop app follows). Both in [`kdf_from`]'s shape.
pub fn kdf_weaker(kdf: &str, stored: &str) -> Result<bool> {
    Ok(kdf_from(kdf)?.is_weaker_than(&kdf_from(stored)?))
}

#[wasm_bindgen(js_name = kdfIsWeakerThan)]
pub fn kdf_is_weaker_than(kdf: &str, stored: &str) -> Result<bool, JsValue> {
    js(kdf_weaker(kdf, stored))
}

// ── Logging in and unlocking ──────────────────────────────
//
// Every password and PIN comes in as a `String`, not a `&str`: wasm-bindgen
// hands a `String` over, so it is wiped here when it drops, where the copy
// behind a `&str` would be freed as it is and linger in the module's memory.

/// The master key from the password, kept for [`unlock`]; the hash the server
/// gets.
#[wasm_bindgen(js_name = deriveLogin)]
pub fn derive_login(email: &str, password: String, kdf: &str) -> Result<String, JsValue> {
    let password = Zeroizing::new(password);
    js(session::derive_login(email, &password, kdf))
}

/// Opens the user key with the master key from [`derive_login`]. A wrong
/// master password shows up here.
#[wasm_bindgen]
pub fn unlock(email: &str, kdf: &str, protected_key: &str) -> Result<(), JsValue> {
    js(session::unlock(email, kdf, protected_key))
}

/// Derives the master key and opens the user key in one; the hash the server
/// gets, for a login that follows.
#[wasm_bindgen(js_name = unlockWithPassword)]
pub fn unlock_with_password(
    email: &str,
    kdf: &str,
    protected_key: &str,
    password: String,
) -> Result<String, JsValue> {
    let password = Zeroizing::new(password);
    js(session::unlock_with_password(
        email,
        kdf,
        protected_key,
        &password,
    ))
}

/// Opens the vault with the user key itself (base64 of its 64 bytes), kept
/// while the browser runs: a background worker that was stopped starts again
/// without asking for the password.
#[wasm_bindgen(js_name = unlockWithKey)]
pub fn unlock_with_key(
    email: &str,
    kdf: &str,
    protected_key: &str,
    user_key: &str,
) -> Result<(), JsValue> {
    js(session::unlock_with_key(
        email,
        kdf,
        protected_key,
        user_key,
    ))
}

/// The unlocked user key, base64 of its 64 bytes, for [`unlock_with_key`].
#[wasm_bindgen(js_name = userKey)]
pub fn user_key() -> Result<String, JsValue> {
    js(session::user_key())
}

/// The user key under a key made from a PIN, like Bitwarden's "unlock with
/// PIN": for [`unlock_with_pin`].
#[wasm_bindgen(js_name = pinProtect)]
pub fn pin_protect(pin: String) -> Result<String, JsValue> {
    let pin = Zeroizing::new(pin);
    js(session::pin_protect(&pin))
}

/// Opens the vault with the PIN and what [`pin_protect`] gave.
#[wasm_bindgen(js_name = unlockWithPin)]
pub fn unlock_with_pin(
    email: &str,
    kdf: &str,
    protected_key: &str,
    pin: String,
    pin_protected: &str,
) -> Result<(), JsValue> {
    let pin = Zeroizing::new(pin);
    js(session::unlock_with_pin(
        email,
        kdf,
        protected_key,
        &pin,
        pin_protected,
    ))
}

/// Wipes every key.
#[wasm_bindgen]
pub fn lock() {
    session::lock();
}

#[wasm_bindgen(js_name = isUnlocked)]
pub fn is_unlocked() -> bool {
    session::is_unlocked()
}

/// The master password again, for an item that asks for it before showing
/// anything.
#[wasm_bindgen(js_name = verifyReprompt)]
pub fn verify_reprompt(id: &str, password: String) -> Result<(), JsValue> {
    let password = Zeroizing::new(password);
    js(session::verify_reprompt(id, &password))
}

/// The master password hash, checked against the unlocked key first: for
/// everything the server asks the password for.
#[wasm_bindgen(js_name = passwordHash)]
pub fn password_hash(password: String) -> Result<String, JsValue> {
    let password = Zeroizing::new(password);
    js(with_unlocked(|unlocked| {
        session::check_password(unlocked, &password)
    }))
}

// ── The vault ─────────────────────────────────────────────

/// Opens a sync, as the server sent it (`/api/sync`).
#[wasm_bindgen]
pub fn open(sync: &str) -> Result<(), JsValue> {
    js(session::open(sync))
}

/// Folders, collections, organisations, and how many items were skipped.
#[wasm_bindgen]
pub fn overview() -> Result<String, JsValue> {
    js(view::overview())
}

#[wasm_bindgen]
pub fn items() -> Result<String, JsValue> {
    js(view::items())
}

#[wasm_bindgen]
pub fn item(id: &str) -> Result<String, JsValue> {
    js(view::item(id))
}

/// One secret of an item, by name (see `view::value_of`). `now` is
/// `Date.now() / 1000`, for a TOTP code.
#[wasm_bindgen]
pub fn reveal(id: &str, field: &str, now: f64) -> Result<String, JsValue> {
    js(view::reveal(id, field, now as u64))
}

#[wasm_bindgen]
pub fn totp(id: &str, now: f64) -> Result<String, JsValue> {
    js(view::totp_code(id, now as u64))
}

/// An item as the server takes it: a new one (`id` empty) or a change to one,
/// from what the editor sends. `now` is an ISO date for the password history.
#[wasm_bindgen(js_name = sealDraft)]
pub fn seal_draft(id: &str, draft: &str, now: &str) -> Result<String, JsValue> {
    js(draft::seal_draft(id, draft, now))
}

/// A login with only its password changed, the one before it kept in the
/// history: for "update the password?" after a form was sent.
#[wasm_bindgen(js_name = sealPassword)]
pub fn seal_password(id: &str, password: String, now: &str) -> Result<String, JsValue> {
    let password = Zeroizing::new(password);
    js(draft::seal_password(id, &password, now))
}

/// Text encrypted under the user key: a folder name.
#[wasm_bindgen(js_name = encryptText)]
pub fn encrypt_text(text: &str) -> Result<String, JsValue> {
    js(session::encrypt_text(text))
}

// ── The generator ─────────────────────────────────────────

/// A random password: `{"password", "bits"}`.
#[wasm_bindgen]
pub fn generate(options: &str) -> Result<String, JsValue> {
    js(generator::password(options))
}

/// A passphrase: `{"password", "bits"}`.
#[wasm_bindgen]
pub fn passphrase(options: &str) -> Result<String, JsValue> {
    js(generator::passphrase(options))
}

/// How strong a password is, in bits, for the strength meter.
#[wasm_bindgen(js_name = entropyBits)]
pub fn entropy_bits(password: String) -> u32 {
    let password = Zeroizing::new(password);
    uwulock_core::generator::entropy_bits(&password)
}

// ── UwULock Server's extras ───────────────────────────────

/// Opens the extras key from `GET /uwu/v1/keys`: `{"state": "open" | "none"
/// | "lost"}`. The extension never makes one nor wraps it again.
#[wasm_bindgen(js_name = openExtras)]
pub fn open_extras(keys: &str) -> Result<String, JsValue> {
    js(extras::open_extras(keys))
}

/// Own icons from `POST /uwu/v1/icons/own/get`, opened: `[{cipherId, png}]`,
/// the PNG as base64. Those that don't open are left out.
#[wasm_bindgen(js_name = openIcons)]
pub fn open_icons(icons: &str) -> Result<String, JsValue> {
    js(extras::open_icons(icons))
}

/// The owner's labels of file requests: `[{id, label}]`.
#[wasm_bindgen(js_name = fileRequestLabels)]
pub fn file_request_labels(requests: &str) -> Result<String, JsValue> {
    js(extras::file_request_labels(requests))
}

/// A file request's link, on the main host or a send domain.
#[wasm_bindgen(js_name = fileRequestLink)]
pub fn file_request_link(request: &str, base: &str, send_domain: bool) -> Result<String, JsValue> {
    js(extras::file_request_link(request, base, send_domain))
}

/// The names of an item's values that can be shared in a Send:
/// `[{name, label?}]`. Never the authenticator key.
#[wasm_bindgen(js_name = shareableFields)]
pub fn shareable_fields(id: &str) -> Result<String, JsValue> {
    js(extras::shareable_fields(id))
}

/// A text Send with chosen values of an item: the body of `POST /api/sends`.
#[wasm_bindgen(js_name = sealShare)]
pub fn seal_share(id: &str, options: &str) -> Result<String, JsValue> {
    js(extras::seal_share(id, options))
}

/// A Send's link from the `key` and `accessId` the server answered.
#[wasm_bindgen(js_name = sendLink)]
pub fn send_link(
    key: &str,
    access_id: &str,
    base: &str,
    send_domain: bool,
) -> Result<String, JsValue> {
    js(extras::send_link(key, access_id, base, send_domain))
}

// ── Autofill ──────────────────────────────────────────────

/// What the background matches pages against: every item that can be used,
/// with its addresses and passkeys, and no secrets.
#[wasm_bindgen(js_name = autofillIndex)]
pub fn autofill_index() -> Result<String, JsValue> {
    js(autofill::index())
}

/// The values to fill into a page from an item somebody picked.
#[wasm_bindgen(js_name = fillValues)]
pub fn fill_values(id: &str, now: f64) -> Result<String, JsValue> {
    js(autofill::fill_values(id, now as u64))
}

// ── Passkeys ──────────────────────────────────────────────

/// A new passkey for `navigator.credentials.create()`, saved with a login.
#[wasm_bindgen(js_name = passkeyCreate)]
pub fn passkey_create(request: &str) -> Result<String, JsValue> {
    js(passkeys::create(request))
}

/// A passkey's signature for `navigator.credentials.get()`.
#[wasm_bindgen(js_name = passkeyAssert)]
pub fn passkey_assert(request: &str) -> Result<String, JsValue> {
    js(passkeys::assert(request))
}
