//! The integration tests, one binary for the crate: linked once instead of once per file.
//!
//! - [`flow`] — the whole way against the toy server: prelogin, two-step login, sync
//! - [`saving`] — creating, changing and deleting items, conflicts
//! - [`moving`] — moving a vault from a Bitwarden to a UwULock Server
//! - [`support`] — the toy server, and the one moves go between

mod flow;
mod moving;
mod saving;
mod support;
