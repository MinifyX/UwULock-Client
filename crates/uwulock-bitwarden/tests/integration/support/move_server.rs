//! A small Bitwarden server with one account, for moving a vault: the same
//! code plays the source (Bitwarden's cloud, a Vaultwarden) and the target
//! (a UwULock Server, with or without families).
//!
//! It keeps what is written — folders, items, attachments and their files,
//! Sends and their files, organisations and collections — and hands it back
//! in the sync, the way the real servers do: attachments are announced
//! (`attachment/v2`), then uploaded as `multipart/form-data`; file Sends the
//! same (`sends/file/v2`); an organisation comes with its first collection.
//! It checks what a real server checks and a move could get wrong: an
//! organisation's item needs a collection of that organisation, an upload
//! must match what was announced, a family needs `planType` 22 and a key
//! wrapped for the account.
//!
//! Plain HTTP on 127.0.0.1:0, one thread per request. Never for real data.

#![allow(dead_code)]

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use uwulock_bitwarden::crypto::{self, EncString, Kdf, PrivateKey, SymmetricKey};

pub const PASSWORD: &str = "correct horse battery staple";
const KDF: Kdf = Kdf::Pbkdf2 { iterations: 5_000 };

#[derive(Clone, Copy)]
pub enum Kind {
    /// Bitwarden or Vaultwarden: no `/uwu/v1`.
    Bitwarden,
    /// A UwULock Server; `families` whether this account may make one.
    UwuLock { families: bool },
}

pub struct Keys {
    pub user: SymmetricKey,
    pub private: PrivateKey,
}

struct Attachment {
    cipher: String,
    id: String,
    file_name: String,
    key: String,
    size: usize,
    data: Option<Vec<u8>>,
}

struct Org {
    id: String,
    name: String,
    /// The organisation key, wrapped for the account's public key.
    key: String,
    membership: String,
}

struct State {
    kind: Kind,
    email: String,
    hash: String,
    protected_user_key: String,
    protected_private_key: String,
    tokens: Vec<String>,
    folders: Vec<Value>,
    ciphers: Vec<Value>,
    attachments: Vec<Attachment>,
    sends: Vec<Value>,
    send_files: HashMap<String, Vec<u8>>,
    orgs: Vec<Org>,
    collections: Vec<Value>,
    counter: u64,
    /// File Sends fetched through their access link.
    send_openings: u32,
}

pub struct MoveServer {
    pub url: String,
    pub email: String,
    pub keys: Keys,
    state: Arc<Mutex<State>>,
}

