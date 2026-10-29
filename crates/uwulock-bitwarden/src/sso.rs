//! Logging in through SSO, as Bitwarden's desktop app does it with UwULock
//! Server: the authorization code flow with PKCE, the browser coming back to
//! a loopback address (RFC 8252).
//!
//! 1. [`Client::sso_prevalidate`](crate::Client::sso_prevalidate) — a short token from the server.
//! 2. [`Start::new`] — a PKCE verifier and a state, both random; [`Loopback::bind`] — a
//!    listener on `127.0.0.1`, port chosen by the system, whose address is the redirect.
//! 3. The system browser opens [`authorize_url`]; the server sends it on to the provider and,
//!    after the login there, back to the loopback address with a code and the state.
//! 4. [`Loopback::wait`] takes exactly that one request: the right path, the right state.
//!    Each connection is read on its own; anything else is answered with an error and
//!    ignored; after the timeout it gives up.
//! 5. [`Client::login_sso`](crate::Client::login_sso) trades the code with the verifier.
//!
//! SSO only says who somebody is. The vault is still opened with the master
//! password; an account that came through SSO without keys gets them here
//! first ([`NewKeys::make`] and [`Client::set_password`](crate::Client::set_password)).

use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD};
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use zeroize::Zeroizing;

use crate::api::App;
use crate::crypto::{self, EncString, Kdf, PrivateKey, SymmetricKey};
use crate::Error;

/// The path the browser comes back to on the loopback listener.
pub const CALLBACK_PATH: &str = "/sso-callback";

/// What the SSO identifier is when nobody typed one: UwULock Server takes any.
pub const DEFAULT_IDENTIFIER: &str = "uwulock";

/// How long a request to the listener may take to arrive in full. Each
/// connection is read on its own, so this only bounds an idle one.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// The most connections the listener reads at once; more are closed.
const MAX_CONNECTIONS: usize = 32;

/// The most a request head to the listener may have.
const MAX_REQUEST: usize = 8 * 1024;

/// The secrets of one SSO login: the PKCE verifier (kept here) and its
/// challenge (sent), and the state the browser has to bring back.
pub struct Start {
    pub code_verifier: Zeroizing<String>,
    pub code_challenge: String,
    pub state: Zeroizing<String>,
    pub redirect_uri: String,
}

impl std::fmt::Debug for Start {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Start")
            .field("redirect_uri", &self.redirect_uri)
            .finish_non_exhaustive()
    }
}

impl Start {
    /// A fresh verifier (64 random bytes, 86 characters) and state (32 bytes).
    pub fn new(redirect_uri: String) -> Start {
        let verifier = random_text(64);
        let code_challenge = pkce_challenge(&verifier);
        Start {
            code_verifier: verifier,
            code_challenge,
            state: random_text(32),
            redirect_uri,
        }
    }
}

/// `n` random bytes as base64url without padding.
fn random_text(n: usize) -> Zeroizing<String> {
    let mut bytes = Zeroizing::new(vec![0u8; n]);
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_slice()))
}

