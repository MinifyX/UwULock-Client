//! UwULock Server's own API, under `/uwu/v1` (UwULock-Server's
//! `docs/uwu-api.md`): what the server can do, the extras key, the delta sync.
//! Bitwarden and Vaultwarden don't have it; [`Client::uwu_info`] says `None`
//! there and nothing else in here is called.
//!
//! Errors under `/uwu/v1` carry a stable `code` besides Bitwarden's message;
//! [`UwuError::Refused`] keeps it, so a caller can tell "not connected" from
//! "switched off" without reading English.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::api::{escape, send, Client, Server};
use crate::crypto::{PrivateKey, SymmetricKey};
use crate::Error;
use uwulock_core::extras::{self, Keys, Resolved};

/// What went wrong with a call to `/uwu/v1`.
#[derive(Debug, thiserror::Error)]
pub enum UwuError {
    #[error(transparent)]
    Core(#[from] Error),
    /// The server said no, with its reason.
    #[error("{message}")]
    Refused {
        status: u16,
        /// `invalid`, `not_found`, `feature_off`, `conflict`, `exists`,
        /// `quota`, `not_connected`, … (the contract's codes); empty if the
        /// server gave none.
        code: String,
        message: String,
    },
}

impl UwuError {
    /// The contract's `code`, if the server refused with one.
    pub fn code(&self) -> Option<&str> {
        match self {
            UwuError::Refused { code, .. } if !code.is_empty() => Some(code),
            _ => None,
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            UwuError::Refused { status, .. } => Some(*status),
            UwuError::Core(Error::Server { status, .. }) => Some(*status),
            _ => None,
        }
    }
}

impl From<UwuError> for Error {
    fn from(error: UwuError) -> Self {
        match error {
            UwuError::Core(error) => error,
            UwuError::Refused {
                status, message, ..
            } => match status {
                401 => Error::SessionExpired,
                409 => Error::Conflict,
                403 | 404 => Error::Refused(message),
                _ => Error::Server { status, message },
            },
        }
    }
}

pub type UwuResult<T> = Result<T, UwuError>;

/// `GET /uwu/v1/info`: what this server is and can do.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Info {
    pub name: String,
    pub version: Option<String>,
    pub api_version: Option<u32>,
    pub public_url: Option<String>,
    pub web_vault: bool,
    pub mail: bool,
    /// Present only when the server has the feature and it is switched on.
    pub features: Vec<String>,
    /// The admin's feature switches (0.6.0-beta.2, UwULock-Server's
    /// `docs/features.md`): `true` when the extra works. `None` from an older
    /// server, where `features` tells everything.
    pub switches: Option<BTreeMap<String, bool>>,
    pub send_domains: Vec<SendDomain>,
    pub icons: Option<IconsInfo>,
    pub branding: Option<Value>,
    pub limits: Option<Limits>,
    /// Which breach sources are on (0.7, §15.1); `None` from an older server.
    pub breaches: Option<crate::health::BreachSwitches>,
}

