//! The HTTP side: which server, the login, two-step login, refreshing the
//! session and fetching the vault.
//!
//! UwULock logs in the way Bitwarden's desktop app does — client `desktop`,
//! the device type of this system, a device id of its own — so Bitwarden's
//! cloud and Vaultwarden treat it like any other desktop client. The master
//! password never goes out; only its hash does, once, at login.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::Serialize;
use serde_json::Value;
use std::time::{Duration, SystemTime};
use zeroize::Zeroizing;

use crate::crypto::Kdf;
use crate::wire::{self, lowercase_keys};
use crate::Error;

/// Bitwarden's servers gate a few item types (SSH keys) on the client version.
/// UwULock understands everything a desktop app of this version gets.
pub const CLIENT_VERSION: &str = "2025.8.0";

/// Where the vault lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Server {
    /// bitwarden.com, in the US.
    BitwardenUs,
    /// bitwarden.eu.
    BitwardenEu,
    /// A Vaultwarden or a self-hosted Bitwarden, by its web address.
    SelfHosted { url: String },
}

impl Server {
    /// A self-hosted server from what someone typed: the address of its web
    /// vault, with or without `https://`, with or without a `/#/login` tail.
    ///
    /// Plain `http://` is only allowed to this computer itself: anything else
    /// would send the password hash and the session in the clear.
    pub fn self_hosted(input: &str) -> Result<Server, Error> {
        let mut text = input.trim().to_string();
        if text.is_empty() {
            return Err(Error::Refused("the server address is empty".into()));
        }
        if !text.contains("://") {
            text = format!("https://{text}");
        }
        let mut url = url::Url::parse(&text)
            .map_err(|_| Error::Refused(format!("“{}” isn't a web address", input.trim())))?;
        url.set_fragment(None);
        url.set_query(None);
        match url.scheme() {
            "https" => {}
            "http" if is_loopback(&url) => {}
            "http" => {
                return Err(Error::Refused(
                    "the server must be reached over https:// – plain http:// only works for this computer (localhost)".into(),
                ))
            }
            other => return Err(Error::Refused(format!("{other}:// isn't a web address"))),
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err(Error::Refused(format!(
                "“{}” has no host name",
                input.trim()
            )));
        }
        let mut path = url.path().trim_end_matches('/').to_string();
        // Someone copied the address of a page inside the web vault.
        for tail in ["/api", "/identity", "/vault", "/login"] {
            if let Some(stripped) = path.strip_suffix(tail) {
                path = stripped.to_string();
            }
        }
        url.set_path(&path);
        Ok(Server::SelfHosted {
            url: url.as_str().trim_end_matches('/').to_string(),
        })
    }

    pub fn api(&self) -> String {
        match self {
            Server::BitwardenUs => "https://api.bitwarden.com".into(),
            Server::BitwardenEu => "https://api.bitwarden.eu".into(),
            Server::SelfHosted { url } => format!("{url}/api"),
        }
    }

    pub fn identity(&self) -> String {
        match self {
            Server::BitwardenUs => "https://identity.bitwarden.com".into(),
            Server::BitwardenEu => "https://identity.bitwarden.eu".into(),
            Server::SelfHosted { url } => format!("{url}/identity"),
        }
    }

    /// The web vault, for links out.
    pub fn web(&self) -> String {
        match self {
            Server::BitwardenUs => "https://vault.bitwarden.com".into(),
            Server::BitwardenEu => "https://vault.bitwarden.eu".into(),
            Server::SelfHosted { url } => url.clone(),
        }
    }

    /// How the server is shown: "bitwarden.com", "vault.example.org".
    pub fn label(&self) -> String {
        match self {
            Server::BitwardenUs => "bitwarden.com".into(),
            Server::BitwardenEu => "bitwarden.eu".into(),
            Server::SelfHosted { url } => url
                .split_once("://")
                .map(|(_, rest)| rest)
                .unwrap_or(url)
                .to_string(),
        }
    }
}

fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// This installation, as the server sees it.
#[derive(Debug, Clone)]
pub struct Device {
    /// A UUID made once on this computer and shared by every account on it.
    pub id: String,
    pub name: String,
    /// Bitwarden's device type: 6 Windows, 7 macOS, 8 Linux desktop.
    pub kind: u8,
}

