//! A suite app's way (contract §3, §6) against a small fake of UwULock
//! Server's `/uwu/v1/keys` and `/uwu/v1/suite`: the extras key made by the
//! first app, opened by the next, wrapped again after an official client
//! rotated the user key; the space made once and taken by everyone after;
//! a push with a conflict and a pull.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use uwulock_bitwarden::crypto::{PrivateKey, SymmetricKey};
use uwulock_bitwarden::suite::{Clock, Envelope};
use uwulock_bitwarden::{App, Client, Device, Server};

#[derive(Default)]
struct Fake {
    extras: Option<Value>,
    spaces: Vec<Value>,
    records: HashMap<String, Value>,
    seq: u64,
    user_wraps: u32,
}

fn start(fake: Arc<Mutex<Fake>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let fake = fake.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let path = parts.next().unwrap_or_default().to_string();
                let mut length = 0;
                let mut bearer = String::new();
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    let header = header.trim_end();
                    if header.is_empty() {
                        break;
                    }
                    let (name, value) = header.split_once(':').unwrap();
                    match name.to_lowercase().as_str() {
                        "content-length" => length = value.trim().parse().unwrap(),
                        "authorization" => bearer = value.trim().to_string(),
                        _ => {}
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let (status, answer) = if bearer != "Bearer suite-token" {
                    (
                        401,
                        json!({ "message": "Unauthorized", "code": "unauthorized" }),
                    )
                } else {
                    route(&mut fake.lock().unwrap(), &method, &path, body)
                };
                let text = answer.to_string();
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                );
            });
        }
    });
    url
}

fn route(fake: &mut Fake, method: &str, path: &str, body: Value) -> (u16, Value) {
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    match (method, path) {
        ("GET", "/uwu/v1/keys") => (
            200,
            json!({ "object": "uwuKeys", "extrasKey": fake.extras, "lost": false }),
        ),
        ("POST", "/uwu/v1/keys") => {
            if fake.extras.is_some() {
                return (409, json!({ "message": "exists", "code": "exists" }));
            }
            fake.extras = Some(body);
            (
                200,
                json!({ "object": "uwuKeys", "extrasKey": fake.extras, "lost": false }),
            )
        }
        ("PUT", "/uwu/v1/keys/user-wrap") => {
            let extras = fake.extras.as_mut().unwrap();
            if !extras["userKeyWrapped"].is_null() {
                return (409, json!({ "message": "exists", "code": "exists" }));
            }
            extras["userKeyWrapped"] = body["userKeyWrapped"].clone();
            fake.user_wraps += 1;
            (
                200,
                json!({ "object": "uwuKeys", "extrasKey": fake.extras, "lost": false }),
            )
        }
        ("GET", "/uwu/v1/suite/spaces") => (200, json!({ "object": "list", "data": fake.spaces })),
        ("PUT", "/uwu/v1/suite/spaces/ssh") => {
            if !fake.spaces.is_empty() {
                return (409, json!({ "message": "exists", "code": "exists" }));
            }
            let space = json!({ "object": "suiteSpace", "space": "ssh", "id": body["id"], "key": body["key"], "records": 0, "bytes": 0 });
            fake.spaces.push(space.clone());
            (200, space)
        }
        ("POST", "/uwu/v1/suite/spaces/ssh/records") => {
            assert_eq!(body["schema"], 2);
            let (mut accepted, mut conflicts) = (vec![], vec![]);
            for record in body["records"].as_array().unwrap() {
                let id = record["id"].as_str().unwrap().to_string();
                let stored = fake
                    .records
                    .get(&id)
                    .and_then(|r| r["seq"].as_u64())
                    .unwrap_or(0);
                if record["baseSeq"].as_u64() != Some(stored) {
                    conflicts.push(fake.records[&id].clone());
                    continue;
                }
                fake.seq += 1;
                let mut record = record.clone();
                record["seq"] = json!(fake.seq);
                fake.records.insert(id.clone(), record);
                accepted.push(json!({ "id": id, "seq": fake.seq }));
            }
            (
                200,
                json!({ "object": "suitePush", "accepted": accepted, "conflicts": conflicts, "cursor": fake.seq }),
            )
        }
        ("GET", "/uwu/v1/suite/spaces/ssh/records") => {
            let since: u64 = query
                .split('&')
                .find_map(|p| p.strip_prefix("since="))
                .unwrap()
                .parse()
                .unwrap();
            let mut records: Vec<Value> = fake
                .records
                .values()
                .filter(|r| r["seq"].as_u64().unwrap() > since)
                .cloned()
                .collect();
            records.sort_by_key(|r| r["seq"].as_u64());
            (
                200,
                json!({ "object": "suitePull", "reset": false, "records": records, "cursor": fake.seq, "hasMore": false }),
            )
        }
        _ => (404, json!({ "message": "Not found", "code": "not_found" })),
    }
}

