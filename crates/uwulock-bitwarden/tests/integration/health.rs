//! The password check's calls against a fake of UwULock Server's §15
//! endpoints: breach sources by hash prefix, the lists, change-password
//! pages, the check of addresses and the ignore list with its revision check.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use uwulock_bitwarden::health::{change_password_url, is_conflict, BreachSwitches};
use uwulock_bitwarden::uwu::Info;
use uwulock_bitwarden::{Client, Device, Item, ItemKind, Server};
use uwulock_core::health;
use zeroize::Zeroizing;

use crate::support::fake_http::{Answer, FakeHttp};

const TOKEN: &str = "access-token";

fn client(url: &str) -> Client {
    Client::new(
        Server::self_hosted(url).unwrap(),
        Device::this_system("7c1d1f0e-5b1a-4f8e-9d3c-0e2b6a1c9f00".into()),
    )
    .unwrap()
}

fn login(id: &str, password: &str) -> Item {
    let mut item = Item::new(ItemKind::Login);
    item.id = id.into();
    item.name = Zeroizing::new(id.into());
    item.login.as_mut().unwrap().password = Some(Zeroizing::new(password.into()));
    item
}

fn refused(status: u16, code: &str) -> Answer {
    Answer::json(
        status,
        json!({ "object": "error", "message": format!("refused: {code}"), "code": code }),
    )
}

#[test]
fn the_switches_of_old_and_new_servers() {
    let old: Info = serde_json::from_value(json!({
        "name": "UwULock Server", "features": ["vault", "hibp", "health-report"]
    }))
    .unwrap();
    assert_eq!(
        BreachSwitches::of(&old),
        BreachSwitches {
            hibp: true,
            ..BreachSwitches::default()
        }
    );
    assert!(!BreachSwitches::ignore_list(&old));
    let new: Info = serde_json::from_value(json!({
        "name": "UwULock Server", "features": ["vault", "hibp"],
        "breaches": { "hibp": false, "xonPasswords": true, "siteBreaches": true,
                      "emailCheck": false, "changePassword": true }
    }))
    .unwrap();
    let on = BreachSwitches::of(&new);
    assert!(!on.hibp && on.xon_passwords && on.site_breaches && on.change_password);
    assert!(!on.email_check);
    assert!(BreachSwitches::ignore_list(&new));
}

#[tokio::test]
async fn breached_passwords_are_asked_for_by_prefix_only() {
    let fake = FakeHttp::start(|request| {
        if request.headers.get("authorization").map(String::as_str) != Some("Bearer access-token") {
            return refused(401, "unauthorized");
        }
        match request.path.as_str() {
            // SHA-1("password") = 5BAA6 1E4C9B93F3F0682250B6CF8331B7EE68FD8
            "/uwu/v1/hibp/5BAA6" => Answer::bytes(
                "text/plain",
                b"0018A45C4D1DEF81644B54AB7F969B88D65:1\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:42"
                    .to_vec(),
            ),
            "/uwu/v1/xon/a6818b8188" => {
                Answer::json(200, json!({ "object": "xonPassword", "count": 1000 }))
            }
            path if path.starts_with("/uwu/v1/xon/") => {
                Answer::json(200, json!({ "object": "xonPassword", "count": 0 }))
            }
            // One source failing leaves the rest of the report standing.
            path if path.starts_with("/uwu/v1/hibp/") => refused(502, "upstream"),
            _ => Answer::empty(404),
        }
    });
    let client = client(&fake.url);
    let prepared = health::prepare(&[
        login("c1", "password"),
        login("c2", "password"),
        login("c3", "K8#vR2!qLm9$wZ4^"),
    ]);
    let steps = Mutex::new(Vec::new());
    let answers = client
        .breach_counts(TOKEN, &prepared, true, true, |done, total| {
            steps.lock().unwrap().push((done, total))
        })
        .await;
    assert!(answers.incomplete, "the second HIBP prefix failed");
    let steps = steps.into_inner().unwrap();
    assert_eq!(steps.first(), Some(&(0, 4)));
    assert_eq!(steps.last(), Some(&(4, 4)));
    let mut report = prepared.report.clone();
    answers.counts.apply(&mut report, true, answers.incomplete);
    assert_eq!(report.findings[0].breached, Some(1000));
    assert_eq!(report.findings[1].breach_sources, ["hibp", "xon"]);
    assert_eq!(report.findings[2].breached, Some(0));
    // Only prefixes went out: five hex digits to /hibp, ten to /xon, nothing else.
    for call in fake.calls() {
        let (_, path) = call.split_once(' ').unwrap();
        let prefix = path.rsplit('/').next().unwrap();
        if path.starts_with("/uwu/v1/hibp/") {
            assert_eq!(prefix.len(), 5, "{call}");
        } else {
            assert!(
                path.starts_with("/uwu/v1/xon/") && prefix.len() == 10,
                "{call}"
            );
        }
    }
}

