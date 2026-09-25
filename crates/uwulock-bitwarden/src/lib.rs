//! UwULock's way into Bitwarden and Vaultwarden.
//!
//! - [`api`] — prelogin, login with two-step login, token refresh, sync, saving
//!
//! The crypto and the data formats live in `uwulock-core` (no network, also
//! built for WebAssembly) and are re-exported here under their old paths:
//!
//! - [`crypto`] — master key, password hash, keys and encrypted values
//! - [`wire`] — what the server sends, as it sends it
//! - [`vault`] — the sync, decrypted: items, folders, collections
//! - [`totp`] — codes for items with an authenticator key
//! - [`generator`] — passwords
//!
//! Nothing in here touches the disk or the window; the desktop app decides
//! what is kept where.

pub mod api;

pub use uwulock_core::{crypto, generator, totp, vault, wire, Error};

pub use api::{Client, Device, LoginOutcome, Server, Session, TwoFactorMethod};
pub use crypto::{EncString, Kdf, SymmetricKey};
pub use vault::{Item, ItemKind, Vault};
