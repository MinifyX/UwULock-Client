//! Saving against the toy server: what an edit changes, what it must leave
//! alone, and what the server refuses.
//!
//! Every check goes the long way round — save, sync again, decrypt — so it
//! shows what the server really kept, not what UwULock thinks it sent.

mod support;

use support::toy_server::{self, Options, ToyServer};
use uwulock_bitwarden::api::parse_sync;
use uwulock_bitwarden::crypto::{self, decrypt_user_key, EncString, SymmetricKey};
use uwulock_bitwarden::vault::{Field, FieldKind, Item, LoginUri, Secret};
use uwulock_bitwarden::{
    api::{LoginOutcome, PasswordLogin},
    Client, Device, Error, ItemKind, Server, Vault,
};

const NOW: &str = "2026-09-23T12:30:00.000Z";

struct Account {
    client: Client,
    access: String,
    user_key: SymmetricKey,
}

impl Account {
    /// The vault as it is on the server right now.
    async fn vault(&self) -> Vault {
        let text = self.client.sync(&self.access).await.unwrap();
        Vault::open(&parse_sync(&text).unwrap(), &self.user_key).unwrap()
    }

    /// Saves a changed item and returns the vault afterwards.
    async fn save(&self, item: &Item) -> Result<Vault, Error> {
        let vault = self.vault().await;
        let outer = vault.outer_key(item.organization_id.as_deref(), &self.user_key)?;
        item.can_save()?;
        self.client
            .update_cipher(&self.access, &item.id, item.seal(outer)?)
            .await?;
        Ok(self.vault().await)
    }
}

async fn log_in(server: &ToyServer) -> Account {
    let client = Client::new(
        Server::self_hosted(&server.url).unwrap(),
        Device::this_system("7c1d1f0e-5b1a-4f8e-9d3c-0e2b6a1c9f00".into()),
    )
    .unwrap();
    let kdf = client.prelogin(toy_server::EMAIL).await.unwrap();
    let master = crypto::master_key(toy_server::PASSWORD, toy_server::EMAIL, kdf).unwrap();
    let hash = crypto::master_password_hash(&master, toy_server::PASSWORD);
    let outcome = client
        .login(PasswordLogin {
            email: toy_server::EMAIL,
            password_hash: &hash,
            two_factor: None,
            remember_token: None,
            new_device_code: None,
        })
        .await
        .unwrap();
    let LoginOutcome::LoggedIn(session) = outcome else {
        panic!("expected to be logged in");
    };
    let protected: EncString = session.protected_user_key.clone().unwrap().parse().unwrap();
    let user_key = decrypt_user_key(&master, &protected).unwrap();
    Account {
        client,
        access: session.access_token.to_string(),
        user_key,
    }
}

fn secret(text: &str) -> Secret {
    Secret::new(text.to_string())
}

fn plain(value: &Option<Secret>) -> Option<&str> {
    value.as_ref().map(|s| s.as_str())
}

#[tokio::test]
async fn an_edit_keeps_what_it_doesnt_touch() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    let mut item = account.vault().await.item("c-github").unwrap().clone();
    item.name = secret("GitHub (privat)");
    item.login.as_mut().unwrap().username = Some(secret("nyu-the-other-cat"));
    let vault = account.save(&item).await.unwrap();

    let saved = vault.item("c-github").unwrap();
    let login = saved.login.as_ref().unwrap();
    assert_eq!(saved.name.as_str(), "GitHub (privat)");
    assert_eq!(plain(&login.username), Some("nyu-the-other-cat"));
    // Everything the editor never had in its hands.
    assert_eq!(plain(&login.password), Some("hunter2-but-longer!"));
    assert_eq!(login.passkey_count(), 1, "the passkey is still there");
    assert_eq!(login.autofill_on_page_load, Some(true));
    assert!(login.uris[0].checksum.is_some(), "the checksum travelled");
    assert_eq!(login.uris[0].uri.as_str(), "https://github.com/login");
    assert_eq!(saved.password_history.len(), 1);
    assert_eq!(saved.password_history[0].password.as_str(), "hunter2");
    assert!(saved.favorite);
    assert_eq!(saved.folder_id.as_deref(), Some("f-private"));
    assert!(saved.notes.as_ref().unwrap().contains("cat bed"));
    assert!(!saved.broken);
}

#[tokio::test]
async fn a_changed_password_keeps_the_old_one() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    let mut item = account.vault().await.item("c-github").unwrap().clone();
    item.set_password(secret("Katze-am-Fenster-2026"), NOW);
    let vault = account.save(&item).await.unwrap();

    let saved = vault.item("c-github").unwrap();
    let login = saved.login.as_ref().unwrap();
    assert_eq!(plain(&login.password), Some("Katze-am-Fenster-2026"));
    assert_eq!(login.password_revision_date.as_deref(), Some(NOW));
    assert_eq!(
        saved
            .password_history
            .iter()
            .map(|h| h.password.as_str())
            .collect::<Vec<_>>(),
        ["hunter2-but-longer!", "hunter2"],
        "the one before it went to the front of the history"
    );

    // Saving the same password again adds nothing.
    let mut again = saved.clone();
    again.set_password(secret("Katze-am-Fenster-2026"), "2026-09-24T09:00:00.000Z");
    let vault = account.save(&again).await.unwrap();
    assert_eq!(vault.item("c-github").unwrap().password_history.len(), 2);
}

