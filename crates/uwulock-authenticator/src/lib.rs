//! UwULock as the system's passkey provider — the protocol half.
//!
//! - [`broker`] — the app's side of the Linux uhid broker (`/dev/uhid` stays root's)
//! - [`cbor`] — the CBOR CTAP2 speaks
//! - [`ctap2`] — requests and answers of an authenticator, and the
//!   [`ctap2::Authenticator`] that turns one into the other with the app's
//!   [`ctap2::Backend`] (Linux's virtual security key, Windows' plugin)
//! - [`ctaphid`] — CTAP2 over HID reports (Linux)
//! - [`flight`] — the one request with the person, cancelled and cleared by identity
//! - [`opsign`] — checks Windows' signatures on a plugin's requests
//! - [`rpid`] — which relying party ids are taken (no public suffixes)
//! - [`uhid`] — Linux's `/dev/uhid` events, for a HID device made by UwULock
//! - [`webauthn`] — WebAuthn's JSON (Android's Credential Manager)
//! - [`apple`] — the sealed passkey list and outbox of the iOS/macOS extension
//! - [`autofill`] — which logins belong to a site or an app (match detection)
//!
//! The app holds the vault, asks the person and saves; nothing here touches
//! a device or the network, so all of it is tested on its own. The design
//! and its threat model: docs/passkeys.md.

pub mod apple;
pub mod autofill;
pub mod broker;
pub mod cbor;
pub mod ctap2;
pub mod ctaphid;
pub mod flight;
pub mod opsign;
pub mod rpid;
pub mod uhid;
pub mod webauthn;
