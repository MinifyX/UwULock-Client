//! What the suite sections do with the records, without a window or a
//! server: the records as this device last pulled them, the list the page
//! gets, and how an edit becomes envelopes (UwULock-Server
//! `docs/uwu-api.md` §6, the sync rules of the apps).
//!
//! - A record is edited as its JSON object: the page sends the fields it
//!   changed ([`merge`]), everything else — fields of a newer UwUSSH or
//!   UwURDP — stays as it was.
//! - Deleting is a tombstone, as in the apps: a host takes its port forwards
//!   along; a group leaves its hosts without a group; an identity or a key
//!   goes only when nothing points at it any more, and takes its secrets.
//! - `manifest`, the assistant's kinds and kinds this build doesn't know are
//!   never shown and never written; they stay on the server as they are.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;
use zeroize::Zeroizing;

use uwulock_core::suite::{
    Envelope, Hlc, OpenedRecord, Payload, Pushed, RecordHead, Space, SpaceVault, KIND_SECRET,
};

/// Kinds the page may create and edit.
const EDITABLE: &[&str] = &[
    "host",
    "group",
    "identity",
    "key",
    "snippet",
    "port_forward",
];

/// Kinds the page sees. `secret` only as "there is one", never its value.
const SHOWN: &[&str] = &[
    "host",
    "group",
    "identity",
    "key",
    "snippet",
    "port_forward",
    "known_host",
    KIND_SECRET,
];

/// The order the apps apply a batch in: what is pointed at before what
/// points at it. A push goes in this order too.
const ORDER: &[&str] = &[
    KIND_SECRET,
    "key",
    "identity",
    "group",
    "host",
    "port_forward",
    "snippet",
    "known_host",
];

/// Why a plan was refused. `kind` is the page's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
}

fn refuse(kind: &'static str, message: impl Into<String>) -> Refusal {
    Refusal {
        kind,
        message: message.into(),
    }
}

/// One record as this device last saw it on the server.
#[derive(Debug, Clone)]
struct Stored {
    env: Envelope,
    /// The opened JSON of a shown kind (not a secret), `None` otherwise.
    json: Option<Value>,
    /// A shown kind that didn't open, or isn't a JSON object.
    broken: bool,
}

impl Stored {
    fn live(&self) -> bool {
        !self.env.deleted
    }

    fn seq(&self) -> u64 {
        self.env.seq.unwrap_or(0)
    }
}

/// One space, opened, with its records.
#[derive(Debug, Clone)]
pub(crate) struct SpaceState {
    pub(crate) vault: SpaceVault,
    pub(crate) cursor: u64,
    records: BTreeMap<String, Stored>,
}

/// A record as the page gets it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordView {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) seq: u64,
    pub(crate) updated_ms: u64,
    /// The payload, every field of it; `None` for a secret or a broken one.
    pub(crate) data: Option<Value>,
    pub(crate) broken: bool,
    /// Identities and keys: the records that use them (hosts and groups, or
    /// identities). Such a one can't be deleted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) used_by: Vec<String>,
}

/// What the page asks for, in one batch.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub(crate) enum Op {
    /// Create or change a JSON record: `patch` holds the fields that change.
    /// `seq` is the version the page edited (absent for a new one).
    #[serde(rename_all = "camelCase")]
    Put {
        id: String,
        kind: String,
        #[serde(default)]
        seq: Option<u64>,
        patch: Value,
    },
    /// Create or change a secret: its text (a password, a passphrase, a key).
    #[serde(rename_all = "camelCase")]
    Secret {
        id: String,
        text: String,
        #[serde(default)]
        seq: Option<u64>,
    },
    /// Delete a record, with what goes along with it.
    #[serde(rename_all = "camelCase")]
    Delete {
        id: String,
        #[serde(default)]
        seq: Option<u64>,
    },
}

/// What a record becomes in a plan.
enum Next {
    Json(Value),
    Secret(Zeroizing<Vec<u8>>),
    Tombstone,
}

/// Objects merge field by field, anything else replaces; `null` is a value
/// (`group_id: null` leaves a group), not a removal. Fields the patch doesn't
/// name stay — also those this build has never heard of.
pub(crate) fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                match base.get_mut(key) {
                    Some(existing) if existing.is_object() && value.is_object() => {
                        merge(existing, value)
                    }
                    _ => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

/// What a new record of `kind` holds before the page's fields: every field
/// the app needs to read it.
pub(crate) fn template(space: Space, kind: &str) -> Option<Value> {
    let rdp = space == Space::Rdp;
    Some(match kind {
        "host" if rdp => json!({
            "name": "", "address": "", "port": 3389, "workspace": "private", "position": 0,
            "group_id": null, "identity_id": null,
            "rdp": {
                "display": "fit", "width": 1920, "height": 1080, "smartSizing": true,
                "colorDepth": 32, "audio": "local", "clipboard": true, "admin": false,
                "nla": true, "wallpaper": true, "graphicsPipeline": true
            }
        }),
        "host" => json!({
            "name": "", "address": "", "port": 22, "workspace": "private", "position": 0,
            "group_id": null, "identity_id": null
        }),
        "group" => json!({ "workspace": "private", "name": "", "position": 0 }),
        "identity" => json!({
            "label": "", "username": "", "auth_type": "password", "key_id": null,
            "password_secret_id": null
        }),
        "key" => json!({
            "label": "", "key_type": "", "public_key": "", "private_secret_id": null,
            "passphrase_secret_id": null
        }),
        "snippet" => json!({ "label": "", "body": "", "group_path": null }),
        "port_forward" => json!({
            "host_id": null, "name": "", "kind": "local", "bind_address": "127.0.0.1",
            "bind_port": 0, "target_host": "", "target_port": 0, "autostart": false
        }),
        _ => return None,
    })
}

// ── Checking a payload ─────────────────────────────────────

#[derive(Clone, Copy)]
enum Ty {
    Str,
    OptStr,
    U16,
    U32,
    I64,
    Bool,
    Uuid,
    OptUuid,
    OptObj,
}

fn fits(value: Option<&Value>, ty: Ty) -> bool {
    let is_uuid = |v: &Value| v.as_str().is_some_and(|s| Uuid::parse_str(s).is_ok());
    match (ty, value) {
        (Ty::Str, Some(v)) => v.is_string(),
        (Ty::OptStr, None) => true,
        (Ty::OptStr, Some(v)) => v.is_string() || v.is_null(),
        (Ty::U16, Some(v)) => v.as_u64().is_some_and(|n| n <= u64::from(u16::MAX)),
        (Ty::U32, Some(v)) => v.as_u64().is_some_and(|n| n <= u64::from(u32::MAX)),
        (Ty::I64, Some(v)) => v.is_i64() || v.as_u64().is_some_and(|n| n <= i64::MAX as u64),
        (Ty::Bool, Some(v)) => v.is_boolean(),
        (Ty::Uuid, Some(v)) => is_uuid(v),
        (Ty::OptUuid, None) => true,
        (Ty::OptUuid, Some(v)) => v.is_null() || is_uuid(v),
        (Ty::OptObj, None) => true,
        (Ty::OptObj, Some(v)) => v.is_null() || v.is_object(),
        _ => false,
    }
}

/// Fields that must be there, of the type the app reads them as; optional
/// ones only when they are there.
fn shape(kind: &str) -> &'static [(&'static str, Ty, bool)] {
    // (field, type, required)
    match kind {
        "host" => &[
            ("name", Ty::Str, true),
            ("address", Ty::Str, true),
            ("port", Ty::U16, true),
            ("workspace", Ty::Str, true),
            ("position", Ty::I64, true),
            ("group_id", Ty::OptUuid, true),
            ("identity_id", Ty::OptUuid, true),
            ("rdp", Ty::OptObj, false),
            ("gateway_identity_id", Ty::OptUuid, false),
            ("comment", Ty::Str, false),
        ],
        "group" => &[
            ("workspace", Ty::Str, true),
            ("name", Ty::Str, true),
            ("position", Ty::I64, true),
            ("identity_id", Ty::OptUuid, false),
            ("drives", Ty::OptObj, false),
        ],
        "identity" => &[
            ("label", Ty::Str, true),
            ("username", Ty::Str, true),
            ("domain", Ty::Str, false),
            ("auth_type", Ty::Str, true),
            ("key_id", Ty::OptUuid, true),
            ("password_secret_id", Ty::OptUuid, true),
        ],
        "key" => &[
            ("label", Ty::Str, true),
            ("key_type", Ty::Str, true),
            ("public_key", Ty::Str, true),
            ("private_secret_id", Ty::OptUuid, true),
            ("passphrase_secret_id", Ty::OptUuid, true),
        ],
        "snippet" => &[
            ("label", Ty::Str, true),
            ("body", Ty::Str, true),
            ("group_path", Ty::OptStr, true),
        ],
        "port_forward" => &[
            ("host_id", Ty::Uuid, true),
            ("name", Ty::Str, true),
            ("kind", Ty::Str, true),
            ("bind_address", Ty::Str, true),
            ("bind_port", Ty::U16, true),
            ("target_host", Ty::Str, true),
            ("target_port", Ty::U16, true),
            ("autostart", Ty::Bool, false),
        ],
        _ => &[],
    }
}