#[tokio::test]
async fn an_item_with_its_own_key_stays_under_it() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    let mut item = account.vault().await.item("c-nas").unwrap().clone();
    assert!(item.key.is_some(), "this item has a key of its own");
    item.name = secret("Synology NAS (Keller)");
    item.fields.push(Field {
        name: Some(secret("Seriennummer")),
        value: Some(secret("NYU-1234-UWU")),
        kind: FieldKind::Text,
        linked_id: None,
    });
    let vault = account.save(&item).await.unwrap();

    let saved = vault.item("c-nas").unwrap();
    assert!(!saved.broken, "it still opens with its own key");
    assert_eq!(saved.name.as_str(), "Synology NAS (Keller)");
    assert_eq!(
        plain(&saved.login.as_ref().unwrap().password),
        Some("Katzenklo-2026")
    );
    assert_eq!(plain(&saved.fields[0].value), Some("4711"));
    assert_eq!(plain(&saved.fields[4].value), Some("NYU-1234-UWU"));
    // The linked field points where it did.
    assert_eq!(saved.fields[3].kind, FieldKind::Linked);
    assert_eq!(saved.fields[3].linked_id, Some(100));
}

#[tokio::test]
async fn an_organisation_item_is_saved_under_the_organisation_key() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    let mut item = account.vault().await.item("c-router").unwrap().clone();
    item.login.as_mut().unwrap().uris.push(LoginUri {
        uri: secret("https://fritz.box/admin"),
        match_kind: Some(0),
        checksum: None,
    });
    let vault = account.save(&item).await.unwrap();

    let saved = vault.item("c-router").unwrap();
    assert!(!saved.broken, "it opens with the organisation key");
    assert_eq!(
        plain(&saved.login.as_ref().unwrap().password),
        Some("Kabel-Salat-99")
    );
    assert_eq!(saved.login.as_ref().unwrap().uris.len(), 2);
    assert_eq!(saved.collection_ids, ["col-network"]);
    assert_eq!(saved.organization_id.as_deref(), Some("o-homelab"));
}

#[tokio::test]
async fn a_new_item_comes_back_whole() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    let mut item = Item::new(ItemKind::Login);
    item.name = secret("Vaultwarden Arbeit");
    item.folder_id = Some("f-homelab".into());
    item.favorite = true;
    item.notes = Some(secret("Zugang nur aus dem Büro-Netz."));
    let login = item.login.as_mut().unwrap();
    login.username = Some(secret("lorin"));
    login.totp = Some(secret("JBSWY3DPEHPK3PXP"));
    login.uris.push(LoginUri {
        uri: secret("https://vault.arbeit.example"),
        match_kind: None,
        checksum: None,
    });
    item.set_password(secret("erstes-Passwort-hier"), NOW);
    item.fields.push(Field {
        name: Some(secret("Notfall-PIN")),
        value: Some(secret("0815")),
        kind: FieldKind::Hidden,
        linked_id: None,
    });

    let answer = account
        .client
        .create_cipher(&account.access, item.seal(&account.user_key).unwrap(), &[])
        .await
        .unwrap();
    let id = answer["id"].as_str().unwrap().to_string();

    let vault = account.vault().await;
    let saved = vault.item(&id).unwrap();
    let login = saved.login.as_ref().unwrap();
    assert_eq!(saved.kind, ItemKind::Login);
    assert_eq!(saved.name.as_str(), "Vaultwarden Arbeit");
    assert_eq!(plain(&login.username), Some("lorin"));
    assert_eq!(plain(&login.password), Some("erstes-Passwort-hier"));
    assert_eq!(plain(&login.totp), Some("JBSWY3DPEHPK3PXP"));
    assert_eq!(saved.folder_id.as_deref(), Some("f-homelab"));
    assert!(saved.favorite);
    assert!(saved.password_history.is_empty(), "there was none before");
    assert_eq!(plain(&saved.fields[0].value), Some("0815"));
    assert_eq!(saved.fields[0].kind, FieldKind::Hidden);
    assert!(saved.notes.as_ref().unwrap().contains("Büro-Netz"));
}

