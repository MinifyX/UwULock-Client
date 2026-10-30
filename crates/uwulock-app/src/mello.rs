//! What Mello (MelloAI-Desktop) may know about the vault: whether it is
//! locked, how many entries it has and how many of its passwords are weak or
//! reused. Counts only — never a name, an address, a username or a secret.
//! Contract: docs/mello.md.
//!
//! Off until switched on in the settings. Then UwULock answers
//! `GET /v1/status` on `127.0.0.1` (never another address) to whoever shows
//! the pairing token as `Authorization: Bearer …`. The token is random,
//! kept in `mello-token` (readable by this user only) and made anew on
//! request; switching off stops the listener at once.
//!
//! Asking doesn't count as activity: Mello's polling never keeps the vault
//! open. A request from a browser (it has an `Origin`) or for another host
//! name (DNS rebinding) is turned away before the token is looked at.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

use crate::vault::{Failure, Result};
use crate::App;

/// Where UwULock listens unless `mello.json` says otherwise.
pub const DEFAULT_PORT: u16 = 8748;
const SETTINGS: &str = "mello.json";
const TOKEN: &str = "mello-token";
/// The most a request may be, request line and headers.
const MAX_REQUEST: usize = 8 * 1024;
/// How long a connection may take to say what it wants.
const READ_FOR: Duration = Duration::from_secs(5);
/// Connections answered at the same time; more wait for their turn.
const AT_ONCE: usize = 4;

/// Kept in `mello.json` next to the accounts. Nothing secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub enabled: bool,
    pub port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: false,
            port: DEFAULT_PORT,
        }
    }
}

/// The link as the settings page shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkView {
    enabled: bool,
    /// Listening right now.
    running: bool,
    /// What Mello is told: `http://127.0.0.1:<port>`.
    url: String,
    /// The pairing token to copy into Mello; `None` while switched off.
    token: Option<String>,
    /// Why it doesn't listen although switched on.
    problem: Option<String>,
}

/// What `GET /v1/status` answers. The counts are `None` while locked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub locked: bool,
    pub entries: Option<u32>,
    pub weak: Option<u32>,
    pub reused: Option<u32>,
}

impl Status {
    const LOCKED: Status = Status {
        locked: true,
        entries: None,
        weak: None,
        reused: None,
    };
}

/// The link's state: part of the app.
pub(crate) struct Link {
    dir: PathBuf,
    inner: Mutex<Inner>,
    /// Held for a whole switch on or off, so two switches can't interleave and leave a
    /// listener running while the setting says off.
    switching: Mutex<()>,
}

struct Inner {
    settings: Settings,
    token: Option<Arc<Zeroizing<String>>>,
    server: Option<tokio::task::JoinHandle<()>>,
    problem: Option<String>,
}

impl Link {
    pub(crate) fn new(dir: &std::path::Path) -> Link {
        let settings = std::fs::read(dir.join(SETTINGS))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let token = std::fs::read_to_string(dir.join(TOKEN))
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| text.len() >= 32)
            .map(|text| Arc::new(Zeroizing::new(text)));
        Link {
            dir: dir.to_path_buf(),
            inner: Mutex::new(Inner {
                settings,
                token,
                server: None,
                problem: None,
            }),
            switching: Mutex::new(()),
        }
    }

    /// The token while the link is on; a connection still open after switching off gets
    /// nothing.
    fn token(&self) -> Option<Arc<Zeroizing<String>>> {
        let inner = self.inner.lock();
        inner.token.clone().filter(|_| inner.settings.enabled)
    }

    fn view(&self) -> LinkView {
        let inner = self.inner.lock();
        let on = inner.settings.enabled;
        LinkView {
            enabled: on,
            running: inner.server.as_ref().is_some_and(|s| !s.is_finished()),
            url: format!("http://127.0.0.1:{}", inner.settings.port),
            token: inner
                .token
                .as_ref()
                .filter(|_| on)
                .map(|t| t.as_str().to_string()),
            problem: inner.problem.clone().filter(|_| on),
        }
    }

    fn save_settings(&self, settings: &Settings) -> Result<()> {
        let text = serde_json::to_vec_pretty(settings).expect("settings serialise");
        std::fs::write(self.dir.join(SETTINGS), text)
            .map_err(|e| Failure::new("io", format!("couldn't save the setting: {e}")))
    }

    fn new_token(&self) -> Result<Arc<Zeroizing<String>>> {
        use rand::RngCore;
        let mut bytes = Zeroizing::new([0u8; 32]);
        rand::rngs::OsRng.fill_bytes(bytes.as_mut());
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let token = Zeroizing::new(token);
        write_private(&self.dir.join(TOKEN), token.as_bytes())
            .map_err(|e| Failure::new("io", format!("couldn't keep the token: {e}")))?;
        Ok(Arc::new(token))
    }

    fn stop(&self) {
        if let Some(server) = self.inner.lock().server.take() {
            server.abort();
        }
    }
}

