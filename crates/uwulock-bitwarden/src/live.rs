//! Live updates: the server says *that* something changed, and the app syncs.
//!
//! Two ways, one interface ([`Channel::next`] yields [`Event`]s):
//!
//! - **UwULock Server's realtime channel** (`/uwu/v1/realtime`, contract §5):
//!   one WebSocket, subprotocol `uwu.realtime.v1`, small JSON messages. The
//!   token goes in the first message, never in the URL; a fresh one is sent
//!   on the same connection before the old one runs out
//!   ([`Channel::reauth`]). The cursor of the offline copy goes with it, so a
//!   reconnect that missed something hears `changed` at once.
//! - **Bitwarden's notifications hub** (Bitwarden, Vaultwarden, older UwULock
//!   Servers): SignalR over a WebSocket in MessagePack, the connection
//!   Bitwarden's own apps keep. Its update types say which object changed;
//!   UwULock only needs to know that it should sync, that the session ended,
//!   or that a device asks to log in.
//!
//! Neither carries anything secret, and a lost message costs nothing but a
//! moment: the sync is the truth. [`Backoff`] says how long to wait before
//! connecting again, by the close code and the failures so far.

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::api::Server;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Something happened on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Something in these areas (`vault`, `uwu`, `suite`) changed: sync.
    Changed { areas: Vec<String> },
    /// The session ended (`securityStamp`, `deviceRemoved`, `disabled`,
    /// `keysRotated`, or the hub's LogOut): keep nothing decrypted.
    LogOut { reason: String },
    /// A "log in with device" request for this account.
    AuthRequest { id: String },
    /// A security notice, something for a file request, a reminder that
    /// became due (`securityNotice`, `fileRequest`, `reminderDue`).
    Notice { kind: String, id: Option<String> },
    /// `/uwu/v1/info` changed: ask it again.
    Info,
}

/// Why a connection is over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    /// The WebSocket close code, if the server sent one.
    pub code: Option<u16>,
    pub reason: String,
}

impl Closed {
    fn new(code: Option<u16>, reason: impl Into<String>) -> Self {
        Closed {
            code,
            reason: reason.into(),
        }
    }

    /// The token was missing, invalid or expired: refresh it, then connect.
    pub fn needs_token(&self) -> bool {
        self.code == Some(4401)
    }
}

impl std::fmt::Display for Closed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code {
            Some(code) => write!(f, "closed ({code}): {}", self.reason),
            None => write!(f, "closed: {}", self.reason),
        }
    }
}

// ── Where ──────────────────────────────────────────────────

fn ws_scheme(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        url.to_string()
    }
}

/// `/uwu/v1/realtime` of a UwULock Server.
pub fn realtime_url(server: &Server) -> String {
    ws_scheme(&format!("{}/uwu/v1/realtime", server.web()))
}

/// Bitwarden's hub. The token goes in the URL there — that is how SignalR
/// authenticates a WebSocket, and how Bitwarden's apps do it.
pub fn hub_url(server: &Server, access_token: &str) -> String {
    let base = match server {
        Server::BitwardenUs => "https://notifications.bitwarden.com".to_string(),
        Server::BitwardenEu => "https://notifications.bitwarden.eu".to_string(),
        Server::SelfHosted { url } => format!("{url}/notifications"),
    };
    let token: String = url::form_urlencoded::byte_serialize(access_token.as_bytes()).collect();
    ws_scheme(&format!("{base}/hub?access_token={token}"))
}

// ── TLS ────────────────────────────────────────────────────

/// The same trust as the HTTP client: the bundled roots and the system's own
/// store, so a homelab server behind a private CA works once the system
/// trusts that CA.
fn tls() -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let native = rustls_native_certs::load_native_certs();
    for cert in native.certs {
        let _ = roots.add(cert);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports the default versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

async fn open(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
) -> Result<Socket, Closed> {
    let secure = request.uri().scheme_str() == Some("wss");
    let connector = secure.then(|| tokio_tungstenite::Connector::Rustls(tls()));
    let connecting = tokio_tungstenite::connect_async_tls_with_config(
        request,
        Some(limits()),
        true,
        connector.or(Some(tokio_tungstenite::Connector::Plain)),
    );
    match tokio::time::timeout(Duration::from_secs(15), connecting).await {
        Err(_) => Err(Closed::new(None, "the server didn't answer in time")),
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response))) => Err(Closed::new(
            None,
            format!(
                "the server answered with HTTP {}",
                response.status().as_u16()
            ),
        )),
        Ok(Err(error)) => Err(Closed::new(None, error.to_string())),
        Ok(Ok((socket, _))) => Ok(socket),
    }
}

