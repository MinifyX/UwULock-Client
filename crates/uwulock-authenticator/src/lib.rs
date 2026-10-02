//! UwULock as the system's passkey provider — the protocol half.
//!
//! - [`cbor`] — the CBOR CTAP2 speaks
//! - [`ctap2`] — requests and answers of an authenticator, and the
//!   [`ctap2::Authenticator`] that turns one into the other with the app's
//!   [`ctap2::Backend`] (Linux's virtual security key, Windows' plugin)
//! - [`ctaphid`] — CTAP2 over HID reports (Linux)
//! - [`uhid`] — Linux's `/dev/uhid` events, for a HID device made by UwULock
//! - [`webauthn`] — WebAuthn's JSON (Android's Credential Manager)
//! - [`apple`] — the sealed passkey list and outbox of the iOS/macOS extension
//!
//! The app holds the vault, asks the person and saves; nothing here touches
//! a device or the network, so all of it is tested on its own. The design
//! and its threat model: docs/passkeys.md.

pub mod apple;
pub mod cbor;
pub mod ctap2;
pub mod ctaphid;
pub mod uhid;
pub mod webauthn;