/// Starts the listener when the settings have it on.
pub fn start(app: &App) {
    let _switching = app.mello().switching.lock();
    if app.mello().inner.lock().settings.enabled {
        listen(app);
    }
}

/// (Re)binds the listener, keeping why it couldn't.
fn listen(app: &App) {
    let link = app.mello();
    link.stop();
    let port = link.inner.lock().settings.port;
    let problem = match bind(port) {
        Ok(listener) => {
            let app = app.clone();
            let server = crate::runtime::spawn(async move {
                match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => serve(app, listener).await,
                    Err(error) => tracing::warn!(%error, "the Mello link didn't start"),
                }
            });
            link.inner.lock().server = Some(server);
            None
        }
        Err(error) => {
            tracing::warn!(%error, port, "the Mello link couldn't listen");
            Some(format!("Port {port} on 127.0.0.1 isn't free: {error}"))
        }
    };
    link.inner.lock().problem = problem;
}

fn bind(port: u16) -> std::io::Result<std::net::TcpListener> {
    let listener = std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

impl App {
    pub fn mello_link(&self) -> LinkView {
        self.mello().view()
    }

    /// Switches the link on (making a token if there is none) or off. Off unpairs: the
    /// token is gone, and switching on again makes a new one.
    pub fn set_mello_link(&self, enabled: bool) -> Result<LinkView> {
        if cfg!(mobile) && enabled {
            return Err(Failure::new("unsupported", "Mello runs on computers only."));
        }
        let link = self.mello();
        let _switching = link.switching.lock();
        let mut settings = link.inner.lock().settings.clone();
        settings.enabled = enabled;
        link.save_settings(&settings)?;
        if enabled && link.token().is_none() {
            let token = link.new_token()?;
            link.inner.lock().token = Some(token);
        }
        link.inner.lock().settings = settings;
        if enabled {
            listen(self);
        } else {
            link.stop();
            let mut inner = link.inner.lock();
            inner.problem = None;
            inner.token = None;
            drop(inner);
            match std::fs::remove_file(link.dir.join(TOKEN)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(Failure::new(
                        "io",
                        format!("couldn't remove the token: {e}"),
                    ));
                }
                _ => {}
            }
        }
        Ok(link.view())
    }

    /// A new pairing token: Mello needs it before it can ask again.
    pub fn mello_link_renew(&self) -> Result<LinkView> {
        let link = self.mello();
        let _switching = link.switching.lock();
        let token = link.new_token()?;
        link.inner.lock().token = Some(token);
        Ok(link.view())
    }

    /// What the link answers: the account on screen, counts only.
    pub fn mello_status(&self) -> Status {
        let state = self.vault();
        let Ok((id, _)) = state.active_account() else {
            return Status::LOCKED;
        };
        let unlocked = state.unlocked.read();
        let Some(open) = unlocked.get(&id) else {
            return Status::LOCKED;
        };
        count(&open.vault.items)
    }
}

