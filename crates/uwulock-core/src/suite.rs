//! The suite vault (contract §6): UwUSSH's and UwURDP's records as UwULock
//! keeps them, and what an editor needs to read and write them.
//!
//! The apps own their records — the payloads, the merge, the manifests. This
//! module speaks their format so the web vault and UwULock's apps can show and
//! change those records without breaking anything an app relies on:
//!
//! - [`Envelope`] and the other wire types, in the contract's camelCase;
//! - [`Hlc`], UwUSync's hybrid logical clock (`uwussh-proto` `clock.rs`);
//! - [`Space`]: which spaces there are, their AAD prefix and their kinds
//!   (name on the wire ↔ discriminant in the AAD);
//! - [`associated_data`], exactly §6.2, and [`SpaceVault`], which seals and
//!   opens records and tombstones (XChaCha20-Poly1305 under the space key);
//! - the editor's steps: [`SpaceVault::open_record`] (JSON with every field
//!   kept, or the raw bytes of a `secret`), [`SpaceVault::seal_edit`],
//!   [`SpaceVault::seal_new`], [`SpaceVault::seal_tombstone`];
//! - [`openssh`]: a new Ed25519 key as OpenSSH text, its public line and
//!   fingerprint, and what an imported key is.
//!
//! The rules every writer follows: an edit is a new envelope with the same id
//! and kind, a clock strictly after the record's and not before the wall
//! clock, `baseSeq` = the record's `seq`, a fresh nonce. Payloads are edited as
//! JSON objects, so fields this build doesn't know survive. `manifest` records
//! are never written here.

pub mod openssh;

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload as AeadPayload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::SymmetricKey;
use crate::extras::SpaceKey;
use crate::Error;

/// `schema` of a push.
pub const SCHEMA: u32 = 2;
/// The most records one push or pull carries.
pub const PAGE: usize = 500;
/// The largest sealed payload (`blob`) the protocol carries.
pub const MAX_BLOB_BYTES: usize = 256 * 1024;
/// XChaCha20-Poly1305's nonce.
pub const NONCE_LEN: usize = 24;
/// The kind whose payload is raw bytes (a password, a private key, a
/// passphrase), not JSON. The same name in every space.
pub const KIND_SECRET: &str = "secret";
/// What one device holds. Never shown, never written by UwULock.
pub const KIND_MANIFEST: &str = "manifest";

// ── Spaces and kinds ───────────────────────────────────────

/// A space: one app's data in one account (§6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Space {
    /// UwUSSH.
    Ssh,
    /// UwURDP.
    Rdp,
    /// UwUMail apps (reserved).
    Mail,
    /// Any other UwU app.
    Generic,
}

const SSH_KINDS: &[(&str, u8)] = &[
    ("host", 0),
    ("group", 1),
    ("identity", 2),
    ("key", 3),
    ("snippet", 4),
    ("port_forward", 5),
    ("known_host", 6),
    ("terminal_profile", 7),
    ("secret", 8),
    ("manifest", 9),
    ("assist_config", 10),
    ("assist_cache", 11),
];

/// UwURDP's `uwurdp-proto` `EntityKind`: the same numbers as UwUSSH's up to
/// `manifest`.
const RDP_KINDS: &[(&str, u8)] = &[
    ("host", 0),
    ("group", 1),
    ("identity", 2),
    ("key", 3),
    ("snippet", 4),
    ("port_forward", 5),
    ("known_host", 6),
    ("terminal_profile", 7),
    ("secret", 8),
    ("manifest", 9),
];

const MAIL_KINDS: &[(&str, u8)] = &[("account", 0), ("secret", 8), ("manifest", 9)];
const GENERIC_KINDS: &[(&str, u8)] = &[("item", 0), ("secret", 8), ("manifest", 9)];

impl Space {
    pub const ALL: [Space; 4] = [Space::Ssh, Space::Rdp, Space::Mail, Space::Generic];