/// PKCE's S256: base64url of the SHA-256 of the verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The authorize address, with what Bitwarden's desktop app puts there.
pub fn authorize_url(identity: &str, app: &App, start: &Start, sso_token: &str) -> String {
    let mut url = url::Url::parse(&format!("{identity}/connect/authorize"))
        .expect("the identity address of a parsed server");
    url.query_pairs_mut()
        .append_pair("client_id", &app.client_id)
        .append_pair("redirect_uri", &start.redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &app.scope)
        .append_pair("state", &start.state)
        .append_pair("code_challenge", &start.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("response_mode", "query")
        .append_pair("domain_hint", DEFAULT_IDENTIFIER)
        .append_pair("ssoToken", sso_token);
    url.into()
}

/// What one request to the listener was.
#[derive(Debug, PartialEq, Eq)]
pub enum Callback {
    /// The browser came back with a code and the right state.
    Code(Zeroizing<String>),
    /// It came back with the right state and an error: the login was refused.
    Refused(String),
    /// Not the request this login waits for: another path, another method, no
    /// or another state. Answered with an error, and the listener waits on.
    Ignored,
}

/// Reads one request head (`GET /sso-callback?code=…&state=… HTTP/1.1`, the
/// rest of it unread) and says what it was. The state is compared in
/// constant time.
pub fn parse_callback(head: &str, expected_state: &str) -> Callback {
    let Some(line) = head.lines().next() else {
        return Callback::Ignored;
    };
    let mut parts = line.split(' ');
    let (Some("GET"), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Callback::Ignored;
    };
    if !version.starts_with("HTTP/1.") {
        return Callback::Ignored;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != CALLBACK_PATH {
        return Callback::Ignored;
    }
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut description = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match key.as_ref() {
            "code" => &mut code,
            "state" => &mut state,
            "error" => &mut error,
            "error_description" => &mut description,
            _ => continue,
        };
        // A parameter twice is not what the server sends.
        if slot.replace(Zeroizing::new(value.into_owned())).is_some() {
            return Callback::Ignored;
        }
    }
    let state_ok = state.is_some_and(|state| {
        subtle::ConstantTimeEq::ct_eq(state.as_bytes(), expected_state.as_bytes()).into()
    });
    if !state_ok {
        return Callback::Ignored;
    }
    match (code, error) {
        (_, Some(error)) => {
            let text: String = description
                .as_deref()
                .map(|d| d.as_str())
                .filter(|d| !d.trim().is_empty())
                .unwrap_or(error.as_str())
                .chars()
                .filter(|c| !c.is_control())
                .take(300)
                .collect();
            Callback::Refused(text)
        }
        (Some(code), None) if !code.is_empty() && code.len() <= 4096 => Callback::Code(code),
        _ => Callback::Ignored,
    }
}

/// The page the browser shows once it came back.
const DONE_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>UwULock</title><body style=\"font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem\"><h1>UwULock</h1><p>Angemeldet. Du kannst dieses Fenster schließen und zu UwULock zurückkehren.</p><p>Logged in. You can close this window and go back to UwULock.</p>";
const REFUSED_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>UwULock</title><body style=\"font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem\"><h1>UwULock</h1><p>Die Anmeldung hat nicht geklappt. UwULock sagt dir, warum.</p><p>The login didn't work. UwULock tells you why.</p>";

/// A listener on `127.0.0.1` for the one request an SSO login ends with.
pub struct Loopback {
    listener: TcpListener,
    redirect_uri: String,
}

impl Loopback {
    /// Listens on `127.0.0.1`, on a port the system picks.
    pub async fn bind() -> Result<Loopback, Error> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| Error::Network(format!("couldn't listen for the SSO login: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| Error::Network(e.to_string()))?
            .port();
        Ok(Loopback {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
        })
    }

    /// `http://127.0.0.1:<port>/sso-callback`.
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Waits for the browser to come back with the code, at most `timeout`.
    /// Every connection is read in its own task, each with a couple of
    /// seconds to send its request, so an idle one (a browser's preconnect)
    /// doesn't hold up the one that matters. A request that isn't the one
    /// (another path, no or a wrong state) gets an error page and the wait
    /// goes on, so a stray request can't end the login; the first right one
    /// wins. The listener and the other connections close when this returns.
    pub async fn wait(self, state: &str, timeout: Duration) -> Result<Zeroizing<String>, Error> {
        let deadline = tokio::time::Instant::now() + timeout;
        let state = std::sync::Arc::new(Zeroizing::new(state.to_string()));
        let (found, mut results) = tokio::sync::mpsc::channel::<(Callback, TcpStream)>(1);
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let Ok((stream, peer)) = accepted else {
                        continue;
                    };
                    // Reap the finished ones, and keep a flood of local
                    // connections from piling up tasks.
                    while connections.try_join_next().is_some() {}
                    if !peer.ip().is_loopback() || connections.len() >= MAX_CONNECTIONS {
                        continue;
                    }
                    let (state, found) = (state.clone(), found.clone());
                    connections.spawn(async move {
                        let mut stream = stream;
                        let head =
                            tokio::time::timeout(REQUEST_TIMEOUT, read_head(&mut stream)).await;
                        let Ok(Some(head)) = head else {
                            return;
                        };
                        match parse_callback(&head, &state) {
                            Callback::Ignored => answer(&mut stream, "404 Not Found", "").await,
                            callback => {
                                let _ = found.send((callback, stream)).await;
                            }
                        }
                    });
                }
                Some((callback, mut stream)) = results.recv() => {
                    match callback {
                        Callback::Code(code) => {
                            answer(&mut stream, "200 OK", DONE_PAGE).await;
                            return Ok(code);
                        }
                        Callback::Refused(message) => {
                            answer(&mut stream, "200 OK", REFUSED_PAGE).await;
                            return Err(Error::Refused(message));
                        }
                        // Answered in its task already; never sent here.
                        Callback::Ignored => {}
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    return Err(Error::Refused(
                        "The SSO login took too long. Start it again.".into(),
                    ));
                }
            }
        }
    }
}

