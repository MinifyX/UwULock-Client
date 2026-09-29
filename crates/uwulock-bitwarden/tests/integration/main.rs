//! The integration tests, one binary for the crate: linked once instead of once per file.
//!
//! - [`flow`] — the whole way against the toy server: prelogin, two-step login, sync
//! - [`live`] — live updates against fakes of the realtime channel and the hub
//! - [`saving`] — creating, changing and deleting items, conflicts
//! - [`moving`] — moving a vault from a Bitwarden to a UwULock Server
//! - [`server`] — against a real UwULock Server, ignored by default (`scripts/live-server.sh`)
//! - [`suite`] — a suite app's keys and space against a fake UwULock Server
//! - [`uwu`] — UwULock Server's extras against a fake of `/uwu/v1`
//! - [`support`] — the toy server, the one moves go between, and a small fake HTTP server

mod flow;
mod live;
mod moving;
mod saving;
mod server;
mod suite;
mod support;
mod uwu;
