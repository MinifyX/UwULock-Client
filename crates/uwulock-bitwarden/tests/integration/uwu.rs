//! UwULock's extras against a fake of the `/uwu/v1` endpoints the desktop app
//! uses (UwULock-Server's `docs/uwu-api.md`): file requests from the owner's
//! side, entry versions, own and automatic icons, reminders, travel mode,
//! masked addresses, send domains, and icons fetched from a device on the
//! local network.
//!
//! The fakes check what the contract asks of a request and answer as it
//! says; the crypto on both sides is uwulock-core's, so an envelope the fake
//! "uploader" made opening on the owner's side proves the two fit.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use uwulock_bitwarden::api::parse_cipher;
use uwulock_bitwarden::crypto::{EncString, PrivateKey, SymmetricKey};
use uwulock_bitwarden::uwu::{FileRequestBody, NewMaskedAddress};
use uwulock_bitwarden::{icons, Client, Device, Server, Vault};
use uwulock_core::extras;
use uwulock_core::file_request::{self, LinkSecret, PublicInfo, Sender, SubmissionKey};

use crate::support::fake_http::{Answer, FakeHttp, Request};

const TOKEN: &str = "access-token";

fn client(url: &str) -> Client {
    Client::new(
        Server::self_hosted(url).unwrap(),
        Device::this_system("7c1d1f0e-5b1a-4f8e-9d3c-0e2b6a1c9f00".into()),
    )
    .unwrap()
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([230, 120, 170, 255]));
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn refused(status: u16, code: &str) -> Answer {
    Answer::json(
        status,
        json!({ "object": "error", "message": format!("refused: {code}"), "code": code }),
    )
}

fn authorized(request: &Request) -> bool {
    request.headers.get("authorization").map(String::as_str) == Some("Bearer access-token")
}

// ── File requests ──────────────────────────────────────────

const REQUEST_ID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

#[derive(Default)]
struct Requests {
    body: Option<Value>,
    submission: Option<Value>,
    file: Vec<u8>,
    attached: Option<Value>,
    seen: bool,
    deleted: bool,
}