#[tokio::test]
async fn a_new_note_and_card_carry_their_own_object() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    let mut note = Item::new(ItemKind::Note);
    note.name = secret("Türcode");
    note.notes = Some(secret("Hintertür: 4711#"));
    let mut card = Item::new(ItemKind::Card);
    card.name = secret("Zweitkarte");
    let fields = card.card.as_mut().unwrap();
    fields.cardholder_name = Some(secret("Nyu Neko"));
    fields.number = Some(secret("5555 5555 5555 4444"));
    fields.exp_month = Some(secret("3"));
    fields.exp_year = Some(secret("2030"));
    fields.code = Some(secret("999"));

    for item in [&note, &card] {
        account
            .client
            .create_cipher(&account.access, item.seal(&account.user_key).unwrap(), &[])
            .await
            .unwrap();
    }

    let vault = account.vault().await;
    let saved_note = vault
        .items
        .iter()
        .find(|i| i.name.as_str() == "Türcode")
        .unwrap();
    assert_eq!(saved_note.kind, ItemKind::Note);
    assert!(saved_note.notes.as_ref().unwrap().contains("4711"));
    let saved_card = vault
        .items
        .iter()
        .find(|i| i.name.as_str() == "Zweitkarte")
        .unwrap();
    assert_eq!(
        plain(&saved_card.card.as_ref().unwrap().number),
        Some("5555 5555 5555 4444")
    );
    assert_eq!(plain(&saved_card.card.as_ref().unwrap().code), Some("999"));
}

#[tokio::test]
async fn saving_an_older_copy_is_refused() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    // Two clients with the same item; the second one saves last.
    let stale = account.vault().await.item("c-github").unwrap().clone();
    let mut fresh = stale.clone();
    fresh.name = secret("GitHub (vom anderen Gerät)");
    account.save(&fresh).await.unwrap();

    let mut mine = stale.clone();
    mine.name = secret("GitHub (von hier)");
    match account.save(&mine).await {
        Err(Error::Conflict) => {}
        other => panic!("expected a conflict, got {other:?}"),
    }
    // The newer copy is untouched.
    assert_eq!(
        account
            .vault()
            .await
            .item("c-github")
            .unwrap()
            .name
            .as_str(),
        "GitHub (vom anderen Gerät)"
    );
}

#[tokio::test]
async fn the_trash_holds_an_item_until_it_is_emptied() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    account
        .client
        .trash_cipher(&account.access, "c-card")
        .await
        .unwrap();
    assert!(account.vault().await.item("c-card").unwrap().deleted);

    account
        .client
        .restore_cipher(&account.access, "c-card")
        .await
        .unwrap();
    let vault = account.vault().await;
    let back = vault.item("c-card").unwrap();
    assert!(!back.deleted);
    assert_eq!(plain(&back.card.as_ref().unwrap().code), Some("123"));

    account
        .client
        .delete_cipher(&account.access, "c-card")
        .await
        .unwrap();
    assert!(account.vault().await.item("c-card").is_none());
    assert!(matches!(
        account
            .client
            .delete_cipher(&account.access, "c-card")
            .await,
        Err(Error::Refused(_))
    ));
}

#[tokio::test]
async fn folders_are_made_renamed_and_removed() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;
    let name = |text: &str| EncString::encrypt(text.as_bytes(), &account.user_key).to_string();

    let answer = account
        .client
        .create_folder(&account.access, name("Arbeit"))
        .await
        .unwrap();
    let id = answer["id"].as_str().unwrap().to_string();
    let vault = account.vault().await;
    assert_eq!(
        vault.folders.iter().find(|f| f.id == id).unwrap().name,
        "Arbeit"
    );

    account
        .client
        .rename_folder(&account.access, &id, name("Arbeit (alt)"))
        .await
        .unwrap();
    assert_eq!(
        account
            .vault()
            .await
            .folders
            .iter()
            .find(|f| f.id == id)
            .unwrap()
            .name,
        "Arbeit (alt)"
    );

    // An item in the folder loses the folder, not itself.
    let mut item = account.vault().await.item("c-note").unwrap().clone();
    item.folder_id = Some(id.clone());
    account.save(&item).await.unwrap();
    account
        .client
        .delete_folder(&account.access, &id)
        .await
        .unwrap();
    let vault = account.vault().await;
    assert!(vault.folders.iter().all(|f| f.id != id));
    assert!(vault.item("c-note").unwrap().folder_id.is_none());
}

#[tokio::test]
async fn an_item_that_didnt_open_is_never_written_back() {
    let server = ToyServer::start(Options::default());
    let account = log_in(&server).await;

    let mut broken = account.vault().await.item("c-github").unwrap().clone();
    broken.broken = true;
    assert!(matches!(broken.can_save(), Err(Error::Refused(_))));
    assert!(matches!(
        broken.seal(&account.user_key),
        Err(Error::Refused(_))
    ));

    let mut nameless = account.vault().await.item("c-github").unwrap().clone();
    nameless.name = secret("   ");
    assert!(matches!(nameless.can_save(), Err(Error::Refused(_))));

    let mut half_a_key = account.vault().await.item("c-ssh").unwrap().clone();
    half_a_key.ssh_key.as_mut().unwrap().fingerprint = None;
    assert!(matches!(half_a_key.can_save(), Err(Error::Refused(_))));
}