fn record(id: &str, base_seq: u64) -> Envelope {
    Envelope {
        id: id.into(),
        kind: "host".into(),
        updated_at: Clock {
            wall_ms: 1_790_000_000_000,
            counter: 0,
            device: 7,
        },
        base_seq,
        deleted: false,
        nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
        blob: "c2VhbGVk".into(),
        seq: None,
    }
}

#[tokio::test]
async fn a_suite_app_gets_its_keys_and_syncs_its_space() {
    let fake = Arc::new(Mutex::new(Fake::default()));
    let url = start(fake.clone());
    let device = Device {
        id: "5b0c1e2d-0000-4000-8000-00000000abcd".into(),
        name: "UwUSSH".into(),
        kind: 8,
    };
    let client = Client::new(Server::self_hosted(&url).unwrap(), device)
        .unwrap()
        .with_app(App::suite("uwussh"));
    let token = "suite-token";
    let user_key = SymmetricKey::generate();
    let private = PrivateKey::generate().unwrap();

    // The first app makes the extras key; the next call opens the same one.
    let made = client
        .extras_key(token, &user_key, Some(&private))
        .await
        .unwrap()
        .unwrap();
    let again = client
        .extras_key(token, &user_key, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(made.to_bytes(), again.to_bytes());

    // An official client rotates the user key: the server drops the user
    // wrap; the private key opens it and it is wrapped for the new key.
    let rotated = SymmetricKey::generate();
    fake.lock().unwrap().extras.as_mut().unwrap()["userKeyWrapped"] = Value::Null;
    let opened = client
        .extras_key(token, &rotated, Some(&private))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(opened.to_bytes(), made.to_bytes());
    assert_eq!(fake.lock().unwrap().user_wraps, 1);
    assert!(client
        .extras_key(token, &rotated, None)
        .await
        .unwrap()
        .is_some());

    // The space: made once, then the same id and key for everyone.
    let (id, key) = client.suite_space(token, "ssh", &opened).await.unwrap();
    let (id2, key2) = client.suite_space(token, "ssh", &opened).await.unwrap();
    assert_eq!(
        (id.as_str(), key.as_bytes()),
        (id2.as_str(), key2.as_bytes())
    );

    let pushed = client
        .suite_push(token, "ssh", &[record("a", 0), record("b", 0)])
        .await
        .unwrap();
    assert_eq!(pushed.accepted.len(), 2);
    // Another device changed "a" meanwhile: a push from an old base conflicts.
    let second = client
        .suite_push(token, "ssh", &[record("a", 0)])
        .await
        .unwrap();
    assert!(second.accepted.is_empty());
    assert_eq!(second.conflicts[0].seq, Some(1));
    let pull = client.suite_pull(token, "ssh", 1).await.unwrap();
    assert_eq!(pull.records.len(), 1);
    assert_eq!(pull.records[0].id, "b");
    assert_eq!(pull.cursor, 2);

    // The wrong token is a refusal with the contract's code.
    let refused = client.suite_spaces("nope").await.unwrap_err();
    assert_eq!(refused.code(), Some("unauthorized"));
}
