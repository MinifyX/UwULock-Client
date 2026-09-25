//! Bitwarden's crypto and data formats, without a network.
//!
//! - [`crypto`] — master key, password hash, keys and encrypted values
//! - [`wire`] — what the server sends, as it sends it
//! - [`vault`] — the sync, decrypted: items, folders, collections
//! - [`totp`] — codes for items with an authenticator key
//! - [`generator`] — passwords
//!
//! No HTTP, no disk, no clock that isn't passed in where it matters: the
//! desktop app uses this through `uwulock-bitwarden`, and the web vault of
//! UwULock-Server uses it compiled to WebAssembly (`wasm32-unknown-unknown`).
//! There, [`totp::Totp::now`] can't be used (the standard clock panics in a
//! browser); pass the time to [`totp::Totp::code_at`] instead, e.g.
//! `Date.now() / 1000`.

pub mod crypto;
pub mod generator;
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