#[tokio::test]
async fn a_file_request_from_making_it_to_taking_a_file_into_an_item() {
    let state = Arc::new(Mutex::new(Requests::default()));
    let shared = state.clone();
    let fake = FakeHttp::start(move |request| {
        if !authorized(request) {
            return refused(401, "unauthorized");
        }
        let mut s = shared.lock().unwrap();
        let object = |body: &Value| {
            json!({
                "object": "fileRequest",
                "id": REQUEST_ID,
                "accessId": file_request::access_id(REQUEST_ID).unwrap(),
                "name": body["name"], "linkSecret": body["linkSecret"],
                "publicInfo": body["publicInfo"],
                "passwordSet": body["passwordHash"].is_string(),
                "expirationDate": body["expirationDate"],
                "maxSubmissions": body["maxSubmissions"], "submissionCount": 1,
                "maxFiles": body["maxFiles"], "maxFileBytes": body["maxFileBytes"],
                "textAllowed": body["textAllowed"], "sendDomainId": body["sendDomainId"],
                "disabled": false, "unseen": 1, "bytes": 0,
            })
        };
        let base = format!("/uwu/v1/file-requests/{REQUEST_ID}/submissions/s1");
        match (request.method.as_str(), request.path.as_str()) {
            ("POST", "/uwu/v1/file-requests") => {
                let body = request.json();
                for key in ["name", "linkSecret", "publicInfo", "expirationDate"] {
                    assert!(body[key].is_string(), "{key} missing");
                }
                let answer = object(&body);
                s.body = Some(body);
                Answer::json(200, answer)
            }
            // Two pages, to follow the continuation token.
            ("GET", "/uwu/v1/file-requests") if request.query.is_empty() => Answer::json(
                200,
                json!({ "object": "list", "data": [object(s.body.as_ref().unwrap())], "continuationToken": "next+page" }),
            ),
            ("GET", "/uwu/v1/file-requests") => {
                assert_eq!(request.query, "continuationToken=next%2Bpage");
                Answer::json(
                    200,
                    json!({ "object": "list", "data": [], "continuationToken": null }),
                )
            }
            ("GET", path) if path.ends_with("/submissions") => Answer::json(
                200,
                json!({ "object": "list", "data": [s.submission.clone().unwrap()], "continuationToken": null }),
            ),
            ("GET", path) if path == format!("{base}/files/f1") => {
                Answer::bytes("application/octet-stream", s.file.clone())
            }
            ("POST", path) if path == format!("{base}/files/f1/attach") => {
                s.attached = Some(request.json());
                Answer::json(
                    200,
                    json!({ "id": "c-new", "object": "cipherDetails", "attachments": [{}] }),
                )
            }
            ("POST", path) if path == format!("{base}/seen") => {
                s.seen = true;
                Answer::empty(200)
            }
            ("DELETE", path) if path == base => {
                s.deleted = true;
                Answer::empty(200)
            }
            ("DELETE", path) if path == format!("/uwu/v1/file-requests/{REQUEST_ID}") => {
                Answer::empty(200)
            }
            _ => refused(404, "not_found"),
        }
    });
    let client = client(&fake.url);
    let extras_key = SymmetricKey::generate();
    let private = PrivateKey::generate().unwrap();

    // The owner makes the request.
    let secret = LinkSecret::generate();
    let info = PublicInfo::new(
        "Passport scan",
        Some("Both pages, please."),
        Some("Nyu"),
        &private.public(),
    )
    .unwrap();
    let body = FileRequestBody {
        name: file_request::seal_label("Passport for the bank", &extras_key),
        link_secret: secret.seal(&extras_key),
        public_info: info.seal(&secret).unwrap(),
        password_hash: Some(secret.password_hash("hunter2")),
        remove_password: false,
        expiration_date: "2026-10-05T12:00:00Z".into(),
        max_submissions: Some(1),
        max_files: 10,
        max_file_bytes: 1024 * 1024,
        text_allowed: true,
        send_domain_id: None,
        disabled: false,
    };
    let made = client.create_file_request(TOKEN, &body).await.unwrap();
    assert!(made.password_set);
    let sent = fake.last("POST", "/uwu/v1/file-requests").unwrap().json();
    assert!(sent.get("removePassword").is_none(), "only sent when asked");

    // Listed again, the label and the link open with the extras key.
    let list = client.file_requests(TOKEN).await.unwrap();
    assert_eq!(list.len(), 1);
    let listed = &list[0];
    assert_eq!(
        file_request::open_label(listed.name.as_deref().unwrap(), &extras_key)
            .unwrap()
            .as_str(),
        "Passport for the bank"
    );
    let again = LinkSecret::open(listed.link_secret.as_deref().unwrap(), &extras_key).unwrap();
    let link = file_request::link("https://lock.example.com", &listed.access_id, &again, false);
    assert_eq!(
        link,
        format!(
            "https://lock.example.com/#/request/{}/{}",
            file_request::access_id(REQUEST_ID).unwrap(),
            secret.to_link_part()
        )
    );

    // Somebody uploads, the way the web vault's upload page does.
    let their_info = PublicInfo::open(listed.public_info.as_deref().unwrap(), &again).unwrap();
    let key = SubmissionKey::generate();
    let (file_key, sealed_file) = key.new_file("scan.pdf");
    let contents = b"%PDF-1.7 both pages".to_vec();
    {
        let mut s = state.lock().unwrap();
        s.file = file_key.encrypt(&contents);
        s.submission = Some(json!({
            "object": "fileRequestSubmission", "id": "s1", "requestId": REQUEST_ID,
            "wrappedKey": key.wrap(&their_info.public_key().unwrap()).unwrap(),
            "sender": key.seal_sender(&Sender { name: Some("Bank".into()), email: Some("desk@bank.example".into()) }).unwrap(),
            "text": key.seal_text("Here you go.").unwrap(),
            "files": [{ "id": "f1", "fileName": sealed_file.file_name, "key": sealed_file.key, "size": s.file.len() }],
            "seen": false,
        }));
    }

    // The owner opens it with the private key.
    let submissions = client.submissions(TOKEN, REQUEST_ID).await.unwrap();
    let submission = &submissions[0];
    let opened = SubmissionKey::open(&submission.wrapped_key, &private).unwrap();
    assert_eq!(
        opened
            .open_text(submission.text.as_deref().unwrap())
            .unwrap()
            .as_str(),
        "Here you go."
    );
    let sender = opened
        .open_sender(submission.sender.as_deref().unwrap())
        .unwrap();
    assert_eq!(sender.email.as_deref(), Some("desk@bank.example"));
    let file = &submission.files[0];
    let (name, kf) = opened
        .open_file(&file_request::SealedFile {
            file_name: file.file_name.clone(),
            key: file.key.clone(),
        })
        .unwrap();
    assert_eq!(name.as_str(), "scan.pdf");
    let encrypted = client
        .submission_file(TOKEN, REQUEST_ID, "s1", "f1")
        .await
        .unwrap();
    assert_eq!(kf.decrypt(&encrypted).unwrap().as_slice(), contents);

    // Taken into an item: the name and key again, under the item's key.
    let item_key = SymmetricKey::generate();
    let for_item = kf.for_item(&name, &item_key);
    let cipher = client
        .attach_submission_file(TOKEN, REQUEST_ID, "s1", "f1", "c-new", &for_item)
        .await
        .unwrap();
    assert_eq!(cipher["id"], "c-new");
    let attached = state.lock().unwrap().attached.clone().unwrap();
    assert_eq!(attached["cipherId"], "c-new");
    let file_name: EncString = attached["fileName"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        file_name.decrypt_string(&item_key).unwrap().as_str(),
        "scan.pdf"
    );

    client
        .submission_seen(TOKEN, REQUEST_ID, "s1")
        .await
        .unwrap();
    client
        .delete_submission(TOKEN, REQUEST_ID, "s1")
        .await
        .unwrap();
    client.delete_file_request(TOKEN, REQUEST_ID).await.unwrap();
    let s = state.lock().unwrap();
    assert!(s.seen && s.deleted);
}

