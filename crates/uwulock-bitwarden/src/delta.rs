//! The offline copy of an account, kept up to date by UwULock Server's delta
//! sync (`GET /uwu/v1/sync`, contract §4).
//!
//! The copy is the vault in exactly the shape of Bitwarden's `/api/sync` (so
//! [`crate::api::parse_sync`] and `Vault::open` read it as they read a
//! full sync), UwULock's own state beside it ([`UwuState`]), and the cursor
//! the server gave with the last page. A page with `reset: true` replaces
//! everything; any other is merged in: objects by id, deletions by id, the
//! profile and the other singletons when they changed.
//!
//! The cursor is only ever stored together with what it describes, so a
//! crash between the two can't make the next delta skip anything.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use crate::Error;

/// The lists of `/api/sync` a delta merges by id, with their names in
/// `vault.deleted`.
const LISTS: [&str; 4] = ["folders", "collections", "ciphers", "sends"];
/// What a delta sends only when it changed.
const SINGLES: [&str; 4] = ["profile", "policies", "domains", "userDecryption"];

/// An own icon's entry in the sync (`uwu.icons`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IconRef {
    pub revision_date: Option<String>,
    /// `extras` for a personal item, `organization` for an organisation's.
    pub key_type: String,
}

/// A masked address linked to an item (`uwu.maskedLinks`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MaskedLink {
    pub id: String,
    pub email: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Unseen {
    pub security_notices: u32,
    pub file_request_submissions: u32,
}

/// UwULock's own part of the sync (`uwu`), merged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UwuState {
    /// The §3 object as it last came; opened by whoever needs it.
    pub extras_key: Option<Value>,
    /// Own icons by cipher id.
    pub icons: BTreeMap<String, IconRef>,
    /// The §10 reminders (`reminder` objects), all of them.
    pub reminders: Vec<Value>,
    /// The §9 travel-mode object.
    pub travel: Option<Value>,
    /// Which send domain each Send's link uses (`null`: the main host).
    pub send_domains: BTreeMap<String, Option<String>>,
    /// Masked addresses by cipher id.
    pub masked_links: BTreeMap<String, MaskedLink>,
    pub unseen: Unseen,
}