/// The request head, up to the empty line, at most [`MAX_REQUEST`] bytes.
async fn read_head(stream: &mut TcpStream) -> Option<Zeroizing<String>> {
    let mut buffer = Zeroizing::new(Vec::with_capacity(1024));
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        zeroize::Zeroize::zeroize(&mut chunk);
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > MAX_REQUEST {
            return None;
        }
    }
    String::from_utf8(buffer.to_vec()).ok().map(Zeroizing::new)
}

async fn answer(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = tokio::time::timeout(REQUEST_TIMEOUT, async {
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    })
    .await;
}

// ── The first master password ─────────────────────────────

/// The shortest first master password UwULock sets, as Bitwarden's apps:
/// the floor under whatever the server asks.
pub const MIN_PASSWORD: usize = 12;

/// The longest minimum a server may ask for and still be taken at its word;
/// UwULock Server allows up to 128.
const MAX_MIN_LENGTH: usize = 128;

/// What a first master password has to be: the server's master password
/// policy, never below UwULock's own floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordPolicy {
    /// In characters, at least [`MIN_PASSWORD`].
    pub min_length: usize,
    /// zxcvbn's score, 0 (anything) to 4 (very hard to guess).
    pub min_complexity: u8,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        PasswordPolicy {
            min_length: MIN_PASSWORD,
            min_complexity: 0,
        }
    }
}

impl PasswordPolicy {
    /// The server's rules, raised to the floor and kept within sane bounds.
    pub fn with_floor(min_length: usize, min_complexity: u8) -> Self {
        PasswordPolicy {
            min_length: min_length.clamp(MIN_PASSWORD, MAX_MIN_LENGTH),
            min_complexity: min_complexity.min(4),
        }
    }

    /// Refuses a password that is too short or too easy to guess. The email
    /// counts as known to an attacker, as in Bitwarden's apps.
    pub fn check(&self, password: &str, email: &str) -> Result<(), Error> {
        if password.chars().count() < self.min_length {
            return Err(Error::Refused(format!(
                "The master password needs at least {} characters.",
                self.min_length
            )));
        }
        if self.min_complexity > 0 && strength(password, email) < self.min_complexity {
            return Err(Error::Refused(format!(
                "The master password is too easy to guess: this server wants strength {} of 4.",
                self.min_complexity
            )));
        }
        Ok(())
    }
}

/// zxcvbn's score of a password, 0 to 4, with the parts of the email and the
/// app's name as words an attacker would try first.
pub fn strength(password: &str, email: &str) -> u8 {
    if password.is_empty() {
        return 0;
    }
    let email = crypto::normalize_email(email);
    let mut inputs: Vec<&str> = email
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| part.len() >= 3)
        .collect();
    inputs.extend(["uwulock", "bitwarden"]);
    // zxcvbn gets slow on very long input: only the first 100 characters are
    // rated, which can rate a password lower than it is, never higher.
    let password: String = password.chars().take(100).collect();
    u8::from(zxcvbn::zxcvbn(&password, &inputs).score())
}