// ── Versions, icons, reminders, travel, masked, Sends ──────

#[tokio::test]
async fn versions_icons_reminders_masked_addresses_and_sends() {
    let user_key = SymmetricKey::generate();
    let extras_key = SymmetricKey::generate();
    let enc = |text: &str| EncString::encrypt(text.as_bytes(), &user_key).to_string();
    let version_cipher = json!({
        "type": 1, "name": enc("GitHub"), "notes": null, "key": null,
        "login": { "username": enc("nyu"), "password": enc("old password") },
        "fields": [], "passwordHistory": [], "reprompt": 0,
    });
    let icon = extras::seal_icon(&png(64, 64), &extras_key).unwrap();
    let own_icon_calls = Arc::new(Mutex::new(Vec::<usize>::new()));
    let calls = own_icon_calls.clone();
    let masked: Arc<Mutex<HashMap<String, Value>>> = Arc::default();
    let masked_state = masked.clone();
    let served_icon = icon.clone();

    let fake = FakeHttp::start(move |request| {
        let path = request.path.as_str();
        // Automatic icons need no session.
        if let Some(host) = path
            .strip_prefix("/icons/")
            .and_then(|p| p.strip_suffix("/icon.png"))
        {
            return match host {
                "github.com" => Answer::bytes("image/png", png(32, 32)),
                _ => Answer::empty(404),
            };
        }
        if !authorized(request) {
            return refused(401, "unauthorized");
        }
        let body = request.json();
        match (request.method.as_str(), path) {
            ("GET", "/uwu/v1/ciphers/c1/versions") => Answer::json(
                200,
                json!({ "object": "list", "continuationToken": null, "data": [{
                    "object": "cipherVersion", "id": "v1", "cipherId": "c1",
                    "revisionDate": "2026-09-01T10:00:00.000000Z",
                    "replacedDate": "2026-09-20T08:30:00.000000Z",
                    "size": 1834, "cipher": version_cipher,
                }]}),
            ),
            ("POST", "/uwu/v1/ciphers/c1/versions/v1/restore") => {
                if body["lastKnownRevisionDate"] == "2026-09-20T08:30:00.000000Z" {
                    Answer::json(200, json!({ "id": "c1", "object": "cipherDetails" }))
                } else {
                    refused(409, "conflict")
                }
            }
            ("DELETE", "/uwu/v1/ciphers/c1/versions/v1" | "/uwu/v1/ciphers/c1/versions") => {
                Answer::empty(200)
            }
            ("POST", "/uwu/v1/icons/own/get") => {
                let ids = body["cipherIds"].as_array().unwrap();
                calls.lock().unwrap().push(ids.len());
                let data: Vec<Value> = ids
                    .iter()
                    .filter(|id| *id == "c1")
                    .map(|_| json!({ "object": "ownIcon", "cipherId": "c1", "keyType": "extras", "data": served_icon, "revisionDate": "r1" }))
                    .collect();
                Answer::json(
                    200,
                    json!({ "object": "list", "data": data, "continuationToken": null }),
                )
            }
            ("PUT", "/uwu/v1/icons/own/c1") => {
                assert_eq!(body["keyType"], "extras");
                assert!(body["data"].as_str().unwrap().starts_with("2."));
                Answer::json(
                    200,
                    json!({ "object": "ownIcon", "cipherId": "c1", "keyType": "extras", "revisionDate": "r2" }),
                )
            }
            ("DELETE", "/uwu/v1/icons/own/c1") => Answer::empty(200),
            ("PUT", "/uwu/v1/reminders/c1") => Answer::json(
                200,
                json!({ "object": "reminder", "cipherId": "c1", "due": body["due"].as_str().unwrap_or("2027-03-01"), "everyMonths": body["everyMonths"], "isDue": false, "mailedDate": null }),
            ),
            ("DELETE", "/uwu/v1/reminders/c1") => Answer::empty(200),
            ("GET", "/uwu/v1/travel") => Answer::json(
                200,
                json!({ "object": "travelMode", "enabled": true, "enabledDate": "2026-09-27T08:00:00.000000Z", "folderIds": ["f1"], "hiddenCount": 7 }),
            ),
            ("GET", "/uwu/v1/account") => Answer::json(
                200,
                json!({ "sendDomainId": "d1", "travel": { "enabled": true }, "maskedConnected": true, "securityNoticesUnseen": 2 }),
            ),
            ("GET", "/uwu/v1/masked/connection") => Answer::json(
                200,
                json!({ "object": "maskedConnection", "connected": true, "server": "https://mail.example.com", "username": "nyu@example.com", "domains": ["masked.example.com"], "defaultDomain": "masked.example.com", "status": "ok" }),
            ),
            ("POST", "/uwu/v1/masked/addresses") => {
                assert_eq!(body["forDomain"], "https://shop.example.com");
                let address = json!({ "object": "maskedAddress", "id": "x42", "email": "quiet.otter17@masked.example.com", "state": "enabled", "forDomain": body["forDomain"], "description": body["description"], "cipherId": body["cipherId"] });
                masked_state
                    .lock()
                    .unwrap()
                    .insert("x42".into(), address.clone());
                Answer::json(200, address)
            }
            ("GET", "/uwu/v1/masked/addresses") => {
                let data: Vec<Value> = masked_state.lock().unwrap().values().cloned().collect();
                Answer::json(
                    200,
                    json!({ "object": "list", "data": data, "continuationToken": null }),
                )
            }
            ("PATCH", "/uwu/v1/masked/addresses/x42") => {
                let mut all = masked_state.lock().unwrap();
                let address = all.get_mut("x42").unwrap();
                for (key, value) in body.as_object().unwrap() {
                    address[key] = value.clone();
                }
                Answer::json(200, address.clone())
            }
            ("DELETE", "/uwu/v1/masked/addresses/x42") => {
                masked_state.lock().unwrap().remove("x42");
                Answer::empty(200)
            }
            ("PUT", "/uwu/v1/sends/s1/domain") => Answer::json(
                200,
                json!({ "object": "sendDomainChoice", "sendId": "s1", "sendDomainId": body["sendDomainId"] }),
            ),
            ("POST", "/api/sends") => {
                assert_eq!(body["authType"], 2);
                Answer::json(
                    200,
                    json!({ "id": "s1", "accessId": "abc", "object": "send" }),
                )
            }
            _ => refused(404, "not_found"),
        }
    });
    let client = client(&fake.url);

    // Versions: listed, opened like a cipher of the sync, restored.
    let versions = client.versions(TOKEN, "c1").await.unwrap();
    assert_eq!(versions.len(), 1);
    let cipher = parse_cipher(versions[0].cipher.clone(), Some("c1")).unwrap();
    let item = Vault::default()
        .open_cipher(&cipher, &user_key)
        .unwrap()
        .unwrap();
    assert_eq!(item.name.as_str(), "GitHub");
    assert_eq!(
        item.login.unwrap().password.unwrap().as_str(),
        "old password"
    );
    let stale = client
        .restore_version(TOKEN, "c1", "v1", Some("2026-01-01T00:00:00Z"))
        .await
        .unwrap_err();
    assert_eq!(stale.code(), Some("conflict"));
    let restored = client
        .restore_version(TOKEN, "c1", "v1", Some("2026-09-20T08:30:00.000000Z"))
        .await
        .unwrap();
    assert_eq!(restored["id"], "c1");
    client
        .delete_versions(TOKEN, "c1", Some("v1"))
        .await
        .unwrap();
    client.delete_versions(TOKEN, "c1", None).await.unwrap();

    // Own icons: asked for in batches of 500, opened with the extras key.
    let mut ids: Vec<String> = (0..501).map(|n| format!("x{n}")).collect();
    ids[3] = "c1".into();
    let own = client.own_icons(TOKEN, &ids).await.unwrap();
    assert_eq!(*own_icon_calls.lock().unwrap(), [500, 1]);
    assert_eq!(own.len(), 1);
    let opened = extras::open_icon(own[0].data.as_deref().unwrap(), &extras_key).unwrap();
    assert_eq!(extras::png_size(&opened), Some((64, 64)));
    let stored = client
        .put_own_icon(TOKEN, "c1", &icon, "extras")
        .await
        .unwrap();
    assert_eq!(stored.revision_date.as_deref(), Some("r2"));
    client.delete_own_icon(TOKEN, "c1").await.unwrap();

    // Automatic icons, and none.
    let icons_url = format!("{}/icons", fake.url);
    let found = client
        .automatic_icon(&icons_url, "github.com")
        .await
        .unwrap();
    assert_eq!(extras::png_size(&found.unwrap()), Some((32, 32)));
    assert_eq!(
        client
            .automatic_icon(&icons_url, "none.example.com")
            .await
            .unwrap(),
        None
    );

    // Reminders, travel mode, the account.
    let reminder = client
        .set_reminder(TOKEN, "c1", None, Some(6))
        .await
        .unwrap();
    assert_eq!(reminder.every_months, Some(6));
    client.delete_reminder(TOKEN, "c1").await.unwrap();
    let travel = client.travel(TOKEN).await.unwrap();
    assert!(travel.enabled);
    assert_eq!(travel.hidden_count, 7);
    let account = client.uwu_account(TOKEN).await.unwrap();
    assert_eq!(account.send_domain_id.as_deref(), Some("d1"));

    // Masked addresses.
    assert!(client.masked_connection(TOKEN).await.unwrap().connected);
    let address = client
        .create_masked_address(
            TOKEN,
            &NewMaskedAddress {
                for_domain: "https://shop.example.com".into(),
                description: "Shop".into(),
                cipher_id: Some("c1".into()),
                ..NewMaskedAddress::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(address.email, "quiet.otter17@masked.example.com");
    let off = client
        .update_masked_address(TOKEN, "x42", &json!({ "state": "disabled" }))
        .await
        .unwrap();
    assert_eq!(off.state, "disabled");
    assert_eq!(client.masked_addresses(TOKEN).await.unwrap().len(), 1);
    client.delete_masked_address(TOKEN, "x42").await.unwrap();
    assert!(client.masked_addresses(TOKEN).await.unwrap().is_empty());

    // A Send, then its domain.
    let send = uwulock_core::send::TextSend {
        name: "GitHub".into(),
        notes: None,
        text: zeroize::Zeroizing::new("GitHub\nUsername: nyu".into()),
        hidden: true,
        max_access_count: Some(1),
        deletion_date: "2026-09-29T12:00:00.000Z".into(),
        expiration_date: None,
        password: None,
        emails: vec![],
        hide_email: false,
    }
    .seal(&user_key)
    .unwrap();
    let made = client.create_send(TOKEN, &send.request).await.unwrap();
    assert_eq!(made["accessId"], "abc");
    client
        .set_send_domain(TOKEN, "s1", Some("d2"))
        .await
        .unwrap();
    assert_eq!(
        fake.last("PUT", "/uwu/v1/sends/s1/domain").unwrap().json()["sendDomainId"],
        "d2"
    );
}

// ── Icons from the device itself ───────────────────────────

#[tokio::test]
async fn a_device_gives_its_icon() {
    // A start page naming its icon.
    let named = FakeHttp::start(|request| {
        match request.path.as_str() {
        "/" => Answer::bytes(
            "text/html",
            br#"<html><head><link rel="icon" href="/static/logo.png" sizes="192x192"></head></html>"#
                .to_vec(),
        ),
        "/static/logo.png" => Answer::bytes("image/png", png(192, 192)),
        _ => Answer::empty(404),
    }
    });
    let icon = icons::device_icon(&format!("{}/login", named.url))
        .await
        .unwrap();
    assert_eq!(extras::png_size(&icon), Some((128, 128)));

    // Only a favicon.ico.
    let plain = FakeHttp::start(|request| match request.path.as_str() {
        "/favicon.ico" => {
            let mut ico = Cursor::new(Vec::new());
            image::RgbaImage::from_pixel(32, 32, image::Rgba([1, 2, 3, 255]))
                .write_to(&mut ico, image::ImageFormat::Ico)
                .unwrap();
            Answer::bytes("image/x-icon", ico.into_inner())
        }
        _ => Answer::empty(404),
    });
    let icon = icons::device_icon(&plain.url).await.unwrap();
    assert_eq!(extras::png_size(&icon), Some((32, 32)));
    assert_eq!(plain.calls(), ["GET /", "GET /favicon.ico"]);

    // Nothing is asked of a public host.
    assert!(icons::device_icon("https://shop.example.com")
        .await
        .is_err());
}
