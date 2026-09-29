//! Moving a vault from a Bitwarden to a UwULock Server: everything opens
//! under the target's keys afterwards, a second run moves nothing, and an
//! interrupted run continues where it stopped.

use crate::support::move_server::{enc, Kind, MoveServer, PASSWORD};
use serde_json::{json, Value};
use uwulock_bitwarden::api::{parse_sync, LoginOutcome, PasswordLogin};
use uwulock_bitwarden::crypto::{self, decrypt_file, decrypt_user_key, EncString, SymmetricKey};
use uwulock_bitwarden::moving::{Journal, Mover, Source, Step, Target};
use uwulock_bitwarden::{Client, Device, Error, Server, Session, Vault};
use uwulock_core::passkey::Passkey;

const SOURCE_EMAIL: &str = "nyu@example.com";
const TARGET_EMAIL: &str = "nyu@lock.example.org";
const NOW: &str = "2026-09-28T12:00:00.000Z";
const ATTACHMENT: &[u8] = b"%PDF-1.7 the cat's vaccination record";
const SEND_FILE: &[u8] = b"a photo of the cat, in bytes";

fn client(server: &MoveServer) -> Client {
    Client::new(
        Server::self_hosted(&server.url).unwrap(),
        Device::this_system("4b9f0c3e-2a51-4d7e-8f60-1c2d3e4f5a6b".into()),
    )
    .unwrap()
}

/// Logs in like the app does: prelogin, master key, password login.
async fn log_in(server: &MoveServer) -> (Client, Session, SymmetricKey) {
    let client = client(server);
    let kdf = client.prelogin(&server.email).await.unwrap();
    let master = crypto::master_key(PASSWORD, &server.email, kdf).unwrap();
    let hash = crypto::master_password_hash(&master, PASSWORD);
    let outcome = client
        .login(PasswordLogin {
            email: &server.email,
            password_hash: &hash,
            two_factor: None,
            remember_token: None,
            new_device_code: None,
        })
        .await
        .unwrap();
    let LoginOutcome::LoggedIn(session) = outcome else {
        panic!("expected a session, got {outcome:?}");
    };
    let protected: EncString = session.protected_user_key.clone().unwrap().parse().unwrap();
    let user_key = decrypt_user_key(&master, &protected).unwrap();
    (client, session, user_key)
}

