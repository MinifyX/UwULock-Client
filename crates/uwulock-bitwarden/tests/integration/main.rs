//! The integration tests, one binary for the crate: linked once instead of once per file.
//!
//! - [`flow`] — the whole way against the toy server: prelogin, two-step login, sync
//! - [`live`] — live updates against fakes of the realtime channel and the hub
//! - [`saving`] — creating, changing and deleting items, conflicts
//! - [`suite`] — a suite app's keys and space against a fake UwULock Server
//! - [`support`] — the toy server both use

mod flow;
mod live;
mod saving;
mod suite;
mod support;