/// Neither channel ever sends much; a server that does is not trusted with
/// memory.
fn limits() -> tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
    let mut config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
    config.max_message_size = Some(1 << 20);
    config.max_frame_size = Some(1 << 20);
    config
}

// ── The channel ────────────────────────────────────────────

/// An open connection, either kind.
pub enum Channel {
    Realtime(Realtime),
    Hub(Hub),
}

impl Channel {
    /// The next event; `Err` when the connection is over.
    pub async fn next(&mut self) -> Result<Event, Closed> {
        match self {
            Channel::Realtime(realtime) => realtime.next().await,
            Channel::Hub(hub) => hub.next().await,
        }
    }

    /// Unix seconds when the token this connection was opened with runs out,
    /// if the channel needs a fresh one before then ([`Channel::reauth`]).
    pub fn expires(&self) -> Option<u64> {
        match self {
            Channel::Realtime(realtime) => Some(realtime.expires),
            Channel::Hub(_) => None,
        }
    }

    /// A fresh token on the same connection (realtime only; the hub checks
    /// the token once, when it connects).
    pub async fn reauth(&mut self, access_token: &str, cursor: Option<&str>) -> Result<(), Closed> {
        match self {
            Channel::Realtime(realtime) => realtime.send_auth(access_token, cursor).await,
            Channel::Hub(_) => Ok(()),
        }
    }

    pub async fn close(&mut self) {
        let socket = match self {
            Channel::Realtime(realtime) => &mut realtime.socket,
            Channel::Hub(hub) => &mut hub.socket,
        };
        let _ = tokio::time::timeout(Duration::from_secs(2), socket.close(None)).await;
    }

    pub fn is_realtime(&self) -> bool {
        matches!(self, Channel::Realtime(_))
    }
}

// ── UwULock's realtime channel ─────────────────────────────

pub struct Realtime {
    socket: Socket,
    heartbeat: Duration,
    expires: u64,
    last_ping: Instant,
}

/// What the realtime channel said, read from one text frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeMessage {
    Ready { heartbeat: u64, expires: u64 },
    Pong,
    Event(Event),
}