impl UwuState {
    /// Whether travel mode is on.
    pub fn travelling(&self) -> bool {
        self.travel
            .as_ref()
            .and_then(|t| t.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// What of it the person gets to see on a server with these switches:
    /// the delta sync still carries what an account had before an admin
    /// switched an extra off (nothing is deleted), and the apps hide it.
    /// Only the copy in memory is cut; the offline copy keeps everything, so
    /// switched on again, it is all back without a full sync.
    pub fn honouring(mut self, info: &crate::uwu::Info) -> Self {
        if !info.allows("own-icons") {
            self.icons.clear();
        }
        if !info.allows("reminders") {
            self.reminders.clear();
        }
        if !info.allows("travel-mode") {
            self.travel = None;
        }
        if !info.allows("send-domains") {
            self.send_domains.clear();
        }
        if !info.allows("masked-addresses") {
            self.masked_links.clear();
        }
        if !info.allows("file-requests") {
            self.unseen.file_request_submissions = 0;
        }
        self
    }

    /// The reminder of one item, if it has one.
    pub fn reminder(&self, cipher_id: &str) -> Option<&Value> {
        self.reminders
            .iter()
            .find(|r| r.get("cipherId").and_then(Value::as_str) == Some(cipher_id))
    }

    fn merge(&mut self, uwu: &Value, reset: bool) -> Result<(), Error> {
        if reset {
            *self = UwuState::default();
        }
        let get = |key: &str| uwu.get(key).filter(|v| !v.is_null());
        if let Some(key) = get("extrasKey") {
            self.extras_key = Some(key.clone());
        }
        if let Some(icons) = get("icons").and_then(Value::as_array) {
            for icon in icons {
                let Some(id) = icon.get("cipherId").and_then(Value::as_str) else {
                    continue;
                };
                self.icons.insert(
                    id.to_string(),
                    serde_json::from_value(icon.clone()).map_err(unreadable)?,
                );
            }
        }
        if let Some(gone) = get("iconsDeleted").and_then(Value::as_array) {
            for id in gone.iter().filter_map(Value::as_str) {
                self.icons.remove(id);
            }
        }
        if let Some(reminders) = get("reminders") {
            // The full list, as an array or as a `{ data: [...] }` list.
            let list = reminders
                .as_array()
                .or_else(|| reminders.get("data").and_then(Value::as_array));
            self.reminders = list.cloned().unwrap_or_default();
        }
        if let Some(travel) = get("travel") {
            self.travel = Some(travel.clone());
        }
        if let Some(domains) = get("sendDomains").and_then(Value::as_object) {
            for (send, domain) in domains {
                self.send_domains
                    .insert(send.clone(), domain.as_str().map(str::to_string));
            }
        }
        if let Some(links) = get("maskedLinks").and_then(Value::as_object) {
            self.masked_links = links
                .iter()
                .filter_map(|(cipher, link)| {
                    Some((cipher.clone(), serde_json::from_value(link.clone()).ok()?))
                })
                .collect();
        }
        if let Some(unseen) = get("unseen") {
            self.unseen = serde_json::from_value(unseen.clone()).map_err(unreadable)?;
        }
        Ok(())
    }

    /// Drops what belongs to items and Sends that are gone.
    fn forget(&mut self, ciphers: &[&str], sends: &[&str]) {
        for id in ciphers {
            self.icons.remove(*id);
            self.masked_links.remove(*id);
        }
        self.reminders.retain(|r| {
            r.get("cipherId")
                .and_then(Value::as_str)
                .is_none_or(|id| !ciphers.contains(&id))
        });
        for id in sends {
            self.send_domains.remove(*id);
        }
    }
}

/// What a device keeps of an account on a UwULock Server.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Synced {
    /// The vault as `/api/sync` would give it.
    pub sync: Value,
    pub uwu: UwuState,
    /// Where the next delta starts. `None`: the next sync is a full one.
    pub cursor: Option<String>,
}

impl Synced {
    /// A copy that started from a plain `/api/sync` (Bitwarden, Vaultwarden,
    /// or a UwULock Server before its delta sync): no cursor.
    pub fn from_full_sync(sync: Value) -> Self {
        Synced {
            sync,
            ..Synced::default()
        }
    }

    /// Merges one page of `/uwu/v1/sync`. Returns `hasMore`: whether the
    /// next page should be asked for right away.
    pub fn apply(&mut self, page: &Value) -> Result<bool, Error> {
        let bad = |what: &str| Error::Server {
            status: 200,
            message: format!("the delta sync has no {what}"),
        };
        let cursor = page
            .get("cursor")
            .and_then(Value::as_str)
            .ok_or_else(|| bad("cursor"))?;
        let reset = page.get("reset").and_then(Value::as_bool).unwrap_or(false);
        let vault = page.get("vault").filter(|v| v.is_object());

        if reset {
            let vault = vault.ok_or_else(|| bad("vault"))?;
            let mut sync = Map::new();
            sync.insert("object".into(), json!("sync"));
            for name in LISTS {
                sync.insert(name.into(), list_of(vault, name));
            }
            for name in SINGLES {
                sync.insert(name.into(), vault.get(name).cloned().unwrap_or(Value::Null));
            }
            self.sync = Value::Object(sync);
        } else if let Some(vault) = vault {
            if !self.sync.is_object() {
                return Err(Error::Server {
                    status: 200,
                    message: "a delta came for a vault that isn't here".into(),
                });
            }
            let deleted = vault.get("deleted");
            let mut gone_ciphers = Vec::new();
            let mut gone_sends = Vec::new();
            for name in LISTS {
                let list = list_mut(&mut self.sync, name);
                for changed in vault
                    .get(name)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let Some(id) = id_of(changed) else { continue };
                    match list.iter_mut().find(|o| id_of(o) == Some(id)) {
                        Some(old) => *old = changed.clone(),
                        None => list.push(changed.clone()),
                    }
                }
                let removed: Vec<&str> = deleted
                    .and_then(|d| d.get(name))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                list.retain(|o| id_of(o).is_none_or(|id| !removed.contains(&id)));
                match name {
                    "ciphers" => gone_ciphers = removed,
                    "sends" => gone_sends = removed,
                    _ => {}
                }
            }
            for name in SINGLES {
                if let Some(value) = vault.get(name).filter(|v| !v.is_null()) {
                    self.sync[name] = value.clone();
                }
            }
            self.uwu.forget(&gone_ciphers, &gone_sends);
        }
        if let Some(uwu) = page.get("uwu").filter(|v| v.is_object()) {
            self.uwu.merge(uwu, reset)?;
        } else if reset {
            self.uwu = UwuState::default();
        }
        self.cursor = Some(cursor.to_string());
        Ok(page
            .get("hasMore")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }

