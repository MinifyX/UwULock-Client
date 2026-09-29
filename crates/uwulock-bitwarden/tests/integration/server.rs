//! Against a real UwULock Server, not a fake: what the fakes can't prove —
//! that the client and the server read the contract the same way.
//!
//! Ignored by default. `scripts/live-server.sh <UwULock-Server checkout>`
//! starts a server with a test CA, registers one account per test
//! (`<name>@example.com`) and runs these with `--ignored`; it sets
//! `UWULOCK_TEST_SERVER`, `UWULOCK_TEST_PASSWORD` and `SSL_CERT_FILE`, and
//! for the move `UWULOCK_TEST_VAULTWARDEN` (a filled Vaultwarden).

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::{json, Value};
use std::time::Duration;
use uwulock_bitwarden::api::{parse_sync, LoginOutcome, PasswordLogin};
use uwulock_bitwarden::crypto::{
    self, decrypt_file, decrypt_user_key, EncString, PrivateKey, PublicKey, SymmetricKey,
};
use uwulock_bitwarden::delta::Synced;
use uwulock_bitwarden::live::{Channel, Closed, Event, Hub, Realtime};
use uwulock_bitwarden::uwu::FileRequestBody;
use uwulock_bitwarden::vault::{Item, Secret};
use uwulock_bitwarden::{
    extras, file_request, send, Client, Device, ItemKind, Server, Session, Vault,
};

/// A 1 × 1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

fn env(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} isn't set: run these through scripts/live-server.sh"))
}

