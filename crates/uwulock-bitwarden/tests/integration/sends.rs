//! The account's own Sends against a fake of Bitwarden's `/api/sends`: a file
//! Send made, its file uploaded and opened again, a text Send changed under
//! its own seed, its password removed, and both deleted. The Send's crypto is
//! uwulock-core's; what goes over the wire has the shape Bitwarden and
//! UwULock Server read.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use uwulock_bitwarden::api::parse_sync;
use uwulock_bitwarden::crypto::{decrypt_file, send_key, SymmetricKey};
use uwulock_bitwarden::send::{self, SendAuth, SendDraft};
use uwulock_bitwarden::{Client, Device, Server, Vault};
use zeroize::Zeroizing;

use crate::support::fake_http::{Answer, FakeHttp};

const TOKEN: &str = "access-token";

#[derive(Default)]
struct Sends {
    list: Vec<Value>,
    files: Vec<(String, Vec<u8>)>,
}

/// The sync as the server writes it: what was posted, with ids.
fn sync_of(sends: &[Value]) -> String {
    json!({
        "profile": { "email": "nyu@example.com", "organizations": [] },
        "folders": [], "collections": [], "ciphers": [],
        "sends": sends,
    })
    .to_string()
}

fn fake() -> (FakeHttp, Arc<Mutex<Sends>>) {
    let state = Arc::new(Mutex::new(Sends::default()));
    let shared = state.clone();
    let fake = FakeHttp::start(move |request| {
        if request.headers.get("authorization").map(String::as_str) != Some("Bearer access-token") {
            return Answer::json(401, json!({ "message": "unauthorized" }));
        }
        let mut s = shared.lock().unwrap();
        let id_of = |path: &str| path.split('/').nth(3).unwrap_or_default().to_string();
        match (request.method.as_str(), request.path.as_str()) {
            ("POST", "/api/sends") | ("POST", "/api/sends/file/v2") => {
                let mut send = request.json();
                let id = format!("send-{}", s.list.len() + 1);
                send["id"] = json!(id);
                send["accessId"] = json!(format!("access-{id}"));
                send["accessCount"] = json!(0);
                if let Some(file) = send.get_mut("file").filter(|f| f.is_object()) {
                    file["id"] = json!("file-1");
                    file["size"] = json!(send_size(&request.json()));
                }
                s.list.push(send.clone());
                if request.path.ends_with("v2") {
                    return Answer::json(
                        200,
                        json!({
                            "url": format!("/sends/{id}/file/file-1"),
                            "fileUploadType": 0,
                            "sendResponse": send,
                        }),
                    );
                }
                Answer::json(200, send)
            }
            ("POST", path) if path.starts_with("/api/sends/") && path.contains("/file/") => {
                s.files.push((id_of(path), request.body.clone()));
                Answer::empty(200)
            }
            ("PUT", path) if path.ends_with("/remove-password") => {
                let id = id_of(path);
                let send = s.list.iter_mut().find(|v| v["id"] == json!(id)).unwrap();
                send["password"] = Value::Null;
                send["authType"] = json!(2);
                Answer::json(200, send.clone())
            }
            ("PUT", path) => {
                let id = id_of(path);
                let mut change = request.json();
                assert_eq!(change["id"], json!(id), "a change names its Send");
                let send = s.list.iter_mut().find(|v| v["id"] == json!(id)).unwrap();
                // The password stays unless the change brings one or says nobody needs one.
                if change["password"].is_null() && change["authType"] != json!(2) {
                    change["password"] = send["password"].clone();
                }
                for key in ["accessId", "accessCount", "file"] {
                    change[key] = send[key].clone();
                }
                *send = change;
                Answer::json(200, send.clone())
            }
            ("DELETE", path) => {
                let id = id_of(path);
                s.list.retain(|v| v["id"] != json!(id));
                Answer::empty(200)
            }
            _ => Answer::json(404, json!({ "message": "no such route" })),
        }
    });
    (fake, state)
}

fn send_size(request: &Value) -> String {
    request["fileLength"].as_u64().unwrap_or(0).to_string()
}

fn client(url: &str) -> Client {
    Client::new(
        Server::self_hosted(url).unwrap(),
        Device::this_system("7c1d1f0e-5b1a-4f8e-9d3c-0e2b6a1c9f00".into()),
    )
    .unwrap()
}