/// One text frame of the realtime channel. `None` for anything unknown:
/// newer servers may say more than this client understands.
pub fn parse_realtime(text: &str) -> Option<RealtimeMessage> {
    let value: Value = serde_json::from_str(text).ok()?;
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    let list = |key: &str| -> Vec<String> {
        value
            .get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(match value.get("type")?.as_str()? {
        "ready" => RealtimeMessage::Ready {
            heartbeat: value.get("heartbeat").and_then(Value::as_u64).unwrap_or(25),
            expires: value.get("expires").and_then(Value::as_u64).unwrap_or(0),
        },
        "pong" => RealtimeMessage::Pong,
        "changed" => RealtimeMessage::Event(Event::Changed {
            areas: list("areas"),
        }),
        "logout" => RealtimeMessage::Event(Event::LogOut {
            reason: text("reason").unwrap_or_default(),
        }),
        "authRequest" => RealtimeMessage::Event(Event::AuthRequest { id: text("id")? }),
        "notice" => RealtimeMessage::Event(Event::Notice {
            kind: text("kind")?,
            id: value.get("id").and_then(|id| match id {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            }),
        }),
        "info" => RealtimeMessage::Event(Event::Info),
        _ => return None,
    })
}

impl Realtime {
    /// Connects, authenticates, and waits for `ready` (10 seconds at most).
    pub async fn connect(
        server: &Server,
        access_token: &str,
        cursor: Option<&str>,
    ) -> Result<Realtime, Closed> {
        let mut request = realtime_url(server)
            .into_client_request()
            .map_err(|e| Closed::new(None, e.to_string()))?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            "uwu.realtime.v1".parse().expect("a valid header"),
        );
        let socket = open(request).await?;
        let mut realtime = Realtime {
            socket,
            heartbeat: Duration::from_secs(25),
            expires: 0,
            last_ping: Instant::now(),
        };
        realtime.send_auth(access_token, cursor).await?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let frame = tokio::time::timeout(left, realtime.socket.next())
                .await
                .map_err(|_| Closed::new(None, "the server didn't say ready"))?;
            match frame {
                Some(Ok(Message::Text(text))) => {
                    if let Some(RealtimeMessage::Ready { heartbeat, expires }) =
                        parse_realtime(&text)
                    {
                        realtime.ready(heartbeat, expires);
                        return Ok(realtime);
                    }
                }
                Some(Ok(Message::Close(frame))) => return Err(closed_by(frame)),
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(Closed::new(None, error.to_string())),
                None => return Err(Closed::new(None, "the connection ended")),
            }
        }
    }

    fn ready(&mut self, heartbeat: u64, expires: u64) {
        self.heartbeat = Duration::from_secs(heartbeat.clamp(5, 300));
        self.expires = expires;
    }

    async fn send_auth(&mut self, access_token: &str, cursor: Option<&str>) -> Result<(), Closed> {
        let message = json!({ "type": "auth", "token": access_token, "cursor": cursor });
        self.socket
            .send(Message::Text(message.to_string().into()))
            .await
            .map_err(|e| Closed::new(None, e.to_string()))
    }

    async fn next(&mut self) -> Result<Event, Closed> {
        loop {
            // The server pings every heartbeat; three without a frame and the
            // connection is gone, whatever the socket thinks. An application
            // ping in between keeps idle proxies from cutting it.
            let silence = self.heartbeat * 3;
            let ping_due = self.last_ping + self.heartbeat;
            let frame = tokio::select! {
                frame = tokio::time::timeout(silence, self.socket.next()) => frame,
                _ = tokio::time::sleep_until(ping_due.into()) => {
                    self.last_ping = Instant::now();
                    let ping = json!({ "type": "ping" }).to_string();
                    self.socket
                        .send(Message::Text(ping.into()))
                        .await
                        .map_err(|e| Closed::new(None, e.to_string()))?;
                    continue;
                }
            };
            let frame = frame.map_err(|_| Closed::new(None, "the server went quiet"))?;
            match frame {
                Some(Ok(Message::Text(text))) => match parse_realtime(&text) {
                    Some(RealtimeMessage::Event(event)) => return Ok(event),
                    Some(RealtimeMessage::Ready { heartbeat, expires }) => {
                        self.ready(heartbeat, expires)
                    }
                    Some(RealtimeMessage::Pong) | None => {}
                },
                Some(Ok(Message::Close(frame))) => return Err(closed_by(frame)),
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(Closed::new(None, error.to_string())),
                None => return Err(Closed::new(None, "the connection ended")),
            }
        }
    }
}

fn closed_by(frame: Option<tokio_tungstenite::tungstenite::protocol::CloseFrame>) -> Closed {
    match frame {
        Some(frame) => Closed::new(Some(u16::from(frame.code)), frame.reason.to_string()),
        None => Closed::new(Some(u16::from(CloseCode::Status)), "closed"),
    }
}

// ── Bitwarden's notifications hub ──────────────────────────

/// Bitwarden's `PushType`s that are more than "sync".
const LOG_OUT: i64 = 11;
const AUTH_REQUEST: i64 = 15;
const AUTH_REQUEST_RESPONSE: i64 = 16;

/// SignalR's handshake for the MessagePack protocol, with its record separator.
const HANDSHAKE: &str = "{\"protocol\":\"messagepack\",\"version\":1}\u{1e}";
/// A SignalR ping in MessagePack: length 2, then `[6]`.
const HUB_PING: [u8; 3] = [0x02, 0x91, 0x06];
/// SignalR's default: a client pings every 15 seconds, and a server that says
/// nothing for 30 is gone.
const HUB_PING_EVERY: Duration = Duration::from_secs(15);
const HUB_SILENCE: Duration = Duration::from_secs(30);

