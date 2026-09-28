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
    pub send_domains: Vec<SendDomain>,
    pub icons: Option<IconsInfo>,
    pub branding: Option<Value>,
    pub limits: Option<Limits>,
}

impl Info {
    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
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

    /// Raw bytes from `/uwu/v1` (a file request's file).
    pub async fn uwu_download(&self, access_token: &str, path: &str) -> UwuResult<Vec<u8>> {
        let response = self
            .request(reqwest::Method::GET, self.uwu_url(path))
            .header("Accept", "application/octet-stream")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| UwuError::Core(crate::api::network_error(e)))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(refusal(status, &body));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| UwuError::Core(crate::api::network_error(e)))?;
        Ok(bytes.to_vec())
    }

    // ── The extras key (§3) ────────────────────────────────

    /// The account's extras key, opened: made if there is none yet, wrapped
    /// again for the user key after an official client rotated it. `None`
    /// when it is lost (the key pair changed; the web vault offers to start
    /// over). `private_key` is the account's own; without it, only a key
    /// wrapped for the user key opens.
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
                Resolved::Open { key, rewrap } => {
                    if let Some(body) = rewrap {
                        // Best effort: whoever comes next does it again.
                        if let Err(error) =
                            self.uwu_put(access_token, "/keys/user-wrap", &body).await
                        {
                            tracing::warn!(%error, "couldn't wrap the extras key again");
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
        assert_eq!(info.icons.unwrap().own_pixels, Some(128));
    }
}