#[tokio::test]
async fn lists_pages_and_addresses() {
    let fake = FakeHttp::start(
        |request| match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/uwu/v1/breaches/sites") => Answer::json(
                200,
                json!({
                    "object": "siteBreaches", "updated": "2026-10-01T03:00:00.000Z",
                    "sources": [{ "id": "hibp", "name": "Have I Been Pwned", "url": "https://haveibeenpwned.com/", "license": "CC BY 4.0", "updated": "x" }],
                    "breaches": [{ "domain": "example.com", "title": "Example", "date": "2024-05-01", "added": "2024-06-01",
                                   "records": 1200, "passwords": true, "dataClasses": ["Passwords"], "sources": { "hibp": "Example", "xon": "ExampleLeak" } }]
                }),
            ),
            ("GET", "/uwu/v1/twofa-directory") => Answer::json(
                200,
                json!({
                    "object": "twofaDirectory", "updated": "x",
                    "source": { "name": "2FA Directory", "url": "https://2fa.directory/", "license": "MIT" },
                    "entries": [{ "domain": "example.com", "additionalDomains": [], "name": "Example", "methods": ["totp"], "documentation": null }]
                }),
            ),
            ("GET", "/uwu/v1/change-password/shop.example.com") => Answer::json(
                200,
                json!({ "object": "changePassword", "host": "shop.example.com", "url": "https://shop.example.com/.well-known/change-password" }),
            ),
            // A hostile server points somewhere else entirely.
            ("GET", "/uwu/v1/change-password/bank.example.com") => Answer::json(
                200,
                json!({ "object": "changePassword", "host": "bank.example.com", "url": "https://phish.example.net/bank/change" }),
            ),
            ("GET", path) if path.starts_with("/uwu/v1/change-password/") => {
                Answer::json(200, json!({ "object": "changePassword", "url": null }))
            }
            ("GET", "/uwu/v1/breaches/emails/opt-in") => Answer::json(
                200,
                json!({ "object": "emailBreachOptIn", "optedIn": false, "since": null }),
            ),
            ("PUT", "/uwu/v1/breaches/emails/opt-in") => {
                let on = request.json()["optedIn"] == true;
                Answer::json(
                    200,
                    json!({ "object": "emailBreachOptIn", "optedIn": on, "since": on.then_some("2026-10-01T00:00:00Z") }),
                )
            }
            ("POST", "/uwu/v1/breaches/emails") => {
                let emails = request.json()["emails"].clone();
                let results: Vec<Value> = emails
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| json!({ "email": e, "status": "found", "breaches": ["ExampleLeak"] }))
                    .collect();
                Answer::json(
                    200,
                    json!({ "object": "emailBreaches", "results": results, "retryAfter": null }),
                )
            }
            _ => refused(404, "feature_off"),
        },
    );
    let client = client(&fake.url);
    let sites = client.site_breaches(TOKEN).await.unwrap();
    assert_eq!(sites.breaches[0].sources["xon"], "ExampleLeak");
    assert_eq!(sites.sources[0].license.as_deref(), Some("CC BY 4.0"));
    let directory = client.twofa_directory(TOKEN).await.unwrap();
    assert_eq!(directory.entries[0].methods, ["totp"]);
    assert_eq!(
        client
            .change_password_page(TOKEN, "Shop.Example.com")
            .await
            .unwrap()
            .as_deref(),
        Some("https://shop.example.com/.well-known/change-password")
    );
    assert_eq!(
        client
            .change_password_page(TOKEN, "example.org")
            .await
            .unwrap(),
        None
    );
    // Only whether the page exists comes from the server; the address is the login's own host.
    assert_eq!(
        client
            .change_password_page(TOKEN, "bank.example.com")
            .await
            .unwrap()
            .as_deref(),
        Some("https://bank.example.com/.well-known/change-password")
    );
    // Nothing but a host name goes in front of the path, and such a host isn't even asked about.
    for host in [
        "evil.example/x",
        "user@example.com",
        "example.com:8443",
        "[2001:db8::1]",
        "",
    ] {
        assert_eq!(change_password_url(host), None, "{host}");
        assert_eq!(
            client.change_password_page(TOKEN, host).await.unwrap(),
            None
        );
    }
    assert!(!client.email_opt_in(TOKEN).await.unwrap().opted_in);
    assert!(client.set_email_opt_in(TOKEN, true).await.unwrap().opted_in);
    let many: Vec<String> = (0..60).map(|n| format!("n{n}@example.com")).collect();
    let answer = client.check_emails(TOKEN, &many).await.unwrap();
    assert_eq!(answer.results.len(), 50, "at most 50 a request");
    assert_eq!(answer.retry_after, None);
    // A source that is switched off says so with its code.
    let off = client.health_ignores(TOKEN).await.unwrap_err();
    assert_eq!(off.code(), Some("feature_off"));
}

