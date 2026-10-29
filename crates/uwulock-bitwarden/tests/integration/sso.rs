//! An SSO login against a fake of UwULock Server's identity endpoints: the
//! prevalidate token, the browser coming back to the loopback listener, the
//! code traded with its PKCE verifier (also after two-step login), and the
//! first master password of an account that has no keys yet.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::Duration;
use uwulock_bitwarden::crypto::{self, EncString, Kdf};
use uwulock_bitwarden::sso::{self, Loopback, NewKeys, Start};
use uwulock_bitwarden::{Client, Device, LoginOutcome, Server, SetPassword, SsoLogin};

use crate::support::fake_http::{Answer, FakeHttp, Request};

fn client(url: &str) -> Client {
    Client::new(
        Server::self_hosted(url).unwrap(),
        Device::this_system("7c1d1f0e-5b1a-4f8e-9d3c-0e2b6a1c9f00".into()),
    )
    .unwrap()
}

fn form(request: &Request) -> HashMap<String, String> {
    url::form_urlencoded::parse(&request.body)
        .into_owned()
        .collect()
}

fn jwt(email: &str) -> String {
    let payload = URL_SAFE_NO_PAD.encode(json!({ "email": email, "sub": "u1" }).to_string());
    format!("eyJhbGciOiJSUzI1NiJ9.{payload}.c2ln")
}

/// What the browser does once the provider is done: the server redirects it to
/// the loopback address with the code and the state.
fn browser_comes_back(redirect_uri: &str, query: &str) -> String {
    let url = url::Url::parse(redirect_uri).unwrap();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", url.port().unwrap())).unwrap();
    write!(
        stream,
        "GET {}?{query} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
        url.path()
    )
    .unwrap();
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    answer
}

#[tokio::test]
async fn sso_login_of_a_new_account_sets_its_first_master_password() {
    let fake = FakeHttp::start(
        |request| match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/identity/sso/prevalidate") => {
                Answer::json(200, json!({ "token": "pre.token" }))
            }
            ("POST", "/identity/connect/token") => {
                let form = form(request);
                if form.get("code").map(String::as_str) != Some("the-code") {
                    return Answer::json(400, json!({ "error": "invalid_grant" }));
                }
                Answer::json(
                    200,
                    json!({
                        "access_token": jwt("Nyu@Example.com"),
                        "refresh_token": "refresh",
                        "expires_in": 3600,
                        "Kdf": 0,
                        "KdfIterations": 600000,
                        "UserDecryptionOptions": { "HasMasterPassword": false },
                    }),
                )
            }
            ("POST", "/api/accounts/set-password") => Answer::empty(200),
            _ => Answer::empty(404),
        },
    );
    let client = client(&fake.url);

    let pre = client
        .sso_prevalidate(sso::DEFAULT_IDENTIFIER)
        .await
        .unwrap();
    assert_eq!(pre.as_str(), "pre.token");
    assert!(fake
        .last("GET", "/identity/sso/prevalidate")
        .unwrap()
        .query
        .contains("domainHint=uwulock"));

    let loopback = Loopback::bind().await.unwrap();
    let start = Start::new(loopback.redirect_uri().to_string());
    let authorize = url::Url::parse(&client.sso_authorize_url(&start, &pre)).unwrap();
    assert!(authorize
        .as_str()
        .starts_with(&format!("{}/identity/connect/authorize?", fake.url)));

    let state = start.state.to_string();
    let redirect = start.redirect_uri.clone();
    let browser = std::thread::spawn(move || {
        // Somebody else's request first: wrong state, ignored.
        let forged = browser_comes_back(&redirect, "code=evil&state=guessed");
        assert!(forged.starts_with("HTTP/1.1 404"));
        browser_comes_back(&redirect, &format!("code=the-code&state={state}"))
    });
    let code = loopback
        .wait(&start.state, Duration::from_secs(10))
        .await
        .unwrap();
    assert!(browser.join().unwrap().starts_with("HTTP/1.1 200"));
    assert_eq!(code.as_str(), "the-code");

    let outcome = client
        .login_sso(SsoLogin {
            code: &code,
            code_verifier: &start.code_verifier,
            redirect_uri: &start.redirect_uri,
            two_factor: None,
            remember_token: None,
        })
        .await
        .unwrap();
    let LoginOutcome::LoggedIn(session) = outcome else {
        panic!("logged in");
    };
    let sent = form(&fake.last("POST", "/identity/connect/token").unwrap());
    assert_eq!(sent["grant_type"], "authorization_code");
    assert_eq!(sent["code_verifier"], start.code_verifier.as_str());
    assert_eq!(sent["redirect_uri"], start.redirect_uri);
    assert_eq!(sent["client_id"], "desktop");
    assert_eq!(sent["scope"], "api offline_access");
    assert!(!session.has_master_password);
    assert!(session.protected_user_key.is_none());
    assert_eq!(session.email().as_deref(), Some("nyu@example.com"));
    let kdf = sso::kdf_for_new_password(session.kdf);
    assert_eq!(
        kdf,
        Kdf::Pbkdf2 {
            iterations: 600_000
        }
    );

    // Cheaper rounds for the test; the KDF goes to the server as it is.
    let cheap = Kdf::Pbkdf2 { iterations: 5_000 };
    let keys = NewKeys::make(
        "a long first password",
        "nyu@example.com",
        cheap,
        &sso::PasswordPolicy::default(),
    )
    .unwrap();
    let set = client
        .set_password(&session.access_token, &keys, sso::DEFAULT_IDENTIFIER)
        .await
        .unwrap();
    assert_eq!(set, SetPassword::Set);
    let request = fake.last("POST", "/api/accounts/set-password").unwrap();
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some(format!("Bearer {}", jwt("Nyu@Example.com")).as_str())
    );
    let body = request.json();
    assert_eq!(
        body["masterPasswordHash"],
        keys.master_password_hash.as_str()
    );
    assert_eq!(body["kdfIterations"], 5_000);
    // What was sent opens with the password, and only with it.
    let master = crypto::master_key("a long first password", "nyu@example.com", cheap).unwrap();
    let protected: EncString = body["key"].as_str().unwrap().parse().unwrap();
    crypto::decrypt_user_key(&master, &protected).unwrap();
    let wrong = crypto::master_key("another long password", "nyu@example.com", cheap).unwrap();
    assert!(crypto::decrypt_user_key(&wrong, &protected).is_err());
}