impl Device {
    pub fn this_system(id: String) -> Self {
        let kind = if cfg!(target_os = "windows") {
            6
        } else if cfg!(target_os = "macos") {
            7
        } else {
            8
        };
        Device {
            id,
            name: "UwULock".into(),
            kind,
        }
    }
}

/// One way of two-step login the account has set up.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TwoFactorMethod {
    /// Bitwarden's provider number.
    pub provider: u8,
    /// `authenticator`, `email`, `yubikey`, `duo`, `webauthn`, …
    pub kind: &'static str,
    /// Whether UwULock can do this one yet.
    pub supported: bool,
    /// For email codes: the masked address the code goes to.
    pub hint: Option<String>,
}

fn two_factor_kind(provider: u8) -> (&'static str, bool) {
    match provider {
        0 => ("authenticator", true),
        1 => ("email", true),
        2 | 6 => ("duo", false),
        3 => ("yubikey", true),
        4 => ("u2f", false),
        7 => ("webauthn", false),
        _ => ("other", false),
    }
}

/// A code for two-step login.
#[derive(Debug, Clone)]
pub struct TwoFactorAnswer {
    pub provider: u8,
    pub code: String,
    /// Ask the server for a token that skips two-step login on this device next time.
    pub remember: bool,
}

/// A logged-in session, fresh from the token endpoint.
pub struct Session {
    pub access_token: Zeroizing<String>,
    pub refresh_token: Option<Zeroizing<String>>,
    pub expires_at: SystemTime,
    /// The user key, wrapped under the stretched master key (login only).
    pub protected_user_key: Option<String>,
    pub protected_private_key: Option<String>,
    /// A "remember this device" token for two-step login, if one was asked for.
    pub remember_token: Option<Zeroizing<String>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Whether the access token should be renewed before the next call.
    pub fn is_expiring(&self) -> bool {
        SystemTime::now() + Duration::from_secs(120) >= self.expires_at
    }

    fn from_token(token: wire::Token) -> Self {
        Session {
            access_token: Zeroizing::new(token.access_token),
            refresh_token: token.refresh_token.map(Zeroizing::new),
            expires_at: SystemTime::now()
                + Duration::from_secs(token.expires_in.unwrap_or(3600).clamp(60, 86_400)),
            protected_user_key: token.key,
            protected_private_key: token.private_key,
            remember_token: token.two_factor_token.map(Zeroizing::new),
        }
    }
}

#[derive(Debug)]
pub enum LoginOutcome {
    LoggedIn(Session),
    /// The account has two-step login: ask for a code and try again.
    /// `message` says why when a code was already given and didn't count.
    TwoFactor {
        methods: Vec<TwoFactorMethod>,
        message: Option<String>,
    },
    /// Bitwarden's cloud doesn't know this device yet and emailed a code.
    NewDeviceCode,
}

pub struct PasswordLogin<'a> {
    pub email: &'a str,
    pub password_hash: &'a str,
    pub two_factor: Option<TwoFactorAnswer>,
    /// A remembered device token from an earlier two-step login.
    pub remember_token: Option<&'a str>,
    pub new_device_code: Option<&'a str>,
}

pub struct Client {
    http: reqwest::Client,
    server: Server,
    device: Device,
}