/// The KDF for a new master password: the server's, if it said one that is at
/// least as strong as Bitwarden's default (PBKDF2, 600 000 rounds) or is
/// Argon2id within the limits; else that default.
pub fn kdf_for_new_password(offered: Option<Kdf>) -> Kdf {
    let default = Kdf::Pbkdf2 {
        iterations: 600_000,
    };
    match offered {
        Some(kdf)
            if kdf.check().is_ok()
                && kdf.check_ceilings().is_ok()
                && !kdf.is_weaker_than(&default) =>
        {
            kdf
        }
        _ => default,
    }
}

/// What an account without keys gets with its first master password: a new
/// user key under the stretched master key, an RSA key pair with the private
/// half under the user key, and the master password hash.
pub struct NewKeys {
    pub email: String,
    pub kdf: Kdf,
    pub master_key: Zeroizing<[u8; 32]>,
    pub master_password_hash: Zeroizing<String>,
    pub user_key: SymmetricKey,
    /// The user key, wrapped under the stretched master key.
    pub protected_user_key: String,
    pub encrypted_private_key: String,
    /// SPKI DER, base64.
    pub public_key: String,
}

impl NewKeys {
    /// Checks the password against the policy, then derives the master key
    /// (Argon2 or PBKDF2: a moment) and makes the keys.
    pub fn make(
        password: &str,
        email: &str,
        kdf: Kdf,
        policy: &PasswordPolicy,
    ) -> Result<NewKeys, Error> {
        policy.check(password, email)?;
        let email = crypto::normalize_email(email);
        let master_key = crypto::master_key(password, &email, kdf)?;
        let master_password_hash =
            Zeroizing::new(crypto::master_password_hash(&master_key, password));
        let user_key = SymmetricKey::generate();
        let protected_user_key =
            EncString::encrypt(&user_key.to_bytes(), &SymmetricKey::stretch(&master_key))
                .to_string();
        let private = PrivateKey::generate()?;
        let encrypted_private_key = EncString::encrypt(&private.to_der()?, &user_key).to_string();
        let public_key = B64.encode(private.public().to_der()?);
        Ok(NewKeys {
            email,
            kdf,
            master_key,
            master_password_hash,
            user_key,
            protected_user_key,
            encrypted_private_key,
            public_key,
        })
    }

    /// The body of `/api/accounts/set-password`: Bitwarden's classic fields,
    /// as UwULock Server takes them.
    pub fn set_password_body(&self, identifier: &str) -> serde_json::Value {
        let (kind, iterations, memory, parallelism) = match self.kdf {
            Kdf::Pbkdf2 { iterations } => (0, iterations, None, None),
            Kdf::Argon2id {
                iterations,
                memory_mib,
                parallelism,
            } => (1, iterations, Some(memory_mib), Some(parallelism)),
        };
        serde_json::json!({
            "masterPasswordHash": self.master_password_hash.as_str(),
            "key": self.protected_user_key,
            "masterPasswordHint": null,
            "keys": {
                "encryptedPrivateKey": self.encrypted_private_key,
                "publicKey": self.public_key,
            },
            "kdf": kind,
            "kdfIterations": iterations,
            "kdfMemory": memory,
            "kdfParallelism": parallelism,
            "orgIdentifier": identifier,
        })
    }
}