    /// The name in paths and in `suiteSpace.space`.
    pub fn as_str(self) -> &'static str {
        match self {
            Space::Ssh => "ssh",
            Space::Rdp => "rdp",
            Space::Mail => "mail",
            Space::Generic => "generic",
        }
    }

    /// The first bytes of every record's associated data (§6.2).
    pub fn prefix(self) -> &'static [u8] {
        match self {
            Space::Ssh => b"uwussh/record/v2",
            Space::Rdp => b"uwurdp/record/v2",
            Space::Mail | Space::Generic => b"uwulock/suite/v1",
        }
    }

    /// Every kind of the space: the name on the wire and the discriminant in
    /// the associated data. Append only.
    pub fn kinds(self) -> &'static [(&'static str, u8)] {
        match self {
            Space::Ssh => SSH_KINDS,
            Space::Rdp => RDP_KINDS,
            Space::Mail => MAIL_KINDS,
            Space::Generic => GENERIC_KINDS,
        }
    }

    /// The discriminant of a kind name, `None` for one this build doesn't
    /// know (such a record is passed on, not opened).
    pub fn kind_discriminant(self, kind: &str) -> Option<u8> {
        self.kinds()
            .iter()
            .find(|(name, _)| *name == kind)
            .map(|(_, d)| *d)
    }

    /// The name of a discriminant.
    pub fn kind_name(self, discriminant: u8) -> Option<&'static str> {
        self.kinds()
            .iter()
            .find(|(_, d)| *d == discriminant)
            .map(|(name, _)| *name)
    }
}

impl fmt::Display for Space {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Space {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        Space::ALL
            .into_iter()
            .find(|space| space.as_str() == s)
            .ok_or_else(|| Error::Unsupported(format!("suite space {s:?}")))
    }
}

// ── The clock ──────────────────────────────────────────────

/// A record's `updatedAt`: UwUSync's hybrid logical clock. Ordered by wall
/// time, then counter, then device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hlc {
    /// Milliseconds since the Unix epoch.
    pub wall_ms: u64,
    /// Breaks ties within one millisecond.
    pub counter: u32,
    /// The device that wrote it, only to break exact ties.
    pub device: u32,
}

impl Hlc {
    pub fn new(wall_ms: u64, counter: u32, device: u32) -> Self {
        Hlc {
            wall_ms,
            counter,
            device,
        }
    }

    /// The next timestamp after this one, as `uwussh-proto`'s `Hlc::tick`:
    /// the wall clock when it moved on, otherwise the counter advances (a
    /// counter at its end moves on to the next millisecond). Keeps the device.
    pub fn tick(self, now_ms: u64) -> Self {
        if now_ms > self.wall_ms {
            Hlc::new(now_ms, 0, self.device)
        } else {
            match self.counter.checked_add(1) {
                Some(counter) => Hlc::new(self.wall_ms, counter, self.device),
                None => Hlc::new(self.wall_ms.saturating_add(1), 0, self.device),
            }
        }
    }

    /// The clock of an edit by `device` of a record last written at
    /// `previous` (by whichever device): strictly after `previous`, and not
    /// before `now_ms`.
    pub fn after(previous: Hlc, now_ms: u64, device: u32) -> Self {
        Hlc { device, ..previous }.tick(now_ms)
    }
}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> Ordering {
        self.wall_ms
            .cmp(&other.wall_ms)
            .then(self.counter.cmp(&other.counter))
            .then(self.device.cmp(&other.device))
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A device id for [`Hlc::device`]: random, never 0. Kept per install (the
/// web vault: per browser and account).
pub fn new_device_id() -> u32 {
    loop {
        let id = rand::rngs::OsRng.next_u32();
        if id != 0 {
            return id;
        }
    }
}

/// A random UUID (v4): a new record's id, or a new space's.
pub fn new_id() -> Uuid {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    uuid::Builder::from_random_bytes(bytes).into_uuid()
}

// ── Wire types ─────────────────────────────────────────────

/// `suiteSpace` (§6.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SuiteSpace {
    pub space: String,
    /// The `vault_id` of every record's associated data.
    pub id: String,
    /// The space key under the extras key.
    pub key: String,
    pub records: u64,
    pub bytes: u64,
    pub creation_date: Option<String>,
    pub revision_date: Option<String>,
}

/// The body of `PUT /uwu/v1/suite/spaces/{space}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSpaceRequest {
    pub id: String,
    pub key: String,
}

/// One record as it travels (§6.3). `id`, `nonce` and `blob` stay text, as
/// on the wire: a record of a kind this build doesn't know is passed on
/// untouched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub id: String,
    pub kind: String,
    pub updated_at: Hlc,
    /// What the writer last saw of this id, 0 for a new one.
    #[serde(default)]
    pub base_seq: u64,
    #[serde(default)]
    pub deleted: bool,
    /// Base64, 24 bytes.
    pub nonce: String,
    /// Base64: ciphertext and tag.
    pub blob: String,
    /// Set by the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