impl Info {
    /// Whether the server offers `feature`: it lists it, and no switch says
    /// it is off.
    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature) && self.allows(feature)
    }

    /// Whether the admin left the extra `switch` on. An older server, or a
    /// name it doesn't switch, allows it: that is how it was before switches.
    pub fn allows(&self, switch: &str) -> bool {
        self.switches
            .as_ref()
            .and_then(|switches| switches.get(switch))
            .copied()
            .unwrap_or(true)
    }

    /// The features without any a switch says is off.
    pub fn offered(&self) -> Vec<String> {
        self.features
            .iter()
            .filter(|f| self.allows(f))
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SendDomain {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IconsInfo {
    pub automatic: bool,
    pub url: Option<String>,
    pub own_max_bytes: Option<u64>,
    pub own_pixels: Option<u32>,
    pub library: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    pub max_file_bytes: Option<u64>,
    pub versions_per_item: Option<u32>,
    pub version_days: Option<u32>,
    pub file_request_max_files: Option<u32>,
    pub file_request_max_days: Option<u32>,
}

/// Which parts of the account a delta sync covers.
pub const SYNC_INCLUDE: &str = "vault,uwu";

impl Client {
    /// `<server>/uwu/v1<path>`.
    pub fn uwu_url(&self, path: &str) -> String {
        format!("{}/uwu/v1{path}", self.server().web())
    }

    /// Whether the server could be a UwULock Server at all: the Bitwarden
    /// clouds never are.
    pub fn may_be_uwulock(&self) -> bool {
        matches!(self.server(), Server::SelfHosted { .. })
    }

    /// `GET /uwu/v1/info`. `None`: not a UwULock Server (Bitwarden,
    /// Vaultwarden, or one that doesn't answer it).
    pub async fn uwu_info(&self) -> Result<Option<Info>, Error> {
        if !self.may_be_uwulock() {
            return Ok(None);
        }
        let response = send(self.request(reqwest::Method::GET, self.uwu_url("/info"))).await?;
        if !response.ok() {
            return Ok(None);
        }
        let Ok(info) = serde_json::from_str::<Info>(&response.body) else {
            return Ok(None);
        };
        Ok(info.name.starts_with("UwULock").then_some(info))
    }

    /// A JSON call under `/uwu/v1` with the session. `path` starts with `/`;
    /// ids in it go through [`uwu_path`]. An empty answer is `Value::Null`.
    pub async fn uwu_call<B: Serialize + ?Sized>(
        &self,
        access_token: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<&B>,
    ) -> UwuResult<Value> {
        let mut request = self
            .request(method, self.uwu_url(path))
            .bearer_auth(access_token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = send(request).await?;
        if !response.ok() {
            return Err(refusal(response.status, &response.body));
        }
        if response.body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&response.body).map_err(|e| {
            UwuError::Core(Error::Server {
                status: response.status,
                message: format!("the server's answer isn't JSON: {e}"),
            })
        })
    }

    pub async fn uwu_get(&self, access_token: &str, path: &str) -> UwuResult<Value> {
        self.uwu_call::<()>(access_token, reqwest::Method::GET, path, None)
            .await
    }

    pub async fn uwu_post<B: Serialize + ?Sized>(
        &self,
        access_token: &str,
        path: &str,
        body: &B,
    ) -> UwuResult<Value> {
        self.uwu_call(access_token, reqwest::Method::POST, path, Some(body))
            .await
    }

    pub async fn uwu_put<B: Serialize + ?Sized>(
        &self,
        access_token: &str,
        path: &str,
        body: &B,
    ) -> UwuResult<Value> {
        self.uwu_call(access_token, reqwest::Method::PUT, path, Some(body))
            .await
    }

    pub async fn uwu_patch<B: Serialize + ?Sized>(
        &self,
        access_token: &str,
        path: &str,
        body: &B,
    ) -> UwuResult<Value> {
        self.uwu_call(access_token, reqwest::Method::PATCH, path, Some(body))
            .await
    }

    pub async fn uwu_delete(&self, access_token: &str, path: &str) -> UwuResult<Value> {
        self.uwu_call::<()>(access_token, reqwest::Method::DELETE, path, None)
            .await
    }

    /// Raw bytes from `/uwu/v1` (a file request's file), at most `max` of them.
    pub async fn uwu_download(
        &self,
        access_token: &str,
        path: &str,
        max: u64,
    ) -> UwuResult<Vec<u8>> {
        let response = self
            .request(reqwest::Method::GET, self.uwu_url(path))
            .header("Accept", "application/octet-stream")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| UwuError::Core(crate::api::network_error(e)))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body = crate::api::error_text(response).await;
            return Err(refusal(status, &body));
        }
        crate::api::read_capped(response, usize::try_from(max).unwrap_or(usize::MAX))
            .await
            .map_err(UwuError::Core)
    }

    // ── The extras key (§3) ────────────────────────────────

    /// The account's extras key, opened: made if there is none yet, wrapped
    /// again for the user key after an official client rotated it, and for
    /// the private key if it was made before that wrap existed. `None` when
    /// it is lost (the key pair changed; the web vault offers to start over).
    /// `private_key` is the account's own; without it, only a key wrapped for
    /// the user key opens, and the two wraps aren't checked against each
    /// other.
    pub async fn extras_key(
        &self,
        access_token: &str,
        user_key: &SymmetricKey,
        private_key: Option<&PrivateKey>,
    ) -> UwuResult<Option<SymmetricKey>> {
        // Twice at most: a client that loses the race to make the key takes
        // the winner's.
        for _ in 0..2 {
            let keys: Keys = serde_json::from_value(self.uwu_get(access_token, "/keys").await?)
                .map_err(|e| UwuError::Core(Error::Crypto(format!("/uwu/v1/keys: {e}"))))?;
            match extras::resolve(&keys, user_key, private_key)? {
                Resolved::Lost => return Ok(None),
                Resolved::Open {
                    key,
                    rewrap,
                    private_wrap,
                } => {
                    // Best effort: whoever comes next does it again.
                    if let Some(body) = rewrap {
                        if let Err(error) =
                            self.uwu_put(access_token, "/keys/user-wrap", &body).await
                        {
                            tracing::warn!(%error, "couldn't wrap the extras key again");
                        }
                    }
                    if let Some(body) = private_wrap {
                        if let Err(error) = self
                            .uwu_put(access_token, "/keys/private-wrap", &body)
                            .await
                        {
                            tracing::warn!(%error, "couldn't add the extras key's private wrap");
                        }
                    }
                    return Ok(Some(key));
                }
                Resolved::Create(made) => {
                    match self.uwu_post(access_token, "/keys", &made.request).await {
                        Ok(_) => return Ok(Some(made.key)),
                        Err(error) if error.code() == Some("exists") => continue,
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Err(UwuError::Core(Error::Conflict))
    }

    // ── Delta sync (§4) ────────────────────────────────────

    /// One page of `GET /uwu/v1/sync`: everything without a cursor, else what
    /// changed since. Follow `hasMore` with the new cursor.
    pub async fn uwu_sync(&self, access_token: &str, since: Option<&str>) -> UwuResult<Value> {
        let mut path = format!("/sync?include={SYNC_INCLUDE}");
        if let Some(cursor) = since {
            path.push_str("&since=");
            path.push_str(&uwu_path(cursor));
        }
        self.uwu_get(access_token, &path).await
    }
}

// ── Typed calls for UwULock's extras ───────────────────────
//
// Each is one endpoint of the contract, named after it. The values that are
// encrypted stay EncStrings here; opening them is the caller's business.

/// `GET /uwu/v1/account` (§2): the parts a client shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UwuAccount {
    /// The account's default send domain; `None` is the main host.
    pub send_domain_id: Option<String>,
    pub travel: Option<Value>,
    pub masked_connected: bool,
    pub security_notices_unseen: u32,
    /// The account's consent to the check of its addresses (0.7, §15.4);
    /// `None` while the admin has the check switched off.
    pub email_breach_check: Option<crate::health::EmailOptIn>,
}

/// An own icon (§7.3). `data` only when it was asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OwnIcon {
    pub cipher_id: String,
    /// `extras` or `organization`.
    pub key_type: String,
    pub data: Option<String>,
    pub revision_date: Option<String>,
}

/// The most own icons one `POST /uwu/v1/icons/own/get` may ask for.
pub const OWN_ICONS_PER_CALL: usize = 500;

/// The most an automatic icon may weigh (the server makes them far smaller).
pub const MAX_AUTOMATIC_ICON: usize = 256 * 1024;

/// An entry version (§8.4). `cipher` has the shape of a cipher in `/api/sync`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CipherVersion {
    pub id: String,
    pub cipher_id: String,
    pub revision_date: Option<String>,
    pub replaced_date: Option<String>,
    pub size: u64,
    pub cipher: Value,
}

/// Travel mode (§9.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Travel {
    pub enabled: bool,
    pub enabled_date: Option<String>,
    pub folder_ids: Vec<String>,
    pub hidden_count: u32,
}