/// UwURDP's `RdpSettings` (camelCase), `GatewaySettings` and
/// `DriveRedirection`: every field optional, but of its type.
const RDP_SETTINGS: &[(&str, Ty)] = &[
    ("display", Ty::Str),
    ("width", Ty::U16),
    ("height", Ty::U16),
    ("smartSizing", Ty::Bool),
    ("colorDepth", Ty::U32),
    ("audio", Ty::Str),
    ("clipboard", Ty::Bool),
    ("admin", Ty::Bool),
    ("nla", Ty::Bool),
    ("wallpaper", Ty::Bool),
    ("graphicsPipeline", Ty::Bool),
    ("gateway", Ty::OptObj),
    ("drives", Ty::OptObj),
];
const GATEWAY: &[(&str, Ty)] = &[
    ("address", Ty::Str),
    ("port", Ty::U16),
    ("useHostLogin", Ty::Bool),
    ("bypassLocal", Ty::Bool),
];

fn check_optional(
    object: &Map<String, Value>,
    fields: &[(&str, Ty)],
    what: &str,
) -> Result<(), Refusal> {
    for (field, ty) in fields {
        if let Some(value) = object.get(*field) {
            if !fits(Some(value), *ty) {
                return Err(refuse(
                    "invalid",
                    format!("{what}.{field} has the wrong type"),
                ));
            }
        }
    }
    Ok(())
}

fn check_drives(value: Option<&Value>, what: &str) -> Result<(), Refusal> {
    let Some(Value::Object(drives)) = value else {
        return Ok(());
    };
    check_optional(drives, &[("enabled", Ty::Bool)], what)?;
    match drives.get("drives") {
        None => Ok(()),
        Some(Value::Array(list)) => {
            for drive in list {
                let Value::Object(drive) = drive else {
                    return Err(refuse(
                        "invalid",
                        format!("{what}.drives holds a non-object"),
                    ));
                };
                check_optional(drive, &[("name", Ty::Str), ("path", Ty::Str)], what)?;
            }
            Ok(())
        }
        Some(_) => Err(refuse("invalid", format!("{what}.drives isn't a list"))),
    }
}

/// Whether the app that owns the record can read it back.
pub(crate) fn check_shape(kind: &str, value: &Value) -> Result<(), Refusal> {
    let Value::Object(object) = value else {
        return Err(refuse("invalid", format!("a {kind} is a JSON object")));
    };
    for (field, ty, required) in shape(kind) {
        let present = object.get(*field);
        if present.is_none() && !required {
            continue;
        }
        if !fits(present, *ty) {
            return Err(refuse(
                "invalid",
                format!("{kind}.{field} is missing or has the wrong type"),
            ));
        }
    }
    if let Some(Value::Object(rdp)) = object.get("rdp") {
        check_optional(rdp, RDP_SETTINGS, "rdp")?;
        if let Some(Value::Object(gateway)) = rdp.get("gateway") {
            check_optional(gateway, GATEWAY, "rdp.gateway")?;
        }
        check_drives(rdp.get("drives"), "rdp.drives")?;
    }
    check_drives(object.get("drives"), "drives")?;
    Ok(())
}

/// The fields of `kind` that point at another record, and that record's
/// kind.
fn references(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "host" => &[
            ("group_id", "group"),
            ("identity_id", "identity"),
            ("gateway_identity_id", "identity"),
        ],
        "group" => &[("identity_id", "identity")],
        "identity" => &[("key_id", "key"), ("password_secret_id", KIND_SECRET)],
        "key" => &[
            ("private_secret_id", KIND_SECRET),
            ("passphrase_secret_id", KIND_SECRET),
        ],
        "port_forward" => &[("host_id", "host")],
        _ => &[],
    }
}

fn reference<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

/// Whether `value` holds `id` anywhere, as a string.
fn mentions(value: &Value, id: &str) -> bool {
    match value {
        Value::String(s) => s == id,
        Value::Array(list) => list.iter().any(|v| mentions(v, id)),
        Value::Object(map) => map.values().any(|v| mentions(v, id)),
        _ => false,
    }
}

/// A name to show for a record in a hint.
fn title_of(kind: &str, value: &Value) -> String {
    let field = match kind {
        "identity" | "key" | "snippet" => "label",
        _ => "name",
    };
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("address")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| kind.to_owned())
}

// ── The space ──────────────────────────────────────────────

impl SpaceState {
    pub(crate) fn new(vault: SpaceVault) -> Self {
        SpaceState {
            vault,
            cursor: 0,
            records: BTreeMap::new(),
        }
    }

    pub(crate) fn space(&self) -> Space {
        self.vault.space
    }