impl Client {
    pub fn new(server: Server, device: Device) -> Result<Self, Error> {
        let http = reqwest::Client::builder()
            .user_agent(format!("UwULock/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            .https_only(
                !matches!(&server, Server::SelfHosted { url } if url.starts_with("http://")),
            )
            .build()
            .map_err(|e| Error::Network(e.to_string()))?;
        Ok(Client {
            http,
            server,
            device,
        })
    }

    pub fn server(&self) -> &Server {
        &self.server
    }

    fn request(&self, method: reqwest::Method, url: String) -> reqwest::RequestBuilder {
        self.http
            .request(method, url)
            .header("Accept", "application/json")
            .header("Bitwarden-Client-Name", "desktop")
            .header("Bitwarden-Client-Version", CLIENT_VERSION)
            .header("Device-Type", self.device.kind.to_string())
    }

    /// How the master key is derived for this email.
    pub async fn prelogin(&self, email: &str) -> Result<Kdf, Error> {
        let body = serde_json::json!({ "email": crate::crypto::normalize_email(email) });
        let mut last = None;
        // Bitwarden answers on the identity server; older Vaultwardens only on the API.
        for base in [self.server.identity(), self.server.api()] {
            let response = send(
                self.request(reqwest::Method::POST, format!("{base}/accounts/prelogin"))
                    .json(&body),
            )
            .await?;
            if response.status == 404 || response.status == 405 {
                last = Some(response);
                continue;
            }
            let prelogin: wire::Prelogin = response.json()?;
            return kdf_from(
                prelogin.kdf,
                prelogin.kdf_iterations,
                prelogin.kdf_memory,
                prelogin.kdf_parallelism,
            );
        }
        Err(last
            .map(|r| r.error())
            .unwrap_or_else(|| Error::Network("no answer".into())))
    }

    /// Logs in with the master password hash, and a two-step code if one is given.
    pub async fn login(&self, login: PasswordLogin<'_>) -> Result<LoginOutcome, Error> {
        let email = crate::crypto::normalize_email(login.email);
        let device_type = self.device.kind.to_string();
        let mut form: Vec<(&str, String)> = vec![
            ("grant_type", "password".into()),
            ("username", email.clone()),
            ("password", login.password_hash.into()),
            ("scope", "api offline_access".into()),
            ("client_id", "desktop".into()),
            ("deviceType", device_type),
            ("deviceIdentifier", self.device.id.clone()),
            ("deviceName", self.device.name.clone()),
        ];
        let answered = login.two_factor.is_some();
        if let Some(answer) = &login.two_factor {
            form.push(("twoFactorToken", answer.code.trim().replace(' ', "")));
            form.push(("twoFactorProvider", answer.provider.to_string()));
            form.push(("twoFactorRemember", u8::from(answer.remember).to_string()));
        } else if let Some(token) = login.remember_token {
            form.push(("twoFactorToken", token.into()));
            form.push(("twoFactorProvider", "5".into()));
            form.push(("twoFactorRemember", "0".into()));
        }
        if let Some(code) = login.new_device_code {
            form.push(("newDeviceOtp", code.trim().into()));
        }

        let response = send(
            self.request(
                reqwest::Method::POST,
                format!("{}/connect/token", self.server.identity()),
            )
            .header("Auth-Email", URL_SAFE_NO_PAD.encode(email.as_bytes()))
            .form(&form),
        )
        .await;
        // The form holds the password hash.
        for (_, value) in form.iter_mut() {
            zeroize::Zeroize::zeroize(value);
        }
        let response = response?;

        if response.ok() {
            return Ok(LoginOutcome::LoggedIn(Session::from_token(
                response.json()?,
            )));
        }
        let refusal: wire::TokenError = response.json().unwrap_or_default();
        if let Some(providers) = &refusal.two_factor_providers2 {
            let mut methods: Vec<TwoFactorMethod> = providers
                .iter()
                .filter_map(|(provider, details)| {
                    let provider: u8 = provider.parse().ok()?;
                    let (kind, supported) = two_factor_kind(provider);
                    let hint = details
                        .get("email")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    Some(TwoFactorMethod {
                        provider,
                        kind,
                        supported,
                        hint,
                    })
                })
                .collect();
            if methods.is_empty() {
                if let Some(list) = &refusal.two_factor_providers {
                    methods = list
                        .iter()
                        .filter_map(|p| match p {
                            Value::Number(n) => n.as_u64().map(|n| n as u8),
                            Value::String(s) => s.parse().ok(),
                            _ => None,
                        })
                        .map(|provider| {
                            let (kind, supported) = two_factor_kind(provider);
                            TwoFactorMethod {
                                provider,
                                kind,
                                supported,
                                hint: None,
                            }
                        })
                        .collect();
                }
            }
            // Remembered devices are not a method to pick.
            methods.retain(|m| m.provider != 5);
            methods.sort_by_key(|m| (!m.supported, m.provider));
            let message = if answered {
                Some(refusal_message(
                    &refusal,
                    "The two-step code was not accepted.",
                ))
            } else {
                None
            };
            return Ok(LoginOutcome::TwoFactor { methods, message });
        }
        let description = refusal
            .error_description
            .as_deref()
            .unwrap_or_default()
            .to_lowercase();
        if description.contains("new device") || description.contains("device verification") {
            return Ok(LoginOutcome::NewDeviceCode);
        }
        if response.status == 400 || response.status == 401 {
            return Err(Error::Refused(refusal_message(
                &refusal,
                "Email or master password is wrong.",
            )));
        }
        Err(response.error())
    }

    /// Asks the server to email a two-step code.
    pub async fn send_email_code(&self, email: &str, password_hash: &str) -> Result<(), Error> {
        let body = serde_json::json!({
            "email": crate::crypto::normalize_email(email),
            "masterPasswordHash": password_hash,
            "deviceIdentifier": self.device.id,
        });
        let response = send(
            self.request(
                reqwest::Method::POST,
                format!("{}/two-factor/send-email-login", self.server.api()),
            )
            .json(&body),
        )
        .await?;
        if response.ok() {
            Ok(())
        } else {
            Err(response.error())
        }
    }

    /// A new access token from the refresh token.
    pub async fn refresh(&self, refresh_token: &str) -> Result<Session, Error> {
        let form = [
            ("grant_type", "refresh_token"),
            ("client_id", "desktop"),
            ("refresh_token", refresh_token),
        ];
        let response = send(
            self.request(
                reqwest::Method::POST,
                format!("{}/connect/token", self.server.identity()),
            )
            .form(&form),
        )
        .await?;
        if response.ok() {
            let mut session = Session::from_token(response.json()?);
            // Some servers hand out a new refresh token, others keep the old one.
            if session.refresh_token.is_none() {
                session.refresh_token = Some(Zeroizing::new(refresh_token.to_string()));
            }
            return Ok(session);
        }
        if matches!(response.status, 400 | 401) {
            return Err(Error::SessionExpired);
        }
        Err(response.error())
    }

    /// The whole vault, still encrypted, as the server's JSON text.
    pub async fn sync(&self, access_token: &str) -> Result<String, Error> {
        let response = send(
            self.request(
                reqwest::Method::GET,
                format!("{}/sync?excludeDomains=true", self.server.api()),
            )
            .bearer_auth(access_token),
        )
        .await?;
        if response.status == 401 {
            return Err(Error::SessionExpired);
        }
        if !response.ok() {
            return Err(response.error());
        }
        // Checked here, so a cache is never written from something that isn't a sync.
        parse_sync(&response.body)?;
        Ok(response.body)
    }

    // ── Writing ────────────────────────────────────────────
    //
    // Every save carries the revision UwULock last saw, so a server that has a
    // newer copy of the item refuses instead of letting the older one win.
    // What comes back is the item as the server now has it, which the caller
    // puts into its own copy of the vault.

    /// A new item. `collection_ids` is for an item that belongs to an
    /// organisation; a personal item takes an empty list.
    pub async fn create_cipher(
        &self,
        access_token: &str,
        cipher: wire::CipherRequest,
        collection_ids: &[String],
    ) -> Result<Value, Error> {
        let api = self.server.api();
        let request = if cipher.organization_id.is_some() {
            self.request(reqwest::Method::POST, format!("{api}/ciphers/create"))
                .json(&wire::ShareRequest {
                    cipher,
                    collection_ids: collection_ids.to_vec(),
                })
        } else {
            self.request(reqwest::Method::POST, format!("{api}/ciphers"))
                .json(&cipher)
        };
        self.write(request.bearer_auth(access_token)).await
    }

    pub async fn update_cipher(
        &self,
        access_token: &str,
        id: &str,
        cipher: wire::CipherRequest,
    ) -> Result<Value, Error> {
        self.write(
            self.request(
                reqwest::Method::PUT,
                format!("{}/ciphers/{}", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token)
            .json(&cipher),
        )
        .await
    }

    /// Into the trash, where the server keeps it for 30 days.
    pub async fn trash_cipher(&self, access_token: &str, id: &str) -> Result<(), Error> {
        self.write(
            self.request(
                reqwest::Method::PUT,
                format!("{}/ciphers/{}/delete", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token),
        )
        .await
        .map(drop)
    }

    pub async fn restore_cipher(&self, access_token: &str, id: &str) -> Result<Value, Error> {
        self.write(
            self.request(
                reqwest::Method::PUT,
                format!("{}/ciphers/{}/restore", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token),
        )
        .await
    }

    /// Gone for good.
    pub async fn delete_cipher(&self, access_token: &str, id: &str) -> Result<(), Error> {
        self.write(
            self.request(
                reqwest::Method::DELETE,
                format!("{}/ciphers/{}", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token),
        )
        .await
        .map(drop)
    }

    /// `name` is already encrypted under the user key.
    pub async fn create_folder(&self, access_token: &str, name: String) -> Result<Value, Error> {
        self.write(
            self.request(
                reqwest::Method::POST,
                format!("{}/folders", self.server.api()),
            )
            .bearer_auth(access_token)
            .json(&wire::FolderRequest { name }),
        )
        .await
    }

    pub async fn rename_folder(
        &self,
        access_token: &str,
        id: &str,
        name: String,
    ) -> Result<Value, Error> {
        self.write(
            self.request(
                reqwest::Method::PUT,
                format!("{}/folders/{}", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token)
            .json(&wire::FolderRequest { name }),
        )
        .await
    }

    /// Removes the folder. The items in it stay, without a folder.
    pub async fn delete_folder(&self, access_token: &str, id: &str) -> Result<(), Error> {
        self.write(
            self.request(
                reqwest::Method::DELETE,
                format!("{}/folders/{}", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token),
        )
        .await
        .map(drop)
    }

    /// Sends a write and returns what the server made of it, as it wrote it —
    /// keys and all, so the answer can go straight into the cached vault.
    async fn write(&self, request: reqwest::RequestBuilder) -> Result<Value, Error> {
        let response = send(request).await?;
        if response.status == 401 {
            return Err(Error::SessionExpired);
        }
        if !response.ok() {
            return Err(response.write_error());
        }
        if response.body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&response.body).map_err(|e| Error::Server {
            status: response.status,
            message: format!("the server's answer isn't JSON: {e}"),
        })
    }
}

/// An id goes into a path; a server that hands out something odd shouldn't be
/// able to steer the request somewhere else. Ids are UUIDs, so this rarely has
/// anything to do.
fn escape(id: &str) -> String {
    id.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// A sync, from the text [`Client::sync`] returned (or a cached copy of it).
pub fn parse_sync(text: &str) -> Result<wire::Sync, Error> {
    let value: Value = serde_json::from_str(text).map_err(|e| Error::Server {
        status: 200,
        message: format!("the sync isn't JSON: {e}"),
    })?;
    serde_json::from_value(lowercase_keys(value)).map_err(|e| Error::Server {
        status: 200,
        message: format!("the sync doesn't look like Bitwarden's: {e}"),
    })
}

fn kdf_from(
    kind: Option<u32>,
    iterations: Option<u32>,
    memory: Option<u32>,
    parallelism: Option<u32>,
) -> Result<Kdf, Error> {
    let kdf = match kind.unwrap_or(0) {
        0 => Kdf::Pbkdf2 {
            iterations: iterations.unwrap_or(600_000),
        },
        1 => Kdf::Argon2id {
            iterations: iterations.unwrap_or(3),
            memory_mib: memory.unwrap_or(64),
            parallelism: parallelism.unwrap_or(4),
        },
        other => return Err(Error::Unsupported(format!("key derivation type {other}"))),
    };
    kdf.check()?;
    kdf.check_ceilings()?;
    Ok(kdf)
}

fn refusal_message(refusal: &wire::TokenError, fallback: &str) -> String {
    refusal
        .error_model
        .as_ref()
        .and_then(|m| m.message.clone())
        .or_else(|| refusal.message.clone())
        .or_else(|| {
            refusal
                .error_description
                .clone()
                .filter(|d| !d.eq_ignore_ascii_case("invalid_username_or_password"))
        })
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

struct Response {
    status: u16,
    body: String,
}

impl Response {
    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, Error> {
        let value: Value = serde_json::from_str(&self.body).map_err(|_| Error::Server {
            status: self.status,
            message: format!(
                "the server didn't answer with JSON (HTTP {}) – is this the address of a Bitwarden or Vaultwarden server?",
                self.status
            ),
        })?;
        serde_json::from_value(lowercase_keys(value)).map_err(|e| Error::Server {
            status: self.status,
            message: format!("unexpected answer from the server: {e}"),
        })
    }

    fn error(&self) -> Error {
        let refusal: wire::TokenError = self.json().unwrap_or_default();
        let message = refusal_message(&refusal, "");
        Error::Server {
            status: self.status,
            message: if message.is_empty() {
                format!("the server answered with HTTP {}", self.status)
            } else {
                message
            },
        }
    }

    /// A refused write. The one case that isn't a plain error is the item
    /// having changed elsewhere since the last sync — then the server keeps
    /// the newer copy, and UwULock says so instead of trying again.
    fn write_error(&self) -> Error {
        let error = self.error();
        let message = error.to_string().to_lowercase();
        if self.status == 400
            && (message.contains("out of date")
                || message.contains("has changed")
                || message.contains("resync"))
        {
            return Error::Conflict;
        }
        if self.status == 403 || self.status == 404 {
            return Error::Refused(match &error {
                Error::Server { message, .. } if !message.is_empty() => message.clone(),
                _ => "the server didn't allow this change".into(),
            });
        }
        error
    }
}

async fn send(request: reqwest::RequestBuilder) -> Result<Response, Error> {
    let response = request.send().await.map_err(network_error)?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(network_error)?;
    Ok(Response { status, body })
}

fn network_error(error: reqwest::Error) -> Error {
    use std::error::Error as _;
    // reqwest's own message ("error sending request") says nothing; the cause does.
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message = cause.to_string();
        source = cause.source();
    }
    if error.is_timeout() {
        message = "the server didn't answer in time".into();
    }
    Error::Network(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_hosted_addresses() {
        let url = |s: &str| match Server::self_hosted(s).unwrap() {
            Server::SelfHosted { url } => url,
            _ => unreachable!(),
        };
        assert_eq!(url("vault.example.org"), "https://vault.example.org");
        assert_eq!(
            url("https://vault.example.org/"),
            "https://vault.example.org"
        );
        assert_eq!(
            url("https://vault.example.org/#/login"),
            "https://vault.example.org"
        );
        assert_eq!(url("https://example.org/vw/"), "https://example.org/vw");
        assert_eq!(
            url("https://example.org:8443/api"),
            "https://example.org:8443"
        );
        assert_eq!(url("http://localhost:8000"), "http://localhost:8000");
        assert!(Server::self_hosted("http://vault.example.org").is_err());
        assert!(Server::self_hosted("").is_err());
        assert!(Server::self_hosted("ftp://x.org").is_err());
    }

    #[test]
    fn endpoints() {
        let server = Server::self_hosted("vault.example.org").unwrap();
        assert_eq!(server.api(), "https://vault.example.org/api");
        assert_eq!(server.identity(), "https://vault.example.org/identity");
        assert_eq!(server.label(), "vault.example.org");
        assert_eq!(
            Server::BitwardenEu.identity(),
            "https://identity.bitwarden.eu"
        );
    }

    #[test]
    fn a_prelogin_stays_between_floor_and_ceiling() {
        // Too little to be worth anything, or too much to ever finish.
        assert!(kdf_from(Some(0), Some(0), None, None).is_err());
        assert!(kdf_from(Some(0), Some(u32::MAX), None, None).is_err());
        assert!(kdf_from(Some(0), Some(10_000_001), None, None).is_err());
        assert!(kdf_from(Some(1), Some(u32::MAX), Some(64), Some(4)).is_err());
        assert!(kdf_from(Some(1), Some(3), Some(64), Some(u32::MAX)).is_err());
        assert!(kdf_from(Some(1), Some(3), Some(u32::MAX), Some(4)).is_err());
        assert!(kdf_from(Some(2), None, None, None).is_err());
        // Bitwarden's old and current defaults, and its maxima.
        for iterations in [5_000, 100_000, 600_000, 2_000_000] {
            assert!(kdf_from(Some(0), Some(iterations), None, None).is_ok());
        }
        assert!(kdf_from(Some(1), Some(3), Some(64), Some(4)).is_ok());
        assert!(kdf_from(Some(1), Some(10), Some(1024), Some(16)).is_ok());
        assert!(kdf_from(None, None, None, None).is_ok());
    }
}