/// A password renewal reminder (§10).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Reminder {
    pub cipher_id: String,
    /// `YYYY-MM-DD`.
    pub due: Option<String>,
    pub every_months: Option<u32>,
    pub is_due: bool,
    pub mailed_date: Option<String>,
}

/// A file request as its owner sees it (§11.4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileRequest {
    pub id: String,
    pub access_id: String,
    /// The owner's label, under the extras key.
    pub name: Option<String>,
    /// The link secret, under the extras key.
    pub link_secret: Option<String>,
    /// Title, note, owner and public key, under the link key.
    pub public_info: Option<String>,
    pub password_set: bool,
    pub expiration_date: Option<String>,
    pub deletion_date: Option<String>,
    pub max_submissions: Option<u32>,
    pub submission_count: u32,
    pub max_files: u32,
    pub max_file_bytes: Option<u64>,
    pub text_allowed: bool,
    pub send_domain_id: Option<String>,
    pub disabled: bool,
    pub unseen: u32,
    pub bytes: u64,
    pub creation_date: Option<String>,
    pub revision_date: Option<String>,
}

/// The body of `POST`/`PUT /uwu/v1/file-requests[/{id}]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRequestBody {
    pub name: String,
    pub link_secret: String,
    pub public_info: String,
    /// Left out on a change: the password stays as it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub remove_password: bool,
    pub expiration_date: String,
    pub max_submissions: Option<u32>,
    pub max_files: u32,
    /// Always a number: the server's own limit (`limits.maxFileBytes` of
    /// `/uwu/v1/info`) when the person set none. Ignored when `max_files` is 0.
    pub max_file_bytes: u64,
    pub text_allowed: bool,
    pub send_domain_id: Option<String>,
    pub disabled: bool,
}