impl std::fmt::Debug for NewKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewKeys")
            .field("email", &self.email)
            .field("kdf", &self.kdf)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc_7636() {
        // RFC 7636, appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn start_is_random_and_well_formed() {
        let a = Start::new("http://127.0.0.1:1234/sso-callback".into());
        let b = Start::new("http://127.0.0.1:1234/sso-callback".into());
        assert_ne!(*a.code_verifier, *b.code_verifier);
        assert_ne!(*a.state, *b.state);
        // RFC 7636: 43 to 128 characters of the unreserved set.
        assert!((43..=128).contains(&a.code_verifier.len()));
        assert!(a
            .code_verifier
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.~".contains(&c)));
        assert_eq!(a.code_challenge, pkce_challenge(&a.code_verifier));
        assert_eq!(a.code_challenge.len(), 43);
        assert!(a.state.len() >= 43);
        assert!(!format!("{a:?}").contains(a.code_verifier.as_str()));
    }

    #[test]
    fn authorize_url_carries_what_the_server_checks() {
        let start = Start::new("http://127.0.0.1:4711/sso-callback".into());
        let url = authorize_url(
            "https://lock.example.com/identity",
            &App::default(),
            &start,
            "sso.token",
        );
        let url = url::Url::parse(&url).unwrap();
        assert_eq!(url.host_str(), Some("lock.example.com"));
        assert_eq!(url.path(), "/identity/connect/authorize");
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["client_id"], "desktop");
        assert_eq!(pairs["redirect_uri"], "http://127.0.0.1:4711/sso-callback");
        assert_eq!(pairs["response_type"], "code");
        assert_eq!(pairs["scope"], "api offline_access");
        assert_eq!(pairs["state"], start.state.as_str());
        assert_eq!(pairs["code_challenge"], start.code_challenge);
        assert_eq!(pairs["code_challenge_method"], "S256");
        assert_eq!(pairs["ssoToken"], "sso.token");
        assert!(!pairs.values().any(|v| v == start.code_verifier.as_str()));
    }

    #[test]
    fn callback_takes_only_the_right_request() {
        let state = "s7aTe";
        let get = |target: &str| format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
        assert_eq!(
            parse_callback(&get("/sso-callback?code=abc%2Bd&state=s7aTe"), state),
            Callback::Code(Zeroizing::new("abc+d".into()))
        );
        assert_eq!(
            parse_callback(
                &get("/sso-callback?error=access_denied&error_description=No+way&state=s7aTe"),
                state
            ),
            Callback::Refused("No way".into())
        );
        assert_eq!(
            parse_callback(&get("/sso-callback?error=access_denied&state=s7aTe"), state),
            Callback::Refused("access_denied".into())
        );
        for ignored in [
            get("/sso-callback?code=abc&state=other"),
            get("/sso-callback?code=abc&state=s7aT"),
            get("/sso-callback?code=abc"),
            get("/sso-callback?state=s7aTe"),
            get("/sso-callback?code=&state=s7aTe"),
            get("/sso-callback?code=a&code=b&state=s7aTe"),
            get("/sso-callback?code=a&state=s7aTe&state=s7aTe"),
            get("/favicon.ico"),
            get("/sso-callback/../x?code=abc&state=s7aTe"),
            get("/other?code=abc&state=s7aTe"),
            get("http://127.0.0.1/sso-callback?code=abc&state=s7aTe"),
            "POST /sso-callback?code=abc&state=s7aTe HTTP/1.1\r\n\r\n".into(),
            "GET /sso-callback?code=abc&state=s7aTe\r\n\r\n".into(),
            "GET /sso-callback?code=abc&state=s7aTe HTTP/1.1 x\r\n\r\n".into(),
            String::new(),
        ] {
            assert_eq!(
                parse_callback(&ignored, state),
                Callback::Ignored,
                "{ignored:?}"
            );
        }
        let long = "x".repeat(5000);
        assert_eq!(
            parse_callback(
                &get(&format!("/sso-callback?code={long}&state=s7aTe")),
                state
            ),
            Callback::Ignored
        );
        // An error description is shown in the app: no control characters, not too long.
        let Callback::Refused(text) = parse_callback(
            &get(&format!(
                "/sso-callback?error=x&error_description=a%0Ab{}&state=s7aTe",
                "c".repeat(400)
            )),
            state,
        ) else {
            panic!("refused");
        };
        assert!(text.starts_with("ab") && text.chars().count() == 300);
    }

    async fn send(port: u16, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer).await;
        answer
    }

    fn port_of(loopback: &Loopback) -> u16 {
        url::Url::parse(loopback.redirect_uri())
            .unwrap()
            .port()
            .unwrap()
    }

    #[tokio::test]
    async fn loopback_waits_for_the_one_request() {
        let loopback = Loopback::bind().await.unwrap();
        assert!(loopback.redirect_uri().starts_with("http://127.0.0.1:"));
        assert!(loopback.redirect_uri().ends_with(CALLBACK_PATH));
        let port = port_of(&loopback);
        let waiting =
            tokio::spawn(async move { loopback.wait("st4te", Duration::from_secs(10)).await });

        // Stray requests are answered and don't end the wait.
        let stray = send(port, "GET /favicon.ico HTTP/1.1\r\n\r\n").await;
        assert!(stray.starts_with("HTTP/1.1 404"));
        let forged = send(
            port,
            "GET /sso-callback?code=evil&state=nope HTTP/1.1\r\n\r\n",
        )
        .await;
        assert!(forged.starts_with("HTTP/1.1 404"));
        // A connection that sends nothing and closes doesn't either.
        drop(TcpStream::connect(("127.0.0.1", port)).await.unwrap());

        let done = send(
            port,
            "GET /sso-callback?code=g00d&state=st4te HTTP/1.1\r\n\r\n",
        )
        .await;
        assert!(done.starts_with("HTTP/1.1 200"));
        assert!(!done.contains("g00d"), "nothing of the request is echoed");
        assert_eq!(waiting.await.unwrap().unwrap().as_str(), "g00d");
        // The listener is closed now.
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    }

    #[tokio::test]
    async fn loopback_reports_a_refusal() {
        let loopback = Loopback::bind().await.unwrap();
        let port = port_of(&loopback);
        let waiting =
            tokio::spawn(async move { loopback.wait("st4te", Duration::from_secs(10)).await });
        send(
            port,
            "GET /sso-callback?error=access_denied&error_description=Nope&state=st4te HTTP/1.1\r\n\r\n",
        )
        .await;
        match waiting.await.unwrap() {
            Err(Error::Refused(message)) => assert_eq!(message, "Nope"),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn loopback_gives_up_after_the_timeout() {
        let loopback = Loopback::bind().await.unwrap();
        let result = loopback.wait("st4te", Duration::from_millis(20)).await;
        assert!(matches!(result, Err(Error::Refused(_))));
    }

    #[test]
    fn new_keys_open_with_the_password() {
        let kdf = Kdf::Pbkdf2 { iterations: 5_000 };
        let keys = NewKeys::make(
            "correct horse battery",
            " Nyu@Example.com ",
            kdf,
            &PasswordPolicy::default(),
        )
        .unwrap();
        assert_eq!(keys.email, "nyu@example.com");
        let master = crypto::master_key("correct horse battery", "nyu@example.com", kdf).unwrap();
        assert_eq!(
            *keys.master_password_hash,
            crypto::master_password_hash(&master, "correct horse battery")
        );
        let protected: EncString = keys.protected_user_key.parse().unwrap();
        let user_key = crypto::decrypt_user_key(&master, &protected).unwrap();
        assert_eq!(*user_key.to_bytes(), *keys.user_key.to_bytes());
        let private: EncString = keys.encrypted_private_key.parse().unwrap();
        let der = private.decrypt(&user_key).unwrap();
        let private = PrivateKey::from_der(&der).unwrap();
        assert_eq!(
            B64.encode(private.public().to_der().unwrap()),
            keys.public_key
        );

        let body = keys.set_password_body("uwulock");
        assert_eq!(
            body["masterPasswordHash"],
            keys.master_password_hash.as_str()
        );
        assert_eq!(body["key"], keys.protected_user_key);
        assert_eq!(body["keys"]["publicKey"], keys.public_key);
        assert_eq!(
            body["keys"]["encryptedPrivateKey"],
            keys.encrypted_private_key
        );
        assert_eq!(body["kdf"], 0);
        assert_eq!(body["kdfIterations"], 5_000);
        assert!(body["kdfMemory"].is_null());
        assert!(!body.to_string().contains("correct horse"));

        assert!(matches!(
            NewKeys::make(
                "too short",
                "nyu@example.com",
                kdf,
                &PasswordPolicy::default()
            ),
            Err(Error::Refused(_))
        ));
    }

    #[tokio::test]
    async fn loopback_is_not_held_up_by_an_idle_connection() {
        let loopback = Loopback::bind().await.unwrap();
        let port = port_of(&loopback);
        let waiting =
            tokio::spawn(async move { loopback.wait("st4te", Duration::from_secs(10)).await });
        // A browser's preconnect: open, and nothing sent.
        let idle = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let started = std::time::Instant::now();
        let done = send(
            port,
            "GET /sso-callback?code=g00d&state=st4te HTTP/1.1\r\n\r\n",
        )
        .await;
        assert!(done.starts_with("HTTP/1.1 200"));
        assert_eq!(waiting.await.unwrap().unwrap().as_str(), "g00d");
        assert!(
            started.elapsed() < REQUEST_TIMEOUT,
            "{:?}",
            started.elapsed()
        );
        drop(idle);
    }

    #[test]
    fn policy_has_a_floor_and_bounds() {
        assert_eq!(PasswordPolicy::default(), PasswordPolicy::with_floor(0, 0));
        assert_eq!(PasswordPolicy::with_floor(8, 0).min_length, MIN_PASSWORD);
        assert_eq!(PasswordPolicy::with_floor(16, 0).min_length, 16);
        assert_eq!(PasswordPolicy::with_floor(usize::MAX, 9).min_length, 128);
        assert_eq!(PasswordPolicy::with_floor(0, 9).min_complexity, 4);
    }

    #[test]
    fn policy_checks_length_and_strength() {
        let email = "nyu@example.com";
        let long = PasswordPolicy::with_floor(16, 0);
        let error = long.check("fifteen chars!!", email).unwrap_err();
        assert!(error.to_string().contains("at least 16"), "{error}");
        long.check("sixteen chars!!!", email).unwrap();

        let strong = PasswordPolicy::with_floor(0, 4);
        // Long enough, but a pattern zxcvbn sees through: refused.
        let error = strong.check("aaaaaaaaaaaaaaaa", email).unwrap_err();
        assert!(error.to_string().contains("strength 4"), "{error}");
        // The email is a word an attacker tries first.
        assert!(
            strength("zorblaxquintet", "zorblaxquintet@example.com")
                < strength("zorblaxquintet", email)
        );
        strong
            .check("vivid-Otter-71-kumquat-Lantern", email)
            .unwrap();
        assert_eq!(strength("", email), 0);
        // Very long input stays fast; a repeated pattern stays weak.
        assert!(strength(&"x9!Q".repeat(10_000), email) < 4);
    }

    #[test]
    fn kdf_for_a_new_password_is_never_weak() {
        let default = Kdf::Pbkdf2 {
            iterations: 600_000,
        };
        assert_eq!(kdf_for_new_password(None), default);
        assert_eq!(
            kdf_for_new_password(Some(Kdf::Pbkdf2 { iterations: 5_000 })),
            default
        );
        let strong = Kdf::Pbkdf2 {
            iterations: 800_000,
        };
        assert_eq!(kdf_for_new_password(Some(strong)), strong);
        let argon = Kdf::Argon2id {
            iterations: 3,
            memory_mib: 64,
            parallelism: 4,
        };
        assert_eq!(kdf_for_new_password(Some(argon)), argon);
        let huge = Kdf::Pbkdf2 {
            iterations: 50_000_000,
        };
        assert_eq!(kdf_for_new_password(Some(huge)), default);
    }
}