impl Envelope {
    /// What an edit or a tombstone of this record starts from.
    pub fn head(&self) -> Result<RecordHead, Error> {
        Ok(RecordHead {
            id: parse_id(&self.id)?,
            kind: self.kind.clone(),
            updated_at: self.updated_at,
            seq: self.seq.unwrap_or(0),
        })
    }
}

/// `suitePull`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pull {
    /// `since` was too old: pull again from 0 and merge as after a fresh
    /// install.
    pub reset: bool,
    pub records: Vec<Envelope>,
    pub cursor: u64,
    pub has_more: bool,
}

/// The body of a push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    pub schema: u32,
    /// The space id the records were sealed for: a push after a rekey is
    /// refused with 409 `space_changed` instead of written.
    pub space_id: String,
    pub records: Vec<Envelope>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Accepted {
    pub id: String,
    pub seq: u64,
}

/// `suitePush`: what was taken, and the server's copies of what wasn't.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pushed {
    pub accepted: Vec<Accepted>,
    pub conflicts: Vec<Envelope>,
    pub cursor: u64,
}

// ── Sealing ────────────────────────────────────────────────

/// The associated data of a record (§6.2): `prefix || id || kind || spaceId
/// || wallMs (u64 BE) || counter (u32 BE) || device (u32 BE) || deleted`.
pub fn associated_data(
    space: Space,
    id: &Uuid,
    kind: u8,
    space_id: &Uuid,
    updated_at: Hlc,
    deleted: bool,
) -> Vec<u8> {
    let prefix = space.prefix();
    let mut aad = Vec::with_capacity(prefix.len() + 16 + 1 + 16 + 16 + 1);
    aad.extend_from_slice(prefix);
    aad.extend_from_slice(id.as_bytes());
    aad.push(kind);
    aad.extend_from_slice(space_id.as_bytes());
    aad.extend_from_slice(&updated_at.wall_ms.to_be_bytes());
    aad.extend_from_slice(&updated_at.counter.to_be_bytes());
    aad.extend_from_slice(&updated_at.device.to_be_bytes());
    aad.push(u8::from(deleted));
    aad
}

/// Everything of an envelope but the sealed payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordHeader {
    pub id: Uuid,
    pub kind: String,
    pub updated_at: Hlc,
    pub deleted: bool,
    pub base_seq: u64,
}

/// Where an edit or a tombstone starts from: a record as the server holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordHead {
    pub id: Uuid,
    pub kind: String,
    pub updated_at: Hlc,
    /// The server's `seq`, which becomes the edit's `baseSeq`.
    pub seq: u64,
}

/// A record's plaintext, as an editor sees it.
///
/// On the wire as `{"json": {...}}`, `{"text": "…"}` or `{"bytes":
/// "<base64>"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Payload {
    /// Every kind but `secret`: the JSON object, with every field it had.
    Json(Value),
    /// A `secret` that is UTF-8 (a password, a passphrase, a private key in
    /// OpenSSH or PEM text).
    Text(String),
    /// A `secret` that isn't UTF-8, or a payload that isn't JSON.
    Bytes(#[serde(with = "base64_bytes")] Vec<u8>),
}

impl Payload {
    fn to_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error> {
        Ok(Zeroizing::new(match self {
            Payload::Json(value) => serde_json::to_vec(value)
                .map_err(|e| Error::Crypto(format!("a payload doesn't serialise: {e}")))?,
            Payload::Text(text) => text.as_bytes().to_vec(),
            Payload::Bytes(bytes) => bytes.clone(),
        }))
    }
}

/// A record, opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedRecord {
    pub id: Uuid,
    pub kind: String,
    pub updated_at: Hlc,
    pub seq: u64,
    pub deleted: bool,
    /// `None` for a tombstone.
    pub payload: Option<Payload>,
}

impl OpenedRecord {
    pub fn head(&self) -> RecordHead {
        RecordHead {
            id: self.id,
            kind: self.kind.clone(),
            updated_at: self.updated_at,
            seq: self.seq,
        }
    }
}

/// One space, opened: its name, id and key. Seals and opens its records.
#[derive(Debug, Clone)]
pub struct SpaceVault {
    pub space: Space,
    pub id: Uuid,
    key: SpaceKey,
}