/// One upload to a file request (§11.4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Submission {
    pub id: String,
    pub request_id: String,
    pub creation_date: Option<String>,
    /// The submission key for the owner's public key (type 4).
    pub wrapped_key: String,
    pub sender: Option<String>,
    pub text: Option<String>,
    pub files: Vec<SubmissionFile>,
    pub seen: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SubmissionFile {
    pub id: String,
    pub file_name: String,
    pub key: String,
    pub size: u64,
}

/// The account's connection to UwUMail for masked addresses (§13.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MaskedConnection {
    pub connected: bool,
    pub server: Option<String>,
    pub username: Option<String>,
    pub domains: Option<Vec<String>>,
    pub default_domain: Option<String>,
    /// `ok`, `revoked` or `unreachable`.
    pub status: Option<String>,
}

/// A masked address (§13.3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MaskedAddress {
    pub id: String,
    pub email: String,
    /// `enabled`, `disabled`, `deleted` (or UwUMail's `pending`).
    pub state: String,
    pub for_domain: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub last_message_at: Option<String>,
    pub cipher_id: Option<String>,
}

/// What a new masked address is for (`POST /uwu/v1/masked/addresses`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMaskedAddress {
    pub for_domain: String,
    pub description: String,
    pub domain: Option<String>,
    pub email_prefix: Option<String>,
    pub cipher_id: Option<String>,
}

