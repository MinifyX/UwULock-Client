//! A toy Bitwarden server: just enough of Vaultwarden's API for UwULock to
//! log in, pass two-step login, refresh and sync — with a vault encrypted the
//! way Bitwarden's apps encrypt one, so decrypting it proves the real thing.
//!
//! Account `nyu@uwu.local`, master password `uwu-nyu-nyu-nyu`. Two-step login
//! with an authenticator (secret [`TOTP_SECRET`]) or the email code `123456`.
//! Plain HTTP on 127.0.0.1, one thread per request. Never for real data.

#![allow(dead_code)]

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use rsa::pkcs8::EncodePrivateKey;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use uwulock_bitwarden::crypto::{self, EncString, Kdf, SymmetricKey};
use uwulock_bitwarden::totp::Totp;

pub const EMAIL: &str = "nyu@uwu.local";
pub const PASSWORD: &str = "uwu-nyu-nyu-nyu";
pub const TOTP_SECRET: &str = "JBSWY3DPEHPK3PXP";
pub const EMAIL_CODE: &str = "123456";
/// The authenticator key stored in the GitHub item.
pub const ITEM_TOTP: &str =
    "otpauth://totp/GitHub:nyu?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=GitHub";

pub struct Options {
    pub two_factor: bool,
    pub kdf: Kdf,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            two_factor: false,
            // Fewer rounds than Bitwarden's 600 000, so tests stay quick.
            kdf: Kdf::Pbkdf2 { iterations: 5_000 },
        }
    }
}

struct State {
    options: Options,
    hash: String,
    protected_user_key: String,
    protected_private_key: String,
    sync: Value,
    access_tokens: Vec<String>,
    refresh_tokens: Vec<String>,
    remember_tokens: Vec<String>,
    counter: u64,
    pub logins: u32,
}

pub struct ToyServer {
    pub url: String,
    state: Arc<Mutex<State>>,
}

impl ToyServer {
    pub fn start(options: Options) -> ToyServer {
        Self::start_on("127.0.0.1:0", options)
    }

    pub fn start_on(address: &str, options: Options) -> ToyServer {
        let master = crypto::master_key(PASSWORD, EMAIL, options.kdf).unwrap();
        let hash = crypto::master_password_hash(&master, PASSWORD);
        let user_key = SymmetricKey::generate();
        let protected_user_key =
            EncString::encrypt(&user_key.to_bytes(), &SymmetricKey::stretch(&master)).to_string();

        let private = rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap();
        let der = private.to_pkcs8_der().unwrap();
        let protected_private_key = EncString::encrypt(der.as_bytes(), &user_key).to_string();
        let org_key = SymmetricKey::generate();
        let public = rsa::RsaPublicKey::from(&private);
        let org_wrapped = crypto::wrap_for(&public, &org_key).unwrap().to_string();

        let sync = sample_vault(
            &user_key,
            &org_key,
            &org_wrapped,
            &protected_user_key,
            &protected_private_key,
        );
        let listener = TcpListener::bind(address).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State {
            options,
            hash,
            protected_user_key,
            protected_private_key,
            sync,
            access_tokens: Vec::new(),
            refresh_tokens: Vec::new(),
            remember_tokens: Vec::new(),
            counter: 0,
            logins: 0,
        }));
        let shared = state.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = shared.clone();
                std::thread::spawn(move || {
                    let _ = serve(stream, &state);
                });
            }
        });
        ToyServer { url, state }
    }

    /// Forgets every session, as if the account logged out everywhere.
    pub fn revoke_sessions(&self) {
        let mut state = self.state.lock().unwrap();
        state.access_tokens.clear();
        state.refresh_tokens.clear();
    }

    pub fn logins(&self) -> u32 {
        self.state.lock().unwrap().logins
    }
}

fn enc(value: &str, key: &SymmetricKey) -> Value {
    Value::String(EncString::encrypt(value.as_bytes(), key).to_string())
}