impl SpaceVault {
    pub fn new(space: Space, id: Uuid, key: SpaceKey) -> Self {
        SpaceVault { space, id, key }
    }

    /// A space as `GET /uwu/v1/suite/spaces` lists it, opened with the
    /// extras key.
    pub fn open_space(found: &SuiteSpace, extras: &SymmetricKey) -> Result<Self, Error> {
        let space = found.space.parse()?;
        let id = parse_id(&found.id)?;
        Ok(SpaceVault::new(
            space,
            id,
            SpaceKey::unwrap(&found.key, extras)?,
        ))
    }

    /// A new space with a fresh id and key, and the body that makes it
    /// (`PUT /uwu/v1/suite/spaces/{space}`). On 409 `exists` another device
    /// was quicker: list the spaces again and [`SpaceVault::open_space`]
    /// that one.
    pub fn create(space: Space, extras: &SymmetricKey) -> (Self, CreateSpaceRequest) {
        let vault = SpaceVault::new(space, new_id(), SpaceKey::generate());
        let request = CreateSpaceRequest {
            id: vault.id.to_string(),
            key: vault.key.wrap(extras),
        };
        (vault, request)
    }

    pub fn key(&self) -> &SpaceKey {
        &self.key
    }

    /// The body of a push of `records`.
    pub fn push_request(&self, records: Vec<Envelope>) -> PushRequest {
        PushRequest {
            schema: SCHEMA,
            space_id: self.id.to_string(),
            records,
        }
    }