#[tokio::test]
async fn the_ignore_list_is_only_replaced_when_nobody_changed_it_meanwhile() {
    let stored: Arc<Mutex<(Option<String>, Option<String>)>> = Arc::default();
    let shared = stored.clone();
    let fake = FakeHttp::start(move |request| {
        let mut stored = shared.lock().unwrap();
        let object = |s: &(Option<String>, Option<String>)| json!({ "object": "healthIgnores", "data": s.0, "revisionDate": s.1 });
        match request.method.as_str() {
            "GET" => Answer::json(200, object(&stored)),
            "PUT" => {
                let body = request.json();
                if body["revisionDate"].as_str() != stored.1.as_deref() {
                    return refused(409, "conflict");
                }
                let revision = format!("rev-{}", body["data"].as_str().unwrap_or("").len());
                *stored = (body["data"].as_str().map(str::to_string), Some(revision));
                Answer::json(200, object(&stored))
            }
            _ => Answer::empty(405),
        }
    });
    let client = client(&fake.url);
    let empty = client.health_ignores(TOKEN).await.unwrap();
    assert_eq!(empty.data, None);
    let saved = client
        .put_health_ignores(TOKEN, "2.a|b|c", None)
        .await
        .unwrap();
    assert_eq!(saved.revision_date.as_deref(), Some("rev-7"));
    assert_eq!(
        fake.last("PUT", "/uwu/v1/reports/health/ignored")
            .unwrap()
            .json()["revisionDate"],
        Value::Null
    );
    // Another device saved in between: this one read nothing.
    let stale = client
        .put_health_ignores(TOKEN, "2.d|e|f", None)
        .await
        .unwrap_err();
    assert!(is_conflict(&stale));
    let again = client
        .put_health_ignores(TOKEN, "2.dd|e|f", saved.revision_date.as_deref())
        .await
        .unwrap();
    assert_eq!(again.revision_date.as_deref(), Some("rev-8"));
}
