//! The integration tests, one binary for the crate: linked once instead of once per file.
//!
//! - [`health`] — the password check: hashes, report, breached sites, 2FA, ignore list, cards
//! - [`uwu`] — UwULock's own crypto: extras key, suite spaces, file requests
//! - [`suite`] — the suite vault: records sealed by UwUSSH and UwURDP, the editor's steps,
//!   SSH keys
//! - [`vectors`] — known answers from Bitwarden's SDK
//! - [`wifi`] — Wi-Fi networks: a note with the marker field, unchanged through a save

mod health;
mod suite;
mod uwu;
mod vectors;
mod wifi;