/// A Bitwarden vault with a bit of everything a move has to carry.
fn source_vault() -> MoveServer {
    let source = MoveServer::start(Kind::Bitwarden, SOURCE_EMAIL);
    let user = &source.keys.user;
    source.add_folder("f-private", "Privat");

    // A login with a key of its own, a passkey, fields, history, a file.
    let item_key = SymmetricKey::generate();
    let k = &item_key;
    source.add_cipher(json!({
        "id": "c-vet", "type": 1, "organizationId": null, "folderId": "f-private",
        "key": EncString::encrypt(&item_key.to_bytes(), user).to_string(),
        "name": enc("Tierarzt", k), "notes": enc("Impfpass im Anhang", k),
        "favorite": true, "reprompt": 1,
        "login": {
            "username": enc("nyu", k), "password": enc("miau-miau-2026", k), "totp": null,
            "uris": [{ "uri": enc("https://vet.example.com/login", k), "match": null,
                       "uriChecksum": enc("checksum-of-the-address", k) }],
            "passwordRevisionDate": "2026-09-01T00:00:00.000Z",
            "fido2Credentials": [{
                "credentialId": enc("b2a5d2c1-1111-4222-8333-944455556666", k),
                "keyType": enc("public-key", k), "keyAlgorithm": enc("ECDSA", k),
                "keyCurve": enc("P-256", k), "keyValue": enc("not-a-real-key", k),
                "rpId": enc("vet.example.com", k), "userName": enc("nyu", k),
                "counter": enc("0", k), "discoverable": enc("true", k),
                "creationDate": "2026-09-02T00:00:00.000Z"
            }]
        },
        "fields": [{ "name": enc("Kundennummer", k), "value": enc("4711", k), "type": 1, "linkedId": null }],
        "passwordHistory": [{ "password": enc("miau-2025", k), "lastUsedDate": "2026-09-01T00:00:00.000Z" }]
    }));
    source.add_attachment(
        "c-vet",
        "a-vaccination",
        "impfpass.pdf",
        ATTACHMENT,
        &item_key,
    );

    // An old-style note, directly under the user key.
    source.add_cipher(json!({
        "id": "c-note", "type": 2, "organizationId": null, "folderId": null,
        "name": enc("WLAN", user), "notes": enc("miau-miau-miau", user), "secureNote": { "type": 0 }
    }));
    // In the trash: stays.
    source.add_cipher(json!({
        "id": "c-old", "type": 2, "name": enc("Alt", user), "secureNote": { "type": 0 },
        "deletedDate": "2026-09-10T00:00:00.000Z"
    }));

    // An organisation with a collection, an item in it, and one only to read.
    let org = source.add_org("o-family", "Familie Neko");
    source.add_collection("col-streaming", "o-family", "Streaming", &org);
    source.add_cipher(json!({
        "id": "c-stream", "type": 1, "organizationId": "o-family", "collectionIds": ["col-streaming"],
        "name": enc("Streaming", &org),
        "login": { "username": enc("family@example.com", &org), "password": enc("popcorn!", &org), "uris": [] },
        "edit": true, "viewPassword": true
    }));
    source.add_cipher(json!({
        "id": "c-readonly", "type": 1, "organizationId": "o-family", "collectionIds": ["col-streaming"],
        "name": enc("Nur lesen", &org),
        "login": { "username": enc("x", &org), "password": enc("y", &org), "uris": [] },
        "edit": false, "viewPassword": true
    }));

    // A text Send with a password, and a file Send without.
    let seed = crypto::generate_send_seed();
    let send_key = crypto::send_key(seed.as_ref()).unwrap();
    source.add_send(
        json!({
            "id": "s-text", "accessId": "c2VuZC10ZXh0", "type": 0,
            "name": enc("WLAN für Oma", &send_key), "notes": null,
            "key": EncString::encrypt(seed.as_ref(), user).to_string(),
            "text": { "text": enc("miau-miau-miau", &send_key), "hidden": true },
            "maxAccessCount": 3, "accessCount": 1,
            "password": "vTIDfdj3FTDbejmMf+mJWpYdMXsxfeSd1Sma3sjCtiQ=",
            "disabled": false, "hideEmail": false,
            "deletionDate": "2026-10-04T12:00:00.000Z", "expirationDate": null
        }),
        None,
    );
    let file_seed = crypto::generate_send_seed();
    let file_key = crypto::send_key(file_seed.as_ref()).unwrap();
    let file = crypto::encrypt_file(SEND_FILE, &file_key);
    source.add_send(
        json!({
            "id": "s-file", "accessId": "c2VuZC1maWxl", "type": 1,
            "name": enc("Katzenfoto", &file_key),
            "key": EncString::encrypt(file_seed.as_ref(), user).to_string(),
            "file": { "id": "file-1", "fileName": enc("nyu.jpg", &file_key), "size": file.len().to_string() },
            "maxAccessCount": null, "accessCount": 0, "password": null, "disabled": false,
            "deletionDate": "2026-10-04T12:00:00.000Z", "expirationDate": null
        }),
        Some(file),
    );
    // Ran out yesterday: stays.
    source.add_send(
        json!({
            "id": "s-gone", "type": 0, "name": enc("weg", &send_key),
            "key": EncString::encrypt(seed.as_ref(), user).to_string(),
            "text": { "text": enc("weg", &send_key) },
            "deletionDate": "2026-09-27T12:00:00.000Z"
        }),
        None,
    );
    source
}

async fn prepare(
    source: &MoveServer,
    target: &MoveServer,
    journal: Journal,
) -> Result<Mover, Error> {
    let (source_client, session, source_key) = log_in(source).await;
    let (target_client, target_session, target_key) = log_in(target).await;
    Mover::prepare(
        Source::new(source_client, session, source_key, SOURCE_EMAIL),
        Target {
            client: target_client,
            user_key: target_key,
            email: TARGET_EMAIL.into(),
        },
        &target_session.access_token,
        journal,
        NOW,
    )
    .await
}

async fn target_token(target: &MoveServer) -> String {
    log_in(target).await.1.access_token.to_string()
}

/// Runs a move to its end, or for `limit` steps.
async fn run(
    mover: &mut Mover,
    token: &str,
    limit: Option<usize>,
) -> Option<uwulock_bitwarden::moving::Summary> {
    let mut steps = 0;
    loop {
        if limit.is_some_and(|limit| steps == limit) {
            return None;
        }
        match mover.step(token).await.unwrap() {
            Step::Working(progress) => {
                steps += 1;
                assert_eq!(progress.done, steps);
                assert!(progress.done <= progress.total);
            }
            Step::Finished(summary) => return Some(summary),
        }
    }
}

fn plain(value: &Option<zeroize::Zeroizing<String>>) -> Option<&str> {
    value.as_ref().map(|s| s.as_str())
}