#[tokio::test]
async fn sso_login_asks_for_the_second_step_and_sends_the_code_again() {
    let fake = FakeHttp::start(|request| {
        let form = form(request);
        if form.contains_key("twoFactorToken") {
            Answer::json(
                200,
                json!({
                    "access_token": jwt("nyu@example.com"),
                    "expires_in": 3600,
                    "Key": "2.AAAA|BBBB|CCCC",
                    "Kdf": 0,
                    "KdfIterations": 600000,
                    "UserDecryptionOptions": { "HasMasterPassword": true },
                }),
            )
        } else {
            Answer::json(
                400,
                json!({
                    "error": "invalid_grant",
                    "error_description": "Two factor required.",
                    "TwoFactorProviders": ["0"],
                    "TwoFactorProviders2": { "0": null },
                }),
            )
        }
    });
    let client = client(&fake.url);
    let login = |two_factor| SsoLogin {
        code: "c0de",
        code_verifier: "v".repeat(43).leak(),
        redirect_uri: "http://127.0.0.1:4711/sso-callback",
        two_factor,
        remember_token: None,
    };
    let outcome = client.login_sso(login(None)).await.unwrap();
    assert!(
        matches!(outcome, LoginOutcome::TwoFactor { .. }),
        "{outcome:?}"
    );
    let outcome = client
        .login_sso(login(Some(uwulock_bitwarden::api::TwoFactorAnswer {
            provider: 0,
            code: "123456".into(),
            remember: false,
        })))
        .await
        .unwrap();
    let LoginOutcome::LoggedIn(session) = outcome else {
        panic!("logged in");
    };
    assert!(session.has_master_password);
    assert_eq!(
        session.kdf,
        Some(Kdf::Pbkdf2 {
            iterations: 600_000
        })
    );
    let sent = form(&fake.last("POST", "/identity/connect/token").unwrap());
    assert_eq!(sent["code"], "c0de");
    assert_eq!(sent["twoFactorToken"], "123456");
}