fn sample_vault(
    user: &SymmetricKey,
    org: &SymmetricKey,
    org_wrapped: &str,
    protected_user_key: &str,
    protected_private_key: &str,
) -> Value {
    let date = "2026-09-20T10:15:00.000Z";
    let login = |id: &str,
                 name: &str,
                 user_name: &str,
                 password: &str,
                 uri: &str,
                 key: &SymmetricKey| {
        json!({
            "id": id, "organizationId": null, "folderId": null, "type": 1,
            "name": enc(name, key), "notes": null, "favorite": false, "reprompt": 0,
            "login": { "username": enc(user_name, key), "password": enc(password, key),
                       "totp": null, "uris": [{ "uri": enc(uri, key), "match": null }],
                       "passwordRevisionDate": null },
            "fields": null, "passwordHistory": null, "attachments": null,
            "collectionIds": [], "revisionDate": date, "creationDate": date, "deletedDate": null,
            "object": "cipherDetails"
        })
    };

    let mut github = login(
        "c-github",
        "GitHub",
        "nyu-the-cat",
        "hunter2-but-longer!",
        "https://github.com/login",
        user,
    );
    github["favorite"] = json!(true);
    github["folderId"] = json!("f-private");
    github["login"]["totp"] = enc(ITEM_TOTP, user);
    github["notes"] = enc("Recovery codes are in the safe under the cat bed.", user);
    github["passwordHistory"] =
        json!([{ "password": enc("hunter2", user), "lastUsedDate": "2025-01-01T00:00:00.000Z" }]);

    let mut vaultwarden = login(
        "c-vaultwarden",
        "Vaultwarden Admin",
        "admin",
        "Adm1n-Token-🐾",
        "https://vault.uwu.local/admin",
        user,
    );
    vaultwarden["folderId"] = json!("f-homelab");
    vaultwarden["reprompt"] = json!(1);

    // An item with a key of its own, like new Bitwarden items.
    let item_key = SymmetricKey::generate();
    let mut nas = login(
        "c-nas",
        "Synology NAS",
        "nyu",
        "Katzenklo-2026",
        "nas.uwu.local:5001",
        &item_key,
    );
    nas["key"] = Value::String(EncString::encrypt(&item_key.to_bytes(), user).to_string());
    nas["folderId"] = json!("f-homelab");
    nas["fields"] = json!([
        { "name": enc("Admin-PIN", &item_key), "value": enc("4711", &item_key), "type": 1, "linkedId": null },
        { "name": enc("Standort", &item_key), "value": enc("Keller, Regal 2", &item_key), "type": 0, "linkedId": null },
        { "name": enc("2FA aktiv", &item_key), "value": enc("true", &item_key), "type": 2, "linkedId": null }
    ]);

    let mut router = login(
        "c-router",
        "FritzBox",
        "",
        "Kabel-Salat-99",
        "http://fritz.box",
        org,
    );
    router["organizationId"] = json!("o-homelab");
    router["collectionIds"] = json!(["col-network"]);

    let card = json!({
        "id": "c-card", "organizationId": null, "folderId": "f-private", "type": 3,
        "name": enc("Katzenfutter-Karte", user), "notes": null, "favorite": false, "reprompt": 0,
        "card": { "cardholderName": enc("Nyu Neko", user), "brand": enc("Visa", user),
                  "number": enc("4111 1111 1111 1234", user), "expMonth": enc("7", user),
                  "expYear": enc("2029", user), "code": enc("123", user) },
        "collectionIds": [], "revisionDate": date, "creationDate": date, "deletedDate": null
    });
    let identity = json!({
        "id": "c-identity", "organizationId": null, "folderId": null, "type": 4,
        "name": enc("Nyu privat", user), "notes": null, "favorite": false, "reprompt": 0,
        "identity": { "title": enc("Frau", user), "firstName": enc("Nyu", user), "lastName": enc("Neko", user),
                      "email": enc("nyu@uwu.local", user), "phone": enc("+49 30 1234567", user),
                      "address1": enc("Kratzbaumweg 3", user), "postalCode": enc("10115", user),
                      "city": enc("Berlin", user), "country": enc("DE", user),
                      "passportNumber": enc("C01X00T47", user) },
        "collectionIds": [], "revisionDate": date, "creationDate": date, "deletedDate": null
    });
    let note = json!({
        "id": "c-note", "organizationId": null, "folderId": null, "type": 2,
        "name": enc("WLAN für Gäste", user), "notes": enc("SSID: UwU-Gast\nPasswort: miau-miau-miau", user),
        "secureNote": { "type": 0 }, "favorite": false, "reprompt": 0,
        "collectionIds": [], "revisionDate": date, "creationDate": date, "deletedDate": null
    });
    let ssh = json!({
        "id": "c-ssh", "organizationId": null, "folderId": "f-homelab", "type": 5,
        "name": enc("homelab ed25519", user), "notes": null, "favorite": false, "reprompt": 0,
        "sshKey": { "privateKey": enc("-----BEGIN OPENSSH PRIVATE KEY-----\nnot-a-real-key\n-----END OPENSSH PRIVATE KEY-----", user),
                    "publicKey": enc("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIUwUNyuNyuNyuNyuNyuNyuNyuNyuNyuNyu nyu@uwu", user),
                    "keyFingerprint": enc("SHA256:nyuNyuNyu0UwU1uwuUWU2owo3OwO4", user) },
        "collectionIds": [], "revisionDate": date, "creationDate": date, "deletedDate": null
    });
    let mut trashed = login(
        "c-old",
        "Altes Forum",
        "nyu",
        "forum123",
        "https://forum.example.org",
        user,
    );
    trashed["deletedDate"] = json!(date);

    json!({
        "profile": {
            "id": "u-nyu", "name": "Nyu", "email": EMAIL, "key": protected_user_key,
            "privateKey": protected_private_key, "securityStamp": "stamp",
            "organizations": [{ "id": "o-homelab", "name": "Homelab", "key": org_wrapped, "enabled": true, "type": 0 }],
            "object": "profile"
        },
        "folders": [
            { "id": "f-private", "name": enc("Privat", user), "revisionDate": date },
            { "id": "f-homelab", "name": enc("Homelab", user), "revisionDate": date }
        ],
        "collections": [
            { "id": "col-network", "organizationId": "o-homelab", "name": enc("Netzwerk", org) }
        ],
        "ciphers": [github, vaultwarden, nas, router, card, identity, note, ssh, trashed],
        "domains": null, "policies": [], "sends": [], "object": "sync"
    })
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_string());
        }
    }
    let length: usize = headers
        .get("content-length")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}