    /// Takes a record from the server, unless one with a higher `seq` is
    /// already here.
    ///
    /// Every record of a kind this build knows is authenticated here, also a
    /// secret and a tombstone: one that doesn't open (the server changed its
    /// clock, its kind, its `deleted`, or it was sealed for something else)
    /// is `broken`, and nothing is ever sealed on top of a broken one, so a
    /// server can't push this device into writing a clock of its choosing.
    /// An id that isn't a UUID in its usual form is dropped: the AAD holds
    /// its bytes, so the same record could otherwise come twice.
    pub(crate) fn apply(&mut self, env: Envelope) {
        if parse_id(&env.id).ok().as_deref() != Some(env.id.as_str()) {
            tracing::warn!("a suite record with an id in an unusual form: ignored");
            return;
        }
        let seq = env.seq.unwrap_or(0);
        if let Some(old) = self.records.get(&env.id) {
            if old.seq() > seq {
                return;
            }
        }
        let shown = !env.deleted
            && self.vault.space.kind_discriminant(&env.kind).is_some()
            && SHOWN.contains(&env.kind.as_str())
            && env.kind != KIND_SECRET;
        let (json, broken) = if shown {
            match self.vault.open_record(&env) {
                Ok(OpenedRecord {
                    payload: Some(Payload::Json(value)),
                    ..
                }) if value.is_object() => (Some(value), false),
                Ok(_) => (None, true),
                Err(error) => {
                    tracing::warn!(%error, "a suite record didn't open");
                    (None, true)
                }
            }
        } else if self.vault.space.kind_discriminant(&env.kind).is_some() {
            // Opened only to authenticate it; the plaintext goes right away.
            match self.vault.open(&env) {
                Ok(_) => (None, false),
                Err(error) => {
                    tracing::warn!(%error, "a suite record didn't open");
                    (None, true)
                }
            }
        } else {
            (None, false)
        };
        self.records
            .insert(env.id.clone(), Stored { env, json, broken });
    }

    /// What a push answered: the accepted records with their `seq`, and the
    /// server's copies of the ones it refused. Gives the refused ids.
    pub(crate) fn pushed(&mut self, sent: Vec<Envelope>, answer: Pushed) -> Vec<String> {
        let accepted: HashMap<String, u64> =
            answer.accepted.into_iter().map(|a| (a.id, a.seq)).collect();
        for mut env in sent {
            if let Some(seq) = accepted.get(&env.id) {
                env.seq = Some(*seq);
                env.base_seq = 0;
                self.apply(env);
            }
        }
        let mut refused = Vec::new();
        for env in answer.conflicts {
            refused.push(env.id.clone());
            self.apply(env);
        }
        // The cursor stays: records of others between it and this push are
        // still to be pulled.
        refused
    }

    fn live(&self, id: &str) -> Option<&Stored> {
        self.records.get(id).filter(|s| s.live())
    }

    /// A live record of `kind`, with its JSON.
    fn live_json(&self, id: &str, kind: &str) -> Option<&Value> {
        self.live(id)
            .filter(|s| s.env.kind == kind)
            .and_then(|s| s.json.as_ref())
    }

    /// A live secret's text. `None` when there is no such secret or it
    /// isn't text.
    pub(crate) fn secret_text(&self, id: &str) -> Result<Zeroizing<String>, Refusal> {
        let stored = self
            .live(id)
            .filter(|s| s.env.kind == KIND_SECRET)
            .ok_or_else(|| refuse("not-found", "no such secret"))?;
        match self.vault.open_record(&stored.env) {
            Ok(OpenedRecord {
                payload: Some(Payload::Text(text)),
                ..
            }) => Ok(Zeroizing::new(text)),
            Ok(_) => Err(refuse("crypto", "this secret isn't text")),
            Err(error) => Err(refuse("crypto", error.to_string())),
        }
    }

    /// A live JSON record of `kind`, for the commands.
    pub(crate) fn record(&self, id: &str, kind: &str) -> Result<&Value, Refusal> {
        self.live_json(id, kind)
            .ok_or_else(|| refuse("not-found", format!("no such {kind}")))
    }

    /// Who uses each identity and key: id → ids of the live records that
    /// point at it.
    fn users(&self) -> HashMap<String, Vec<String>> {
        let mut users: HashMap<String, Vec<String>> = HashMap::new();
        for (id, stored) in &self.records {
            let Some(json) = stored.json.as_ref().filter(|_| stored.live()) else {
                continue;
            };
            for (field, target) in references(&stored.env.kind) {
                if *target == "identity" || *target == "key" {
                    if let Some(to) = reference(json, field) {
                        let list = users.entry(to.to_owned()).or_default();
                        if !list.contains(id) {
                            list.push(id.clone());
                        }
                    }
                }
            }
        }
        users
    }

    /// The list the page shows: live records of the shown kinds.
    pub(crate) fn view(&self) -> Vec<RecordView> {
        let users = self.users();
        self.records
            .iter()
            .filter(|(_, s)| s.live() && SHOWN.contains(&s.env.kind.as_str()))
            .map(|(id, s)| RecordView {
                id: id.clone(),
                kind: s.env.kind.clone(),
                seq: s.seq(),
                updated_ms: s.env.updated_at.wall_ms,
                data: s.json.clone(),
                broken: s.broken,
                used_by: users.get(id).cloned().unwrap_or_default(),
            })
            .collect()
    }