impl MoveServer {
    pub fn start(kind: Kind, email: &str) -> MoveServer {
        let master = crypto::master_key(PASSWORD, email, KDF).unwrap();
        let hash = crypto::master_password_hash(&master, PASSWORD);
        let user = SymmetricKey::generate();
        let protected_user_key =
            EncString::encrypt(&user.to_bytes(), &SymmetricKey::stretch(&master)).to_string();
        let private = PrivateKey::generate().unwrap();
        let protected_private_key =
            EncString::encrypt(&private.to_der().unwrap(), &user).to_string();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State {
            kind,
            email: email.to_string(),
            hash,
            protected_user_key,
            protected_private_key,
            tokens: Vec::new(),
            folders: Vec::new(),
            ciphers: Vec::new(),
            attachments: Vec::new(),
            sends: Vec::new(),
            send_files: HashMap::new(),
            orgs: Vec::new(),
            collections: Vec::new(),
            counter: 0,
            send_openings: 0,
        }));
        let shared = state.clone();
        let base = url.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = shared.clone();
                let base = base.clone();
                std::thread::spawn(move || {
                    let _ = serve(stream, &state, &base);
                });
            }
        });
        MoveServer {
            url,
            email: email.to_string(),
            keys: Keys { user, private },
            state,
        }
    }

    pub fn send_openings(&self) -> u32 {
        self.state.lock().unwrap().send_openings
    }

    /// How many of each the server holds: folders, items, attachments, Sends,
    /// organisations, collections.
    pub fn counts(&self) -> [usize; 6] {
        let state = self.state.lock().unwrap();
        [
            state.folders.len(),
            state.ciphers.len(),
            state
                .attachments
                .iter()
                .filter(|a| a.data.is_some())
                .count(),
            state.sends.len(),
            state.orgs.len(),
            state.collections.len(),
        ]
    }

    /// The stored (encrypted) file of a Send.
    pub fn send_file(&self, send: &str) -> Option<Vec<u8>> {
        let state = self.state.lock().unwrap();
        let send = state.sends.iter().find(|s| s["id"] == json!(send))?;
        state
            .send_files
            .get(&format!(
                "{}/{}",
                send["id"].as_str()?,
                send["file"]["id"].as_str()?
            ))
            .cloned()
    }

    // ── Seeding a source vault ─────────────────────────────

    pub fn add_folder(&self, id: &str, name: &str) {
        let name = enc(name, &self.keys.user);
        self.state
            .lock()
            .unwrap()
            .folders
            .push(json!({ "id": id, "name": name, "revisionDate": DATE, "object": "folder" }));
    }

    /// An organisation this account owns; its key comes back.
    pub fn add_org(&self, id: &str, name: &str) -> SymmetricKey {
        let key = SymmetricKey::generate();
        let wrapped = crypto::wrap_for(&self.keys.private.public(), &key)
            .unwrap()
            .to_string();
        self.state.lock().unwrap().orgs.push(Org {
            id: id.into(),
            name: name.into(),
            key: wrapped,
            membership: format!("{id}-me"),
        });
        key
    }

    pub fn add_collection(&self, id: &str, org: &str, name: &str, key: &SymmetricKey) {
        self.state.lock().unwrap().collections.push(
            json!({ "id": id, "organizationId": org, "name": enc(name, key), "object": "collectionDetails" }),
        );
    }

    /// An item as it is (all of it encrypted by the caller).
    pub fn add_cipher(&self, cipher: Value) {
        let mut cipher = cipher;
        for (key, default) in [
            ("revisionDate", json!(DATE)),
            ("creationDate", json!(DATE)),
            ("deletedDate", Value::Null),
            ("collectionIds", json!([])),
            ("favorite", json!(false)),
            ("reprompt", json!(0)),
        ] {
            if cipher.get(key).is_none() {
                cipher[key] = default;
            }
        }
        cipher["object"] = json!("cipherDetails");
        self.state.lock().unwrap().ciphers.push(cipher);
    }

    /// A file on an item: `plain` encrypted under `file_key`, which is
    /// wrapped under `item_key`, as is the name.
    pub fn add_attachment(
        &self,
        cipher: &str,
        id: &str,
        name: &str,
        plain: &[u8],
        item_key: &SymmetricKey,
    ) {
        let file_key = SymmetricKey::generate();
        let data = crypto::encrypt_file(plain, &file_key);
        self.state.lock().unwrap().attachments.push(Attachment {
            cipher: cipher.into(),
            id: id.into(),
            file_name: EncString::encrypt(name.as_bytes(), item_key).to_string(),
            key: EncString::encrypt(&file_key.to_bytes(), item_key).to_string(),
            size: data.len(),
            data: Some(data),
        });
    }

    /// A Send as it is; a file Send's encrypted file with it.
    pub fn add_send(&self, send: Value, file: Option<Vec<u8>>) {
        let mut state = self.state.lock().unwrap();
        if let Some(file) = file {
            let path = format!(
                "{}/{}",
                send["id"].as_str().unwrap(),
                send["file"]["id"].as_str().unwrap()
            );
            state.send_files.insert(path, file);
        }
        state.sends.push(send);
    }
}

pub const DATE: &str = "2026-09-20T10:15:00.000Z";

pub fn enc(value: &str, key: &SymmetricKey) -> Value {
    Value::String(EncString::encrypt(value.as_bytes(), key).to_string())
}