/// The `data` of a list answer, each element read as `T`. Elements that don't
/// read are left out rather than failing the whole list.
fn list_of<T: serde::de::DeserializeOwned>(value: &Value) -> Vec<T> {
    value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn read<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> UwuResult<T> {
    serde_json::from_value(value).map_err(|e| {
        UwuError::Core(Error::Server {
            status: 200,
            message: format!("the server's {what} doesn't read: {e}"),
        })
    })
}

impl Client {
    /// Every page of a list under `/uwu/v1`, following `continuationToken`.
    pub async fn uwu_list<T: serde::de::DeserializeOwned>(
        &self,
        access_token: &str,
        path: &str,
    ) -> UwuResult<Vec<T>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        // Not an endless loop, whatever the server says.
        for _ in 0..100 {
            let page_path = match &token {
                None => path.to_string(),
                Some(next) => {
                    let joiner = if path.contains('?') { '&' } else { '?' };
                    format!("{path}{joiner}continuationToken={}", uwu_path(next))
                }
            };
            let page = self.uwu_get(access_token, &page_path).await?;
            out.extend(list_of::<T>(&page));
            token = page
                .get("continuationToken")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_string);
            if token.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// `GET /uwu/v1/account`.
    pub async fn uwu_account(&self, access_token: &str) -> UwuResult<UwuAccount> {
        read(self.uwu_get(access_token, "/account").await?, "account")
    }

    // ── Icons (§7) ─────────────────────────────────────────

    /// The own icons of these items, those that exist and are visible, with
    /// their `data`. Asks in batches of [`OWN_ICONS_PER_CALL`].
    pub async fn own_icons(
        &self,
        access_token: &str,
        cipher_ids: &[String],
    ) -> UwuResult<Vec<OwnIcon>> {
        let mut out = Vec::new();
        for chunk in cipher_ids.chunks(OWN_ICONS_PER_CALL) {
            let answer = self
                .uwu_post(
                    access_token,
                    "/icons/own/get",
                    &serde_json::json!({ "cipherIds": chunk }),
                )
                .await?;
            out.extend(list_of::<OwnIcon>(&answer));
        }
        Ok(out)
    }

    /// Stores an own icon: `data` is the sealed PNG
    /// ([`uwulock_core::extras::seal_icon`]), `key_type` `extras` or
    /// `organization`.
    pub async fn put_own_icon(
        &self,
        access_token: &str,
        cipher_id: &str,
        data: &str,
        key_type: &str,
    ) -> UwuResult<OwnIcon> {
        let answer = self
            .uwu_put(
                access_token,
                &format!("/icons/own/{}", uwu_path(cipher_id)),
                &serde_json::json!({ "data": data, "keyType": key_type }),
            )
            .await?;
        read(answer, "icon")
    }

    pub async fn delete_own_icon(&self, access_token: &str, cipher_id: &str) -> UwuResult<()> {
        self.uwu_delete(access_token, &format!("/icons/own/{}", uwu_path(cipher_id)))
            .await
            .map(drop)
    }

    /// An automatic icon (§7.1): `<icons_url>/<host>/icon.png`, no session.
    /// `None` when the server has none (404) or sends something that isn't a
    /// PNG.
    pub async fn automatic_icon(
        &self,
        icons_url: &str,
        host: &str,
    ) -> Result<Option<Vec<u8>>, Error> {
        let url = format!(
            "{}/{}/icon.png",
            icons_url.trim_end_matches('/'),
            escape(host)
        );
        let response = self
            .request(reqwest::Method::GET, url)
            .header("Accept", "image/png")
            .send()
            .await
            .map_err(crate::api::network_error)?;
        if !response.status().is_success() {
            return Ok(None);
        }
        // An icon larger than this isn't one: nothing is kept of it.
        let bytes = match crate::api::read_capped(response, MAX_AUTOMATIC_ICON).await {
            Ok(bytes) => bytes,
            Err(Error::Refused(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(extras::png_size(&bytes).map(|_| bytes))
    }

    // ── Entry versions (§8) ────────────────────────────────

    /// An item's versions, newest first.
    pub async fn versions(
        &self,
        access_token: &str,
        cipher_id: &str,
    ) -> UwuResult<Vec<CipherVersion>> {
        self.uwu_list(
            access_token,
            &format!("/ciphers/{}/versions", uwu_path(cipher_id)),
        )
        .await
    }

    /// Brings a version back. `last_known` is the item's `revisionDate` as
    /// this client has it; another one on the server is 409 `conflict`.
    /// Answers with the cipher as `PUT /api/ciphers/{id}` would.
    pub async fn restore_version(
        &self,
        access_token: &str,
        cipher_id: &str,
        version_id: &str,
        last_known: Option<&str>,
    ) -> UwuResult<Value> {
        self.uwu_post(
            access_token,
            &format!(
                "/ciphers/{}/versions/{}/restore",
                uwu_path(cipher_id),
                uwu_path(version_id)
            ),
            &serde_json::json!({ "lastKnownRevisionDate": last_known }),
        )
        .await
    }

    /// One version, or with `None` all of them.
    pub async fn delete_versions(
        &self,
        access_token: &str,
        cipher_id: &str,
        version_id: Option<&str>,
    ) -> UwuResult<()> {
        let mut path = format!("/ciphers/{}/versions", uwu_path(cipher_id));
        if let Some(version) = version_id {
            path.push('/');
            path.push_str(&uwu_path(version));
        }
        self.uwu_delete(access_token, &path).await.map(drop)
    }

    // ── Travel mode (§9) ───────────────────────────────────

    pub async fn travel(&self, access_token: &str) -> UwuResult<Travel> {
        read(self.uwu_get(access_token, "/travel").await?, "travel mode")
    }

    // ── Reminders (§10) ────────────────────────────────────

    /// Sets an item's reminder: a date, every so many months, or both.
    pub async fn set_reminder(
        &self,
        access_token: &str,
        cipher_id: &str,
        due: Option<&str>,
        every_months: Option<u32>,
    ) -> UwuResult<Reminder> {
        let answer = self
            .uwu_put(
                access_token,
                &format!("/reminders/{}", uwu_path(cipher_id)),
                &serde_json::json!({ "due": due, "everyMonths": every_months }),
            )
            .await?;
        read(answer, "reminder")
    }

    pub async fn delete_reminder(&self, access_token: &str, cipher_id: &str) -> UwuResult<()> {
        self.uwu_delete(access_token, &format!("/reminders/{}", uwu_path(cipher_id)))
            .await
            .map(drop)
    }

    // ── File requests (§11.4) ──────────────────────────────

    pub async fn file_requests(&self, access_token: &str) -> UwuResult<Vec<FileRequest>> {
        self.uwu_list(access_token, "/file-requests").await
    }

    pub async fn create_file_request(
        &self,
        access_token: &str,
        body: &FileRequestBody,
    ) -> UwuResult<FileRequest> {
        read(
            self.uwu_post(access_token, "/file-requests", body).await?,
            "file request",
        )
    }

    pub async fn update_file_request(
        &self,
        access_token: &str,
        id: &str,
        body: &FileRequestBody,
    ) -> UwuResult<FileRequest> {
        read(
            self.uwu_put(
                access_token,
                &format!("/file-requests/{}", uwu_path(id)),
                body,
            )
            .await?,
            "file request",
        )
    }

    pub async fn delete_file_request(&self, access_token: &str, id: &str) -> UwuResult<()> {
        self.uwu_delete(access_token, &format!("/file-requests/{}", uwu_path(id)))
            .await
            .map(drop)
    }

    pub async fn submissions(
        &self,
        access_token: &str,
        request_id: &str,
    ) -> UwuResult<Vec<Submission>> {
        self.uwu_list(
            access_token,
            &format!("/file-requests/{}/submissions", uwu_path(request_id)),
        )
        .await
    }

    fn submission_path(request_id: &str, submission_id: &str) -> String {
        format!(
            "/file-requests/{}/submissions/{}",
            uwu_path(request_id),
            uwu_path(submission_id)
        )
    }

    /// A submitted file, still encrypted (an EncArrayBuffer under its key).
    /// One file of a submission, encrypted; at most `max` bytes (the file
    /// limit, plus what encryption adds).
    pub async fn submission_file(
        &self,
        access_token: &str,
        request_id: &str,
        submission_id: &str,
        file_id: &str,
        max: u64,
    ) -> UwuResult<Vec<u8>> {
        let path = format!(
            "{}/files/{}",
            Self::submission_path(request_id, submission_id),
            uwu_path(file_id)
        );
        self.uwu_download(access_token, &path, max).await
    }

    pub async fn submission_seen(
        &self,
        access_token: &str,
        request_id: &str,
        submission_id: &str,
    ) -> UwuResult<()> {
        let path = format!("{}/seen", Self::submission_path(request_id, submission_id));
        self.uwu_post(access_token, &path, &serde_json::json!({}))
            .await
            .map(drop)
    }

    pub async fn delete_submission(
        &self,
        access_token: &str,
        request_id: &str,
        submission_id: &str,
    ) -> UwuResult<()> {
        self.uwu_delete(
            access_token,
            &Self::submission_path(request_id, submission_id),
        )
        .await
        .map(drop)
    }

    /// Moves a submitted file into an item's attachments: its name and key
    /// already under the item's key ([`uwulock_core::file_request::FileKey::for_item`]).
    /// Answers with the cipher as after an attachment upload.
    pub async fn attach_submission_file(
        &self,
        access_token: &str,
        request_id: &str,
        submission_id: &str,
        file_id: &str,
        cipher_id: &str,
        file: &uwulock_core::file_request::SealedFile,
    ) -> UwuResult<Value> {
        let path = format!(
            "{}/files/{}/attach",
            Self::submission_path(request_id, submission_id),
            uwu_path(file_id)
        );
        self.uwu_post(
            access_token,
            &path,
            &serde_json::json!({
                "cipherId": cipher_id,
                "fileName": file.file_name,
                "key": file.key,
            }),
        )
        .await
    }

    // ── Masked addresses (§13) ─────────────────────────────

    pub async fn masked_connection(&self, access_token: &str) -> UwuResult<MaskedConnection> {
        read(
            self.uwu_get(access_token, "/masked/connection").await?,
            "masked connection",
        )
    }

    pub async fn masked_addresses(&self, access_token: &str) -> UwuResult<Vec<MaskedAddress>> {
        self.uwu_list(access_token, "/masked/addresses").await
    }

    pub async fn create_masked_address(
        &self,
        access_token: &str,
        new: &NewMaskedAddress,
    ) -> UwuResult<MaskedAddress> {
        read(
            self.uwu_post(access_token, "/masked/addresses", new)
                .await?,
            "masked address",
        )
    }

    /// Changes an address: any of `state`, `description`, `forDomain`,
    /// `cipherId` (a `null` there unlinks it).
    pub async fn update_masked_address(
        &self,
        access_token: &str,
        id: &str,
        change: &Value,
    ) -> UwuResult<MaskedAddress> {
        read(
            self.uwu_patch(
                access_token,
                &format!("/masked/addresses/{}", uwu_path(id)),
                change,
            )
            .await?,
            "masked address",
        )
    }

    pub async fn delete_masked_address(&self, access_token: &str, id: &str) -> UwuResult<()> {
        self.uwu_delete(access_token, &format!("/masked/addresses/{}", uwu_path(id)))
            .await
            .map(drop)
    }

    // ── Send domains (§14.2) ───────────────────────────────

    /// Which domain a Send's link uses; `None` is the main host.
    pub async fn set_send_domain(
        &self,
        access_token: &str,
        send_id: &str,
        send_domain_id: Option<&str>,
    ) -> UwuResult<()> {
        self.uwu_put(
            access_token,
            &format!("/sends/{}/domain", uwu_path(send_id)),
            &serde_json::json!({ "sendDomainId": send_domain_id }),
        )
        .await
        .map(drop)
    }
}

/// A value for a path segment or query of `/uwu/v1`: ids, cursors.
pub fn uwu_path(value: &str) -> String {
    escape(value)
}

fn refusal(status: u16, body: &str) -> UwuError {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };
    let message = text("message")
        .or_else(|| {
            value
                .get("errorModel")
                .and_then(|m| m.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("the server answered with HTTP {status}"));
    UwuError::Refused {
        status,
        code: text("code").unwrap_or_default(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_keeps_its_code() {
        let error = refusal(
            409,
            r#"{"message":"Connect UwUMail first.","code":"not_connected","object":"error"}"#,
        );
        assert_eq!(error.code(), Some("not_connected"));
        assert_eq!(error.to_string(), "Connect UwUMail first.");
        assert!(matches!(Error::from(error), Error::Conflict));
        let bare = refusal(502, "");
        assert_eq!(bare.code(), None);
        assert_eq!(bare.status(), Some(502));
    }

    #[test]
    fn info_reads_the_contracts_example() {
        let info: Info = serde_json::from_str(
            r#"{"object":"info","name":"UwULock Server","version":"0.6.0","apiVersion":1,
                "publicUrl":"https://lock.example.com","webVault":true,"mail":true,
                "features":["vault","delta-sync","realtime","masked-addresses"],
                "sendDomains":[{"id":"5b0c","url":"https://send.example.com"}],
                "icons":{"automatic":true,"url":"https://lock.example.com/icons","ownMaxBytes":98304,"ownPixels":128,"library":true},
                "limits":{"maxFileBytes":524288000,"versionsPerItem":20}}"#,
        )
        .unwrap();
        assert!(info.has("realtime") && !info.has("sso"));
        assert_eq!(info.send_domains[0].url, "https://send.example.com");
        assert_eq!(info.icons.as_ref().unwrap().own_pixels, Some(128));
        // An older server has no switches: everything it lists is there.
        assert_eq!(info.switches, None);
        assert!(info.allows("masked-addresses") && info.allows("families"));
    }

    #[test]
    fn a_switch_that_is_off_hides_its_feature() {
        let info: Info = serde_json::from_str(
            r#"{"object":"info","name":"UwULock Server","version":"0.6.0",
                "features":["vault","delta-sync","reminders","versions","own-icons"],
                "switches":{"reminders":true,"versions":false,"file-requests":false,
                            "own-icons":true,"suite":false,"scim":false}}"#,
        )
        .unwrap();
        assert!(info.has("reminders") && info.has("own-icons") && info.has("vault"));
        // Listed by mistake, or by a server in between: the switch wins.
        assert!(!info.has("versions"));
        assert!(!info.allows("file-requests") && !info.allows("suite"));
        // Names the server doesn't switch stay as listed.
        assert!(info.allows("delta-sync") && info.allows("families"));
        assert_eq!(
            info.offered(),
            ["vault", "delta-sync", "reminders", "own-icons"]
        );
    }
}
