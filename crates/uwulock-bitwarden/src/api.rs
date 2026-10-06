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

/// Cheap to clone: the connection pool is shared.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    server: Server,
    device: Device,
    app: App,
}

/// Who logs in, as the token endpoint sees it: UwULock desktop as Bitwarden's
/// desktop client (`desktop`, scope `api`), or a UwU app with a token for its
/// own suite space only (contract §6.5: `uwussh`, `uwurdp`, … with scope
/// `uwu.suite`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub client_id: String,
    pub scope: String,
}

impl Default for App {
    fn default() -> Self {
        App {
            client_id: "desktop".into(),
            scope: "api offline_access".into(),
        }
    }
}

impl App {
    /// A suite app (`uwussh`, `uwurdp`, `uwumail`, `uwusuite`): its token
    /// opens its own space on a UwULock Server and nothing else.
    pub fn suite(client_id: &str) -> Self {
        App {
            client_id: client_id.into(),
            scope: "uwu.suite offline_access".into(),
        }
    }
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
            app: App::default(),
        })
    }

    /// The same client, logging in as another app (see [`App`]).
    pub fn with_app(mut self, app: App) -> Self {
        self.app = app;
        self
    }

    pub fn server(&self) -> &Server {
        &self.server
    }

    pub(crate) fn request(&self, method: reqwest::Method, url: String) -> reqwest::RequestBuilder {
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
            ("scope", self.app.scope.clone()),
            ("client_id", self.app.client_id.clone()),
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
            ("client_id", self.app.client_id.as_str()),
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

    // ── Files, Sends, organisations ─────────────────────────
    //
    // What moving a vault over takes beyond items and folders. The shapes are
    // Bitwarden's (server `CiphersController`, `SendsController`,
    // `OrganizationsController`, `CollectionsController`), which Vaultwarden
    // and UwULock Server answer the same way.

    /// Where an attachment can be fetched right now:
    /// `GET /api/ciphers/{id}/attachment/{attachmentId}` answers Bitwarden's
    /// `AttachmentResponseModel`, whose `url` is a short-lived link (a signed
    /// Azure blob at Bitwarden's cloud, `/attachments/…?token=` at Vaultwarden
    /// and UwULock Server). The link needs no session.
    pub async fn attachment_url(
        &self,
        access_token: &str,
        cipher_id: &str,
        attachment_id: &str,
    ) -> Result<String, Error> {
        let answer = self
            .write(
                self.request(
                    reqwest::Method::GET,
                    format!(
                        "{}/ciphers/{}/attachment/{}",
                        self.server.api(),
                        escape(cipher_id),
                        escape(attachment_id)
                    ),
                )
                .bearer_auth(access_token),
            )
            .await?;
        text_of(&answer, "url").ok_or_else(|| Error::Server {
            status: 200,
            message: "the server gave no link for the attachment".into(),
        })
    }

    /// A file from a link the server handed out (an attachment's, a Send
    /// file's), without the session: such links carry their own token, and
    /// Bitwarden's point at Azure, which must never see the session. A link
    /// without a host is taken as the server's own. Only links to the
    /// server's own hosts or to Azure's blob storage ([`Client::may_download`])
    /// are followed, and at most `max` bytes are read.
    pub async fn download(&self, url: &str, max: u64) -> Result<Vec<u8>, Error> {
        let url = if url.starts_with('/') {
            format!("{}{url}", self.server.web())
        } else {
            url.to_string()
        };
        let parsed = url::Url::parse(&url)
            .map_err(|_| Error::Refused("the server named a file link that isn't one".into()))?;
        if !self.may_download(&parsed) {
            return Err(Error::Refused(format!(
                "the server named a file link elsewhere ({})",
                parsed.host_str().unwrap_or_default()
            )));
        }
        let response = self
            .http
            .get(url)
            .header("Accept", "application/octet-stream")
            .timeout(FILE_TIMEOUT)
            .send()
            .await
            .map_err(network_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body = error_text(response).await;
            return Err(Response {
                status,
                body,
                retry_after: None,
            }
            .error());
        }
        read_capped(response, usize::try_from(max).unwrap_or(usize::MAX)).await
    }

    /// Whether [`Client::download`] may follow a file link: https (http only
    /// for a server on this computer) to one of the server's own hosts, or,
    /// since Bitwarden keeps files there, to Azure's blob storage; for
    /// Bitwarden's cloud also its own domain. A hostile server can't make the
    /// app fetch from anywhere else, the local network included.
    pub fn may_download(&self, url: &url::Url) -> bool {
        let Some(host) = url
            .host_str()
            .map(|h| h.trim_end_matches('.').to_ascii_lowercase())
        else {
            return false;
        };
        let own: Vec<String> = [self.server.web(), self.server.api(), self.server.identity()]
            .iter()
            .filter_map(|u| url::Url::parse(u).ok())
            .filter_map(|u| u.host_str().map(str::to_ascii_lowercase))
            .collect();
        let is_own = own.contains(&host);
        match url.scheme() {
            "https" => {}
            "http" if is_own && is_loopback(url) => {}
            _ => return false,
        }
        let cloud = match self.server {
            Server::BitwardenUs => host.ends_with(".bitwarden.com"),
            Server::BitwardenEu => host.ends_with(".bitwarden.eu"),
            Server::SelfHosted { .. } => false,
        };
        is_own || cloud || host.ends_with(".blob.core.windows.net")
    }

    /// Announces an attachment (`POST /api/ciphers/{id}/attachment/v2`, body
    /// `{ key, fileName, fileSize, adminRequest }`: the attachment's key under
    /// the item key, its name under the item key, the size of the encrypted
    /// file). The answer is Bitwarden's `AttachmentUploadDataResponseModel`:
    /// `{ attachmentId, url, fileUploadType, cipherResponse }`.
    pub async fn announce_attachment(
        &self,
        access_token: &str,
        cipher_id: &str,
        request: &AttachmentRequest,
    ) -> Result<Upload, Error> {
        let answer = self
            .write(
                self.request(
                    reqwest::Method::POST,
                    format!(
                        "{}/ciphers/{}/attachment/v2",
                        self.server.api(),
                        escape(cipher_id)
                    ),
                )
                .bearer_auth(access_token)
                .json(request),
            )
            .await?;
        Upload::from_answer(&answer, "attachmentid")
    }

    /// Removes an attachment (one whose upload failed half-way).
    pub async fn delete_attachment(
        &self,
        access_token: &str,
        cipher_id: &str,
        attachment_id: &str,
    ) -> Result<(), Error> {
        self.write(
            self.request(
                reqwest::Method::DELETE,
                format!(
                    "{}/ciphers/{}/attachment/{}",
                    self.server.api(),
                    escape(cipher_id),
                    escape(attachment_id)
                ),
            )
            .bearer_auth(access_token),
        )
        .await
        .map(drop)
    }

    /// Uploads an encrypted file where [`Client::announce_attachment`] or
    /// [`Client::create_file_send`] said. Only `fileUploadType` 0 ("direct",
    /// to the server itself) is done: the answer's `url` is relative to the
    /// API (`/ciphers/{id}/attachment/{attachmentId}`,
    /// `/sends/{id}/file/{fileId}`) and takes a `multipart/form-data` POST
    /// with the file in a part called `data`, its file name the encrypted
    /// name. That is what Vaultwarden and UwULock Server hand out; Bitwarden's
    /// cloud uploads to Azure (type 1), which a move never writes to.
    pub async fn upload_file(
        &self,
        access_token: &str,
        upload: &Upload,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<(), Error> {
        if upload.kind != 0 {
            return Err(Error::Unsupported(format!(
                "uploads of type {} (only direct uploads to the server)",
                upload.kind
            )));
        }
        let api = self.server.api();
        let url = if upload.url.starts_with('/') {
            format!("{api}{}", upload.url)
        } else if upload.url.starts_with(&format!("{api}/")) {
            upload.url.clone()
        } else {
            // The session goes along; only to the server it belongs to.
            return Err(Error::Refused(
                "the server wants the file uploaded somewhere else".into(),
            ));
        };
        let (content_type, body) = multipart(file_name, bytes);
        let response = send(
            self.request(reqwest::Method::POST, url)
                .bearer_auth(access_token)
                .header("Content-Type", content_type)
                .timeout(FILE_TIMEOUT)
                .body(body),
        )
        .await?;
        if response.status == 401 {
            return Err(Error::SessionExpired);
        }
        if !response.ok() {
            return Err(response.write_error());
        }
        Ok(())
    }

    /// A text Send (`POST /api/sends`, Bitwarden's `SendRequestModel`).
    pub async fn create_send(&self, access_token: &str, send: &Value) -> Result<Value, Error> {
        self.write(
            self.request(
                reqwest::Method::POST,
                format!("{}/sends", self.server.api()),
            )
            .bearer_auth(access_token)
            .json(send),
        )
        .await
    }

    /// A file Send, announced (`POST /api/sends/file/v2`: the
    /// `SendRequestModel` with `file: { fileName }` and `fileLength`, the size
    /// of the encrypted file). The answer is `SendFileUploadDataResponseModel`:
    /// `{ url, fileUploadType, sendResponse }`; `Upload::id` is the new Send's.
    pub async fn create_file_send(
        &self,
        access_token: &str,
        send: &Value,
    ) -> Result<Upload, Error> {
        let answer = self
            .write(
                self.request(
                    reqwest::Method::POST,
                    format!("{}/sends/file/v2", self.server.api()),
                )
                .bearer_auth(access_token)
                .json(send),
            )
            .await?;
        let mut upload = Upload::from_answer(&answer, "")?;
        upload.id = answer
            .get("sendResponse")
            .or_else(|| answer.get("SendResponse"))
            .and_then(|send| text_of(send, "id"))
            .ok_or_else(|| Error::Server {
                status: 200,
                message: "the server didn't say which Send it made".into(),
            })?;
        Ok(upload)
    }

    /// A change to a Send (`PUT /api/sends/{id}`, the same `SendRequestModel`
    /// with its `id`; a file Send's file stays).
    pub async fn update_send(
        &self,
        access_token: &str,
        id: &str,
        send: &Value,
    ) -> Result<Value, Error> {
        self.write(
            self.request(
                reqwest::Method::PUT,
                format!("{}/sends/{}", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token)
            .json(send),
        )
        .await
    }

    /// Anybody with the link may open the Send again: its password goes
    /// (`PUT /api/sends/{id}/remove-password`, which every server has), or
    /// with `emails` its list of addresses (`remove-auth`, Bitwarden's newer
    /// name for both; UwULock Server takes either).
    pub async fn remove_send_auth(
        &self,
        access_token: &str,
        id: &str,
        emails: bool,
    ) -> Result<Value, Error> {
        let what = if emails {
            "remove-auth"
        } else {
            "remove-password"
        };
        self.write(
            self.request(
                reqwest::Method::PUT,
                format!("{}/sends/{}/{what}", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token)
            .json(&serde_json::json!({})),
        )
        .await
    }

    pub async fn delete_send(&self, access_token: &str, id: &str) -> Result<(), Error> {
        self.write(
            self.request(
                reqwest::Method::DELETE,
                format!("{}/sends/{}", self.server.api(), escape(id)),
            )
            .bearer_auth(access_token),
        )
        .await
        .map(drop)
    }

    /// Where the file of one of the account's own file Sends can be fetched,
    /// the way whoever has its link fetches it: `POST
    /// /api/sends/{id}/access/file/{fileId}` with `{}` (no password), answer
    /// `{ url }`. Bitwarden's cloud names the Send by its access id there,
    /// Vaultwarden and UwULock Server by its id, so both are tried. The server
    /// counts this as one opening of the Send.
    pub async fn send_file_url(
        &self,
        id: &str,
        access_id: Option<&str>,
        file_id: &str,
    ) -> Result<String, Error> {
        let mut names = vec![id];
        if let Some(access) = access_id.filter(|a| !a.is_empty()) {
            if matches!(self.server, Server::SelfHosted { .. }) {
                names.push(access);
            } else {
                names.insert(0, access);
            }
        }
        let mut last = None;
        for name in names {
            let result = self
                .write(
                    self.request(
                        reqwest::Method::POST,
                        format!(
                            "{}/sends/{}/access/file/{}",
                            self.server.api(),
                            escape(name),
                            escape(file_id)
                        ),
                    )
                    .json(&serde_json::json!({})),
                )
                .await;
            match result {
                Ok(answer) => {
                    return text_of(&answer, "url").ok_or_else(|| Error::Server {
                        status: 200,
                        message: "the server gave no link for the Send's file".into(),
                    })
                }
                Err(error) => last = Some(error),
            }
        }
        Err(last.unwrap_or_else(|| Error::Refused("no such Send".into())))
    }

    /// A new organisation (`POST /api/organizations`, Bitwarden's
    /// `OrganizationCreateRequestModel`): `{ name, billingEmail, planType,
    /// key, keys: { publicKey, encryptedPrivateKey }, collectionName }`. On
    /// UwULock Server, `planType` 22 makes a family. Answers the
    /// `OrganizationResponseModel`.
    pub async fn create_organization(
        &self,
        access_token: &str,
        request: &Value,
    ) -> Result<Value, Error> {
        self.write(
            self.request(
                reqwest::Method::POST,
                format!("{}/organizations", self.server.api()),
            )
            .bearer_auth(access_token)
            .json(request),
        )
        .await
    }

    /// A new collection (`POST /api/organizations/{orgId}/collections`:
    /// `{ name, externalId, groups, users }`, the name under the
    /// organisation key).
    pub async fn create_collection(
        &self,
        access_token: &str,
        organization_id: &str,
        request: &Value,
    ) -> Result<Value, Error> {
        self.write(
            self.request(
                reqwest::Method::POST,
                format!(
                    "{}/organizations/{}/collections",
                    self.server.api(),
                    escape(organization_id)
                ),
            )
            .bearer_auth(access_token)
            .json(request),
        )
        .await
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

/// Files take longer than the 60 seconds everything else gets.
const FILE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Announcing an attachment: `POST /api/ciphers/{id}/attachment/v2`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRequest {
    /// The attachment's own key, under the item key.
    pub key: String,
    /// Under the item key.
    pub file_name: String,
    /// Of the encrypted file.
    pub file_size: u64,
    pub admin_request: bool,
}

/// Where an announced file goes.
#[derive(Debug, Clone)]
pub struct Upload {
    /// The attachment's id, or the Send's.
    pub id: String,
    pub url: String,
    /// Bitwarden's `FileUploadType`: 0 direct to the server, 1 Azure.
    pub kind: u8,
}

impl Upload {
    fn from_answer(answer: &Value, id_key: &str) -> Result<Upload, Error> {
        let answer = lowercase_keys(answer.clone());
        let missing = |what: &str| Error::Server {
            status: 200,
            message: format!("the server's upload answer has no {what}"),
        };
        Ok(Upload {
            id: if id_key.is_empty() {
                String::new()
            } else {
                text_of(&answer, id_key).ok_or_else(|| missing(id_key))?
            },
            url: text_of(&answer, "url").ok_or_else(|| missing("url"))?,
            kind: answer
                .get("fileuploadtype")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u8,
        })
    }
}

/// A string value by its key in camelCase, PascalCase or lower case.
pub(crate) fn text_of(value: &Value, key: &str) -> Option<String> {
    let map = value.as_object()?;
    map.get(key)
        .or_else(|| {
            map.iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value)
        })
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// A `multipart/form-data` body with one file in the part `data`, as
/// Bitwarden's clients upload attachments and Send files.
fn multipart(file_name: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let mut random = [0u8; 12];
    rand_bytes(&mut random);
    let boundary = format!("uwulock-{}", URL_SAFE_NO_PAD.encode(random));
    // Encrypted names are base64 with `.` and `|`: nothing to quote.
    let name: String = file_name
        .chars()
        .filter(|c| !matches!(c, '"' | '\r' | '\n'))
        .collect();
    let mut body = Vec::with_capacity(bytes.len() + 256);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"data\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn rand_bytes(out: &mut [u8]) {
    // A fresh key's bytes are as random as it gets, without another dependency.
    let key = crate::crypto::SymmetricKey::generate();
    let bytes = key.to_bytes();
    let n = out.len().min(bytes.len());
    out[..n].copy_from_slice(&bytes[..n]);
}

/// An id goes into a path; a server that hands out something odd shouldn't be
/// able to steer the request somewhere else. Ids are UUIDs, so this rarely has
/// anything to do.
pub(crate) fn escape(id: &str) -> String {
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

/// One cipher as the server writes it (an answer to a save, an entry
/// version), in whatever case its keys come. A version carries no `id`; give
/// it the item's.
pub fn parse_cipher(mut value: Value, id: Option<&str>) -> Result<wire::Cipher, Error> {
    if let (Some(id), Some(object)) = (id, value.as_object_mut()) {
        if !object.keys().any(|k| k.eq_ignore_ascii_case("id")) {
            object.insert("id".into(), Value::String(id.to_string()));
        }
    }
    serde_json::from_value(lowercase_keys(value)).map_err(|e| Error::Server {
        status: 200,
        message: format!("an item doesn't look like Bitwarden's: {e}"),
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

pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) body: String,
    /// `Retry-After` in seconds, when the answer had one.
    pub(crate) retry_after: Option<u64>,
}

impl Response {
    pub(crate) fn ok(&self) -> bool {
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

pub(crate) async fn send(request: reqwest::RequestBuilder) -> Result<Response, Error> {
    let response = request.send().await.map_err(network_error)?;
    let status = response.status().as_u16();
    let retry_after = retry_after_of(response.headers());
    let max = if (200..300).contains(&status) {
        MAX_JSON
    } else {
        MAX_ERROR
    };
    let body = read_capped(response, max).await?;
    let body = String::from_utf8(body).map_err(|_| Error::Server {
        status,
        message: "the server's answer isn't text".into(),
    })?;
    Ok(Response {
        status,
        body,
        retry_after,
    })
}

/// `Retry-After` in seconds; a date instead of seconds counts as none.
pub(crate) fn retry_after_of(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The most an answer of the API may weigh. Not a few MiB: a full sync of a
/// large vault with its organisations is tens of MiB, and that must work.
pub(crate) const MAX_JSON: usize = 64 * 1024 * 1024;
/// The most an error answer may weigh; only its message is shown.
pub(crate) const MAX_ERROR: usize = 64 * 1024;

/// A body, at most `max` bytes of it: a longer one — by its `Content-Length`
/// or as it comes in — is refused before it all sits in memory. Every body
/// this crate reads goes through here.
pub(crate) async fn read_capped(
    mut response: reqwest::Response,
    max: usize,
) -> Result<Vec<u8>, Error> {
    let too_much = || Error::Refused(format!("the answer is larger than {max} bytes"));
    if response
        .content_length()
        .is_some_and(|length| length > max as u64)
    {
        return Err(too_much());
    }
    let mut body = Vec::with_capacity(
        response
            .content_length()
            .map_or(0, |length| length as usize)
            .min(1024 * 1024),
    );
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if body.len() + chunk.len() > max {
            return Err(too_much());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// An error answer's text, at most [`MAX_ERROR`] bytes of it.
pub(crate) async fn error_text(response: reqwest::Response) -> String {
    read_capped(response, MAX_ERROR)
        .await
        .map(|body| String::from_utf8_lossy(&body).into_owned())
        .unwrap_or_default()
}

pub(crate) fn network_error(error: reqwest::Error) -> Error {
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
    fn file_links_only_to_the_server_or_its_storage() {
        let device = || Device::this_system("7c1d1f0e-5b1a-4f8e-9d3c-0e2b6a1c9f00".into());
        let allowed = |server: Server, link: &str| {
            Client::new(server, device())
                .unwrap()
                .may_download(&url::Url::parse(link).unwrap())
        };
        let own = || Server::self_hosted("https://lock.example.com").unwrap();
        assert!(allowed(
            own(),
            "https://lock.example.com/attachments/a/b?token=x"
        ));
        assert!(allowed(
            own(),
            "https://store.blob.core.windows.net/attachments/a"
        ));
        for elsewhere in [
            "https://evil.example.net/a",
            "https://192.168.1.1/admin",
            "https://nas.local/a",
            "http://lock.example.com/attachments/a",
            "file:///etc/passwd",
            "https://lock.example.com.evil.example/a",
        ] {
            assert!(!allowed(own(), elsewhere), "{elsewhere}");
        }
        assert!(allowed(
            Server::BitwardenUs,
            "https://attachments.bitwarden.com/a"
        ));
        assert!(!allowed(
            Server::BitwardenUs,
            "https://attachments.bitwarden.eu/a"
        ));
        let local = Server::self_hosted("http://127.0.0.1:8080").unwrap();
        assert!(allowed(
            local.clone(),
            "http://127.0.0.1:8080/attachments/a"
        ));
        assert!(!allowed(local, "http://127.0.0.2:8080/attachments/a"));
    }

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