/// The counts of an open vault: entries outside the trash, and the logins
/// the password check finds weak or sharing their password with another.
fn count(items: &[uwulock_bitwarden::vault::Item]) -> Status {
    let entries = items.iter().filter(|item| !item.deleted).count();
    let report = uwulock_core::health::prepare(items).report;
    let weak = report.findings.iter().filter(|f| f.weak).count();
    let reused = report.findings.iter().filter(|f| f.reused > 0).count();
    Status {
        locked: false,
        entries: Some(entries as u32),
        weak: Some(weak as u32),
        reused: Some(reused as u32),
    }
}

async fn serve(app: App, listener: tokio::net::TcpListener) {
    let turns = Arc::new(Semaphore::new(AT_ONCE));
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                // Out of file handles and the like: don't spin on it.
                tracing::debug!(%error, "the Mello link couldn't accept");
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        if !peer.ip().is_loopback() {
            continue;
        }
        let Ok(turn) = turns.clone().acquire_owned().await else {
            return;
        };
        let app = app.clone();
        tokio::spawn(async move {
            let _turn = turn;
            let port = listener_port(&stream);
            answer_on(app, stream, port).await;
        });
    }
}

fn listener_port(stream: &tokio::net::TcpStream) -> u16 {
    stream.local_addr().map(|a| a.port()).unwrap_or_default()
}

async fn answer_on(app: App, mut stream: tokio::net::TcpStream, port: u16) {
    let request = match tokio::time::timeout(READ_FOR, read_head(&mut stream)).await {
        Ok(Ok(Some(head))) => head,
        Ok(Ok(None)) => {
            let _ = stream
                .write_all(&response(431, "Request Header Fields Too Large", None))
                .await;
            return;
        }
        _ => return,
    };
    let token = app.mello().token();
    let (code, reason, body) =
        respond(&request, port, token.as_deref().map(|t| t.as_str()), || {
            app.mello_status()
        });
    let _ = stream.write_all(&response(code, reason, body)).await;
    let _ = stream.shutdown().await;
}

/// Reads up to the blank line after the headers; `None` when that is more
/// than [`MAX_REQUEST`].
async fn read_head(stream: &mut tokio::net::TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut head = Vec::with_capacity(1024);
    let mut buf = [0u8; 1024];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(Some(head));
        }
        head.extend_from_slice(&buf[..n]);
        if head.windows(4).any(|w| w == b"\r\n\r\n") {
            return Ok(Some(head));
        }
        if head.len() > MAX_REQUEST {
            return Ok(None);
        }
    }
}

/// The answer to one request: status, reason and JSON body. `status` is only
/// asked once the request has passed every check.
pub fn respond(
    head: &[u8],
    port: u16,
    token: Option<&str>,
    status: impl FnOnce() -> Status,
) -> (u16, &'static str, Option<String>) {
    let Some(end) = head.windows(4).position(|w| w == b"\r\n\r\n") else {
        return (400, "Bad Request", None);
    };
    let Ok(text) = std::str::from_utf8(&head[..end]) else {
        return (400, "Bad Request", None);
    };
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (first.next(), first.next(), first.next(), first.next())
    else {
        return (400, "Bad Request", None);
    };
    if !version.starts_with("HTTP/1.") {
        return (400, "Bad Request", None);
    }
    let mut host = None;
    let mut authorization = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return (400, "Bad Request", None);
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            // Browsers send it with every cross-origin request; Mello doesn't.
            "origin" => return (403, "Forbidden", None),
            "host" if host.replace(value).is_some() => return (400, "Bad Request", None),
            "authorization" if authorization.replace(value).is_some() => {
                return (400, "Bad Request", None)
            }
            _ => {}
        }
    }
    let host_ok = host.is_some_and(|host| {
        [format!("127.0.0.1:{port}"), format!("localhost:{port}")]
            .iter()
            .any(|ok| host.eq_ignore_ascii_case(ok))
    });
    if !host_ok {
        return (403, "Forbidden", None);
    }
    if target != "/v1/status" {
        return (404, "Not Found", None);
    }
    if method != "GET" {
        return (405, "Method Not Allowed", None);
    }
    let shown = authorization
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim);
    let allowed = match (token, shown) {
        (Some(token), Some(shown)) => same(token.as_bytes(), shown.as_bytes()),
        _ => false,
    };
    if !allowed {
        return (401, "Unauthorized", None);
    }
    let body = serde_json::to_string(&status()).expect("status serialises");
    (200, "OK", Some(body))
}