#[tokio::test]
async fn a_vault_moves_over_and_opens_under_the_new_keys() {
    let source = source_vault();
    let target = MoveServer::start(Kind::UwuLock { families: true }, TARGET_EMAIL);

    let mut mover = prepare(&source, &target, Journal::default()).await.unwrap();
    let preview = mover.preview().clone();
    assert!(preview.families);
    assert_eq!((preview.folders.todo, preview.organizations.todo), (1, 1));
    assert_eq!((preview.collections.todo, preview.items.todo), (1, 2 + 1));
    assert_eq!((preview.attachments.todo, preview.sends.todo), (1, 2));
    let notices: Vec<(&str, usize)> = preview.notices.iter().map(|n| (n.code, n.count)).collect();
    for expected in [
        ("trash", 1),
        ("read-only", 1),
        ("send-password", 1),
        ("send-expired", 1),
        ("send-file-counted", 1),
        ("org-members", 1),
    ] {
        assert!(notices.contains(&expected), "{expected:?} in {notices:?}");
    }

    let token = target_token(&target).await;
    let summary = run(&mut mover, &token, None).await.unwrap();
    assert!(summary.failed.is_empty(), "{:?}", summary.failed);
    assert_eq!(summary.moved.items, 3);
    assert_eq!(summary.moved.attachments, 1);
    assert_eq!(summary.moved.sends, 2);
    assert_eq!(summary.moved.organizations, 1);
    assert_eq!(summary.moved.collections, 1);
    assert_eq!(source.send_openings(), 1);

    // Everything opens with the target's keys.
    let (client, session, key) = log_in(&target).await;
    let text = client.sync(&session.access_token).await.unwrap();
    let sync = parse_sync(&text).unwrap();
    let vault = Vault::open(&sync, &key).unwrap();
    assert_eq!(vault.skipped, 0);
    assert_eq!(vault.items.len(), 3);
    assert!(vault.items.iter().all(|i| !i.broken && i.key.is_some()));
    assert_eq!(vault.folders[0].name, "Privat");
    assert_eq!(vault.organizations[0].name, "Familie Neko");
    assert_eq!(vault.collections[0].name, "Streaming");

    let vet = vault
        .items
        .iter()
        .find(|i| i.name.as_str() == "Tierarzt")
        .unwrap();
    let login = vet.login.as_ref().unwrap();
    assert_eq!(plain(&login.password), Some("miau-miau-2026"));
    assert_eq!(vet.folder_id.as_deref(), Some(vault.folders[0].id.as_str()));
    assert!(vet.favorite && vet.reprompt);
    assert_eq!(vet.password_history[0].password.as_str(), "miau-2025");
    assert_eq!(plain(&vet.fields[0].value), Some("4711"));
    let item_key = vet.key.as_ref().unwrap();
    let passkey = Passkey::open(&login.passkeys.as_ref().unwrap()[0], item_key).unwrap();
    assert_eq!(passkey.rp_id, "vet.example.com");
    assert_eq!(passkey.creation_date, "2026-09-02T00:00:00.000Z");
    let checksum: EncString = login.uris[0].checksum.as_deref().unwrap().parse().unwrap();
    assert_eq!(
        checksum.decrypt_string(item_key).unwrap().as_str(),
        "checksum-of-the-address"
    );

    // The attachment, under a new key under the new item key.
    let cipher = sync.ciphers.iter().find(|c| c.id == vet.id).unwrap();
    let attachment = &cipher.attachments[0];
    let file_key = attachment
        .key
        .as_deref()
        .unwrap()
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(item_key)
        .unwrap();
    let name: EncString = attachment.file_name.as_deref().unwrap().parse().unwrap();
    assert_eq!(
        name.decrypt_string(item_key).unwrap().as_str(),
        "impfpass.pdf"
    );
    let url = client
        .attachment_url(&session.access_token, &vet.id, &attachment.id)
        .await
        .unwrap();
    let bytes = client.download(&url, 1 << 20).await.unwrap();
    assert_eq!(
        decrypt_file(&bytes, &file_key).unwrap().as_slice(),
        ATTACHMENT
    );

    // The organisation's item, in the family's collection.
    let stream = vault
        .items
        .iter()
        .find(|i| i.name.as_str() == "Streaming")
        .unwrap();
    assert_eq!(
        stream.organization_id.as_deref(),
        Some(vault.organizations[0].id.as_str())
    );
    assert_eq!(stream.collection_ids, [vault.collections[0].id.clone()]);
    assert_eq!(
        plain(&stream.login.as_ref().unwrap().password),
        Some("popcorn!")
    );

    // The Sends, with new seeds and without the password.
    let raw: Value = serde_json::from_str(&text).unwrap();
    for send in &sync.sends {
        let seed = uwulock_core::send::open_seed(send.key.as_deref().unwrap(), &key).unwrap();
        let send_key = crypto::send_key(&seed).unwrap();
        let open = |v: &Option<String>| {
            v.as_deref()
                .unwrap()
                .parse::<EncString>()
                .unwrap()
                .decrypt_string(&send_key)
                .unwrap()
                .to_string()
        };
        let raw_send = raw["sends"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == json!(send.id))
            .unwrap();
        assert!(raw_send["password"].is_null());
        match send.kind {
            0 => {
                assert_eq!(open(&send.name), "WLAN für Oma");
                assert_eq!(open(&send.text.as_ref().unwrap().text), "miau-miau-miau");
                assert_eq!(send.text.as_ref().unwrap().hidden, Some(true));
                assert_eq!(send.max_access_count, Some(3));
            }
            _ => {
                assert_eq!(open(&send.name), "Katzenfoto");
                assert_eq!(open(&send.file.as_ref().unwrap().file_name), "nyu.jpg");
                let stored = target.send_file(&send.id).unwrap();
                assert_eq!(
                    decrypt_file(&stored, &send_key).unwrap().as_slice(),
                    SEND_FILE
                );
            }
        }
    }

    // A second run moves nothing.
    let journal = mover.journal().clone();
    let before = target.counts();
    let mut again = prepare(&source, &target, journal).await.unwrap();
    let preview = again.preview();
    assert_eq!(preview.items.moved, 3);
    for count in [
        &preview.folders,
        &preview.organizations,
        &preview.collections,
        &preview.items,
        &preview.attachments,
        &preview.sends,
    ] {
        assert_eq!(count.todo, 0);
    }
    let summary = run(&mut again, &token, None).await.unwrap();
    assert_eq!(summary.moved, Default::default());
    assert_eq!(target.counts(), before);
}

