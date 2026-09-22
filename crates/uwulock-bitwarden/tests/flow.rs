//! The whole way against the toy server: prelogin, login with two-step login,
//! remembering the device, refresh, sync, and opening every kind of item.

mod support;

use support::toy_server::{self, Options, ToyServer};
use uwulock_bitwarden::api::{parse_sync, LoginOutcome, PasswordLogin, TwoFactorAnswer};
use uwulock_bitwarden::crypto::{self, decrypt_user_key, EncString};
use uwulock_bitwarden::totp::Totp;
use uwulock_bitwarden::vault::FieldKind;
use uwulock_bitwarden::{Client, Device, Error, ItemKind, Kdf, Server, Vault};

fn client(server: &ToyServer) -> Client {
    Client::new(
        Server::self_hosted(&server.url).unwrap(),
        Device::this_system("7c1d1f0e-5b1a-4f8e-9d3c-0e2b6a1c9f00".into()),
    )
    .unwrap()
}

fn plain(value: &Option<zeroize::Zeroizing<String>>) -> Option<&str> {
    value.as_ref().map(|s| s.as_str())
}

async fn password_hash(client: &Client, password: &str) -> (zeroize::Zeroizing<[u8; 32]>, String) {
    let kdf = client.prelogin(toy_server::EMAIL).await.unwrap();
    let master = crypto::master_key(password, toy_server::EMAIL, kdf).unwrap();
    let hash = crypto::master_password_hash(&master, password);
    (master, hash)
}

async fn log_in(
    client: &Client,
    hash: &str,
    two_factor: Option<TwoFactorAnswer>,
    remember_token: Option<&str>,
) -> Result<LoginOutcome, Error> {
    client
        .login(PasswordLogin {
            email: toy_server::EMAIL,
            password_hash: hash,
            two_factor,
            remember_token,
            new_device_code: None,
        })
        .await
}

#[tokio::test]
async fn login_sync_and_open() {
    let server = ToyServer::start(Options::default());
    let client = client(&server);
    let (master, hash) = password_hash(&client, toy_server::PASSWORD).await;

    let outcome = client
        .login(PasswordLogin {
            email: "  NYU@uwu.local ",
            password_hash: &hash,
            two_factor: None,
            remember_token: None,
            new_device_code: None,
        })
        .await
        .unwrap();
    let LoginOutcome::LoggedIn(session) = outcome else {
        panic!("expected to be logged in, got {outcome:?}");
    };

    let protected: EncString = session.protected_user_key.clone().unwrap().parse().unwrap();
    let user_key = decrypt_user_key(&master, &protected).unwrap();
    let text = client.sync(&session.access_token).await.unwrap();
    let vault = Vault::open(&parse_sync(&text).unwrap(), &user_key).unwrap();

    assert_eq!(vault.email, toy_server::EMAIL);
    assert_eq!(vault.items.len(), 9);
    assert_eq!(vault.skipped, 0);
    assert!(vault.items.iter().all(|i| !i.broken), "every item opens");
    assert_eq!(vault.organizations[0].name, "Homelab");
    assert_eq!(vault.collections[0].name, "Netzwerk");
    let mut folders: Vec<_> = vault.folders.iter().map(|f| f.name.as_str()).collect();
    folders.sort();
    assert_eq!(folders, ["Homelab", "Privat"]);

    let github = vault.item("c-github").unwrap();
    let login = github.login.as_ref().unwrap();
    assert_eq!(github.name.as_str(), "GitHub");
    assert_eq!(plain(&login.password), Some("hunter2-but-longer!"));
    assert_eq!(login.uris[0].uri.as_str(), "https://github.com/login");
    assert!(github.favorite);
    assert_eq!(github.password_history[0].password.as_str(), "hunter2");
    let totp = Totp::parse(login.totp.as_ref().unwrap()).unwrap();
    assert_eq!(totp.code_at(59).0.len(), 6);

    // An item with its own key.
    let nas = vault.item("c-nas").unwrap();
    assert_eq!(
        plain(&nas.login.as_ref().unwrap().password),
        Some("Katzenklo-2026")
    );
    assert_eq!(nas.fields[0].kind, FieldKind::Hidden);
    assert_eq!(plain(&nas.fields[0].value), Some("4711"));
    assert_eq!(nas.fields[2].kind, FieldKind::Boolean);

    // An organisation item, through the RSA-wrapped organisation key.
    let router = vault.item("c-router").unwrap();
    assert_eq!(
        plain(&router.login.as_ref().unwrap().password),
        Some("Kabel-Salat-99")
    );
    assert_eq!(router.collection_ids, ["col-network"]);

    let card = vault.item("c-card").unwrap();
    assert_eq!(card.kind, ItemKind::Card);
    assert_eq!(plain(&card.card.as_ref().unwrap().code), Some("123"));
    assert_eq!(vault.item("c-identity").unwrap().kind, ItemKind::Identity);
    assert!(vault
        .item("c-note")
        .unwrap()
        .notes
        .as_ref()
        .unwrap()
        .contains("miau"));
    assert!(vault
        .item("c-ssh")
        .unwrap()
        .ssh_key
        .as_ref()
        .unwrap()
        .private_key
        .is_some());
    assert!(vault.item("c-vaultwarden").unwrap().reprompt);
    assert!(vault.item("c-old").unwrap().deleted);

    // Refresh gives a working token; a revoked session says so.
    let refreshed = client
        .refresh(session.refresh_token.as_ref().unwrap())
        .await
        .unwrap();
    assert!(client.sync(&refreshed.access_token).await.is_ok());
    server.revoke_sessions();
    assert!(matches!(
        client.sync(&refreshed.access_token).await,
        Err(Error::SessionExpired)
    ));
    assert!(matches!(
        client
            .refresh(refreshed.refresh_token.as_ref().unwrap())
            .await,
        Err(Error::SessionExpired)
    ));
}

