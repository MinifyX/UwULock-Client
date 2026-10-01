//! The integration tests, one binary for the crate: linked once instead of once per file.
//!
//! - [`uwu`] — UwULock's own crypto: extras key, suite spaces, file requests
//! - [`vectors`] — known answers from Bitwarden's SDK
//! - [`wifi`] — Wi-Fi networks: a note with the marker field, unchanged through a save

mod uwu;
mod vectors;
mod wifi;
