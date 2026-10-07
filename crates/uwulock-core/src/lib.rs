//! Bitwarden's crypto and data formats, without a network.
//!
//! - [`crypto`] — master key, password hash, keys and encrypted values;
//!   files, Sends, key pairs, passkeys that unlock, fingerprint phrases
//! - [`wire`] — what the server sends, as it sends it
//! - [`vault`] — the sync, decrypted: items, folders, collections
//! - [`totp`] — codes for items with an authenticator key
//! - [`passkey`] — passkeys that sign in, in Bitwarden's format: a WebAuthn
//!   authenticator's keys, authenticator data and signatures
//! - [`generator`] — passwords and passphrases
//! - [`send`] — text Sends, and sharing an item as one
//! - [`entry_send`] — an item shared as an entry Send: readable lines plus
//!   the `uwulock-entry:v1:` marker the Send page shows as an entry
//! - [`extras`] — UwULock's extras key (and what is under it: suite space
//!   keys, own icons), entry versions in a key rotation
//! - [`file_request`] — file requests: the link, its public details, the
//!   envelope of what somebody uploads
//! - [`suite`] — the suite vault: UwUSSH's and UwURDP's records, sealed and
//!   opened for an editor; new SSH keys
//! - [`health`] — the password check: weak, reused, breached, sites with a
//!   breach or with two-step login; the cards of the review, the ignore list
//! - [`import`] — moving in from a file: Bitwarden's JSON and CSV exports
//!   (what the apps' import module makes of every other app's), its
//!   password-protected JSON, KeePass's key derivations
//!
//! No HTTP, no disk, no clock that isn't passed in where it matters: the
//! desktop app uses this through `uwulock-bitwarden`; the browser extension
//! (through `uwulock-wasm`) and the web vault of UwULock-Server use it
//! compiled to WebAssembly (`wasm32-unknown-unknown`).
//! There, [`totp::Totp::now`] can't be used (the standard clock panics in a
//! browser); pass the time to [`totp::Totp::code_at`] instead, e.g.
//! `Date.now() / 1000`.

pub mod crypto;
pub mod entry_send;
pub mod extras;
pub mod file_request;
pub mod generator;
pub mod health;
pub mod import;
pub mod passkey;
pub mod send;
pub mod suite;
pub mod totp;
pub mod vault;
pub mod wire;

pub use crypto::{EncString, Kdf, SymmetricKey};
pub use vault::{Item, ItemKind, Vault};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No answer from the server: offline, wrong address, TLS.
    #[error("{0}")]
    Network(String),
    /// The server answered, but not with what was asked for.
    #[error("{message}")]
    Server { status: u16, message: String },
    /// Email, master password or two-step code were refused.
    #[error("{0}")]
    Refused(String),
    /// The session is gone: logged out elsewhere, password changed, device removed.
    #[error("the session has expired")]
    SessionExpired,
    /// The item changed somewhere else since the last sync. The server keeps
    /// the newer copy rather than letting this save overwrite it.
    #[error("the item has changed on the server since the last sync")]
    Conflict,
    /// A MAC didn't match: the wrong key, which usually means the wrong master password.
    #[error("wrong key")]
    WrongKey,
    #[error("{0}")]
    Crypto(String),
    /// Something Bitwarden can do that this beta can't yet.
    #[error("not supported yet: {0}")]
    Unsupported(String),
}