#[tokio::test]
async fn an_interrupted_move_continues_where_it_stopped() {
    let source = source_vault();
    let target = MoveServer::start(Kind::UwuLock { families: true }, TARGET_EMAIL);
    let token = target_token(&target).await;

    // Folder, family, collection, and the first item: then the app closes.
    let mut first = prepare(&source, &target, Journal::default()).await.unwrap();
    assert!(run(&mut first, &token, Some(4)).await.is_none());
    let journal = first.journal().clone();
    drop(first);
    let [folders, items, ..] = target.counts();
    assert_eq!((folders, items), (1, 1));

    let mut second = prepare(&source, &target, journal).await.unwrap();
    assert_eq!(second.preview().items.moved, 1);
    assert_eq!(second.preview().items.todo, 2);
    let summary = run(&mut second, &token, None).await.unwrap();
    assert!(summary.failed.is_empty(), "{:?}", summary.failed);
    assert_eq!(summary.moved.items, 2);
    assert_eq!(
        summary.moved.organizations, 0,
        "the family isn't made twice"
    );
    // Everything once: a folder, three items, one file, two Sends, one family
    // with one collection.
    assert_eq!(target.counts(), [1, 3, 1, 2, 1, 1]);
}

#[tokio::test]
async fn without_families_an_organisation_becomes_a_folder() {
    let source = source_vault();
    let target = MoveServer::start(Kind::UwuLock { families: false }, TARGET_EMAIL);
    let token = target_token(&target).await;

    let mut mover = prepare(&source, &target, Journal::default()).await.unwrap();
    let preview = mover.preview().clone();
    assert!(!preview.families);
    assert_eq!(preview.collections.total, 0);
    assert!(preview
        .notices
        .iter()
        .any(|n| n.code == "orgs-as-folders" && n.count == 1));
    let summary = run(&mut mover, &token, None).await.unwrap();
    assert!(summary.failed.is_empty(), "{:?}", summary.failed);

    let (client, session, key) = log_in(&target).await;
    let vault = Vault::open(
        &parse_sync(&client.sync(&session.access_token).await.unwrap()).unwrap(),
        &key,
    )
    .unwrap();
    assert!(vault.organizations.is_empty());
    let folder = vault
        .folders
        .iter()
        .find(|f| f.name == "Familie Neko")
        .unwrap();
    let stream = vault
        .items
        .iter()
        .find(|i| i.name.as_str() == "Streaming")
        .unwrap();
    assert_eq!(stream.organization_id, None);
    assert_eq!(stream.folder_id.as_deref(), Some(folder.id.as_str()));
}

#[tokio::test]
async fn only_a_uwulock_server_takes_a_move() {
    let source = source_vault();
    let target = MoveServer::start(Kind::Bitwarden, TARGET_EMAIL);
    let refused = prepare(&source, &target, Journal::default()).await;
    assert!(matches!(refused, Err(Error::Refused(_))));
}