fn new_device() -> String {
    let bytes: [u8; 16] = rand_bytes();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-4{}-a{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

fn rand_bytes<const N: usize>() -> [u8; N] {
    let key = SymmetricKey::generate();
    let mut out = [0u8; N];
    out.copy_from_slice(&key.to_bytes()[..N]);
    out
}

/// One device of one account, logged in and unlocked.
struct Live {
    client: Client,
    session: Session,
    user_key: SymmetricKey,
    email: String,
    password: String,
    device: String,
}

impl Live {
    /// `<name>@example.com` on a device of its own.
    async fn log_in(name: &str) -> Live {
        Live::log_in_with(
            name,
            &env("UWULOCK_TEST_PASSWORD"),
            &env("UWULOCK_TEST_SERVER"),
        )
        .await
    }

    async fn log_in_with(name: &str, password: &str, url: &str) -> Live {
        let email = if name.contains('@') {
            name.to_string()
        } else {
            format!("{name}@example.com")
        };
        let device = new_device();
        let client = Client::new(
            Server::self_hosted(url).unwrap(),
            Device::this_system(device.clone()),
        )
        .unwrap();
        let kdf = client.prelogin(&email).await.unwrap();
        let master = crypto::master_key(password, &email, kdf).unwrap();
        let hash = crypto::master_password_hash(&master, password);
        let outcome = client
            .login(PasswordLogin {
                email: &email,
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
        Live {
            client,
            session,
            user_key,
            email,
            password: password.to_string(),
            device,
        }
    }

    fn token(&self) -> &str {
        &self.session.access_token
    }

    /// A Bitwarden call the crate has no function for (they are the official
    /// clients' business): only to set the scene.
    async fn api_post(&self, path: &str, body: &Value) {
        let response = reqwest::Client::new()
            .post(format!("{}{path}", self.client.server().web()))
            .bearer_auth(self.token())
            .json(body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        assert!(
            status.is_success(),
            "{path}: {status} {}",
            response.text().await.unwrap()
        );
    }

    async fn vault(&self) -> Vault {
        let text = self.client.sync(self.token()).await.unwrap();
        Vault::open(&parse_sync(&text).unwrap(), &self.user_key).unwrap()
    }

    async fn private_key(&self) -> PrivateKey {
        self.vault().await.private_key().unwrap().clone()
    }

    async fn extras(&self) -> SymmetricKey {
        let private = self.private_key().await;
        self.client
            .extras_key(self.token(), &self.user_key, Some(&private))
            .await
            .unwrap()
            .expect("an extras key")
    }

    /// A new login item; its id.
    async fn add_login(&self, name: &str, password: &str) -> String {
        let mut item = Item::new(ItemKind::Login);
        item.name = Secret::new(name.to_string());
        item.set_password(
            Secret::new(password.to_string()),
            "2026-09-29T10:00:00.000Z",
        );
        let answer = self
            .client
            .create_cipher(self.token(), item.seal(&self.user_key).unwrap(), &[])
            .await
            .unwrap();
        answer["id"].as_str().unwrap().to_string()
    }

    /// Changes an item's password; the answer's revision date.
    async fn change_password(&self, id: &str, password: &str) -> String {
        let vault = self.vault().await;
        let mut item = vault.item(id).unwrap().clone();
        item.set_password(
            Secret::new(password.to_string()),
            "2026-09-29T11:00:00.000Z",
        );
        let answer = self
            .client
            .update_cipher(self.token(), id, item.seal(&self.user_key).unwrap())
            .await
            .unwrap();
        answer["revisionDate"].as_str().unwrap().to_string()
    }

    /// The delta sync the way the desktop app runs it: pages until
    /// `hasMore` is false, an unreadable cursor dropped for a full sync.
    async fn delta(&self, synced: &mut Synced) -> Vec<Value> {
        let mut pages = Vec::new();
        loop {
            let page = match self
                .client
                .uwu_sync(self.token(), synced.cursor.as_deref())
                .await
            {
                Ok(page) => page,
                Err(error) if error.code() == Some("invalid") && synced.cursor.is_some() => {
                    synced.cursor = None;
                    continue;
                }
                Err(error) => panic!("delta sync: {error}"),
            };
            let more = synced.apply(&page).unwrap();
            pages.push(page);
            if !more {
                return pages;
            }
        }
    }
}

fn names_of(vault: &Vault) -> Vec<String> {
    let mut names: Vec<String> = vault
        .items
        .iter()
        .filter(|i| !i.deleted)
        .map(|i| i.name.as_str().to_string())
        .collect();
    names.sort();
    names
}

fn password_of(item: &Item) -> String {
    item.login
        .as_ref()
        .and_then(|l| l.password.as_ref())
        .map(|p| p.as_str().to_string())
        .unwrap_or_default()
}

async fn next_event(channel: &mut Channel) -> Result<Event, Closed> {
    tokio::time::timeout(Duration::from_secs(15), channel.next())
        .await
        .expect("an event within 15 seconds")
}

/// The next event that isn't `info` (admin settings may change any time).
async fn next_news(channel: &mut Channel) -> Result<Event, Closed> {
    loop {
        match next_event(channel).await {
            Ok(Event::Info) => continue,
            other => return other,
        }
    }
}

// ── Delta sync (§4) ────────────────────────────────────────

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn delta_sync_merges_into_the_offline_copy() {
    let nyu = Live::log_in("sync").await;
    let mut synced = Synced::default();
    let pages = nyu.delta(&mut synced).await;
    assert_eq!(pages[0]["reset"], true, "the first sync is a full one");
    assert!(pages[0]["uwu"]["unseen"].is_object());
    let first_cursor = synced.cursor.clone().unwrap();

    // Three new items, a page of two at a time: `hasMore`, followed.
    let ids = [
        nyu.add_login("Router", "r1").await,
        nyu.add_login("NAS", "n1").await,
        nyu.add_login("Drucker", "d1").await,
    ];
    let path = format!(
        "/sync?include=vault,uwu&limit=2&since={}",
        uwulock_bitwarden::uwu::uwu_path(&first_cursor)
    );
    let page = nyu.client.uwu_get(nyu.token(), &path).await.unwrap();
    assert_eq!(page["reset"], false);
    assert_eq!(page["hasMore"], true, "{page}");
    assert_eq!(page["vault"]["ciphers"].as_array().unwrap().len(), 2);
    assert!(synced.apply(&page).unwrap());
    nyu.delta(&mut synced).await;
    let vault = Vault::open(&parse_sync(&synced.sync_text()).unwrap(), &nyu.user_key).unwrap();
    assert_eq!(names_of(&vault), ["Drucker", "NAS", "Router"]);

    // A change, the trash, a delete for good, a folder.
    nyu.change_password(&ids[0], "r2").await;
    nyu.client.trash_cipher(nyu.token(), &ids[1]).await.unwrap();
    nyu.client
        .delete_cipher(nyu.token(), &ids[2])
        .await
        .unwrap();
    let folder = EncString::encrypt(b"Keller", &nyu.user_key).to_string();
    nyu.client.create_folder(nyu.token(), folder).await.unwrap();
    let pages = nyu.delta(&mut synced).await;
    let deleted = &pages.last().unwrap()["vault"]["deleted"]["ciphers"];
    assert_eq!(deleted, &json!([ids[2]]), "{deleted}");

    // The offline copy is now what a full sync says.
    let merged = Vault::open(&parse_sync(&synced.sync_text()).unwrap(), &nyu.user_key).unwrap();
    let full = nyu.vault().await;
    assert_eq!(names_of(&merged), names_of(&full));
    assert_eq!(names_of(&merged), ["Router"]);
    assert_eq!(password_of(merged.item(&ids[0]).unwrap()), "r2");
    assert!(merged.item(&ids[1]).unwrap().deleted, "in the trash");
    assert!(merged.item(&ids[2]).is_none());
    assert_eq!(merged.folders.len(), full.folders.len());
    assert_eq!(merged.folders[0].name, "Keller");

    // Nothing new: an empty delta, the cursor moves on (or stays).
    let pages = nyu.delta(&mut synced).await;
    assert_eq!(pages.len(), 1);
    assert!(pages[0]["vault"]["ciphers"].as_array().unwrap().is_empty());

    // A cursor the server can't read: 400 `invalid`, then a full sync.
    let error = nyu
        .client
        .uwu_sync(nyu.token(), Some("not-a-cursor"))
        .await
        .unwrap_err();
    assert_eq!(
        (error.status(), error.code()),
        (Some(400), Some("invalid")),
        "{error}"
    );
    let mut stale = synced.clone();
    stale.cursor = Some("bm90LWEtY3Vyc29y".into());
    let pages = nyu.delta(&mut stale).await;
    assert_eq!(pages[0]["reset"], true);
    assert_eq!(stale.sync["ciphers"], synced.sync["ciphers"]);

    // The same cursor with another include set: a full sync, reset.
    let path = format!(
        "/sync?include=vault&since={}",
        uwulock_bitwarden::uwu::uwu_path(synced.cursor.as_deref().unwrap())
    );
    let page = nyu.client.uwu_get(nyu.token(), &path).await.unwrap();
    assert_eq!(page["reset"], true, "another include set");
}

// ── Realtime channel (§5) and Bitwarden's hub ──────────────

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn realtime_channel_announces_changes_resumes_and_ends_with_the_session() {
    let phone = Live::log_in("realtime").await;
    let desk = Live::log_in("realtime").await;
    let mut synced = Synced::default();
    desk.delta(&mut synced).await;

    let server = desk.client.server().clone();
    let mut channel = Channel::Realtime(
        Realtime::connect(&server, desk.token(), synced.cursor.as_deref())
            .await
            .unwrap(),
    );
    let expires = channel.expires().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(expires > now && expires < now + 86_400, "expires {expires}");

    // Another device changes something: news.
    let id = phone.add_login("Router", "r1").await;
    match next_news(&mut channel).await.unwrap() {
        Event::Changed { areas } => assert!(areas.contains(&"vault".to_string()), "{areas:?}"),
        other => panic!("expected a change, got {other:?}"),
    }

    // A fresh token on the same connection, then still news.
    let refreshed = desk
        .client
        .refresh(desk.session.refresh_token.as_ref().unwrap())
        .await
        .unwrap();
    channel
        .reauth(&refreshed.access_token, synced.cursor.as_deref())
        .await
        .unwrap();
    phone.change_password(&id, "r2").await;
    match next_news(&mut channel).await.unwrap() {
        Event::Changed { areas } => assert!(areas.contains(&"vault".to_string())),
        other => panic!("expected a change, got {other:?}"),
    }

    // Resume: connecting with a cursor from before a change says so at once.
    let old_cursor = synced.cursor.clone();
    let mut resumed = Channel::Realtime(
        Realtime::connect(&server, &refreshed.access_token, old_cursor.as_deref())
            .await
            .unwrap(),
    );
    match next_news(&mut resumed).await.unwrap() {
        Event::Changed { .. } => {}
        other => panic!("expected a change right after ready, got {other:?}"),
    }
    resumed.close().await;
    desk.delta(&mut synced).await;
    let mut current = Channel::Realtime(
        Realtime::connect(&server, &refreshed.access_token, synced.cursor.as_deref())
            .await
            .unwrap(),
    );

    // A token that isn't one: 4401.
    let refused = Realtime::connect(&server, "not.a.token", None)
        .await
        .err()
        .unwrap();
    assert!(refused.needs_token(), "{refused}");

    // The security stamp changes (another device logs everyone out): logout, then 4401.
    let kdf = phone.client.prelogin(&phone.email).await.unwrap();
    let master = crypto::master_key(&phone.password, &phone.email, kdf).unwrap();
    let hash = crypto::master_password_hash(&master, &phone.password);
    phone
        .api_post(
            "/api/accounts/security-stamp",
            &json!({ "masterPasswordHash": hash }),
        )
        .await;
    for channel in [&mut channel, &mut current] {
        // Changes not taken yet may come first.
        loop {
            match next_news(channel).await {
                Ok(Event::Changed { .. }) => continue,
                Ok(Event::LogOut { reason }) => break assert_eq!(reason, "securityStamp"),
                other => panic!("expected a logout, got {other:?}"),
            }
        }
        let closed = next_event(channel).await.unwrap_err();
        assert!(closed.needs_token(), "{closed}");
    }
}

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn bitwardens_hub_announces_changes_too() {
    let phone = Live::log_in("hub").await;
    let desk = Live::log_in("hub").await;
    let server = desk.client.server().clone();
    let mut hub = Channel::Hub(
        Hub::connect(&server, desk.token(), &desk.device)
            .await
            .unwrap(),
    );
    // The hub has no "ready": give the handshake a moment to be answered by
    // making the change only after a ping went through.
    phone.add_login("Router", "r1").await;
    match next_news(&mut hub).await.unwrap() {
        Event::Changed { areas } => assert!(areas.contains(&"vault".to_string()), "{areas:?}"),
        other => panic!("expected a change, got {other:?}"),
    }
    hub.close().await;
}

// ── The extras key (§3) ────────────────────────────────────

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn two_devices_racing_for_the_extras_key_end_with_the_same_one() {
    let one = Live::log_in("keys").await;
    let two = Live::log_in("keys").await;
    let (a, b) = tokio::join!(one.extras(), two.extras());
    assert_eq!(*a.to_bytes(), *b.to_bytes());
    // A third call opens it with the user key alone.
    let again = one
        .client
        .extras_key(one.token(), &one.user_key, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*again.to_bytes(), *a.to_bytes());
    // Made twice on purpose: 409 `exists`.
    let private = one.private_key().await;
    let made = extras::create(&one.user_key, &private.public()).unwrap();
    let error = one
        .client
        .uwu_post(one.token(), "/keys", &made.request)
        .await
        .unwrap_err();
    assert_eq!(
        (error.status(), error.code()),
        (Some(409), Some("exists")),
        "{error}"
    );
}

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn the_extras_key_survives_an_official_rotation() {
    let nyu = Live::log_in("rotate").await;
    let extras_before = nyu.extras().await;
    let private = nyu.private_key().await;

    // A rotation as Bitwarden's clients make it: a new user key, the same
    // key pair, the master password unchanged.
    let kdf = nyu.client.prelogin(&nyu.email).await.unwrap();
    let master = crypto::master_key(&nyu.password, &nyu.email, kdf).unwrap();
    let hash = crypto::master_password_hash(&master, &nyu.password);
    let new_user_key = SymmetricKey::generate();
    let iterations = match kdf {
        crypto::Kdf::Pbkdf2 { iterations } => iterations,
        other => panic!("the test accounts use PBKDF2, not {other:?}"),
    };
    let rotation = json!({
        "oldMasterKeyAuthenticationHash": hash,
        "accountUnlockData": {
            "masterPasswordUnlockData": {
                "kdfType": 0, "kdfIterations": iterations, "kdfMemory": null, "kdfParallelism": null,
                "email": nyu.email,
                "masterKeyAuthenticationHash": hash,
                "masterKeyEncryptedUserKey": EncString::encrypt(
                    &new_user_key.to_bytes(), &SymmetricKey::stretch(&master)).to_string(),
            },
            "emergencyAccessUnlockData": [], "organizationAccountRecoveryUnlockData": [], "passkeyUnlockData": [],
        },
        "accountKeys": {
            "userKeyEncryptedAccountPrivateKey": EncString::encrypt(
                &private.to_der().unwrap(), &new_user_key).to_string(),
            "accountPublicKey": B64.encode(private.public().to_der().unwrap()),
        },
        "accountData": { "ciphers": [], "folders": [], "sends": [] },
    });
    nyu.api_post(
        "/api/accounts/key-management/rotate-user-account-keys",
        &rotation,
    )
    .await;

    // Logged in again, with the new user key: only the public-key wrap is left.
    let after = Live::log_in("rotate").await;
    assert_eq!(*after.user_key.to_bytes(), *new_user_key.to_bytes());
    let keys = after.client.uwu_get(after.token(), "/keys").await.unwrap();
    assert!(keys["extrasKey"]["userKeyWrapped"].is_null(), "{keys}");
    assert_eq!(keys["lost"], false);
    // The next client opens it with the private key and wraps it again.
    let extras_after = after.extras().await;
    assert_eq!(*extras_after.to_bytes(), *extras_before.to_bytes());
    let keys = after.client.uwu_get(after.token(), "/keys").await.unwrap();
    assert!(keys["extrasKey"]["userKeyWrapped"].is_string(), "{keys}");
    let by_user_key = after
        .client
        .extras_key(after.token(), &after.user_key, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*by_user_key.to_bytes(), *extras_before.to_bytes());
}

// ── Icons (§7) ─────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn own_icons_go_up_come_down_and_show_in_the_delta() {
    let nyu = Live::log_in("icons").await;
    let id = nyu.add_login("Kamera", "k1").await;
    let extras = nyu.extras().await;
    let mut synced = Synced::default();
    nyu.delta(&mut synced).await;

    // Another device hears of the icon in the `uwu` area.
    let other = Live::log_in("icons").await;
    let mut channel = Channel::Realtime(
        Realtime::connect(other.client.server(), other.token(), None)
            .await
            .unwrap(),
    );
    let png = B64.decode(PNG).unwrap();
    let sealed = extras::seal_icon(&png, &extras).unwrap();
    let stored = nyu
        .client
        .put_own_icon(nyu.token(), &id, &sealed, "extras")
        .await
        .unwrap();
    loop {
        match next_news(&mut channel).await.unwrap() {
            Event::Changed { areas } if areas.contains(&"uwu".to_string()) => break,
            Event::Changed { .. } => continue,
            other => panic!("expected a change in uwu, got {other:?}"),
        }
    }
    channel.close().await;
    assert_eq!(
        (stored.cipher_id.as_str(), stored.key_type.as_str()),
        (id.as_str(), "extras")
    );
    assert!(stored.revision_date.is_some());

    let icons = nyu
        .client
        .own_icons(nyu.token(), std::slice::from_ref(&id))
        .await
        .unwrap();
    assert_eq!(icons.len(), 1);
    let opened = extras::open_icon(icons[0].data.as_deref().unwrap(), &extras).unwrap();
    assert_eq!(*opened, png);

    nyu.delta(&mut synced).await;
    let icon = synced.uwu.icons.get(&id).expect("the icon in uwu.icons");
    assert_eq!(icon.key_type, "extras");

    // The wrong key type: 400 `invalid`.
    let error = nyu
        .client
        .put_own_icon(nyu.token(), &id, &sealed, "organization")
        .await
        .unwrap_err();
    assert_eq!(
        (error.status(), error.code()),
        (Some(400), Some("invalid")),
        "{error}"
    );

    nyu.client.delete_own_icon(nyu.token(), &id).await.unwrap();
    nyu.delta(&mut synced).await;
    assert!(
        !synced.uwu.icons.contains_key(&id),
        "gone with iconsDeleted"
    );
    assert!(nyu
        .client
        .own_icons(nyu.token(), &[id])
        .await
        .unwrap()
        .is_empty());

    // Automatic icons need no session; a host the server may not fetch is no icon.
    let info = nyu.client.uwu_info().await.unwrap().unwrap();
    let url = info
        .icons
        .as_ref()
        .and_then(|i| i.url.clone())
        .expect("icons.url");
    assert_eq!(
        nyu.client.automatic_icon(&url, "localhost").await.unwrap(),
        None
    );
    assert_eq!(
        nyu.client.automatic_icon(&url, "192.0.2.1").await.unwrap(),
        None
    );
}

// ── Entry versions (§8) ────────────────────────────────────

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn versions_list_restore_and_go() {
    let nyu = Live::log_in("versions").await;
    let id = nyu.add_login("Bank", "p1").await;
    nyu.change_password(&id, "p2").await;
    let current = nyu.change_password(&id, "p3").await;

    let versions = nyu.client.versions(nyu.token(), &id).await.unwrap();
    assert_eq!(versions.len(), 2, "{versions:?}");
    let vault = nyu.vault().await;
    let opened: Vec<String> = versions
        .iter()
        .map(|v| {
            let cipher = uwulock_bitwarden::api::parse_cipher(v.cipher.clone(), Some(&id)).unwrap();
            password_of(&vault.open_cipher(&cipher, &nyu.user_key).unwrap().unwrap())
        })
        .collect();
    assert_eq!(opened, ["p2", "p1"], "newest first");

    // Brought back with a stale revision date: 409 `conflict`.
    let error = nyu
        .client
        .restore_version(
            nyu.token(),
            &id,
            &versions[1].id,
            Some("2020-01-01T00:00:00.000Z"),
        )
        .await
        .unwrap_err();
    assert_eq!(
        (error.status(), error.code()),
        (Some(409), Some("conflict")),
        "{error}"
    );
    let restored = nyu
        .client
        .restore_version(nyu.token(), &id, &versions[1].id, Some(&current))
        .await
        .unwrap();
    assert_eq!(restored["id"], id.as_str());
    assert_eq!(password_of(nyu.vault().await.item(&id).unwrap()), "p1");
    let versions = nyu.client.versions(nyu.token(), &id).await.unwrap();
    assert_eq!(versions.len(), 3, "p3 became a version");

    nyu.client
        .delete_versions(nyu.token(), &id, Some(&versions[0].id))
        .await
        .unwrap();
    assert_eq!(
        nyu.client.versions(nyu.token(), &id).await.unwrap().len(),
        2
    );
    nyu.client
        .delete_versions(nyu.token(), &id, None)
        .await
        .unwrap();
    assert!(nyu
        .client
        .versions(nyu.token(), &id)
        .await
        .unwrap()
        .is_empty());
}

// ── Reminders (§10) and travel mode (§9) ───────────────────

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn reminders_and_travel_mode_come_with_the_sync() {
    let nyu = Live::log_in("reminders").await;
    let id = nyu.add_login("Mail", "m1").await;
    let mut synced = Synced::default();
    nyu.delta(&mut synced).await;
    assert!(!synced.uwu.travelling());
    let travel = nyu.client.travel(nyu.token()).await.unwrap();
    assert!(!travel.enabled);

    let due = nyu
        .client
        .set_reminder(nyu.token(), &id, Some("2020-01-01"), None)
        .await
        .unwrap();
    assert_eq!(due.due.as_deref(), Some("2020-01-01"));
    assert!(due.is_due);
    let every = nyu
        .client
        .set_reminder(nyu.token(), &id, None, Some(6))
        .await
        .unwrap();
    assert_eq!(every.every_months, Some(6));
    assert_eq!(
        every.due.as_deref(),
        Some("2027-03-29"),
        "the password's date plus six months"
    );
    nyu.delta(&mut synced).await;
    let reminder = synced
        .uwu
        .reminder(&id)
        .expect("the reminder in uwu.reminders");
    assert_eq!(reminder["everyMonths"], 6);

    let error = nyu
        .client
        .set_reminder(nyu.token(), &id, None, Some(61))
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(400), "{error}");

    nyu.client.delete_reminder(nyu.token(), &id).await.unwrap();
    nyu.delta(&mut synced).await;
    assert!(synced.uwu.reminder(&id).is_none());
}

// ── File requests (§11) ────────────────────────────────────

/// What an uploader does, as `uwulock_core::file_request` does it (the web
/// vault's page does the same in WebAssembly).
async fn upload(
    http: &reqwest::Client,
    origin: &str,
    access_id: &str,
    secret: &file_request::LinkSecret,
    password: &str,
    contents: &[u8],
) -> String {
    let public = format!("{origin}/uwu/v1/public/file-requests/{access_id}");
    let access: Value = http
        .get(&public)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(access["passwordRequired"], true, "{access}");
    let wrong = http
        .post(format!("{public}/open"))
        .json(&json!({ "passwordHash": secret.password_hash("wrong") }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status().as_u16(), 400);
    let opened: Value = http
        .post(format!("{public}/open"))
        .json(&json!({ "passwordHash": secret.password_hash(password) }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = opened["token"].as_str().unwrap().to_string();
    let info =
        file_request::PublicInfo::open(opened["publicInfo"].as_str().unwrap(), secret).unwrap();
    let key = file_request::SubmissionKey::generate();
    let (file_key, sealed) = key.new_file("scan.pdf");
    let encrypted = file_key.encrypt(contents);
    let started: Value = http
        .post(format!("{public}/submissions"))
        .bearer_auth(&token)
        .json(&json!({
            "wrappedKey": key.wrap(&info.public_key().unwrap()).unwrap(),
            "sender": key.seal_sender(&file_request::Sender {
                name: Some("Mio".into()), email: Some("mio@example.com".into()) }).unwrap(),
            "text": key.seal_text("Beide Seiten.").unwrap(),
            "files": [{ "fileName": sealed.file_name, "key": sealed.key, "size": encrypted.len() }],
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let file_url = started["files"][0]["url"].as_str().expect("a file url");
    let put = http
        .put(format!("{origin}{file_url}"))
        .bearer_auth(&token)
        .header("content-type", "application/octet-stream")
        .body(encrypted)
        .send()
        .await
        .unwrap();
    assert!(put.status().is_success(), "{}", put.text().await.unwrap());
    let id = started["id"].as_str().unwrap();
    let done = http
        .post(format!("{public}/submissions/{id}/complete"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert!(done.status().is_success(), "{}", done.text().await.unwrap());
    id.to_string()
}

fn in_days(days: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let secs = now + days * 86_400;
    // Days since 1970 to a civil date (Howard Hinnant's algorithm).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let rest = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn a_file_request_takes_an_upload_and_hands_it_over() {
    let nyu = Live::log_in("requests").await;
    let origin = env("UWULOCK_TEST_SERVER");
    let extras = nyu.extras().await;
    let private = nyu.private_key().await;
    let public: PublicKey = private.public();

    let secret = file_request::LinkSecret::generate();
    let info =
        file_request::PublicInfo::new("Reisepass", Some("Beide Seiten"), Some("Nyu"), &public)
            .unwrap();
    let body = FileRequestBody {
        name: file_request::seal_label("Pass für die Bank", &extras),
        link_secret: secret.seal(&extras),
        public_info: info.seal(&secret).unwrap(),
        password_hash: Some(secret.password_hash("miau")),
        remove_password: false,
        expiration_date: in_days(7),
        max_submissions: Some(2),
        max_files: 3,
        max_file_bytes: 1 << 20,
        text_allowed: true,
        send_domain_id: None,
        disabled: false,
    };
    let request = nyu
        .client
        .create_file_request(nyu.token(), &body)
        .await
        .unwrap();
    assert!(request.password_set);
    assert_eq!(
        request.access_id,
        file_request::access_id(&request.id).unwrap()
    );
    let listed = nyu.client.file_requests(nyu.token()).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        file_request::open_label(listed[0].name.as_deref().unwrap(), &extras)
            .unwrap()
            .as_str(),
        "Pass für die Bank"
    );
    let again =
        file_request::LinkSecret::open(listed[0].link_secret.as_deref().unwrap(), &extras).unwrap();
    let link = file_request::link(&origin, &request.access_id, &again, false);
    assert!(
        link.starts_with(&format!("{origin}/#/request/{}/", request.access_id)),
        "{link}"
    );

    // The owner's device hears of it.
    let mut channel = Channel::Realtime(
        Realtime::connect(nyu.client.server(), nyu.token(), None)
            .await
            .unwrap(),
    );
    let contents = b"%PDF-1.7 both pages of the passport";
    let http = reqwest::Client::new();
    let submission_id = upload(
        &http,
        &origin,
        &request.access_id,
        &secret,
        "miau",
        contents,
    )
    .await;
    let mut noticed = false;
    for _ in 0..3 {
        if let Event::Notice { kind, id } = next_news(&mut channel).await.unwrap() {
            assert_eq!(kind, "fileRequest");
            assert_eq!(id.as_deref(), Some(request.id.as_str()));
            noticed = true;
            break;
        }
    }
    assert!(noticed, "a fileRequest notice");
    channel.close().await;

    let mut synced = Synced::default();
    nyu.delta(&mut synced).await;
    assert_eq!(synced.uwu.unseen.file_request_submissions, 1);

    // Opened by the owner, all of it.
    let submissions = nyu
        .client
        .submissions(nyu.token(), &request.id)
        .await
        .unwrap();
    assert_eq!(submissions.len(), 1);
    let submission = &submissions[0];
    assert_eq!(submission.id, submission_id);
    let key = file_request::SubmissionKey::open(&submission.wrapped_key, &private).unwrap();
    assert_eq!(
        key.open_text(submission.text.as_deref().unwrap())
            .unwrap()
            .as_str(),
        "Beide Seiten."
    );
    let sender = key
        .open_sender(submission.sender.as_deref().unwrap())
        .unwrap();
    assert_eq!(sender.email.as_deref(), Some("mio@example.com"));
    let file = &submission.files[0];
    let (name, file_key) = key
        .open_file(&file_request::SealedFile {
            file_name: file.file_name.clone(),
            key: file.key.clone(),
        })
        .unwrap();
    assert_eq!(name.as_str(), "scan.pdf");
    let bytes = nyu
        .client
        .submission_file(nyu.token(), &request.id, &submission.id, &file.id)
        .await
        .unwrap();
    assert_eq!(&**file_key.decrypt(&bytes).unwrap(), contents);
    nyu.client
        .submission_seen(nyu.token(), &request.id, &submission.id)
        .await
        .unwrap();

    // Taken into an item: a note, the file attached under its key.
    let mut note = Item::new(ItemKind::Note);
    note.name = Secret::new("Reisepass".into());
    let answer = nyu
        .client
        .create_cipher(nyu.token(), note.seal(&nyu.user_key).unwrap(), &[])
        .await
        .unwrap();
    let cipher_id = answer["id"].as_str().unwrap().to_string();
    let vault = nyu.vault().await;
    let item = vault.item(&cipher_id).unwrap();
    let item_key = item.key.clone().unwrap_or_else(|| nyu.user_key.clone());
    let sealed = file_key.for_item("scan.pdf", &item_key);
    let attached = nyu
        .client
        .attach_submission_file(
            nyu.token(),
            &request.id,
            &submission.id,
            &file.id,
            &cipher_id,
            &sealed,
        )
        .await
        .unwrap();
    let attachment = &attached["attachments"][0];
    let attachment_id = attachment["id"].as_str().expect("an attachment");
    let url = nyu
        .client
        .attachment_url(nyu.token(), &cipher_id, attachment_id)
        .await
        .unwrap();
    let encrypted = nyu.client.download(&url).await.unwrap();
    let attachment_key = attachment["key"]
        .as_str()
        .unwrap()
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&item_key)
        .unwrap();
    assert_eq!(
        &**decrypt_file(&encrypted, &attachment_key).unwrap(),
        contents
    );

    nyu.client
        .delete_submission(nyu.token(), &request.id, &submission.id)
        .await
        .unwrap();
    assert!(nyu
        .client
        .submissions(nyu.token(), &request.id)
        .await
        .unwrap()
        .is_empty());
    nyu.client
        .delete_file_request(nyu.token(), &request.id)
        .await
        .unwrap();
    assert!(nyu
        .client
        .file_requests(nyu.token())
        .await
        .unwrap()
        .is_empty());
    let gone = http
        .get(format!(
            "{origin}/uwu/v1/public/file-requests/{}",
            request.access_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status().as_u16(), 404);
}

#[tokio::test]
#[ignore = "needs a running UwULock Server and a browser: LIVE_BROWSER=1 scripts/live-server.sh"]
async fn the_web_vaults_upload_page_speaks_uwulock_core() {
    let Ok(uploader) = std::env::var("UWULOCK_TEST_UPLOADER") else {
        eprintln!("skipped: no browser (LIVE_BROWSER=1 scripts/live-server.sh)");
        return;
    };
    let nyu = Live::log_in("uploads").await;
    let origin = env("UWULOCK_TEST_SERVER");
    let extras = nyu.extras().await;
    let private = nyu.private_key().await;
    let secret = file_request::LinkSecret::generate();
    let info = file_request::PublicInfo::new(
        "Ausweis-Scan",
        Some("Bitte beide Seiten."),
        None,
        &private.public(),
    )
    .unwrap();
    let body = FileRequestBody {
        name: file_request::seal_label("Ausweis", &extras),
        link_secret: secret.seal(&extras),
        public_info: info.seal(&secret).unwrap(),
        password_hash: Some(secret.password_hash("katzen")),
        remove_password: false,
        expiration_date: in_days(1),
        max_submissions: None,
        max_files: 2,
        // "No limit of my own", as the desktop app sends it: the server's.
        max_file_bytes: nyu
            .client
            .uwu_info()
            .await
            .unwrap()
            .and_then(|i| i.limits)
            .and_then(|l| l.max_file_bytes)
            .unwrap(),
        text_allowed: true,
        send_domain_id: None,
        disabled: false,
    };
    let request = nyu
        .client
        .create_file_request(nyu.token(), &body)
        .await
        .unwrap();
    let link = file_request::link(&origin, &request.access_id, &secret, false);

    let contents = "die Vorderseite ✧";
    let status = std::process::Command::new(&uploader)
        .args([
            &link,
            "katzen",
            "vorne.txt",
            contents,
            "Hier, wie besprochen.",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "the upload page failed");

    let submissions = nyu
        .client
        .submissions(nyu.token(), &request.id)
        .await
        .unwrap();
    assert_eq!(submissions.len(), 1);
    let submission = &submissions[0];
    let key = file_request::SubmissionKey::open(&submission.wrapped_key, &private).unwrap();
    assert_eq!(
        key.open_text(submission.text.as_deref().unwrap())
            .unwrap()
            .as_str(),
        "Hier, wie besprochen."
    );
    let sender = key
        .open_sender(submission.sender.as_deref().unwrap())
        .unwrap();
    assert_eq!(sender.name.as_deref(), Some("Mika"));
    let file = &submission.files[0];
    let (name, file_key) = key
        .open_file(&file_request::SealedFile {
            file_name: file.file_name.clone(),
            key: file.key.clone(),
        })
        .unwrap();
    assert_eq!(name.as_str(), "vorne.txt");
    let bytes = nyu
        .client
        .submission_file(nyu.token(), &request.id, &submission.id, &file.id)
        .await
        .unwrap();
    assert_eq!(bytes.len() as u64, file.size);
    assert_eq!(&**file_key.decrypt(&bytes).unwrap(), contents.as_bytes());
}

// ── Sends (§14.3) ──────────────────────────────────────────

#[tokio::test]
#[ignore = "needs a running UwULock Server: scripts/live-server.sh"]
async fn sends_carry_their_auth_type() {
    let nyu = Live::log_in("sends").await;
    let info = nyu.client.uwu_info().await.unwrap().unwrap();
    let text = |password: Option<&str>, emails: Vec<String>| send::TextSend {
        name: "WLAN".into(),
        notes: None,
        text: zeroize::Zeroizing::new("miau-miau".into()),
        hidden: false,
        max_access_count: Some(3),
        deletion_date: in_days(7),
        expiration_date: None,
        password: password.map(|p| zeroize::Zeroizing::new(p.to_string())),
        emails,
        hide_email: false,
    };

    let open = text(None, vec![]).seal(&nyu.user_key).unwrap();
    let made = nyu
        .client
        .create_send(nyu.token(), &open.request)
        .await
        .unwrap();
    assert_eq!(made["authType"], 2, "{made}");
    let with_password = text(Some("geheim"), vec![]).seal(&nyu.user_key).unwrap();
    let made = nyu
        .client
        .create_send(nyu.token(), &with_password.request)
        .await
        .unwrap();
    assert_eq!(made["authType"], 1);

    // The recipient's side: the password hash opens it, without it 401.
    let http = reqwest::Client::new();
    let access = format!(
        "{}/api/sends/access/{}",
        env("UWULOCK_TEST_SERVER"),
        made["accessId"].as_str().unwrap()
    );
    let refused = http.post(&access).json(&json!({})).send().await.unwrap();
    assert_eq!(refused.status().as_u16(), 401);
    let hash = crypto::send_password_hash("geheim", with_password.seed.as_ref());
    let opened: Value = http
        .post(&access)
        .json(&json!({ "password": hash }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let key = crypto::send_key(with_password.seed.as_ref()).unwrap();
    let plain = opened["text"]["text"]
        .as_str()
        .unwrap()
        .parse::<EncString>()
        .unwrap()
        .decrypt_string(&key)
        .unwrap();
    assert_eq!(plain.as_str(), "miau-miau");

    let for_addresses = text(None, vec!["Mio@Example.com".into()])
        .seal(&nyu.user_key)
        .unwrap();
    let answer = nyu
        .client
        .create_send(nyu.token(), &for_addresses.request)
        .await;
    if info.has("send-emails") {
        let made = answer.unwrap();
        assert_eq!(
            (made["authType"].clone(), made["emails"].clone()),
            (json!(0), json!("mio@example.com"))
        );
    } else {
        let error = answer.unwrap_err().to_string();
        assert!(
            error.contains("mail"),
            "without mail, addresses are refused: {error}"
        );
    }
}

// ── Moving from Vaultwarden ────────────────────────────────

#[tokio::test]
#[ignore = "needs a running UwULock Server and a filled Vaultwarden: LIVE_VAULTWARDEN=1 scripts/live-server.sh"]
async fn a_vault_moves_over_from_vaultwarden() {
    use uwulock_bitwarden::moving::{Journal, Mover, Source, Step, Target};
    if std::env::var("UWULOCK_TEST_VAULTWARDEN").is_err() {
        eprintln!("skipped: no Vaultwarden (LIVE_VAULTWARDEN=1 scripts/live-server.sh)");
        return;
    }
    let state: Value = serde_json::from_str(
        &std::fs::read_to_string(env("UWULOCK_TEST_VAULTWARDEN_STATE")).unwrap(),
    )
    .unwrap();
    let source_email = state["nyu"]["email"].as_str().unwrap();
    let source_password = state["nyu"]["password"].as_str().unwrap();
    let source = Live::log_in_with(
        source_email,
        source_password,
        &env("UWULOCK_TEST_VAULTWARDEN"),
    )
    .await;
    let target = Live::log_in("move").await;
    let now = in_days(0);

    let mut journal = Journal::default();
    for run in 0..2 {
        let from = Source::new(
            source.client.clone(),
            Live::log_in_with(
                source_email,
                source_password,
                &env("UWULOCK_TEST_VAULTWARDEN"),
            )
            .await
            .session,
            source.user_key.clone(),
            source_email,
        );
        let to = Target {
            client: target.client.clone(),
            user_key: target.user_key.clone(),
            email: target.email.clone(),
        };
        let mut mover = Mover::prepare(from, to, target.token(), journal.clone(), &now)
            .await
            .unwrap();
        let preview = mover.preview().clone();
        if run == 0 {
            assert!(preview.families, "the target makes families: {preview:?}");
            assert_eq!(preview.items.todo, 3, "{preview:?}");
            assert_eq!(preview.attachments.todo, 1);
            assert_eq!(preview.sends.todo, 2);
        } else {
            assert_eq!(
                preview.items.todo + preview.attachments.todo + preview.sends.todo,
                0,
                "{preview:?}"
            );
        }
        let summary = loop {
            match mover.step(target.token()).await.unwrap() {
                Step::Working(_) => continue,
                Step::Finished(summary) => break summary,
            }
        };
        assert!(summary.failed.is_empty(), "{summary:?}");
        journal = mover.journal().clone();
    }

    let vault = target.vault().await;
    assert_eq!(names_of(&vault), ["Notiz", "Router", "WLAN"]);
    let family = vault
        .organizations
        .iter()
        .find(|o| o.name == "Familie")
        .expect("a family");
    let wlan = vault
        .items
        .iter()
        .find(|i| i.name.as_str() == "WLAN")
        .unwrap();
    assert_eq!(wlan.organization_id.as_deref(), Some(family.id.as_str()));
    let collection = vault
        .collections
        .iter()
        .find(|c| c.name == "Gemeinsam")
        .expect("its collection");
    assert_eq!(collection.organization_id, family.id);
    assert_eq!(wlan.collection_ids, std::slice::from_ref(&collection.id));
    let account = target
        .client
        .uwu_get(target.token(), "/account")
        .await
        .unwrap();
    assert_eq!(account["families"]["owned"], 1, "{account}");
    let router = vault
        .items
        .iter()
        .find(|i| i.name.as_str() == "Router")
        .unwrap();
    assert_eq!(router.attachments, 1);
    assert_eq!(
        vault
            .folders
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["Arbeit"]
    );

    // The attachment opens under the target's keys.
    let text = target.client.sync(target.token()).await.unwrap();
    let sync: Value = serde_json::from_str(&text).unwrap();
    let cipher = sync["ciphers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == router.id.as_str())
        .unwrap();
    let attachment = &cipher["attachments"][0];
    let url = target
        .client
        .attachment_url(
            target.token(),
            &router.id,
            attachment["id"].as_str().unwrap(),
        )
        .await
        .unwrap();
    let encrypted = target.client.download(&url).await.unwrap();
    let item_key = router
        .key
        .clone()
        .unwrap_or_else(|| target.user_key.clone());
    let key = attachment["key"]
        .as_str()
        .unwrap()
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&item_key)
        .unwrap();
    assert_eq!(
        &**decrypt_file(&encrypted, &key).unwrap(),
        "Anhang aus Vaultwarden ✧\n".as_bytes()
    );

    // Both Sends, the file one with its file.
    let sends = sync["sends"].as_array().unwrap();
    assert_eq!(sends.len(), 2, "{sends:?}");
    let file_send = sends.iter().find(|s| s["type"] == 1).expect("a file Send");
    assert!(file_send["file"]["id"].is_string());
}
