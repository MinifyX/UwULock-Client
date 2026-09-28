//! UwULock's way into Bitwarden and Vaultwarden.
//!
//! - [`api`] — prelogin, login with two-step login, token refresh, sync, saving
//! - [`delta`] — the offline copy, kept up to date by UwULock Server's delta sync
//! - [`uwu`] — UwULock Server's own API: its features, the extras key, delta sync
//! - [`moving`] — moving a vault from Bitwarden or Vaultwarden to a UwULock Server
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
pub mod delta;
pub mod moving;
pub mod uwu;

pub use uwulock_core::{crypto, generator, totp, vault, wire, Error};

pub use api::{Client, Device, LoginOutcome, Server, Session, TwoFactorMethod};
pub use crypto::{EncString, Kdf, SymmetricKey};
pub use vault::{Item, ItemKind, Vault};