#[tokio::test]
async fn wrong_password_is_refused() {
    let server = ToyServer::start(Options::default());
    let client = client(&server);
    let (_, hash) = password_hash(&client, "wrong-password!").await;
    match log_in(&client, &hash, None, None).await {
        Err(Error::Refused(message)) => assert!(message.contains("incorrect"), "{message}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn two_step_login_and_remembered_device() {
    let server = ToyServer::start(Options {
        two_factor: true,
        ..Options::default()
    });
    let client = client(&server);
    let (_, hash) = password_hash(&client, toy_server::PASSWORD).await;

    // No code: the server lists the methods.
    let LoginOutcome::TwoFactor { methods, message } =
        log_in(&client, &hash, None, None).await.unwrap()
    else {
        panic!("expected two-step login");
    };
    assert!(message.is_none());
    assert_eq!(
        methods.iter().map(|m| m.kind).collect::<Vec<_>>(),
        ["authenticator", "email"]
    );
    assert_eq!(methods[1].hint.as_deref(), Some("n***@uwu.local"));
    client
        .send_email_code(toy_server::EMAIL, &hash)
        .await
        .unwrap();

    // A wrong code says why.
    let wrong = TwoFactorAnswer {
        provider: 0,
        code: "000000".into(),
        remember: false,
    };
    let LoginOutcome::TwoFactor { message, .. } =
        log_in(&client, &hash, Some(wrong), None).await.unwrap()
    else {
        panic!("expected two-step login again");
    };
    assert!(message.unwrap().contains("Invalid"));

    // The authenticator code, with a space in it, remembering the device.
    let code = Totp::parse(toy_server::TOTP_SECRET)
        .unwrap()
        .now()
        .0
        .to_string();
    let right = TwoFactorAnswer {
        provider: 0,
        code: format!("{} {}", &code[..3], &code[3..]),
        remember: true,
    };
    let LoginOutcome::LoggedIn(session) = log_in(&client, &hash, Some(right), None).await.unwrap()
    else {
        panic!("expected to be logged in");
    };
    let remember = session.remember_token.clone().expect("a remember token");

    // Next time the remembered device skips the code; the email code works too.
    assert!(matches!(
        log_in(&client, &hash, None, Some(&remember)).await.unwrap(),
        LoginOutcome::LoggedIn(_)
    ));
    let email = TwoFactorAnswer {
        provider: 1,
        code: toy_server::EMAIL_CODE.into(),
        remember: false,
    };
    assert!(matches!(
        log_in(&client, &hash, Some(email), None).await.unwrap(),
        LoginOutcome::LoggedIn(_)
    ));
    assert_eq!(server.logins(), 3);
}

#[tokio::test]
async fn argon2id_accounts() {
    let kdf = Kdf::Argon2id {
        iterations: 3,
        memory_mib: 16,
        parallelism: 2,
    };
    let server = ToyServer::start(Options {
        kdf,
        ..Options::default()
    });
    let client = client(&server);
    assert_eq!(client.prelogin(toy_server::EMAIL).await.unwrap(), kdf);
    let (_, hash) = password_hash(&client, toy_server::PASSWORD).await;
    assert!(matches!(
        log_in(&client, &hash, None, None).await.unwrap(),
        LoginOutcome::LoggedIn(_)
    ));
}

#[tokio::test]
async fn unreachable_server() {
    // Nothing listens on the discard port.
    let dead = Client::new(
        Server::self_hosted("http://127.0.0.1:9").unwrap(),
        Device::this_system("x".into()),
    )
    .unwrap();
    assert!(matches!(
        dead.prelogin("a@b.c").await,
        Err(Error::Network(_))
    ));
}