fn opened(state: &Mutex<Sends>, user: &SymmetricKey) -> Vault {
    let text = sync_of(&state.lock().unwrap().list);
    Vault::open(&parse_sync(&text).unwrap(), user).unwrap()
}

#[tokio::test]
async fn sends_are_made_changed_and_deleted() {
    let (fake, state) = fake();
    let client = client(&fake.url);
    let user = SymmetricKey::generate();

    // A file Send: announced, then its encrypted file uploaded.
    let file = SendDraft {
        kind: 1,
        name: "Scan".into(),
        file_name: Some("scan.pdf".into()),
        deletion_date: "2026-10-13T12:00:00.000Z".into(),
        ..SendDraft::default()
    };
    let sealed = send::seal_draft(&file, None, &user, Some(b"%PDF-1.7 scan")).unwrap();
    let upload = client
        .create_file_send(TOKEN, &sealed.request)
        .await
        .unwrap();
    client
        .upload_file(TOKEN, &upload, "scan.pdf", &sealed.file)
        .await
        .unwrap();
    let uploaded = state.lock().unwrap().files[0].1.clone();
    assert!(
        uploaded
            .windows(sealed.file.len())
            .any(|w| w == sealed.file.as_slice()),
        "the multipart body carries the encrypted file"
    );
    let key = send_key(&sealed.seed).unwrap();
    assert_eq!(
        decrypt_file(&sealed.file, &key).unwrap().as_slice(),
        b"%PDF-1.7 scan"
    );

    // A text Send with a password.
    let text = SendDraft {
        kind: 0,
        name: "Wi-Fi".into(),
        text: Some(Zeroizing::new("hunter2".into())),
        password: Some(Zeroizing::new("pw".into())),
        auth_type: Some(1),
        deletion_date: "2026-10-13T12:00:00.000Z".into(),
        ..SendDraft::default()
    };
    let sealed = send::seal_draft(&text, None, &user, None).unwrap();
    client.create_send(TOKEN, &sealed.request).await.unwrap();

    let vault = opened(&state, &user);
    assert_eq!(vault.sends.len(), 2);
    let scan = vault.sends.iter().find(|s| s.kind == 1).unwrap();
    assert_eq!(
        (scan.name.as_str(), scan.file_name.as_deref()),
        ("Scan", Some("scan.pdf"))
    );
    let wifi = vault.sends.iter().find(|s| s.kind == 0).unwrap().clone();
    assert!(wifi.has_password);
    assert_eq!(wifi.auth, SendAuth::Password);

    // Changed: the same seed, so the link the person shared still opens it.
    let change = SendDraft {
        name: "Wi-Fi (guest)".into(),
        text: Some(Zeroizing::new("guest-pass".into())),
        hide_email: true,
        max_access_count: Some(5),
        auth_type: Some(1),
        password: None,
        ..text.clone()
    };
    let sealed = send::seal_draft(&change, Some(&wifi), &user, None).unwrap();
    client
        .update_send(TOKEN, &wifi.id, &sealed.request)
        .await
        .unwrap();
    let vault = opened(&state, &user);
    let changed = vault.sends.iter().find(|s| s.id == wifi.id).unwrap();
    assert_eq!(changed.name, "Wi-Fi (guest)");
    assert_eq!(changed.readable(), Some("guest-pass"));
    assert_eq!(changed.seed.as_slice(), wifi.seed.as_slice());
    assert!(changed.hide_email && changed.has_password);
    assert_eq!(changed.max_access_count, Some(5));

    client
        .remove_send_auth(TOKEN, &wifi.id, false)
        .await
        .unwrap();
    let vault = opened(&state, &user);
    let open_to_all = vault.sends.iter().find(|s| s.id == wifi.id).unwrap();
    assert!(!open_to_all.has_password);
    assert_eq!(open_to_all.auth, SendAuth::None);

    for send in &vault.sends {
        client.delete_send(TOKEN, &send.id).await.unwrap();
    }
    assert!(opened(&state, &user).sends.is_empty());
    assert!(fake
        .calls()
        .contains(&format!("PUT /api/sends/{}/remove-password", wifi.id)));
}
