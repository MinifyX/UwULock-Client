//! UwULock's way into Bitwarden and Vaultwarden.
//!
//! - [`api`] — prelogin, login with two-step login, token refresh, sync, saving
//! - [`delta`] — the offline copy, kept up to date by UwULock Server's delta sync
//! - [`health`] — the password check's calls: breach sources, lists, the ignore list
//! - [`icons`] — icons of devices on the local network, made into own icons
//! - [`live`] — live updates: UwULock's realtime channel, Bitwarden's SignalR hub
//! - [`moving`] — moving a vault from Bitwarden or Vaultwarden to a UwULock Server
//! - [`suite`] — the suite vault for UwUSSH and UwURDP: spaces, their keys, pull and push
//! - [`uwu`] — UwULock Server's own API: its features, the extras key, delta sync,
//!   and the calls behind its extras (icons, versions, file requests, …)
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
pub mod health;
pub mod icons;
pub mod live;
pub mod moving;
pub mod suite;
pub mod uwu;

pub use uwulock_core::{
    crypto, entry_send, extras, file_request, generator, import, passkey, send, totp, vault, wire,
    Error,
};

pub use api::{App, Client, Device, LoginOutcome, Server, Session, TwoFactorMethod};
pub use crypto::{EncString, Kdf, SymmetricKey};
pub use vault::{Item, ItemKind, Vault};