fn respond(stream: &mut TcpStream, status: u16, body: &Value) -> std::io::Result<()> {
    let text = body.to_string();
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        _ => "Not Found",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )
}

fn form(body: &[u8]) -> HashMap<String, String> {
    url::form_urlencoded::parse(body).into_owned().collect()
}

fn serve(mut stream: TcpStream, state: &Mutex<State>) -> std::io::Result<()> {
    let Some(request) = read_request(&mut stream) else {
        return Ok(());
    };
    let path = request
        .path
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let (status, body) = route(&request, &path, &mut state.lock().unwrap());
    respond(&mut stream, status, &body)
}

fn route(request: &Request, path: &str, state: &mut State) -> (u16, Value) {
    match (request.method.as_str(), path) {
        ("POST", "/identity/accounts/prelogin") => {
            let (kdf, iterations, memory, parallelism) = match state.options.kdf {
                Kdf::Pbkdf2 { iterations } => (0, iterations, None, None),
                Kdf::Argon2id {
                    iterations,
                    memory_mib,
                    parallelism,
                } => (1, iterations, Some(memory_mib), Some(parallelism)),
            };
            (
                200,
                json!({ "kdf": kdf, "kdfIterations": iterations, "kdfMemory": memory, "kdfParallelism": parallelism }),
            )
        }
        ("POST", "/identity/connect/token") => token(request, state),
        ("POST", "/api/two-factor/send-email-login") => (200, json!({})),
        ("GET", "/api/sync") => {
            let bearer = request
                .headers
                .get("authorization")
                .and_then(|h| h.strip_prefix("Bearer "))
                .unwrap_or_default();
            if state.access_tokens.iter().any(|t| t == bearer) {
                (200, state.sync.clone())
            } else {
                (401, json!({ "message": "Unauthorized" }))
            }
        }
        _ => (404, json!({ "message": "Not found" })),
    }
}

fn token(request: &Request, state: &mut State) -> (u16, Value) {
    let form = form(&request.body);
    let get = |name: &str| form.get(name).map(String::as_str).unwrap_or_default();
    let invalid = |message: &str| {
        (
            400,
            json!({ "error": "invalid_grant", "error_description": "invalid_username_or_password",
                      "ErrorModel": { "Message": message, "Object": "error" } }),
        )
    };
    match get("grant_type") {
        "password" => {
            for required in ["deviceIdentifier", "deviceName", "deviceType", "client_id"] {
                if get(required).is_empty() {
                    return (
                        400,
                        json!({ "error": "invalid_request", "error_description": format!("{required} missing") }),
                    );
                }
            }
            if get("username") != EMAIL || get("password") != state.hash {
                return invalid("Username or password is incorrect. Try again");
            }
            if state.options.two_factor {
                let code = get("twoFactorToken");
                let passed = match get("twoFactorProvider") {
                    "0" => Totp::parse(TOTP_SECRET).is_ok_and(|t| t.now().0.as_str() == code),
                    "1" => code == EMAIL_CODE,
                    "5" => state.remember_tokens.iter().any(|t| t == code),
                    _ => false,
                };
                if !passed {
                    let mut body = json!({
                        "error": "invalid_grant", "error_description": "Two factor required.",
                        "TwoFactorProviders": ["0", "1"],
                        "TwoFactorProviders2": { "0": null, "1": { "Email": "n***@uwu.local" } }
                    });
                    if !code.is_empty() && get("twoFactorProvider") != "5" {
                        body["ErrorModel"] =
                            json!({ "Message": "Invalid TOTP code! Server time: now" });
                    }
                    return (400, body);
                }
            }
            state.logins += 1;
            let mut body = issue(state);
            body["Key"] = json!(state.protected_user_key);
            body["PrivateKey"] = json!(state.protected_private_key);
            if state.options.two_factor && get("twoFactorRemember") == "1" {
                let remember = format!("remember-{}", state.counter);
                state.remember_tokens.push(remember.clone());
                body["TwoFactorToken"] = json!(remember);
            }
            (200, body)
        }
        "refresh_token" => {
            let refresh = get("refresh_token").to_string();
            if !state.refresh_tokens.contains(&refresh) {
                return (400, json!({ "error": "invalid_grant" }));
            }
            (200, issue(state))
        }
        _ => (400, json!({ "error": "unsupported_grant_type" })),
    }
}

fn issue(state: &mut State) -> Value {
    state.counter += 1;
    let access = format!(
        "access-{}-{}",
        state.counter,
        B64.encode(state.counter.to_be_bytes())
    );
    let refresh = format!("refresh-{}", state.counter);
    state.access_tokens.push(access.clone());
    state.refresh_tokens.push(refresh.clone());
    json!({
        "access_token": access, "expires_in": 3600, "token_type": "Bearer",
        "refresh_token": refresh, "scope": "api offline_access", "unofficialServer": true
    })
}