#[tokio::test]
async fn a_server_without_sso_says_so() {
    let fake = FakeHttp::start(|_| {
        Answer::json(
            400,
            json!({ "message": "SSO is not enabled on this server." }),
        )
    });
    let error = client(&fake.url)
        .sso_prevalidate(sso::DEFAULT_IDENTIFIER)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("SSO"), "{error}");
}

#[tokio::test]
async fn the_servers_master_password_policy_is_found_and_kept() {
    let fake = FakeHttp::start(
        |request| match (request.method.as_str(), request.path.as_str()) {
            ("POST", "/api/organizations/domain/sso/verified") => Answer::json(
                200,
                json!({ "object": "list", "data": [{ "organizationIdentifier": "Acme Corp" }] }),
            ),
            ("GET", "/api/organizations/Acme%20Corp/auto-enroll-status") => {
                Answer::json(200, json!({ "id": "0f6e2a52-3b8e-5c1d-9a7e-4b2c1d0e9f11" }))
            }
            (
                "GET",
                "/api/organizations/0f6e2a52-3b8e-5c1d-9a7e-4b2c1d0e9f11/policies/master-password",
            ) => Answer::json(
                200,
                json!({
                    "type": 1,
                    "enabled": true,
                    "data": { "minLength": 14, "minComplexity": 3, "enforceOnLogin": false },
                }),
            ),
            ("POST", "/api/accounts/set-password") => Answer::json(
                400,
                json!({ "message": "This account has a master password already." }),
            ),
            _ => Answer::empty(404),
        },
    );
    let client = client(&fake.url);
    let organization = client
        .sso_organization("access", "nyu@example.com")
        .await
        .unwrap();
    assert_eq!(organization.identifier, "Acme Corp");
    assert_eq!(
        organization.policy,
        sso::PasswordPolicy {
            min_length: 14,
            min_complexity: 3
        }
    );
    assert_eq!(
        fake.last("POST", "/api/organizations/domain/sso/verified")
            .unwrap()
            .json()["email"],
        "nyu@example.com"
    );
    let policy = fake
        .last(
            "GET",
            "/api/organizations/0f6e2a52-3b8e-5c1d-9a7e-4b2c1d0e9f11/policies/master-password",
        )
        .unwrap();
    assert_eq!(
        policy.headers.get("authorization").map(String::as_str),
        Some("Bearer access")
    );

    // An earlier try went through but its answer got lost.
    let keys = NewKeys::make(
        "a long first password",
        "nyu@example.com",
        Kdf::Pbkdf2 { iterations: 5_000 },
        &sso::PasswordPolicy::default(),
    )
    .unwrap();
    let set = client
        .set_password("access", &keys, &organization.identifier)
        .await
        .unwrap();
    assert_eq!(set, SetPassword::AlreadySet);
}

#[tokio::test]
async fn a_server_without_a_policy_leaves_the_floor() {
    // Vaultwarden without SSO organizations, say: everything is 404.
    let fake = FakeHttp::start(|_| Answer::empty(404));
    let organization = client(&fake.url)
        .sso_organization("access", "nyu@example.com")
        .await
        .unwrap();
    assert_eq!(organization.identifier, sso::DEFAULT_IDENTIFIER);
    assert_eq!(organization.policy, sso::PasswordPolicy::default());

    // A policy that is switched off asks for nothing more either.
    assert_eq!(
        uwulock_bitwarden::api::parse_password_policy(
            &json!({ "enabled": false, "data": { "minLength": 30, "minComplexity": 4 } })
        ),
        sso::PasswordPolicy::default()
    );
}

#[tokio::test]
async fn a_failing_policy_lookup_is_an_error() {
    let fake = FakeHttp::start(
        |request| match (request.method.as_str(), request.path.as_str()) {
            ("GET", path) if path.ends_with("/auto-enroll-status") => {
                Answer::json(200, json!({ "id": "x" }))
            }
            ("GET", path) if path.ends_with("/master-password") => {
                Answer::json(500, json!({ "message": "boom" }))
            }
            _ => Answer::empty(404),
        },
    );
    assert!(client(&fake.url)
        .sso_organization("access", "nyu@example.com")
        .await
        .is_err());
}
