//! UwULock's way into Bitwarden and Vaultwarden.
//!
//! - [`crypto`] — master key, password hash, keys and encrypted values
//! - [`api`] — prelogin, login with two-step login, token refresh, sync
//! - [`wire`] — what the server sends, as it sends it
//! - [`vault`] — the sync, decrypted: items, folders, collections
//! - [`totp`] — codes for items with an authenticator key
//! - [`generator`] — passwords
//!
//! Nothing in here touches the disk or the window; the desktop app decides
//! what is kept where.

pub mod api;
pub mod crypto;
pub mod generator;
pub mod totp;
pub mod vault;
pub mod wire;

pub use api::{Client, Device, LoginOutcome, Server, Session, TwoFactorMethod};
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