    /// Seals `plaintext` under a fresh random nonce. A tombstone seals an
    /// empty payload.
    pub fn seal(&self, header: &RecordHeader, plaintext: &[u8]) -> Result<Envelope, Error> {
        let mut nonce = [0u8; NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        self.seal_with_nonce(header, plaintext, nonce)
    }

    /// [`SpaceVault::seal`] with a given nonce. Only for known-answer tests:
    /// a nonce must never be used twice under one key.
    pub fn seal_with_nonce(
        &self,
        header: &RecordHeader,
        plaintext: &[u8],
        nonce: [u8; NONCE_LEN],
    ) -> Result<Envelope, Error> {
        let kind = self.discriminant(&header.kind)?;
        let aad = associated_data(
            self.space,
            &header.id,
            kind,
            &self.id,
            header.updated_at,
            header.deleted,
        );
        let blob = XChaCha20Poly1305::new(self.key.as_bytes().into())
            .encrypt(
                XNonce::from_slice(&nonce),
                AeadPayload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| Error::Crypto("a record doesn't seal".into()))?;
        if blob.len() > MAX_BLOB_BYTES {
            return Err(Error::Refused(format!(
                "a record is at most {} KiB sealed",
                MAX_BLOB_BYTES / 1024
            )));
        }
        Ok(Envelope {
            id: header.id.to_string(),
            kind: header.kind.clone(),
            updated_at: header.updated_at,
            base_seq: header.base_seq,
            deleted: header.deleted,
            nonce: B64.encode(nonce),
            blob: B64.encode(blob),
            seq: None,
        })
    }

    /// Opens a record: its plaintext (empty for a tombstone). Fails for a
    /// kind this build doesn't know, and for one that was changed on the way
    /// or sealed for another record, space or clock.
    pub fn open(&self, envelope: &Envelope) -> Result<Zeroizing<Vec<u8>>, Error> {
        let id = parse_id(&envelope.id)?;
        let kind = self.discriminant(&envelope.kind)?;
        let nonce = B64
            .decode(&envelope.nonce)
            .ok()
            .filter(|n| n.len() == NONCE_LEN)
            .ok_or_else(|| Error::Crypto("a record's nonce isn't 24 bytes of base64".into()))?;
        let blob = B64
            .decode(&envelope.blob)
            .map_err(|_| Error::Crypto("a record's blob isn't base64".into()))?;
        let aad = associated_data(
            self.space,
            &id,
            kind,
            &self.id,
            envelope.updated_at,
            envelope.deleted,
        );
        XChaCha20Poly1305::new(self.key.as_bytes().into())
            .decrypt(
                XNonce::from_slice(&nonce),
                AeadPayload {
                    msg: &blob,
                    aad: &aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| {
                Error::Crypto(format!(
                    "the {} record {} doesn't open: another key, or changed on the way",
                    envelope.kind, envelope.id
                ))
            })
    }

    /// Opens a record for an editor: a `secret` as text (or bytes when it
    /// isn't UTF-8), a `manifest` as bytes, anything else as its JSON with
    /// every field kept. A tombstone has no payload.
    pub fn open_record(&self, envelope: &Envelope) -> Result<OpenedRecord, Error> {
        let plain = self.open(envelope)?;
        let payload = if envelope.deleted {
            None
        } else if envelope.kind == KIND_SECRET {
            Some(match std::str::from_utf8(&plain) {
                Ok(text) => Payload::Text(text.to_owned()),
                Err(_) => Payload::Bytes(plain.to_vec()),
            })
        } else if envelope.kind == KIND_MANIFEST {
            Some(Payload::Bytes(plain.to_vec()))
        } else {
            Some(match serde_json::from_slice::<Value>(&plain) {
                Ok(value) => Payload::Json(value),
                Err(_) => Payload::Bytes(plain.to_vec()),
            })
        };
        Ok(OpenedRecord {
            id: parse_id(&envelope.id)?,
            kind: envelope.kind.clone(),
            updated_at: envelope.updated_at,
            seq: envelope.seq.unwrap_or(0),
            deleted: envelope.deleted,
            payload,
        })
    }

    /// A new record of `kind` with a random id: `baseSeq` 0, clock now.
    pub fn seal_new(
        &self,
        kind: &str,
        payload: &Payload,
        now_ms: u64,
        device: u32,
    ) -> Result<Envelope, Error> {
        self.check_writable(kind, payload)?;
        let header = RecordHeader {
            id: new_id(),
            kind: kind.to_owned(),
            updated_at: Hlc::new(now_ms, 0, device),
            deleted: false,
            base_seq: 0,
        };
        self.seal(&header, &payload.to_bytes()?)
    }

    /// An edit of the record `head`: same id and kind, the clock after
    /// [`Hlc::after`], `baseSeq` = its `seq`. Also brings a tombstoned id
    /// back.
    pub fn seal_edit(
        &self,
        head: &RecordHead,
        payload: &Payload,
        now_ms: u64,
        device: u32,
    ) -> Result<Envelope, Error> {
        self.check_writable(&head.kind, payload)?;
        let header = RecordHeader {
            id: head.id,
            kind: head.kind.clone(),
            updated_at: Hlc::after(head.updated_at, now_ms, device),
            deleted: false,
            base_seq: head.seq,
        };
        self.seal(&header, &payload.to_bytes()?)
    }

    /// The tombstone of the record `head`: `deleted`, a sealed empty
    /// payload, the next clock, `baseSeq` = its `seq`.
    pub fn seal_tombstone(
        &self,
        head: &RecordHead,
        now_ms: u64,
        device: u32,
    ) -> Result<Envelope, Error> {
        if head.kind == KIND_MANIFEST {
            return Err(manifest_refused());
        }
        let header = RecordHeader {
            id: head.id,
            kind: head.kind.clone(),
            updated_at: Hlc::after(head.updated_at, now_ms, device),
            deleted: true,
            base_seq: head.seq,
        };
        self.seal(&header, &[])
    }

    fn discriminant(&self, kind: &str) -> Result<u8, Error> {
        self.space.kind_discriminant(kind).ok_or_else(|| {
            Error::Unsupported(format!("records of kind {kind:?} in space {}", self.space))
        })
    }

    /// A `secret` is text or bytes, every other kind JSON; a `manifest` is
    /// the apps' own.
    fn check_writable(&self, kind: &str, payload: &Payload) -> Result<(), Error> {
        self.discriminant(kind)?;
        if kind == KIND_MANIFEST {
            return Err(manifest_refused());
        }
        let fits = match payload {
            Payload::Json(_) => kind != KIND_SECRET,
            Payload::Text(_) | Payload::Bytes(_) => kind == KIND_SECRET,
        };
        if fits {
            Ok(())
        } else {
            Err(Error::Refused(format!(
                "a {kind} record's payload is {}",
                if kind == KIND_SECRET {
                    "text or bytes"
                } else {
                    "JSON"
                }
            )))
        }
    }
}

fn manifest_refused() -> Error {
    Error::Refused("manifest records are the apps' own".into())
}

fn parse_id(id: &str) -> Result<Uuid, Error> {
    Uuid::parse_str(id).map_err(|_| Error::Crypto(format!("{id:?} isn't a UUID")))
}

mod base64_bytes {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        B64.decode(text).map_err(serde::de::Error::custom)
    }
}
