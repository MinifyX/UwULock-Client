//! The integration tests, one binary for the crate: linked once instead of once per file.
//!
//! - [`flow`] — the whole way against the toy server: prelogin, two-step login, sync
//! - [`live`] — live updates against fakes of the realtime channel and the hub
//! - [`saving`] — creating, changing and deleting items, conflicts
//! - [`uwu`] — UwULock Server's extras against a fake of `/uwu/v1`
//! - [`support`] — the toy server, and a small fake HTTP server

mod flow;
mod live;
mod saving;
mod support;
mod uwu;