    /// The vault part as text, for `parse_sync`.
    pub fn sync_text(&self) -> String {
        self.sync.to_string()
    }
}

fn unreadable(error: serde_json::Error) -> Error {
    Error::Server {
        status: 200,
        message: format!("the delta sync doesn't read: {error}"),
    }
}

fn list_of(vault: &Value, name: &str) -> Value {
    Value::Array(
        vault
            .get(name)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    )
}

/// The list `name` of a sync, whatever case its key was written in; made
/// if it isn't there.
fn list_mut<'a>(sync: &'a mut Value, name: &str) -> &'a mut Vec<Value> {
    let map = sync.as_object_mut().expect("checked to be an object");
    let key = map
        .keys()
        .find(|k| k.eq_ignore_ascii_case(name))
        .cloned()
        .unwrap_or_else(|| name.to_string());
    let slot = map.entry(key).or_insert_with(|| json!([]));
    if !slot.is_array() {
        *slot = json!([]);
    }
    slot.as_array_mut().expect("an array")
}

fn id_of(value: &Value) -> Option<&str> {
    value
        .get("id")
        .or_else(|| value.get("Id"))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Value {
        json!({
            "object": "uwuSync", "reset": true, "cursor": "c1", "hasMore": false,
            "vault": {
                "profile": { "id": "u1", "key": "2.a|b|c" },
                "folders": [{ "id": "f1", "name": "2.f" }],
                "collections": [],
                "ciphers": [{ "id": "a", "type": 2, "name": "2.a" }, { "id": "b", "type": 2, "name": "2.b" }],
                "sends": [{ "id": "s1", "type": 0 }],
                "policies": [], "domains": null, "userDecryption": null,
                "deleted": { "folders": [], "collections": [], "ciphers": [], "sends": [] }
            },
            "uwu": {
                "extrasKey": null,
                "icons": [{ "cipherId": "a", "revisionDate": "r1", "keyType": "extras" }],
                "iconsDeleted": [],
                "reminders": [{ "object": "reminder", "cipherId": "b", "due": "2027-01-15", "isDue": false }],
                "travel": { "object": "travelMode", "enabled": false },
                "sendDomains": { "s1": null },
                "maskedLinks": { "a": { "id": "x42", "email": "quiet.otter17@masked.example.com" } },
                "unseen": { "securityNotices": 2, "fileRequestSubmissions": 0 }
            }
        })
    }

    #[test]
    fn a_full_sync_then_a_delta() {
        let mut synced = Synced::default();
        assert!(!synced.apply(&full()).unwrap());
        assert_eq!(synced.cursor.as_deref(), Some("c1"));
        assert_eq!(synced.sync["ciphers"].as_array().unwrap().len(), 2);
        assert_eq!(synced.uwu.unseen.security_notices, 2);
        assert_eq!(synced.uwu.masked_links["a"].id, "x42");
        assert!(crate::api::parse_sync(&synced.sync_text()).is_ok());

        let delta = json!({
            "object": "uwuSync", "reset": false, "cursor": "c2", "hasMore": true,
            "vault": {
                "profile": null, "folders": [], "collections": [],
                "ciphers": [{ "id": "b", "name": "2.b2" }, { "id": "c", "name": "2.c" }],
                "sends": [], "policies": null, "domains": null, "userDecryption": null,
                "deleted": { "folders": ["f1"], "collections": [], "ciphers": ["a"], "sends": [] }
            },
            "uwu": {
                "extrasKey": { "userKeyWrapped": "2.k", "privateKeyWrapped": "2.p" },
                "icons": [], "iconsDeleted": [], "reminders": null,
                "travel": { "object": "travelMode", "enabled": true },
                "sendDomains": { "s1": "d1" }, "maskedLinks": null,
                "unseen": { "securityNotices": 0, "fileRequestSubmissions": 1 }
            }
        });
        assert!(synced.apply(&delta).unwrap());
        assert_eq!(synced.cursor.as_deref(), Some("c2"));
        let names: Vec<&str> = synced.sync["ciphers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["2.b2", "2.c"]);
        assert!(synced.sync["folders"].as_array().unwrap().is_empty());
        assert_eq!(synced.sync["profile"]["id"], "u1");
        // What belonged to the deleted item went with it; the rest stayed.
        assert!(synced.uwu.icons.is_empty());
        assert!(synced.uwu.masked_links.is_empty());
        assert_eq!(synced.uwu.reminders.len(), 1);
        assert!(synced.uwu.travelling());
        assert_eq!(synced.uwu.send_domains["s1"].as_deref(), Some("d1"));
        assert_eq!(synced.uwu.unseen.file_request_submissions, 1);
        assert!(synced.uwu.extras_key.is_some());

        // A reset replaces everything, UwULock's part too.
        synced.apply(&full()).unwrap();
        assert_eq!(synced.sync["ciphers"].as_array().unwrap().len(), 2);
        assert!(synced.uwu.extras_key.is_none());
        assert!(!synced.uwu.travelling());
    }

    #[test]
    fn switched_off_extras_are_hidden_but_kept() {
        let mut synced = Synced::default();
        synced.apply(&full()).unwrap();
        synced.uwu.unseen.file_request_submissions = 3;
        let info = |switches: Value| -> crate::uwu::Info {
            serde_json::from_value(json!({ "name": "UwULock Server", "switches": switches }))
                .unwrap()
        };

        // An older server without switches: everything shows.
        let older: crate::uwu::Info =
            serde_json::from_value(json!({ "name": "UwULock Server" })).unwrap();
        assert_eq!(synced.uwu.clone().honouring(&older), synced.uwu);

        let off = info(json!({
            "own-icons": false, "reminders": false, "travel-mode": false,
            "send-domains": false, "masked-addresses": false, "file-requests": false,
            "versions": true
        }));
        let shown = synced.uwu.clone().honouring(&off);
        assert!(shown.icons.is_empty() && shown.reminders.is_empty());
        assert!(shown.travel.is_none() && shown.send_domains.is_empty());
        assert!(shown.masked_links.is_empty());
        assert_eq!(shown.unseen.file_request_submissions, 0);
        // What no switch covers stays.
        assert_eq!(shown.unseen.security_notices, 2);
        // The offline copy still has it all, for when it is on again.
        assert_eq!(synced.uwu.reminders.len(), 1);
        assert_eq!(synced.uwu.masked_links["a"].id, "x42");

        let some = info(json!({ "reminders": false, "own-icons": true }));
        let shown = synced.uwu.clone().honouring(&some);
        assert!(shown.reminders.is_empty());
        assert_eq!(shown.icons.len(), 1);
        assert_eq!(shown.masked_links.len(), 1);
    }

    #[test]
    fn a_page_without_a_cursor_changes_nothing() {
        let mut synced = Synced::default();
        synced.apply(&full()).unwrap();
        let before = synced.clone();
        let mut broken = full();
        broken.as_object_mut().unwrap().remove("cursor");
        assert!(synced.apply(&broken).is_err());
        assert_eq!(synced, before);
        // A delta for a copy that has no vault yet can't be merged.
        let mut empty = Synced::default();
        let mut delta = full();
        delta["reset"] = json!(false);
        assert!(empty.apply(&delta).is_err());
    }
}