pub struct Hub {
    socket: Socket,
    /// This device's id: a change it made itself is not news here.
    device_id: String,
    last_ping: Instant,
    queue: std::collections::VecDeque<Event>,
}

impl Hub {
    pub async fn connect(
        server: &Server,
        access_token: &str,
        device_id: &str,
    ) -> Result<Hub, Closed> {
        let request = hub_url(server, access_token)
            .into_client_request()
            .map_err(|e| Closed::new(None, e.to_string()))?;
        let mut socket = open(request).await?;
        socket
            .send(Message::Text(HANDSHAKE.into()))
            .await
            .map_err(|e| Closed::new(None, e.to_string()))?;
        Ok(Hub {
            socket,
            device_id: device_id.to_string(),
            last_ping: Instant::now(),
            queue: Default::default(),
        })
    }

    async fn next(&mut self) -> Result<Event, Closed> {
        loop {
            if let Some(event) = self.queue.pop_front() {
                return Ok(event);
            }
            let ping_due = self.last_ping + HUB_PING_EVERY;
            let frame = tokio::select! {
                frame = tokio::time::timeout(HUB_SILENCE, self.socket.next()) => frame,
                _ = tokio::time::sleep_until(ping_due.into()) => {
                    self.last_ping = Instant::now();
                    self.socket
                        .send(Message::Binary(HUB_PING.to_vec().into()))
                        .await
                        .map_err(|e| Closed::new(None, e.to_string()))?;
                    continue;
                }
            };
            let frame = frame.map_err(|_| Closed::new(None, "the hub went quiet"))?;
            let bytes: Vec<u8> = match frame {
                Some(Ok(Message::Binary(bytes))) => bytes.to_vec(),
                // The handshake's answer is JSON text: `{}` and a separator,
                // or an error.
                Some(Ok(Message::Text(text))) => {
                    let text = text.trim_end_matches('\u{1e}');
                    if let Some(error) = serde_json::from_str::<Value>(text)
                        .ok()
                        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
                    {
                        return Err(Closed::new(None, error));
                    }
                    continue;
                }
                Some(Ok(Message::Close(frame))) => return Err(closed_by(frame)),
                Some(Ok(_)) => continue,
                Some(Err(error)) => return Err(Closed::new(None, error.to_string())),
                None => return Err(Closed::new(None, "the connection ended")),
            };
            for message in hub_messages(&bytes) {
                match message {
                    HubMessage::Close { error } => {
                        return Err(Closed::new(None, error.unwrap_or_else(|| "closed".into())))
                    }
                    HubMessage::Update {
                        kind,
                        context_id,
                        payload_id,
                    } => {
                        if context_id.as_deref() == Some(self.device_id.as_str()) && kind != LOG_OUT
                        {
                            continue;
                        }
                        let event = match kind {
                            LOG_OUT => Event::LogOut {
                                reason: "logOut".into(),
                            },
                            AUTH_REQUEST => match payload_id {
                                Some(id) => Event::AuthRequest { id },
                                None => continue,
                            },
                            AUTH_REQUEST_RESPONSE => continue,
                            _ => Event::Changed {
                                areas: vec!["vault".into()],
                            },
                        };
                        self.queue.push_back(event);
                    }
                }
            }
        }
    }
}

/// What a hub frame carries, as far as UwULock cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubMessage {
    /// `ReceiveMessage` with Bitwarden's update: its type, the device that
    /// made the change, and the payload's `Id` (the auth request's).
    Update {
        kind: i64,
        context_id: Option<String>,
        payload_id: Option<String>,
    },
    /// SignalR's close message.
    Close { error: Option<String> },
}