/// Compares in time that doesn't depend on where they differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    use sha2::{Digest, Sha256};
    let (a, b) = (Sha256::digest(a), Sha256::digest(b));
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn response(code: u16, reason: &str, body: Option<String>) -> Vec<u8> {
    let body = body.unwrap_or_default();
    let mut head = format!(
        "HTTP/1.1 {code} {reason}\r\nConnection: close\r\nCache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\nContent-Length: {}\r\n",
        body.len()
    );
    if code == 401 {
        head.push_str("WWW-Authenticate: Bearer\r\n");
    }
    if !body.is_empty() {
        head.push_str("Content-Type: application/json\r\n");
    }
    head.push_str("\r\n");
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

/// Written next to the target and renamed over it, readable by this user
/// only where files have modes.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAIRED: &str = "0123456789abcdef0123456789abcdef";

    fn open() -> Status {
        Status {
            locked: false,
            entries: Some(3),
            weak: Some(1),
            reused: Some(2),
        }
    }

    fn ask(request: &str) -> (u16, Option<String>) {
        let (code, _, body) = respond(request.as_bytes(), 8748, Some(PAIRED), open);
        (code, body)
    }

    fn get(extra: &str) -> String {
        format!("GET /v1/status HTTP/1.1\r\nHost: 127.0.0.1:8748\r\n{extra}\r\n")
    }

    #[test]
    fn the_right_token_gets_the_counts() {
        let (code, body) = ask(&get(&format!("Authorization: Bearer {PAIRED}\r\n")));
        assert_eq!(code, 200);
        assert_eq!(
            body.unwrap(),
            r#"{"locked":false,"entries":3,"weak":1,"reused":2}"#
        );
    }

    #[test]
    fn no_or_a_wrong_token_gets_nothing() {
        assert_eq!(ask(&get("")).0, 401);
        assert_eq!(ask(&get("Authorization: Bearer nope\r\n")).0, 401);
        assert_eq!(
            ask(&get(&format!("Authorization: Basic {PAIRED}\r\n"))).0,
            401
        );
        // No token made yet: nobody gets in.
        let request = get("Authorization: Bearer \r\n");
        let (code, _, _) = respond(request.as_bytes(), 8748, None, open);
        assert_eq!(code, 401);
    }

    #[test]
    fn the_status_is_only_worked_out_for_the_paired() {
        let request = get("Authorization: Bearer nope\r\n");
        let (code, _, _) = respond(request.as_bytes(), 8748, Some(PAIRED), || {
            panic!("asked the vault for a stranger")
        });
        assert_eq!(code, 401);
    }

    #[test]
    fn browsers_and_other_hosts_are_turned_away() {
        let auth = format!("Authorization: Bearer {PAIRED}\r\n");
        assert_eq!(
            ask(&get(&format!("{auth}Origin: https://site.example\r\n"))).0,
            403
        );
        let rebound = format!("GET /v1/status HTTP/1.1\r\nHost: evil.example:8748\r\n{auth}\r\n");
        assert_eq!(ask(&rebound).0, 403);
        let no_host = format!("GET /v1/status HTTP/1.1\r\n{auth}\r\n");
        assert_eq!(ask(&no_host).0, 403);
        let localhost = format!("GET /v1/status HTTP/1.1\r\nHost: localhost:8748\r\n{auth}\r\n");
        assert_eq!(ask(&localhost).0, 200);
    }

    #[test]
    fn only_get_status_is_there() {
        let auth = format!("Authorization: Bearer {PAIRED}\r\n");
        let post = format!("POST /v1/status HTTP/1.1\r\nHost: 127.0.0.1:8748\r\n{auth}\r\n");
        assert_eq!(ask(&post).0, 405);
        let other = format!("GET /v1/items HTTP/1.1\r\nHost: 127.0.0.1:8748\r\n{auth}\r\n");
        assert_eq!(ask(&other).0, 404);
        assert_eq!(ask("garbage").0, 400);
        assert_eq!(ask("GET /v1/status\r\n\r\n").0, 400);
        let twice = format!(
            "{}\r\nAuthorization: Bearer x\r\n\r\n",
            get(&auth).trim_end()
        );
        assert_eq!(ask(&twice).0, 400);
    }

    #[test]
    fn an_open_vault_has_counts_and_nothing_else() {
        use uwulock_bitwarden::vault::{Item, ItemKind, Login};
        let login = |name: &str, password: &str| {
            let mut item = Item::new(ItemKind::Login);
            item.name = Zeroizing::new(name.into());
            item.login = Some(Login {
                username: Some(Zeroizing::new("nyu@example.com".into())),
                password: Some(Zeroizing::new(password.into())),
                ..Login::default()
            });
            item
        };
        let strong = "correct-horse-battery-staple-9!Qz";
        let mut trashed = login("Trash", "x");
        trashed.deleted = true;
        let items = vec![
            login("Bank", strong),
            login("Shop", strong),
            login("Forum", "hunter2"),
            Item::new(ItemKind::Note),
            trashed,
        ];
        let status = count(&items);
        assert_eq!(
            status,
            Status {
                locked: false,
                entries: Some(4),
                weak: Some(1),
                reused: Some(2),
            }
        );
        let json = serde_json::to_string(&status).unwrap();
        for secret in ["Bank", "Shop", "nyu@", "hunter2", strong] {
            assert!(!json.contains(secret), "{json}");
        }
    }

    #[test]
    fn a_locked_vault_has_no_counts() {
        let dir = std::env::temp_dir().join(format!("uwulock-mello-{}", uuid::Uuid::new_v4()));
        let app = App::new(
            crate::Storage::new(dir.clone()).unwrap(),
            Arc::new(crate::host::NoHost),
        );
        let status = app.mello_status();
        assert!(status.locked);
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            r#"{"locked":true,"entries":null,"weak":null,"reused":null}"#
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn off_by_default_and_off_unpairs() {
        let dir = std::env::temp_dir().join(format!("uwulock-mello-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // A port of its own: the default may be taken on the machine running the tests.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        std::fs::write(dir.join(SETTINGS), format!(r#"{{"port":{port}}}"#)).unwrap();
        let app = App::new(
            crate::Storage::new(dir.clone()).unwrap(),
            Arc::new(crate::host::NoHost),
        );
        let off = app.mello_link();
        assert!(!off.enabled && off.token.is_none());

        let on = app.set_mello_link(true).unwrap();
        assert!(on.enabled && on.problem.is_none(), "{on:?}");
        let token = on.token.clone().unwrap();
        assert_eq!(token.len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(TOKEN))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }

        // Over the wire, on loopback.
        let answer = crate::runtime::block_on(async move {
            let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            let request = format!(
                "GET /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n\r\n"
            );
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut answer = String::new();
            stream.read_to_string(&mut answer).await.unwrap();
            answer
        });
        assert!(answer.starts_with("HTTP/1.1 200 OK\r\n"), "{answer}");
        assert!(answer.ends_with(r#"{"locked":true,"entries":null,"weak":null,"reused":null}"#));

        let renewed = app.mello_link_renew().unwrap();
        assert_ne!(renewed.token, on.token);

        let off = app.set_mello_link(false).unwrap();
        assert!(!off.enabled && off.token.is_none());
        assert!(!dir.join(TOKEN).exists());
        // The port is free again once the listener is gone.
        let mut free = false;
        for _ in 0..200 {
            if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                free = true;
                break;
            }
            std::thread::yield_now();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(free);
        // Switching on again pairs anew.
        let again = app.set_mello_link(true).unwrap();
        assert!(again.token.is_some() && again.token != renewed.token);
        app.set_mello_link(false).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