    /// Turns the page's batch into envelopes to push, checking everything
    /// first: nothing is sealed when one op is refused.
    pub(crate) fn plan(
        &self,
        ops: Vec<Op>,
        now_ms: u64,
        device: u32,
    ) -> Result<Vec<Envelope>, Refusal> {
        let space = self.space();
        let mut next: BTreeMap<String, (String, Next)> = BTreeMap::new();

        // The JSON a record has in this plan so far.
        fn current(
            state: &SpaceState,
            next: &BTreeMap<String, (String, Next)>,
            id: &str,
        ) -> Option<Value> {
            match next.get(id) {
                Some((_, Next::Json(value))) => Some(value.clone()),
                Some(_) => None,
                None => state.live(id).and_then(|s| s.json.clone()),
            }
        }

        let check_seq = |stored: &Stored, seq: Option<u64>| -> Result<(), Refusal> {
            match seq {
                Some(seq) if seq != stored.seq() => Err(refuse(
                    "suite-conflict",
                    format!("{} {} changed elsewhere", stored.env.kind, stored.env.id),
                )),
                _ => Ok(()),
            }
        };

        for op in ops {
            match op {
                Op::Put {
                    id,
                    kind,
                    seq,
                    patch,
                } => {
                    let id = parse_id(&id)?;
                    if !EDITABLE.contains(&kind.as_str())
                        || space.kind_discriminant(&kind).is_none()
                    {
                        return Err(refuse(
                            "invalid",
                            format!("{kind} records aren't edited here"),
                        ));
                    }
                    if !patch.is_object() {
                        return Err(refuse("invalid", "a patch is a JSON object"));
                    }
                    let mut base = match self.records.get(&id) {
                        Some(stored) => {
                            if !stored.live() {
                                return Err(refuse("not-found", "this record was deleted"));
                            }
                            if stored.env.kind != kind {
                                return Err(refuse("invalid", "a record keeps its kind"));
                            }
                            if stored.broken {
                                return Err(refuse("crypto", "this record didn't open"));
                            }
                            check_seq(stored, seq)?;
                            current(self, &next, &id).unwrap_or_default()
                        }
                        None => match next.get(&id) {
                            Some((k, Next::Json(value))) if *k == kind => value.clone(),
                            Some(_) => return Err(refuse("invalid", "one id, one record")),
                            None => template(space, &kind).unwrap_or_default(),
                        },
                    };
                    merge(&mut base, &patch);
                    check_shape(&kind, &base)?;
                    next.insert(id, (kind, Next::Json(base)));
                }
                Op::Secret { id, text, seq } => {
                    let id = parse_id(&id)?;
                    let text = Zeroizing::new(text);
                    if let Some(stored) = self.records.get(&id) {
                        if !stored.live() || stored.env.kind != KIND_SECRET {
                            return Err(refuse("invalid", "not a secret"));
                        }
                        if stored.broken {
                            return Err(refuse("crypto", "this record didn't open"));
                        }
                        check_seq(stored, seq)?;
                    }
                    next.insert(
                        id,
                        (
                            KIND_SECRET.to_owned(),
                            Next::Secret(Zeroizing::new(text.as_bytes().to_vec())),
                        ),
                    );
                }
                Op::Delete { id, seq } => {
                    let id = parse_id(&id)?;
                    let stored = self
                        .live(&id)
                        .ok_or_else(|| refuse("not-found", "no such record"))?;
                    let kind = stored.env.kind.clone();
                    if !(EDITABLE.contains(&kind.as_str())
                        || kind == "known_host"
                        || kind == KIND_SECRET)
                    {
                        return Err(refuse(
                            "invalid",
                            format!("{kind} records aren't deleted here"),
                        ));
                    }
                    if stored.broken {
                        return Err(refuse("crypto", "this record didn't open"));
                    }
                    check_seq(stored, seq)?;
                    let json = current(self, &next, &id);
                    match kind.as_str() {
                        // Its tunnels go along.
                        "host" => {
                            for (other, s) in &self.records {
                                if s.live()
                                    && s.env.kind == "port_forward"
                                    && current(self, &next, other)
                                        .is_some_and(|v| reference(&v, "host_id") == Some(&id))
                                {
                                    next.insert(
                                        other.clone(),
                                        (s.env.kind.clone(), Next::Tombstone),
                                    );
                                }
                            }
                        }
                        // Its hosts stay, without a group.
                        "group" => {
                            for (other, s) in &self.records {
                                if !s.live() || s.env.kind != "host" {
                                    continue;
                                }
                                if let Some(mut host) = current(self, &next, other) {
                                    if reference(&host, "group_id") == Some(&id) {
                                        host["group_id"] = Value::Null;
                                        next.insert(
                                            other.clone(),
                                            ("host".into(), Next::Json(host)),
                                        );
                                    }
                                }
                            }
                        }
                        // Its secrets go along.
                        "identity" | "key" => {
                            let fields: &[&str] = if kind == "identity" {
                                &["password_secret_id"]
                            } else {
                                &["private_secret_id", "passphrase_secret_id"]
                            };
                            for field in fields {
                                if let Some(secret) =
                                    json.as_ref().and_then(|v| reference(v, field))
                                {
                                    // One that doesn't open stays: nothing is sealed on it.
                                    if self
                                        .live(secret)
                                        .is_some_and(|s| s.env.kind == KIND_SECRET && !s.broken)
                                    {
                                        next.insert(
                                            secret.to_owned(),
                                            (KIND_SECRET.into(), Next::Tombstone),
                                        );
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    next.insert(id, (kind, Next::Tombstone));
                }
            }
        }

        self.check_after(&next)?;

        let mut planned: Vec<(usize, Envelope)> = Vec::with_capacity(next.len());
        for (id, (kind, change)) in next {
            let head = match self.records.get(&id) {
                // Its clock and kind are only trusted when it opened.
                Some(stored) if stored.broken => {
                    return Err(refuse("crypto", "this record didn't open"));
                }
                Some(stored) => stored
                    .env
                    .head()
                    .map_err(|e| refuse("crypto", e.to_string()))?,
                None => RecordHead {
                    id: Uuid::parse_str(&id).map_err(|_| refuse("invalid", "not a UUID"))?,
                    kind: kind.clone(),
                    updated_at: Hlc::default(),
                    seq: 0,
                },
            };
            let sealed = match change {
                Next::Json(value) => {
                    self.vault
                        .seal_edit(&head, &Payload::Json(value), now_ms, device)
                }
                Next::Secret(bytes) => {
                    self.vault
                        .seal_edit(&head, &Payload::Bytes(bytes.to_vec()), now_ms, device)
                }
                Next::Tombstone => {
                    if !self.records.contains_key(&id) {
                        continue;
                    }
                    self.vault.seal_tombstone(&head, now_ms, device)
                }
            }
            .map_err(|e| refuse("invalid", e.to_string()))?;
            let rank = ORDER.iter().position(|k| *k == kind).unwrap_or(ORDER.len());
            planned.push((rank, sealed));
        }
        planned.sort_by_key(|(rank, _)| *rank);
        Ok(planned.into_iter().map(|(_, env)| env).collect())
    }

    /// After the plan: every pointer it set leads to a live record of the
    /// right kind, and nothing it deletes is still pointed at.
    fn check_after(&self, next: &BTreeMap<String, (String, Next)>) -> Result<(), Refusal> {
        // Live after the plan, with its JSON (secrets without).
        let alive = |id: &str| -> Option<(String, Option<Value>)> {
            match next.get(id) {
                Some((_, Next::Tombstone)) => None,
                Some((kind, Next::Json(v))) => Some((kind.clone(), Some(v.clone()))),
                Some((kind, Next::Secret(_))) => Some((kind.clone(), None)),
                None => self.live(id).map(|s| (s.env.kind.clone(), s.json.clone())),
            }
        };

        for (id, (kind, change)) in next {
            let Next::Json(value) = change else { continue };
            let before = self.live(id).and_then(|s| s.json.as_ref());
            for (field, target) in references(kind) {
                let Some(to) = reference(value, field) else {
                    continue;
                };
                // A pointer the app already had stays, even when it leads nowhere.
                if before.and_then(|b| reference(b, field)) == Some(to) {
                    continue;
                }
                match alive(to) {
                    Some((k, _)) if k == *target => {}
                    _ => {
                        return Err(refuse(
                            "invalid",
                            format!("{kind}.{field} points at no {target}"),
                        ))
                    }
                }
            }
        }

        // What is deleted, and who would still point at it.
        let mut deleted: HashSet<&str> = HashSet::new();
        for (id, (_, change)) in next {
            if matches!(change, Next::Tombstone) {
                deleted.insert(id);
            }
        }
        for id in &deleted {
            let Some(stored) = self.live(id) else {
                continue;
            };
            let kind = stored.env.kind.as_str();
            if !matches!(kind, "identity" | "key" | KIND_SECRET | "host" | "group") {
                continue;
            }
            let mut users = Vec::new();
            let ids: HashSet<&String> = self.records.keys().chain(next.keys()).collect();
            for other in ids {
                if deleted.contains(other.as_str()) {
                    continue;
                }
                let Some((other_kind, Some(json))) = alive(other) else {
                    continue;
                };
                let points = if kind == KIND_SECRET {
                    mentions(&json, id)
                } else {
                    references(&other_kind).iter().any(|(field, target)| {
                        *target == kind && reference(&json, field) == Some(id)
                    })
                };
                if points {
                    users.push(title_of(&other_kind, &json));
                }
            }
            if !users.is_empty() {
                users.sort();
                return Err(refuse(
                    "in-use",
                    format!("still used by: {}", users.join(", ")),
                ));
            }
        }
        Ok(())
    }
}

fn parse_id(id: &str) -> Result<String, Refusal> {
    Uuid::parse_str(id)
        .map(|u| u.to_string())
        .map_err(|_| refuse("invalid", format!("{id:?} isn't a UUID")))
}

// ── Extras of a host ───────────────────────────────────────

fn str_of<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or_default()
}

fn int_of(value: &Value, field: &str) -> Option<u64> {
    value.get(field).and_then(Value::as_u64)
}

fn bool_of(value: &Value, field: &str, default: bool) -> bool {
    value.get(field).and_then(Value::as_bool).unwrap_or(default)
}

/// `host:port`, IPv6 in brackets.
fn host_port(address: &str, port: u64) -> String {
    if address.contains(':') && !address.starts_with('[') {
        format!("[{address}]:{port}")
    } else {
        format!("{address}:{port}")
    }
}

/// A `.rdp` file for mstsc and other clients: the host's address and
/// settings, the username of its login — never a password — and no drive
/// redirection, as UwURDP's importer treats such files. Values can't break
/// out of their line.
pub(crate) fn rdp_file(host: &Value, identity: Option<&Value>) -> String {
    let clean = |s: &str| s.replace(['\r', '\n'], " ");
    let rdp = host.get("rdp").cloned().unwrap_or_else(|| json!({}));
    let mut lines: Vec<String> = Vec::new();
    let port = int_of(host, "port").unwrap_or(3389);
    lines.push(format!(
        "full address:s:{}",
        clean(&host_port(str_of(host, "address"), port))
    ));
    if let Some(identity) = identity {
        let user = str_of(identity, "username");
        let domain = str_of(identity, "domain");
        if !user.is_empty() {
            let name = if domain.is_empty() {
                user.to_owned()
            } else {
                format!("{domain}\\{user}")
            };
            lines.push(format!("username:s:{}", clean(&name)));
        }
    }
    let display = str_of(&rdp, "display");
    match display {
        "fullscreen" => lines.push("screen mode id:i:2".into()),
        _ => lines.push("screen mode id:i:1".into()),
    }
    if display == "fixed" {
        lines.push(format!(
            "desktopwidth:i:{}",
            int_of(&rdp, "width").unwrap_or(1920)
        ));
        lines.push(format!(
            "desktopheight:i:{}",
            int_of(&rdp, "height").unwrap_or(1080)
        ));
        lines.push("dynamic resolution:i:0".into());
    } else {
        lines.push("dynamic resolution:i:1".into());
    }
    lines.push(format!(
        "smart sizing:i:{}",
        u8::from(bool_of(&rdp, "smartSizing", true))
    ));
    lines.push(format!(
        "session bpp:i:{}",
        int_of(&rdp, "colorDepth").unwrap_or(32)
    ));
    let audio = match str_of(&rdp, "audio") {
        "remote" => 1,
        "off" => 2,
        _ => 0,
    };
    lines.push(format!("audiomode:i:{audio}"));
    lines.push(format!(
        "redirectclipboard:i:{}",
        u8::from(bool_of(&rdp, "clipboard", true))
    ));
    lines.push(format!(
        "administrative session:i:{}",
        u8::from(bool_of(&rdp, "admin", false))
    ));
    lines.push(format!(
        "enablecredsspsupport:i:{}",
        u8::from(bool_of(&rdp, "nla", true))
    ));
    lines.push(format!(
        "disable wallpaper:i:{}",
        u8::from(!bool_of(&rdp, "wallpaper", true))
    ));
    // Drives stay here: a file passed around must not share them.
    lines.push("redirectdrives:i:0".into());
    lines.push("drivestoredirect:s:".into());
    match rdp.get("gateway").filter(|g| g.is_object()) {
        Some(gateway) if !str_of(gateway, "address").is_empty() => {
            let address = str_of(gateway, "address");
            let gateway_address = match int_of(gateway, "port") {
                Some(port) if port != 0 && port != 443 => host_port(address, port),
                _ => address.to_owned(),
            };
            lines.push(format!("gatewayhostname:s:{}", clean(&gateway_address)));
            let usage = if bool_of(gateway, "bypassLocal", false) {
                2
            } else {
                1
            };
            lines.push(format!("gatewayusagemethod:i:{usage}"));
            lines.push("gatewayprofileusagemethod:i:1".into());
            lines.push("gatewaycredentialssource:i:4".into());
            lines.push(format!(
                "promptcredentialonce:i:{}",
                u8::from(bool_of(gateway, "useHostLogin", false))
            ));
        }
        _ => lines.push("gatewayusagemethod:i:0".into()),
    }
    let mut text = lines.join("\r\n");
    text.push_str("\r\n");
    text
}

/// A private key in a format other than OpenSSH's that UwUSSH still takes:
/// its type as far as the first line says (`""` when it doesn't), `None`
/// for text that is no private key at all.
pub(crate) fn foreign_key_type(text: &str) -> Option<String> {
    let first = text.trim_start().lines().next()?.trim();
    if let Some(rest) = first.strip_prefix("PuTTY-User-Key-File-") {
        return Some(rest.split_once(':')?.1.trim().to_owned());
    }
    if !(first.starts_with("-----BEGIN ") && first.ends_with("PRIVATE KEY-----")) {
        return None;
    }
    Some(
        if first.contains(" RSA ") {
            "ssh-rsa"
        } else if first.contains(" EC ") {
            "ecdsa"
        } else if first.contains(" DSA ") {
            "ssh-dss"
        } else {
            ""
        }
        .to_owned(),
    )
}

/// The login a host connects with: its own, else (UwURDP) its group's.
pub(crate) fn identity_of<'a>(state: &'a SpaceState, host: &Value) -> Option<&'a Value> {
    let own = reference(host, "identity_id").and_then(|id| state.live_json(id, "identity"));
    own.or_else(|| {
        let group = reference(host, "group_id").and_then(|id| state.live_json(id, "group"))?;
        reference(group, "identity_id").and_then(|id| state.live_json(id, "identity"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uwulock_core::extras::SpaceKey;
    use uwulock_core::suite::Accepted;

    const DEVICE: u32 = 0x1234_5678;
    const NOW: u64 = 1_790_000_000_000;

    fn id(n: u8) -> String {
        Uuid::from_bytes([n; 16]).to_string()
    }

    fn state(space: Space) -> SpaceState {
        SpaceState::new(SpaceVault::new(
            space,
            Uuid::from_bytes([0xaa; 16]),
            SpaceKey::generate(),
        ))
    }

    /// Seals `value` as the server would hand it back: `seq` set.
    fn put(state: &mut SpaceState, id: &str, kind: &str, value: Value, seq: u64) {
        let head = RecordHead {
            id: Uuid::parse_str(id).unwrap(),
            kind: kind.into(),
            updated_at: Hlc::new(NOW - 10_000, 0, 7),
            seq: 0,
        };
        let payload = if kind == KIND_SECRET {
            Payload::Text(value.as_str().unwrap().into())
        } else {
            Payload::Json(value)
        };
        let mut env = state
            .vault
            .seal_edit(&head, &payload, NOW - 10_000, 7)
            .unwrap();
        env.seq = Some(seq);
        state.apply(env);
    }

    fn open(state: &SpaceState, env: &Envelope) -> Option<Value> {
        match state.vault.open_record(env).unwrap().payload {
            Some(Payload::Json(v)) => Some(v),
            _ => None,
        }
    }

    fn ssh_host(name: &str, group: Option<&str>, identity: Option<&str>) -> Value {
        json!({ "name": name, "address": "server.example.com", "port": 22,
                "workspace": "private", "position": 0, "group_id": group,
                "identity_id": identity, "tags": ["from-a-newer-uwussh"] })
    }

    #[test]
    fn an_edit_keeps_fields_this_build_does_not_know() {
        let mut s = state(Space::Ssh);
        put(&mut s, &id(1), "host", ssh_host("one", None, None), 5);
        let planned = s
            .plan(
                vec![Op::Put {
                    id: id(1),
                    kind: "host".into(),
                    seq: Some(5),
                    patch: json!({ "name": "renamed", "port": 2222 }),
                }],
                NOW,
                DEVICE,
            )
            .unwrap();
        assert_eq!(planned.len(), 1);
        let env = &planned[0];
        assert_eq!(env.base_seq, 5);
        assert_eq!(env.updated_at, Hlc::new(NOW, 0, DEVICE));
        let value = open(&s, env).unwrap();
        assert_eq!(value["name"], "renamed");
        assert_eq!(value["port"], 2222);
        assert_eq!(value["tags"][0], "from-a-newer-uwussh");
    }

    #[test]
    fn the_clock_moves_past_a_record_from_the_future() {
        let mut s = state(Space::Ssh);
        let head = RecordHead {
            id: Uuid::parse_str(&id(1)).unwrap(),
            kind: "host".into(),
            updated_at: Hlc::new(NOW + 60_000, 3, 9),
            seq: 0,
        };
        let mut env = s
            .vault
            .seal_edit(&head, &Payload::Json(ssh_host("a", None, None)), NOW, 9)
            .unwrap();
        env.seq = Some(2);
        s.apply(env);
        let planned = s
            .plan(
                vec![Op::Put {
                    id: id(1),
                    kind: "host".into(),
                    seq: None,
                    patch: json!({ "name": "b" }),
                }],
                NOW,
                DEVICE,
            )
            .unwrap();
        assert_eq!(planned[0].updated_at, Hlc::new(NOW + 60_000, 5, DEVICE));
    }

    #[test]
    fn a_stale_edit_is_a_conflict_not_an_overwrite() {
        let mut s = state(Space::Ssh);
        put(&mut s, &id(1), "host", ssh_host("one", None, None), 9);
        let refused = s
            .plan(
                vec![Op::Put {
                    id: id(1),
                    kind: "host".into(),
                    seq: Some(4),
                    patch: json!({ "name": "x" }),
                }],
                NOW,
                DEVICE,
            )
            .unwrap_err();
        assert_eq!(refused.kind, "suite-conflict");
    }

    #[test]
    fn a_new_host_has_every_field_the_app_needs() {
        let s = state(Space::Rdp);
        let planned = s
            .plan(
                vec![Op::Put {
                    id: id(2),
                    kind: "host".into(),
                    seq: None,
                    patch: json!({ "name": "desk", "address": "192.0.2.10",
                                   "rdp": { "admin": true } }),
                }],
                NOW,
                DEVICE,
            )
            .unwrap();
        let env = &planned[0];
        assert_eq!(env.base_seq, 0);
        assert_eq!(env.id, id(2));
        let value = open(&s, env).unwrap();
        assert_eq!(value["port"], 3389);
        assert_eq!(value["rdp"]["admin"], true);
        assert_eq!(
            value["rdp"]["nla"], true,
            "the defaults stay beside the change"
        );
        assert_eq!(value["group_id"], Value::Null);
    }

    #[test]
    fn a_payload_the_app_could_not_read_is_refused() {
        let s = state(Space::Ssh);
        for patch in [
            json!({ "port": 70000 }),
            json!({ "name": 5 }),
            json!({ "group_id": "not-a-uuid" }),
            json!({ "position": "first" }),
        ] {
            let refused = s
                .plan(
                    vec![Op::Put {
                        id: id(3),
                        kind: "host".into(),
                        seq: None,
                        patch,
                    }],
                    NOW,
                    DEVICE,
                )
                .unwrap_err();
            assert_eq!(refused.kind, "invalid");
        }
        let rdp = state(Space::Rdp);
        let refused = rdp
            .plan(
                vec![Op::Put {
                    id: id(3),
                    kind: "host".into(),
                    seq: None,
                    patch: json!({ "rdp": { "gateway": { "port": "443" } } }),
                }],
                NOW,
                DEVICE,
            )
            .unwrap_err();
        assert_eq!(refused.kind, "invalid");
    }

    #[test]
    fn a_new_pointer_must_lead_to_a_record_of_its_kind() {
        let mut s = state(Space::Ssh);
        put(
            &mut s,
            &id(1),
            "group",
            json!({ "workspace": "private", "name": "g", "position": 0 }),
            1,
        );
        let to_nothing = s.plan(
            vec![Op::Put {
                id: id(4),
                kind: "host".into(),
                seq: None,
                patch: json!({ "identity_id": id(9) }),
            }],
            NOW,
            DEVICE,
        );
        assert_eq!(to_nothing.unwrap_err().kind, "invalid");
        let wrong_kind = s.plan(
            vec![Op::Put {
                id: id(4),
                kind: "host".into(),
                seq: None,
                patch: json!({ "identity_id": id(1) }),
            }],
            NOW,
            DEVICE,
        );
        assert_eq!(wrong_kind.unwrap_err().kind, "invalid");
        // A secret, an identity and the host that uses it, in one batch.
        let together = s
            .plan(
                vec![
                    Op::Put {
                        id: id(4),
                        kind: "host".into(),
                        seq: None,
                        patch: json!({ "identity_id": id(5), "group_id": id(1) }),
                    },
                    Op::Put {
                        id: id(5),
                        kind: "identity".into(),
                        seq: None,
                        patch: json!({ "label": "root", "username": "root",
                                       "password_secret_id": id(6) }),
                    },
                    Op::Secret {
                        id: id(6),
                        text: "hunter2".into(),
                        seq: None,
                    },
                ],
                NOW,
                DEVICE,
            )
            .unwrap();
        let kinds: Vec<&str> = together.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["secret", "identity", "host"], "pointed-at first");
    }

    #[test]
    fn deleting_a_host_takes_its_tunnels_but_not_its_login() {
        let mut s = state(Space::Ssh);
        put(
            &mut s,
            &id(5),
            "identity",
            json!({ "label": "l", "username": "u",
            "auth_type": "password", "key_id": null, "password_secret_id": id(6) }),
            1,
        );
        put(&mut s, &id(6), KIND_SECRET, json!("pw"), 2);
        put(&mut s, &id(1), "host", ssh_host("h", None, Some(&id(5))), 3);
        let tunnel = |host: &str| {
            json!({ "host_id": host, "name": "t", "kind": "local",
            "bind_address": "127.0.0.1", "bind_port": 8080, "target_host": "localhost",
            "target_port": 80, "autostart": false })
        };
        put(&mut s, &id(7), "port_forward", tunnel(&id(1)), 4);
        put(&mut s, &id(8), "port_forward", tunnel(&id(2)), 5);
        let planned = s
            .plan(
                vec![Op::Delete {
                    id: id(1),
                    seq: Some(3),
                }],
                NOW,
                DEVICE,
            )
            .unwrap();
        let mut gone: Vec<&str> = planned.iter().map(|e| e.id.as_str()).collect();
        gone.sort();
        assert_eq!(gone, [id(1).as_str(), id(7).as_str()]);
        assert!(planned.iter().all(|e| e.deleted));
        for env in &planned {
            assert!(
                s.vault.open(env).unwrap().is_empty(),
                "a tombstone seals nothing"
            );
        }
    }

    #[test]
    fn an_identity_in_use_stays_and_says_by_whom() {
        let mut s = state(Space::Rdp);
        put(
            &mut s,
            &id(5),
            "identity",
            json!({ "label": "admin", "username": "u",
            "auth_type": "password", "key_id": null, "password_secret_id": id(6) }),
            1,
        );
        put(&mut s, &id(6), KIND_SECRET, json!("pw"), 2);
        put(
            &mut s,
            &id(1),
            "group",
            json!({ "workspace": "business", "name": "Office",
            "position": 0, "identity_id": id(5) }),
            3,
        );
        let view = s.view();
        let identity = view.iter().find(|r| r.id == id(5)).unwrap();
        assert_eq!(identity.used_by, [id(1)]);
        let refused = s
            .plan(
                vec![Op::Delete {
                    id: id(5),
                    seq: None,
                }],
                NOW,
                DEVICE,
            )
            .unwrap_err();
        assert_eq!(refused.kind, "in-use");
        assert!(refused.message.contains("Office"));
        // The group lets go of it first: then it goes, with its password.
        let planned = s
            .plan(
                vec![
                    Op::Put {
                        id: id(1),
                        kind: "group".into(),
                        seq: Some(3),
                        patch: json!({ "identity_id": null }),
                    },
                    Op::Delete {
                        id: id(5),
                        seq: None,
                    },
                ],
                NOW,
                DEVICE,
            )
            .unwrap();
        let deleted: HashSet<&str> = planned
            .iter()
            .filter(|e| e.deleted)
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(deleted, HashSet::from([id(5).as_str(), id(6).as_str()]));
    }

    #[test]
    fn a_key_goes_with_both_its_secrets_once_unused() {
        let mut s = state(Space::Ssh);
        put(
            &mut s,
            &id(3),
            "key",
            json!({ "label": "k", "key_type": "ssh-ed25519",
            "public_key": "ssh-ed25519 AAAA", "private_secret_id": id(6),
            "passphrase_secret_id": id(7) }),
            1,
        );
        put(
            &mut s,
            &id(6),
            KIND_SECRET,
            json!("-----BEGIN OPENSSH PRIVATE KEY-----"),
            2,
        );
        put(&mut s, &id(7), KIND_SECRET, json!("pass"), 3);
        put(
            &mut s,
            &id(5),
            "identity",
            json!({ "label": "l", "username": "u",
            "auth_type": "key", "key_id": id(3), "password_secret_id": null }),
            4,
        );
        assert_eq!(
            s.plan(
                vec![Op::Delete {
                    id: id(3),
                    seq: None
                }],
                NOW,
                DEVICE
            )
            .unwrap_err()
            .kind,
            "in-use"
        );
        // A secret something points at isn't deleted on its own either.
        assert_eq!(
            s.plan(
                vec![Op::Delete {
                    id: id(6),
                    seq: None
                }],
                NOW,
                DEVICE
            )
            .unwrap_err()
            .kind,
            "in-use"
        );
        let planned = s
            .plan(
                vec![
                    Op::Delete {
                        id: id(5),
                        seq: None,
                    },
                    Op::Delete {
                        id: id(3),
                        seq: None,
                    },
                ],
                NOW,
                DEVICE,
            )
            .unwrap();
        // The identity had no password: both records, and the key's two secrets.
        let ids: HashSet<&str> = planned.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            HashSet::from([
                id(3).as_str(),
                id(5).as_str(),
                id(6).as_str(),
                id(7).as_str()
            ])
        );
    }

    #[test]
    fn deleting_a_group_leaves_its_hosts_without_one() {
        let mut s = state(Space::Ssh);
        put(
            &mut s,
            &id(1),
            "group",
            json!({ "workspace": "private", "name": "g", "position": 0 }),
            1,
        );
        put(&mut s, &id(2), "host", ssh_host("h", Some(&id(1)), None), 2);
        let planned = s
            .plan(
                vec![Op::Delete {
                    id: id(1),
                    seq: None,
                }],
                NOW,
                DEVICE,
            )
            .unwrap();
        let host = planned.iter().find(|e| e.id == id(2)).unwrap();
        assert!(!host.deleted);
        let value = open(&s, host).unwrap();
        assert_eq!(value["group_id"], Value::Null);
        assert_eq!(value["tags"][0], "from-a-newer-uwussh");
    }

    #[test]
    fn manifests_and_unknown_kinds_are_neither_shown_nor_written() {
        let mut s = state(Space::Ssh);
        let head = RecordHead {
            id: Uuid::parse_str(&id(1)).unwrap(),
            kind: "manifest".into(),
            updated_at: Hlc::new(NOW, 0, 1),
            seq: 0,
        };
        let mut manifest = s
            .vault
            .seal(
                &uwulock_core::suite::RecordHeader {
                    id: head.id,
                    kind: head.kind.clone(),
                    updated_at: head.updated_at,
                    deleted: false,
                    base_seq: 0,
                },
                b"opaque",
            )
            .unwrap();
        manifest.seq = Some(1);
        s.apply(manifest);
        s.apply(Envelope {
            id: id(2),
            kind: "something_new".into(),
            nonce: "AAAA".into(),
            blob: "AAAA".into(),
            seq: Some(2),
            ..Envelope::default()
        });
        put(
            &mut s,
            &id(3),
            "assist_config",
            json!({ "provider": "" }),
            3,
        );
        assert!(s.view().is_empty());
        for (target, kind) in [(1, "manifest"), (3, "assist_config")] {
            assert!(s
                .plan(
                    vec![Op::Delete {
                        id: id(target),
                        seq: None
                    }],
                    NOW,
                    DEVICE
                )
                .is_err());
            assert!(s
                .plan(
                    vec![Op::Put {
                        id: id(target),
                        kind: kind.into(),
                        seq: None,
                        patch: json!({})
                    }],
                    NOW,
                    DEVICE
                )
                .is_err());
        }
    }

    #[test]
    fn a_push_answer_updates_the_records() {
        let mut s = state(Space::Ssh);
        put(&mut s, &id(1), "host", ssh_host("one", None, None), 5);
        put(&mut s, &id(2), "host", ssh_host("two", None, None), 6);
        let sent = s
            .plan(
                vec![
                    Op::Put {
                        id: id(1),
                        kind: "host".into(),
                        seq: Some(5),
                        patch: json!({ "name": "1" }),
                    },
                    Op::Put {
                        id: id(2),
                        kind: "host".into(),
                        seq: Some(6),
                        patch: json!({ "name": "2" }),
                    },
                ],
                NOW,
                DEVICE,
            )
            .unwrap();
        // The server took the first; the second was changed elsewhere.
        let mut theirs = s
            .vault
            .seal_edit(
                &RecordHead {
                    id: Uuid::parse_str(&id(2)).unwrap(),
                    kind: "host".into(),
                    updated_at: Hlc::new(NOW, 0, 99),
                    seq: 6,
                },
                &Payload::Json(ssh_host("theirs", None, None)),
                NOW,
                99,
            )
            .unwrap();
        theirs.seq = Some(8);
        let refused = s.pushed(
            sent,
            Pushed {
                accepted: vec![Accepted { id: id(1), seq: 7 }],
                conflicts: vec![theirs],
                cursor: 8,
            },
        );
        assert_eq!(refused, [id(2)]);
        let view = s.view();
        let one = view.iter().find(|r| r.id == id(1)).unwrap();
        assert_eq!(
            (one.seq, one.data.as_ref().unwrap()["name"].as_str()),
            (7, Some("1"))
        );
        let two = view.iter().find(|r| r.id == id(2)).unwrap();
        assert_eq!(
            (two.seq, two.data.as_ref().unwrap()["name"].as_str()),
            (8, Some("theirs"))
        );
        // An older copy arriving late doesn't win.
        let stale = s.records[&id(1)].env.clone();
        s.apply(Envelope {
            seq: Some(3),
            ..stale
        });
        assert_eq!(s.view().iter().find(|r| r.id == id(1)).unwrap().seq, 7);
    }

    #[test]
    fn nothing_is_sealed_on_a_record_the_server_changed() {
        let mut s = state(Space::Ssh);
        put(&mut s, &id(6), KIND_SECRET, json!("hunter2"), 1);
        put(
            &mut s,
            &id(2),
            "identity",
            json!({ "label": "me", "username": "me", "password_secret_id": id(6) }),
            2,
        );
        put(&mut s, &id(7), KIND_SECRET, json!("other"), 3);
        // The server moves both secrets' clocks to the far future: an edit on
        // top would carry that clock to every device.
        for (n, seq) in [(6, 4), (7, 5)] {
            let mut forged = s.records[&id(n)].env.clone();
            forged.updated_at = Hlc::new(u64::MAX - 1, u32::MAX, 1);
            forged.seq = Some(seq);
            s.apply(forged);
        }
        let refused = s
            .plan(
                vec![Op::Secret {
                    id: id(6),
                    text: "new".into(),
                    seq: None,
                }],
                NOW,
                DEVICE,
            )
            .unwrap_err();
        assert_eq!(refused.kind, "crypto");
        let refused = s
            .plan(
                vec![Op::Delete {
                    id: id(7),
                    seq: None,
                }],
                NOW,
                DEVICE,
            )
            .unwrap_err();
        assert_eq!(refused.kind, "crypto");
        assert!(s.secret_text(&id(6)).is_err());
        // Deleting the identity leaves its broken secret alone.
        let planned = s
            .plan(
                vec![Op::Delete {
                    id: id(2),
                    seq: None,
                }],
                NOW,
                DEVICE,
            )
            .unwrap();
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].id, id(2));
        assert!(planned[0].updated_at.wall_ms < u64::MAX / 2);

        // A tombstone that doesn't open is no tombstone of ours either.
        put(&mut s, &id(1), "host", ssh_host("one", None, None), 6);
        let mut forged = s.records[&id(1)].env.clone();
        forged.deleted = true;
        forged.seq = Some(7);
        s.apply(forged);
        assert!(s.records[&id(1)].broken);
    }

    #[test]
    fn an_id_in_an_unusual_form_is_dropped() {
        let mut s = state(Space::Ssh);
        put(&mut s, &id(0xab), "host", ssh_host("one", None, None), 1);
        let mut copy = s.records[&id(0xab)].env.clone();
        // The same bytes, so it would open: still not taken twice.
        copy.id = id(0xab).to_uppercase();
        copy.seq = Some(2);
        s.apply(copy);
        assert_eq!(s.view().len(), 1);
        assert_eq!(s.view()[0].seq, 1);
    }

    #[test]
    fn secrets_show_up_without_their_value() {
        let mut s = state(Space::Ssh);
        put(&mut s, &id(6), KIND_SECRET, json!("hunter2"), 1);
        let view = s.view();
        assert_eq!(view[0].kind, "secret");
        assert_eq!(view[0].data, None);
        assert_eq!(&*s.secret_text(&id(6)).unwrap(), "hunter2");
        // Changing it keeps the id and moves the clock on.
        let planned = s
            .plan(
                vec![Op::Secret {
                    id: id(6),
                    text: "correct horse".into(),
                    seq: Some(1),
                }],
                NOW,
                DEVICE,
            )
            .unwrap();
        assert_eq!(planned[0].base_seq, 1);
        assert_eq!(&*s.vault.open(&planned[0]).unwrap(), b"correct horse");
    }

    #[test]
    fn merging_replaces_values_and_recurses_into_objects() {
        let mut base = json!({ "a": 1, "rdp": { "nla": true, "x": 1 }, "g": "id" });
        merge(
            &mut base,
            &json!({ "rdp": { "nla": false }, "g": null, "list": [1] }),
        );
        assert_eq!(
            base,
            json!({ "a": 1, "rdp": { "nla": false, "x": 1 }, "g": null, "list": [1] })
        );
    }

    #[test]
    fn an_rdp_file_carries_no_password_and_no_drives() {
        let host = json!({ "name": "desk", "address": "2001:db8::10", "port": 3390,
            "rdp": { "display": "fixed", "width": 1280, "height": 720, "audio": "off",
                     "admin": true, "wallpaper": false,
                     "drives": { "enabled": true, "drives": [{ "name": "C", "path": "*" }] },
                     "gateway": { "address": "gw.example.com", "port": 443, "bypassLocal": true } } });
        let identity = json!({ "username": "anna", "domain": "EXAMPLE\r\nfull address:s:evil",
            "password_secret_id": id(6) });
        let text = rdp_file(&host, Some(&identity));
        assert!(text.contains("full address:s:[2001:db8::10]:3390\r\n"));
        assert!(text.contains("username:s:EXAMPLE  full address:s:evil\\anna\r\n"));
        assert_eq!(
            text.matches("full address").count(),
            2,
            "only in the one line, not as a line of its own"
        );
        assert!(!text.lines().any(|l| l.starts_with("full address:s:evil")));
        assert!(text.contains("desktopwidth:i:1280\r\n"));
        assert!(text.contains("audiomode:i:2\r\n"));
        assert!(text.contains("administrative session:i:1\r\n"));
        assert!(text.contains("disable wallpaper:i:1\r\n"));
        assert!(text.contains("redirectdrives:i:0\r\n"));
        assert!(text.contains("drivestoredirect:s:\r\n"));
        assert!(text.contains("gatewayhostname:s:gw.example.com\r\n"));
        assert!(text.contains("gatewayusagemethod:i:2\r\n"));
        assert!(!text.to_lowercase().contains("password"));
    }

    #[test]
    fn foreign_keys_are_told_by_their_first_line() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----\n";
        assert_eq!(foreign_key_type(pem).as_deref(), Some("ssh-rsa"));
        assert_eq!(
            foreign_key_type("PuTTY-User-Key-File-3: ssh-ed25519\nEncryption: none").as_deref(),
            Some("ssh-ed25519")
        );
        assert_eq!(
            foreign_key_type("-----BEGIN PRIVATE KEY-----\n").as_deref(),
            Some("")
        );
        assert_eq!(foreign_key_type("ssh-ed25519 AAAA user@example.com"), None);
        assert_eq!(foreign_key_type("hello"), None);
    }

    #[test]
    fn a_host_logs_in_with_its_groups_identity_when_it_has_none() {
        let mut s = state(Space::Rdp);
        put(
            &mut s,
            &id(5),
            "identity",
            json!({ "label": "l", "username": "groupuser",
            "auth_type": "password", "key_id": null, "password_secret_id": null }),
            1,
        );
        put(
            &mut s,
            &id(1),
            "group",
            json!({ "workspace": "private", "name": "g",
            "position": 0, "identity_id": id(5) }),
            2,
        );
        let host = ssh_host("h", Some(&id(1)), None);
        assert_eq!(identity_of(&s, &host).unwrap()["username"], "groupuser");
    }
}
