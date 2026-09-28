//! The integration tests, one binary for the crate: linked once instead of once per file.
//!
//! - [`flow`] — the whole way against the toy server: prelogin, two-step login, sync
//! - [`saving`] — creating, changing and deleting items, conflicts
//! - [`support`] — the toy server both use

mod flow;
mod saving;
mod support;