// ── HTTP ───────────────────────────────────────────────────

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Request {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

enum Body {
    Json(Value),
    Bytes(Vec<u8>),
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

fn respond(stream: &mut TcpStream, status: u16, body: Body) -> std::io::Result<()> {
    let (kind, bytes) = match body {
        Body::Json(value) => ("application/json", value.to_string().into_bytes()),
        Body::Bytes(bytes) => ("application/octet-stream", bytes),
    };
    write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    )?;
    stream.write_all(&bytes)
}

fn serve(mut stream: TcpStream, state: &Mutex<State>, base: &str) -> std::io::Result<()> {
    let Some(request) = read_request(&mut stream) else {
        return Ok(());
    };
    let path = request
        .path
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let (status, body) = route(&request, &path, &mut state.lock().unwrap(), base);
    respond(&mut stream, status, body)
}

fn ok(value: Value) -> (u16, Body) {
    (200, Body::Json(value))
}

fn refused(message: &str) -> (u16, Body) {
    (
        400,
        Body::Json(json!({ "message": message, "object": "error" })),
    )
}

fn not_found() -> (u16, Body) {
    (404, Body::Json(json!({ "message": "Not found" })))
}

fn route(request: &Request, path: &str, state: &mut State, base: &str) -> (u16, Body) {
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let method = request.method.as_str();
    // What needs no session.
    match (method, segments.as_slice()) {
        ("POST", ["identity", "accounts", "prelogin"]) => {
            return ok(json!({ "kdf": 0, "kdfIterations": 5_000 }))
        }
        ("POST", ["identity", "connect", "token"]) => return token(request, state),
        ("GET", ["uwu", "v1", "info"]) => {
            return match state.kind {
                Kind::Bitwarden => not_found(),
                Kind::UwuLock { families } => {
                    let mut features = vec!["vault", "delta-sync"];
                    if families {
                        features.push("families");
                    }
                    ok(
                        json!({ "object": "info", "name": "UwULock Server", "apiVersion": 1,
                               "features": features, "limits": { "maxFileBytes": 1_048_576 } }),
                    )
                }
            };
        }
        ("GET", ["attachments", cipher, id]) => {
            return match state
                .attachments
                .iter()
                .find(|a| a.cipher == *cipher && a.id == *id)
                .and_then(|a| a.data.clone())
            {
                Some(data) => (200, Body::Bytes(data)),
                None => not_found(),
            };
        }
        ("POST", ["api", "sends", send, "access", "file", file]) => {
            let Some(found) = state.sends.iter().find(|s| s["id"] == json!(send)) else {
                return not_found();
            };
            if found["password"].is_string() {
                return (401, Body::Json(json!({ "message": "Password required" })));
            }
            state.send_openings += 1;
            return ok(json!({ "object": "send-fileDownload", "id": file,
                              "url": format!("{base}/sendfiles/{send}/{file}?t=token") }));
        }
        ("GET", ["sendfiles", send, file]) => {
            return match state.send_files.get(&format!("{send}/{file}")) {
                Some(data) => (200, Body::Bytes(data.clone())),
                None => not_found(),
            };
        }
        _ => {}
    }
    let bearer = request
        .headers
        .get("authorization")
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !state.tokens.iter().any(|t| t == bearer) {
        return (401, Body::Json(json!({ "message": "Unauthorized" })));
    }
    match (method, segments.as_slice()) {
        ("GET", ["api", "sync"]) => ok(sync(state, base)),
        ("GET", ["uwu", "v1", "account"]) => match state.kind {
            Kind::UwuLock { families } => {
                let owned = state.orgs.len();
                ok(json!({ "object": "account",
                           "families": { "mayCreate": families, "maxMembers": 6, "owned": owned, "perUser": 1 } }))
            }
            Kind::Bitwarden => not_found(),
        },
        ("POST", ["api", "folders"]) => {
            let id = next_id(state, "folder");
            let folder = json!({ "id": id, "name": request.json()["name"], "revisionDate": DATE, "object": "folder" });
            state.folders.push(folder.clone());
            ok(folder)
        }
        ("POST", ["api", "ciphers"]) => save_cipher(state, request.json(), Vec::new(), base),
        ("POST", ["api", "ciphers", "create"]) => {
            let body = request.json();
            let collections = body["collectionIds"]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            save_cipher(state, body["cipher"].clone(), collections, base)
        }
        ("GET", ["api", "ciphers", cipher, "attachment", id]) => {
            match state
                .attachments
                .iter()
                .find(|a| a.cipher == *cipher && a.id == *id && a.data.is_some())
            {
                Some(a) => ok(attachment_json(a, base)),
                None => not_found(),
            }
        }
        ("POST", ["api", "ciphers", cipher, "attachment", "v2"]) => {
            if !state.ciphers.iter().any(|c| c["id"] == json!(cipher)) {
                return not_found();
            }
            let body = request.json();
            let (Some(key), Some(file_name), Some(size)) = (
                body["key"].as_str(),
                body["fileName"].as_str(),
                body["fileSize"].as_u64(),
            ) else {
                return refused("key, fileName and fileSize are required");
            };
            if !key.starts_with("2.") || !file_name.starts_with("2.") {
                return refused("not encrypted");
            }
            let id = next_id(state, "attachment");
            state.attachments.push(Attachment {
                cipher: cipher.to_string(),
                id: id.clone(),
                file_name: file_name.into(),
                key: key.into(),
                size: size as usize,
                data: None,
            });
            ok(
                json!({ "object": "attachment-fileUpload", "attachmentId": id, "fileUploadType": 0,
                       "url": format!("/ciphers/{cipher}/attachment/{id}") }),
            )
        }
        ("POST", ["api", "ciphers", cipher, "attachment", id]) => {
            let Some(data) = multipart_data(request) else {
                return refused("no file");
            };
            match state
                .attachments
                .iter_mut()
                .find(|a| a.cipher == *cipher && a.id == *id)
            {
                Some(a) if a.data.is_none() && a.size == data.len() => {
                    a.data = Some(data);
                    ok(Value::Null)
                }
                Some(_) => refused("the upload doesn't match what was announced"),
                None => not_found(),
            }
        }
        ("DELETE", ["api", "ciphers", cipher, "attachment", id]) => {
            state
                .attachments
                .retain(|a| !(a.cipher == *cipher && a.id == *id));
            ok(Value::Null)
        }
        ("POST", ["api", "sends"]) => {
            let mut send = request.json();
            if send["type"] != json!(0) || !send["text"]["text"].is_string() {
                return refused("File sends go through /sends/file/v2.");
            }
            send["id"] = json!(next_id(state, "send"));
            send["accessCount"] = json!(0);
            send["object"] = json!("send");
            state.sends.push(send.clone());
            ok(send)
        }
        ("POST", ["api", "sends", "file", "v2"]) => {
            let mut send = request.json();
            let (Some(length), Some(name)) = (
                send["fileLength"].as_u64(),
                send["file"]["fileName"].as_str().map(str::to_string),
            ) else {
                return refused("Send data not provided");
            };
            let id = next_id(state, "send");
            let file = next_id(state, "file");
            send["id"] = json!(id);
            send["file"] = json!({ "id": file, "fileName": name, "size": length.to_string() });
            send["accessCount"] = json!(0);
            send["object"] = json!("send");
            state.sends.push(send.clone());
            ok(json!({ "object": "send-fileUpload", "fileUploadType": 0,
                       "url": format!("/sends/{id}/file/{file}"), "sendResponse": send }))
        }
        ("POST", ["api", "sends", send, "file", file]) => {
            let Some(data) = multipart_data(request) else {
                return refused("no file");
            };
            let Some(found) = state.sends.iter().find(|s| s["id"] == json!(send)) else {
                return not_found();
            };
            if found["file"]["id"] != json!(file)
                || found["file"]["size"] != json!(data.len().to_string())
            {
                return refused("Send file size does not match.");
            }
            state.send_files.insert(format!("{send}/{file}"), data);
            ok(Value::Null)
        }
        ("DELETE", ["api", "sends", send]) => {
            state.sends.retain(|s| s["id"] != json!(send));
            ok(Value::Null)
        }
        ("POST", ["api", "organizations"]) => create_org(state, request.json()),
        ("POST", ["api", "organizations", org, "collections"]) => {
            let Some(found) = state.orgs.iter().find(|o| o.id == *org) else {
                return not_found();
            };
            let body = request.json();
            if !body["name"].as_str().is_some_and(|n| n.starts_with("2.")) {
                return refused("The name must be encrypted.");
            }
            let membership = found.membership.clone();
            if body["users"]
                .as_array()
                .is_some_and(|users| users.iter().any(|u| u["id"] != json!(membership)))
            {
                return refused("Not a member.");
            }
            let collection = json!({ "id": next_id(state, "collection"), "organizationId": org,
                                     "name": body["name"], "object": "collection" });
            state.collections.push(collection.clone());
            ok(collection)
        }
        _ => not_found(),
    }
}

fn next_id(state: &mut State, kind: &str) -> String {
    state.counter += 1;
    format!("{kind}-{}", state.counter)
}

fn token(request: &Request, state: &mut State) -> (u16, Body) {
    let form: HashMap<String, String> = url::form_urlencoded::parse(&request.body)
        .into_owned()
        .collect();
    let get = |name: &str| form.get(name).map(String::as_str).unwrap_or_default();
    let issue = |state: &mut State| {
        state.counter += 1;
        let access = format!("access-{}", state.counter);
        state.tokens.push(access.clone());
        json!({ "access_token": access, "expires_in": 3600, "token_type": "Bearer",
                "refresh_token": format!("refresh-{}", state.counter),
                "Key": state.protected_user_key, "PrivateKey": state.protected_private_key })
    };
    match get("grant_type") {
        "password" if get("username") == state.email && get("password") == state.hash => {
            ok(issue(state))
        }
        "password" => (
            400,
            Body::Json(
                json!({ "error": "invalid_grant", "error_description": "invalid_username_or_password" }),
            ),
        ),
        "refresh_token" if get("refresh_token").starts_with("refresh-") => ok(issue(state)),
        _ => (400, Body::Json(json!({ "error": "invalid_grant" }))),
    }
}

fn attachment_json(a: &Attachment, base: &str) -> Value {
    json!({ "id": a.id, "fileName": a.file_name, "key": a.key, "size": a.size.to_string(),
            "sizeName": format!("{} Bytes", a.size),
            "url": format!("{base}/attachments/{}/{}?token=t", a.cipher, a.id), "object": "attachment" })
}

fn sync(state: &State, base: &str) -> Value {
    let ciphers: Vec<Value> = state
        .ciphers
        .iter()
        .map(|cipher| {
            let mut cipher = cipher.clone();
            let id = cipher["id"].clone();
            let attachments: Vec<Value> = state
                .attachments
                .iter()
                .filter(|a| json!(a.cipher) == id && a.data.is_some())
                .map(|a| attachment_json(a, base))
                .collect();
            cipher["attachments"] = if attachments.is_empty() {
                Value::Null
            } else {
                Value::Array(attachments)
            };
            cipher
        })
        .collect();
    let orgs: Vec<Value> = state
        .orgs
        .iter()
        .map(|o| {
            json!({ "id": o.id, "name": o.name, "key": o.key, "type": 0, "status": 2, "enabled": true,
                    "organizationUserId": o.membership, "object": "profileOrganization" })
        })
        .collect();
    json!({
        "object": "sync",
        "profile": { "id": "me", "email": state.email, "name": "Nyu",
                     "key": state.protected_user_key, "privateKey": state.protected_private_key,
                     "organizations": orgs, "object": "profile" },
        "folders": state.folders,
        "collections": state.collections,
        "ciphers": ciphers,
        "sends": state.sends,
        "domains": null,
        "policies": [],
    })
}

fn save_cipher(
    state: &mut State,
    data: Value,
    collections: Vec<String>,
    base: &str,
) -> (u16, Body) {
    let _ = base;
    let kind = data["type"].as_u64().unwrap_or(0);
    let type_key = match kind {
        1 => "login",
        2 => "secureNote",
        3 => "card",
        4 => "identity",
        5 => "sshKey",
        _ => return refused("Invalid type"),
    };
    if !data[type_key].is_object() || !data["name"].as_str().is_some_and(|n| n.starts_with("2.")) {
        return refused("Data missing");
    }
    if let Some(org) = data["organizationId"].as_str() {
        if !state.orgs.iter().any(|o| o.id == org) {
            return refused("Not a member of that organization.");
        }
        let in_org = |id: &String| {
            state
                .collections
                .iter()
                .any(|c| c["id"] == json!(id) && c["organizationId"] == json!(org))
        };
        if collections.is_empty() || !collections.iter().all(in_org) {
            return refused("An organization item needs its organization's collections.");
        }
    }
    if let Some(folder) = data["folderId"].as_str() {
        if !state.folders.iter().any(|f| f["id"] == json!(folder)) {
            return refused("Invalid folder.");
        }
    }
    let id = next_id(state, "cipher");
    let mut cipher = data;
    cipher["id"] = json!(id);
    cipher["collectionIds"] = json!(collections);
    cipher["revisionDate"] = json!(DATE);
    cipher["creationDate"] = json!(DATE);
    cipher["deletedDate"] = Value::Null;
    cipher["edit"] = json!(true);
    cipher["viewPassword"] = json!(true);
    cipher["object"] = json!("cipherDetails");
    if let Some(map) = cipher.as_object_mut() {
        map.remove("lastKnownRevisionDate");
    }
    state.ciphers.push(cipher.clone());
    ok(cipher)
}

fn create_org(state: &mut State, body: Value) -> (u16, Body) {
    if !matches!(state.kind, Kind::UwuLock { families: true }) {
        return (
            403,
            Body::Json(json!({ "message": "You may not create a family." })),
        );
    }
    if body["planType"] != json!(22) {
        return refused("Invalid plan.");
    }
    let (Some(name), Some(key), Some(public), Some(private), Some(collection)) = (
        body["name"].as_str(),
        body["key"].as_str(),
        body["keys"]["publicKey"].as_str(),
        body["keys"]["encryptedPrivateKey"].as_str(),
        body["collectionName"].as_str(),
    ) else {
        return refused("name, key, keys and collectionName are required");
    };
    if !key.starts_with("4.") || !private.starts_with("2.") || B64.decode(public).is_err() {
        return refused("The organization's keys aren't what they should be.");
    }
    if name.chars().count() > 50 || !state.orgs.is_empty() {
        return refused("You can't create another family.");
    }
    let id = next_id(state, "org");
    state.orgs.push(Org {
        id: id.clone(),
        name: name.into(),
        key: key.into(),
        membership: format!("{id}-me"),
    });
    let first = next_id(state, "collection");
    state.collections.push(
        json!({ "id": first, "organizationId": id, "name": collection,
                                   "object": "collection" }),
    );
    ok(json!({ "object": "organization", "id": id, "name": name, "planType": 22 }))
}

/// The part called `data` of a `multipart/form-data` body.
fn multipart_data(request: &Request) -> Option<Vec<u8>> {
    let kind = request.headers.get("content-type")?;
    let boundary = kind.split("boundary=").nth(1)?.trim_matches('"');
    let body = &request.body;
    let start_marker = format!("--{boundary}\r\n");
    let start = find(body, start_marker.as_bytes())? + start_marker.len();
    let headers_end = find(&body[start..], b"\r\n\r\n")? + start;
    let headers = String::from_utf8_lossy(&body[start..headers_end]);
    if !headers.contains("name=\"data\"") {
        return None;
    }
    let data_start = headers_end + 4;
    let end_marker = format!("\r\n--{boundary}--");
    let end = find(&body[data_start..], end_marker.as_bytes())? + data_start;
    Some(body[data_start..end].to_vec())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
