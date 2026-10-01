//! The suite vault on a UwULock Server (contract §6): UwUSSH's and UwURDP's
//! records, kept beside the Bitwarden vault, each app in a space of its own.
//!
//! This is the transport and the keys; sealing and opening the records
//! (XChaCha20-Poly1305 with the AAD of §6.2) is `uwulock_core::suite`, the
//! merge stays the apps'.
//! What an app does:
//!
//! 1. log in with [`crate::App::suite`] (`client_id=uwussh`, scope
//!    `uwu.suite`), open the user key and the private key as usual;
//! 2. [`Client::extras_key`] — opens the extras key, makes it, or wraps it
//!    again after an official client rotated the user key;
//! 3. [`Client::suite_space`] — the space's id and key, made on first use;
//! 4. [`Client::suite_pull`] / [`Client::suite_push`] with its envelopes.

use serde_json::{json, Value};

use crate::api::Client;
use crate::crypto::SymmetricKey;
use crate::uwu::{uwu_path, UwuError, UwuResult};
use crate::Error;
use uwulock_core::extras::SpaceKey;

// The wire types are uwulock-core's, which also seals and opens the records
// (`uwulock_core::suite`); `Space` and `Clock` are their names here.
pub use uwulock_core::suite::{
    Accepted, Envelope, Hlc as Clock, Pull, PushRequest, Pushed, SuiteSpace as Space, PAGE,
};

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> UwuResult<T> {
    serde_json::from_value(value).map_err(|e| {
        UwuError::Core(Error::Server {
            status: 200,
            message: format!("{what} doesn't read: {e}"),
        })
    })
}

impl Client {
    /// `GET /uwu/v1/suite/spaces`.
    pub async fn suite_spaces(&self, access_token: &str) -> UwuResult<Vec<Space>> {
        let list = self.uwu_get(access_token, "/suite/spaces").await?;
        parse(
            list.get("data").cloned().unwrap_or(json!([])),
            "the list of spaces",
        )
    }

    /// The id and key of `space` (`ssh`, `rdp`, `mail`, `generic`), made
    /// with a fresh key and id if there is none yet. Another device that made
    /// it at the same moment wins, and its space is taken.
    pub async fn suite_space(
        &self,
        access_token: &str,
        space: &str,
        extras: &SymmetricKey,
    ) -> UwuResult<(String, SpaceKey)> {
        let existing = self
            .suite_spaces(access_token)
            .await?
            .into_iter()
            .find(|s| s.space == space);
        let found = match existing {
            Some(found) => found,
            None => {
                let key = SpaceKey::generate();
                let id = new_id();
                let body = json!({ "id": id, "key": key.wrap(extras) });
                let path = format!("/suite/spaces/{}", uwu_path(space));
                match self.uwu_put(access_token, &path, &body).await {
                    Ok(_) => return Ok((id, key)),
                    Err(error) if error.code() == Some("exists") => self
                        .suite_spaces(access_token)
                        .await?
                        .into_iter()
                        .find(|s| s.space == space)
                        .ok_or(UwuError::Core(Error::Conflict))?,
                    Err(error) => return Err(error),
                }
            }
        };
        let key = SpaceKey::unwrap(&found.key, extras)?;
        Ok((found.id, key))
    }

    /// Records with `seq > since`, one page. Follow `has_more` with `cursor`.
    pub async fn suite_pull(&self, access_token: &str, space: &str, since: u64) -> UwuResult<Pull> {
        let path = format!(
            "/suite/spaces/{}/records?since={since}&limit={PAGE}",
            uwu_path(space)
        );
        parse(self.uwu_get(access_token, &path).await?, "a pull")
    }

    /// Pushes at most [`PAGE`] records in one transaction. `space_id` is the
    /// id they were sealed for: after a rekey the server refuses the push with
    /// 409 `space_changed` instead of writing it.
    pub async fn suite_push(
        &self,
        access_token: &str,
        space: &str,
        space_id: &str,
        records: &[Envelope],
    ) -> UwuResult<Pushed> {
        if records.len() > PAGE {
            return Err(UwuError::Core(Error::Refused(format!(
                "at most {PAGE} records a push"
            ))));
        }
        let path = format!("/suite/spaces/{}/records", uwu_path(space));
        let body = json!({ "schema": uwulock_core::suite::SCHEMA, "spaceId": space_id, "records": records });
        parse(self.uwu_post(access_token, &path, &body).await?, "a push")
    }
}

/// A random UUID (v4), for a new space.
fn new_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_speak_the_contracts_json() {
        let text = r#"{ "id": "9b2d", "kind": "host",
            "updatedAt": { "wallMs": 1790000000000, "counter": 0, "device": 305419896 },
            "baseSeq": 0, "deleted": false, "nonce": "AAAA", "blob": "BBBB", "seq": 17 }"#;
        let envelope: Envelope = serde_json::from_str(text).unwrap();
        assert_eq!(envelope.updated_at.device, 305_419_896);
        assert_eq!(envelope.seq, Some(17));
        let mut out = serde_json::to_value(&envelope).unwrap();
        assert_eq!(out["updatedAt"]["wallMs"], 1_790_000_000_000u64);
        out.as_object_mut().unwrap().remove("seq");
        let fresh = Envelope {
            seq: None,
            ..envelope
        };
        assert_eq!(serde_json::to_value(&fresh).unwrap(), out);
        let id = new_id();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
    }
}