/// The messages in one binary frame of the hub: each has its length in
/// front, seven bits a byte. Pings and anything else are left out.
pub fn hub_messages(frame: &[u8]) -> Vec<HubMessage> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < frame.len() {
        let mut length = 0usize;
        let mut shift = 0u32;
        let mut done = false;
        while at < frame.len() && shift < 35 {
            let byte = frame[at];
            at += 1;
            length |= usize::from(byte & 0x7f) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                done = true;
                break;
            }
        }
        if !done || length > frame.len() - at {
            break;
        }
        let message = &frame[at..at + length];
        at += length;
        let Some(value) = msgpack::read(message) else {
            continue;
        };
        let Some(items) = value.as_array() else {
            continue;
        };
        match items.first().and_then(Value::as_i64) {
            // Invocation: [1, headers, invocationId, target, arguments, …]
            Some(1) if items.get(3).and_then(Value::as_str) == Some("ReceiveMessage") => {
                let Some(argument) = items
                    .get(4)
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                else {
                    continue;
                };
                let field = |name: &str| {
                    argument
                        .get(name)
                        .or_else(|| argument.get(name.to_lowercase()))
                };
                let Some(kind) = field("Type").and_then(Value::as_i64) else {
                    continue;
                };
                out.push(HubMessage::Update {
                    kind,
                    context_id: field("ContextId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    payload_id: field("Payload")
                        .and_then(|p| p.get("Id").or_else(|| p.get("id")))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
            // Close: [7, error, allowReconnect]
            Some(7) => out.push(HubMessage::Close {
                error: items.get(1).and_then(Value::as_str).map(str::to_string),
            }),
            _ => {}
        }
    }
    out
}

/// Just enough MessagePack to read SignalR's invocations, into JSON values.
/// Binary data becomes `null`, extensions (timestamps) too.
mod msgpack {
    use serde_json::{Map, Number, Value};

    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
        depth: u32,
    }

    pub fn read(bytes: &[u8]) -> Option<Value> {
        Reader {
            bytes,
            at: 0,
            depth: 0,
        }
        .value()
    }

    impl Reader<'_> {
        fn take(&mut self, count: usize) -> Option<&[u8]> {
            let end = self.at.checked_add(count)?;
            let out = self.bytes.get(self.at..end)?;
            self.at = end;
            Some(out)
        }

        fn uint(&mut self, size: usize) -> Option<u64> {
            Some(
                self.take(size)?
                    .iter()
                    .fold(0u64, |acc, b| (acc << 8) | u64::from(*b)),
            )
        }

        fn int(&mut self, size: usize) -> Option<i64> {
            let raw = self.uint(size)?;
            let bits = size as u32 * 8;
            Some(if bits == 64 {
                raw as i64
            } else {
                ((raw << (64 - bits)) as i64) >> (64 - bits)
            })
        }

        fn text(&mut self, length: usize) -> Option<Value> {
            let bytes = self.take(length)?;
            Some(Value::String(String::from_utf8_lossy(bytes).into_owned()))
        }

        fn array(&mut self, length: usize) -> Option<Value> {
            // Every element takes at least a byte: a length past the end is a lie.
            if length > self.bytes.len() - self.at {
                return None;
            }
            (0..length)
                .map(|_| self.value())
                .collect::<Option<Vec<_>>>()
                .map(Value::Array)
        }

        fn map(&mut self, length: usize) -> Option<Value> {
            if length > self.bytes.len() - self.at {
                return None;
            }
            let mut out = Map::new();
            for _ in 0..length {
                let key = match self.value()? {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                let value = self.value()?;
                out.insert(key, value);
            }
            Some(Value::Object(out))
        }

        fn value(&mut self) -> Option<Value> {
            self.depth += 1;
            if self.depth > 32 {
                return None;
            }
            let value = self.value_inner();
            self.depth -= 1;
            value
        }

        fn value_inner(&mut self) -> Option<Value> {
            let tag = self.uint(1)? as u8;
            Some(match tag {
                0x00..=0x7f => Value::from(tag),
                0xe0..=0xff => Value::from(i64::from(tag as i8)),
                0x80..=0x8f => return self.map(usize::from(tag & 0x0f)),
                0x90..=0x9f => return self.array(usize::from(tag & 0x0f)),
                0xa0..=0xbf => return self.text(usize::from(tag & 0x1f)),
                0xc0 => Value::Null,
                0xc2 => Value::Bool(false),
                0xc3 => Value::Bool(true),
                0xc4..=0xc6 => {
                    let length = self.uint(1 << (tag - 0xc4))? as usize;
                    self.take(length)?;
                    Value::Null
                }
                0xc7..=0xc9 => {
                    let length = self.uint(1 << (tag - 0xc7))? as usize;
                    self.take(length + 1)?;
                    Value::Null
                }
                0xca => {
                    let bits = self.uint(4)? as u32;
                    Number::from_f64(f64::from(f32::from_bits(bits)))
                        .map_or(Value::Null, Value::Number)
                }
                0xcb => {
                    let bits = self.uint(8)?;
                    Number::from_f64(f64::from_bits(bits)).map_or(Value::Null, Value::Number)
                }
                0xcc..=0xcf => Value::from(self.uint(1 << (tag - 0xcc))?),
                0xd0..=0xd3 => Value::from(self.int(1 << (tag - 0xd0))?),
                0xd4..=0xd8 => {
                    self.take(1 + (1 << (tag - 0xd4)))?;
                    Value::Null
                }
                0xd9..=0xdb => {
                    let length = self.uint(1 << (tag - 0xd9))? as usize;
                    return self.text(length);
                }
                0xdc | 0xdd => {
                    let length = self.uint(if tag == 0xdc { 2 } else { 4 })? as usize;
                    return self.array(length);
                }
                0xde | 0xdf => {
                    let length = self.uint(if tag == 0xde { 2 } else { 4 })? as usize;
                    return self.map(length);
                }
                0xc1 => return None,
            })
        }
    }
}

// ── Reconnecting ───────────────────────────────────────────

/// How long to wait before connecting again (contract §5.3): by the close
/// code where it says, else exponential from 1 to 60 seconds with ±30 %
/// jitter; a connection that lived a minute starts the count over.
#[derive(Debug, Default)]
pub struct Backoff {
    failures: u32,
}

impl Backoff {
    /// The connection is up; `lived` is how long the last one lasted.
    pub fn connected_for(&mut self, lived: Duration) {
        if lived >= Duration::from_secs(60) {
            self.failures = 0;
        }
    }

    /// The wait after a close, or `None`: do not connect again (4403: not
    /// allowed).
    pub fn after(&mut self, closed: &Closed) -> Option<Duration> {
        let mut rng = rand::thread_rng();
        let mut seconds =
            |low: u64, high: u64| Duration::from_millis(rng.gen_range(low * 1000..=high * 1000));
        let wait = match closed.code {
            Some(4403) => return None,
            Some(1000) | Some(1001) => seconds(1, 5),
            Some(1012) => seconds(2, 10),
            Some(4400) | Some(4429) => seconds(60, 75),
            // The caller refreshes the token first; then at once — but only
            // once in a row: a token refused again waits like any failure.
            Some(4401) if self.failures == 0 => Duration::from_millis(200),
            Some(4408) => seconds(1, 3),
            _ => {
                let base = 60f64.min(2f64.powi(self.failures.min(10) as i32));
                let jitter = rand::thread_rng().gen_range(0.7..=1.3);
                Duration::from_secs_f64(base * jitter)
            }
        };
        self.failures = self.failures.saturating_add(1);
        Some(wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_messages() {
        assert_eq!(
            parse_realtime(
                r#"{"type":"ready","connectionId":"c0d1","expires":1790000000,"heartbeat":25}"#
            ),
            Some(RealtimeMessage::Ready {
                heartbeat: 25,
                expires: 1_790_000_000
            })
        );
        assert_eq!(
            parse_realtime(r#"{"type":"changed","areas":["vault","uwu"]}"#),
            Some(RealtimeMessage::Event(Event::Changed {
                areas: vec!["vault".into(), "uwu".into()]
            }))
        );
        assert_eq!(
            parse_realtime(r#"{"type":"notice","kind":"securityNotice","id":123}"#),
            Some(RealtimeMessage::Event(Event::Notice {
                kind: "securityNotice".into(),
                id: Some("123".into())
            }))
        );
        assert_eq!(
            parse_realtime(r#"{"type":"logout","reason":"keysRotated"}"#),
            Some(RealtimeMessage::Event(Event::LogOut {
                reason: "keysRotated".into()
            }))
        );
        assert_eq!(
            parse_realtime(r#"{"type":"info"}"#),
            Some(RealtimeMessage::Event(Event::Info))
        );
        assert_eq!(parse_realtime(r#"{"type":"somethingNew"}"#), None);
        assert_eq!(parse_realtime("not json"), None);
    }

    /// A hub frame as Bitwarden's server writes it: `ReceiveMessage` with
    /// `{ContextId, Type, Payload}`, length-prefixed; a ping after it.
    pub(crate) fn hub_frame(kind: u8, context: &str, payload_id: &str) -> Vec<u8> {
        let mut m = vec![0x95, 0x01, 0x80, 0xc0];
        m.push(0xa0 | 14);
        m.extend_from_slice(b"ReceiveMessage");
        m.push(0x91);
        m.push(0x83);
        m.push(0xa0 | 9);
        m.extend_from_slice(b"ContextId");
        m.push(0xa0 | context.len() as u8);
        m.extend_from_slice(context.as_bytes());
        m.push(0xa0 | 4);
        m.extend_from_slice(b"Type");
        m.push(kind);
        m.push(0xa0 | 7);
        m.extend_from_slice(b"Payload");
        m.push(0x82);
        m.push(0xa0 | 2);
        m.extend_from_slice(b"Id");
        m.push(0xa0 | payload_id.len() as u8);
        m.extend_from_slice(payload_id.as_bytes());
        m.push(0xa0 | 4);
        m.extend_from_slice(b"Date");
        // A timestamp extension, which is skipped.
        m.extend_from_slice(&[0xd6, 0xff, 0, 0, 0, 1]);
        let mut frame = vec![m.len() as u8];
        frame.extend_from_slice(&m);
        frame.extend_from_slice(&HUB_PING);
        frame
    }

    #[test]
    fn hub_frames() {
        let frame = hub_frame(15, "dev-1", "req-7");
        assert_eq!(
            hub_messages(&frame),
            vec![HubMessage::Update {
                kind: 15,
                context_id: Some("dev-1".into()),
                payload_id: Some("req-7".into())
            }]
        );
        // Truncated, lying about its length, or nonsense: nothing, no panic.
        assert!(hub_messages(&frame[..10]).is_empty());
        assert!(hub_messages(&[0xff, 0xff, 0xff, 0xff, 0x7f]).is_empty());
        assert!(hub_messages(&[0x03, 0xdd, 0xff, 0xff]).is_empty());
        assert!(hub_messages(&[0x02, 0x92, 0x07]).is_empty());
        let close = [0x03, 0x92, 0x07, 0xc0];
        assert_eq!(
            hub_messages(&close),
            vec![HubMessage::Close { error: None }]
        );
    }

    #[test]
    fn urls() {
        let server = Server::self_hosted("https://lock.example.com").unwrap();
        assert_eq!(
            realtime_url(&server),
            "wss://lock.example.com/uwu/v1/realtime"
        );
        assert_eq!(
            hub_url(&server, "a.b+c"),
            "wss://lock.example.com/notifications/hub?access_token=a.b%2Bc"
        );
        assert_eq!(
            hub_url(&Server::BitwardenEu, "t"),
            "wss://notifications.bitwarden.eu/hub?access_token=t"
        );
        let local = Server::self_hosted("http://localhost:8000").unwrap();
        assert_eq!(realtime_url(&local), "ws://localhost:8000/uwu/v1/realtime");
    }

    #[test]
    fn backoff_by_close_code() {
        let mut backoff = Backoff::default();
        let closed = |code| Closed::new(code, "");
        assert_eq!(backoff.after(&closed(Some(4403))), None);
        let wait = backoff.after(&closed(Some(4429))).unwrap();
        assert!(wait >= Duration::from_secs(60));
        let mut backoff = Backoff::default();
        let first = backoff.after(&closed(None)).unwrap();
        assert!(first <= Duration::from_millis(1300));
        for _ in 0..20 {
            backoff.after(&closed(None));
        }
        let late = backoff.after(&closed(None)).unwrap();
        assert!(late >= Duration::from_secs(42) && late <= Duration::from_secs(78));
        backoff.connected_for(Duration::from_secs(61));
        assert!(backoff.after(&closed(None)).unwrap() <= Duration::from_millis(1300));
    }
}
