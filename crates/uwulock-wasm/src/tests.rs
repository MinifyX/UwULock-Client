//! The extension's calls, natively: an account made up here, unlocked every
//! way there is, a sync opened, items sealed and opened again, passkeys made
//! and used. Each test runs on its own thread, so each has its own vault.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use p256::ecdsa::signature::Verifier;
use p256::pkcs8::DecodePublicKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uwulock_core::crypto::{self, EncString, Kdf, SymmetricKey};
use uwulock_core::passkey::{self, Passkey, AAGUID};
use uwulock_core::vault::{Card, Item, ItemKind, LoginUri};
use zeroize::Zeroizing;

use crate::{autofill, draft, extras, generator, passkeys, session, view, Failure};

const EMAIL: &str = "nyu@example.com";
const PASSWORD: &str = "correct horse battery staple";
/// The cheapest KDF the core accepts.
const KDF: &str = r#"{"kdf": 0, "kdfIterations": 5000}"#;
const NOW: &str = "2026-09-28T12:00:00.000Z";

struct Account {
    user_key: SymmetricKey,
    protected: String,
}

fn account() -> Account {
    let user_key = SymmetricKey::generate();
    let master = crypto::master_key(PASSWORD, EMAIL, Kdf::Pbkdf2 { iterations: 5000 }).unwrap();
    let protected = EncString::encrypt(&user_key.to_bytes(), &SymmetricKey::stretch(&master));
    Account {
        user_key,
        protected: protected.to_string(),
    }
}

/// An item as the server has it: sealed, with an id.
fn cipher(item: &Item, id: &str, key: &SymmetricKey) -> Value {
    let mut value = serde_json::to_value(item.seal(key).unwrap()).unwrap();
    value["id"] = id.into();
    value
}

fn sync(account: &Account, ciphers: Vec<Value>) -> String {
    json!({
        "profile": { "email": EMAIL, "key": account.protected },
        "folders": [],
        "ciphers": ciphers,
    })
    .to_string()
}

fn login(name: &str, username: &str, password: &str) -> Item {
    let mut item = Item::new(ItemKind::Login);
    item.name = Zeroizing::new(name.into());
    let login = item.login.as_mut().unwrap();
    login.username = Some(Zeroizing::new(username.into()));
    login.password = Some(Zeroizing::new(password.into()));
    login.uris = vec![LoginUri {
        uri: Zeroizing::new("https://example.com/login".into()),
        match_kind: None,
        checksum: None,
    }];
    item
}

fn unlocked(account: &Account, ciphers: Vec<Value>) {
    session::unlock_with_password(EMAIL, KDF, &account.protected, PASSWORD).unwrap();
    session::open(&sync(account, ciphers)).unwrap();
}

fn kind<T: std::fmt::Debug>(result: Result<T, Failure>) -> &'static str {
    result.expect_err("should fail").kind
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn unlock_with_the_password_and_reveal() {
    let account = account();
    let hash = session::unlock_with_password(EMAIL, KDF, &account.protected, PASSWORD).unwrap();
    let master = crypto::master_key(PASSWORD, EMAIL, Kdf::Pbkdf2 { iterations: 5000 }).unwrap();
    assert_eq!(hash, crypto::master_password_hash(&master, PASSWORD));
    assert!(session::is_unlocked());

    let item = login("Example", "nyu", "hunter2");
    session::open(&sync(
        &account,
        vec![cipher(&item, "i1", &account.user_key)],
    ))
    .unwrap();
    assert_eq!(view::reveal("i1", "password", 0).unwrap(), "hunter2");
    let items = parse(&view::items().unwrap());
    assert_eq!(items[0]["name"], "Example");
    assert_eq!(items[0]["archived"], false);
    assert_eq!(items[0]["host"], "example.com");
    assert_eq!(kind(view::reveal("nope", "password", 0)), "not-found");

    assert_eq!(
        crate::with_unlocked(|u| session::check_password(u, PASSWORD)).unwrap(),
        hash
    );
    assert_eq!(
        kind(crate::with_unlocked(|u| session::check_password(
            u, "wrong"
        ))),
        "wrong-password"
    );

    session::lock();
    assert!(!session::is_unlocked());
    assert_eq!(kind(view::items()), "locked");
    assert_eq!(
        kind(session::unlock_with_password(
            EMAIL,
            KDF,
            &account.protected,
            "wrong"
        )),
        "wrong-password"
    );

    // The two steps of a login: the hash first, then the unlock.
    assert_eq!(
        kind(session::unlock(EMAIL, KDF, &account.protected)),
        "locked"
    );
    assert_eq!(session::derive_login(EMAIL, PASSWORD, KDF).unwrap(), hash);
    session::unlock(EMAIL, KDF, &account.protected).unwrap();
    assert!(session::is_unlocked());
}

#[test]
fn cheap_or_unknown_kdfs_are_refused() {
    let account = account();
    for kdf in [r#"{"kdf": 0, "kdfIterations": 100}"#, r#"{"kdf": 7}"#] {
        assert_eq!(
            kind(session::unlock_with_password(
                EMAIL,
                kdf,
                &account.protected,
                PASSWORD
            )),
            "unsupported"
        );
    }
    assert_eq!(
        kind(session::derive_login(EMAIL, PASSWORD, "no")),
        "invalid"
    );
}

#[test]
fn a_weaker_kdf_than_the_stored_one_is_told_apart() {
    let pbkdf2 = |n: u32| format!(r#"{{"kdf": 0, "kdfIterations": {n}}}"#);
    let argon2 = |i: u32, m: u32, p: u32| {
        format!(r#"{{"kdf": 1, "kdfIterations": {i}, "kdfMemory": {m}, "kdfParallelism": {p}}}"#)
    };
    let weaker = |kdf: &str, stored: &str| crate::kdf_weaker(kdf, stored).unwrap();
    assert!(weaker(&pbkdf2(5_000), &pbkdf2(600_000)));
    assert!(!weaker(&pbkdf2(600_000), &pbkdf2(600_000)));
    assert!(!weaker(&pbkdf2(700_000), &pbkdf2(600_000)));
    assert!(weaker(&pbkdf2(2_000_000), &argon2(3, 64, 4)));
    assert!(weaker(&argon2(3, 32, 4), &argon2(3, 64, 4)));
    assert!(!weaker(&argon2(3, 64, 1), &argon2(3, 64, 4)));
    assert!(!weaker(&argon2(3, 64, 4), &pbkdf2(600_000)));
    // Prelogin fields that are missing mean Bitwarden's defaults, as for the derivation.
    assert!(!weaker(r#"{"kdf": 0}"#, &pbkdf2(600_000)));
    assert_eq!(
        kind(crate::kdf_weaker(&pbkdf2(100), &pbkdf2(600_000))),
        "unsupported"
    );
}

#[test]
fn a_pin_unlocks_and_a_wrong_one_doesnt() {
    let account = account();
    assert_eq!(kind(session::pin_protect("1234")), "locked");
    unlocked(&account, vec![]);
    let protected = session::pin_protect("1234").unwrap();
    assert!(protected.starts_with("2."));
    assert_eq!(kind(session::pin_protect("")), "invalid");
    session::lock();

    assert_eq!(
        kind(session::unlock_with_pin(
            EMAIL,
            KDF,
            &account.protected,
            "0000",
            &protected
        )),
        "wrong-password"
    );
    assert!(!session::is_unlocked());
    session::unlock_with_pin(EMAIL, KDF, &account.protected, "1234", &protected).unwrap();
    assert_eq!(
        session::user_key().unwrap(),
        base64::engine::general_purpose::STANDARD.encode(account.user_key.to_bytes().as_slice())
    );
    // The PIN unlock knows the master password too, for re-prompts.
    crate::with_unlocked(|u| session::check_password(u, PASSWORD)).unwrap();
}

#[test]
fn the_user_key_unlocks_after_a_restart() {
    let account = account();
    let item = login("Example", "nyu", "hunter2");
    let ciphers = vec![cipher(&item, "i1", &account.user_key)];
    unlocked(&account, ciphers.clone());
    let kept = session::user_key().unwrap();
    session::lock();
    assert_eq!(kind(session::user_key()), "locked");

    session::unlock_with_key(EMAIL, KDF, &account.protected, &kept).unwrap();
    session::open(&sync(&account, ciphers)).unwrap();
    assert_eq!(view::reveal("i1", "password", 0).unwrap(), "hunter2");

    assert_eq!(
        kind(session::unlock_with_key(EMAIL, KDF, "", "not base64!")),
        "invalid"
    );
    assert_eq!(
        kind(session::unlock_with_key(EMAIL, KDF, "", "AAAA")),
        "crypto"
    );
}

#[test]
fn a_new_login_from_a_draft_opens_again() {
    let account = account();
    unlocked(&account, vec![]);
    let draft = json!({
        "kind": "login",
        "name": " Example ",
        "notes": "a note",
        "login": {
            "username": "nyu",
            "password": "hunter2",
            "uris": [{ "uri": "https://example.com", "match": null }, { "uri": " " }],
        },
        "fields": [{ "name": "PIN", "kind": "hidden", "value": "1234" }],
    });
    let sealed = parse(&draft::seal_draft("", &draft.to_string(), NOW).unwrap());
    assert_eq!(sealed["type"], 1);
    assert!(sealed["name"].as_str().unwrap().starts_with("2."));

    let mut stored = sealed.clone();
    stored["id"] = "new".into();
    session::open(&sync(&account, vec![stored])).unwrap();
    let detail = parse(&view::item("new").unwrap());
    assert_eq!(detail["summary"]["name"], "Example");
    assert_eq!(detail["login"]["username"], "nyu");
    assert_eq!(detail["login"]["uris"].as_array().unwrap().len(), 1);
    assert_eq!(view::reveal("new", "password", 0).unwrap(), "hunter2");
    assert_eq!(view::reveal("new", "field:0", 0).unwrap(), "1234");
    assert_eq!(view::reveal("new", "notes", 0).unwrap(), "a note");

    // The password prompt after a form was sent: only the password changes,
    // and the old one goes into the history.
    let changed = parse(&draft::seal_password("new", "hunter3", NOW).unwrap());
    let mut stored = changed;
    stored["id"] = "new".into();
    session::open(&sync(&account, vec![stored])).unwrap();
    assert_eq!(view::reveal("new", "password", 0).unwrap(), "hunter3");
    assert_eq!(view::reveal("new", "history:0", 0).unwrap(), "hunter2");
    assert_eq!(view::reveal("new", "username", 0).unwrap(), "nyu");
    assert_eq!(kind(draft::seal_password("new", "", NOW)), "invalid");
}

#[test]
fn fill_values_wait_for_the_reprompt() {
    let account = account();
    let mut guarded = login("Guarded", "nyu", "hunter2");
    guarded.reprompt = true;
    guarded.login.as_mut().unwrap().totp = Some(Zeroizing::new("JBSWY3DPEHPK3PXP".into()));
    let mut card = Item::new(ItemKind::Card);
    card.name = Zeroizing::new("Card".into());
    card.card = Some(Card {
        cardholder_name: Some(Zeroizing::new("Nyu".into())),
        number: Some(Zeroizing::new("4111111111111111".into())),
        exp_month: Some(Zeroizing::new("7".into())),
        ..Card::default()
    });
    let mut note = Item::new(ItemKind::Note);
    note.name = Zeroizing::new("Note".into());
    unlocked(
        &account,
        vec![
            cipher(&guarded, "g", &account.user_key),
            cipher(&card, "c", &account.user_key),
            cipher(&note, "n", &account.user_key),
        ],
    );

    assert_eq!(kind(autofill::fill_values("g", 0)), "reprompt");
    assert_eq!(kind(draft::seal_password("g", "new", NOW)), "reprompt");
    assert_eq!(
        kind(session::verify_reprompt("g", "wrong")),
        "wrong-password"
    );
    session::verify_reprompt("g", PASSWORD).unwrap();
    let values = parse(&autofill::fill_values("g", 59).unwrap());
    assert_eq!(values["kind"], "login");
    assert_eq!(values["username"], "nyu");
    assert_eq!(values["password"], "hunter2");
    assert_eq!(values["totp"].as_str().unwrap().len(), 6);

    let values = parse(&autofill::fill_values("c", 0).unwrap());
    assert_eq!(values["kind"], "card");
    assert_eq!(values["number"], "4111111111111111");
    assert_eq!(values["expMonth"], "7");
    assert_eq!(values["code"], Value::Null);

    assert_eq!(kind(autofill::fill_values("n", 0)), "invalid");
    assert_eq!(kind(autofill::fill_values("gone", 0)), "not-found");
}

#[test]
fn generator_answers() {
    let options = json!({ "length": 24, "lowercase": true, "uppercase": true, "digits": true,
        "symbols": false, "avoidAmbiguous": false });
    let answer = parse(&generator::password(&options.to_string()).unwrap());
    assert_eq!(answer["password"].as_str().unwrap().len(), 24);
    assert!(answer["bits"].as_u64().unwrap() > 100);

    let answer = parse(&generator::passphrase(r#"{"words": 4, "separator": "."}"#).unwrap());
    assert_eq!(answer["password"].as_str().unwrap().split('.').count(), 4);
    assert_eq!(answer["bits"], 51);
    assert_eq!(kind(generator::passphrase("\"six\"")), "invalid");
}

/// What a site checks of a new passkey's attested data: returns the credential
/// id, after checking the rest.
fn check_attested(auth_data: &[u8], flags: u8, public_key: &[u8]) -> Vec<u8> {
    assert_eq!(auth_data[..32], Sha256::digest(b"example.com")[..]);
    assert_eq!(auth_data[32], flags);
    assert_eq!(auth_data[33..37], [0, 0, 0, 0]);
    assert_eq!(auth_data[37..53], AAGUID);
    let length = u16::from_be_bytes([auth_data[53], auth_data[54]]) as usize;
    let id = auth_data[55..55 + length].to_vec();
    // The COSE key's x and y are the SPKI key's.
    let cose = &auth_data[55 + length..];
    assert_eq!(cose.len(), 77);
    let public = p256::PublicKey::from_public_key_der(public_key).unwrap();
    let point = p256::elliptic_curve::sec1::ToEncodedPoint::to_encoded_point(&public, false);
    assert_eq!(cose[10..42], point.x().unwrap()[..]);
    assert_eq!(cose[45..77], point.y().unwrap()[..]);
    id
}

fn verify(public_key: &[u8], auth_data: &[u8], client_data_hash: &[u8], signature: &[u8]) {
    let public = p256::PublicKey::from_public_key_der(public_key).unwrap();
    let verifying = p256::ecdsa::VerifyingKey::from(&public);
    let signature = p256::ecdsa::Signature::from_der(signature).unwrap();
    let mut message = auth_data.to_vec();
    message.extend_from_slice(client_data_hash);
    verifying.verify(&message, &signature).unwrap();
}

fn b64(text: &Value) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(text.as_str().unwrap()).unwrap()
}

#[test]
fn a_passkey_made_in_an_existing_login_signs_in() {
    let account = account();
    // A login with a key of its own, as newer clients make them.
    let mut item = login("Example", "nyu", "hunter2");
    let item_key = SymmetricKey::generate();
    item.wrapped_key =
        Some(EncString::encrypt(&item_key.to_bytes(), &account.user_key).to_string());
    item.key = Some(item_key.clone());
    unlocked(&account, vec![cipher(&item, "i1", &account.user_key)]);

    let request = json!({
        "itemId": "i1", "name": "ignored", "folderId": null,
        "rpId": "example.com", "rpName": "Example",
        "userHandle": URL_SAFE_NO_PAD.encode(b"user-1234"),
        "userName": "nyu@example.com", "userDisplayName": "Nyu",
        "discoverable": true, "userVerified": true, "now": NOW,
    });
    let created = parse(&passkeys::create(&request.to_string()).unwrap());
    assert_eq!(created["itemId"], "i1");
    assert_eq!(created["publicKeyAlgorithm"], -7);
    assert_eq!(created["transports"], json!(["internal", "hybrid"]));
    let public_key = b64(&created["publicKey"]);
    let auth_data = b64(&created["authenticatorData"]);
    let id = check_attested(
        &auth_data,
        passkey::UP | passkey::UV | passkey::BE | passkey::BS | passkey::AT,
        &public_key,
    );
    assert_eq!(id, b64(&created["credentialId"]));
    let attestation = b64(&created["attestationObject"]);
    assert!(attestation.starts_with(b"\xa3\x63fmt\x64none\x67attStmt\xa0\x68authData"));
    assert!(attestation.ends_with(&auth_data));

    // The passkey is under the item's own key, and the item keeps its password.
    let mut stored = created["cipher"].clone();
    let raw = &stored["login"]["fido2Credentials"][0];
    let opened = Passkey::open(raw, &item_key).unwrap();
    assert_eq!(opened.credential_id_bytes().unwrap(), id);
    assert!(Passkey::open(raw, &account.user_key).is_err());
    stored["id"] = "i1".into();
    session::open(&sync(&account, vec![stored])).unwrap();
    assert_eq!(view::reveal("i1", "password", 0).unwrap(), "hunter2");

    let index = parse(&autofill::index().unwrap());
    let listed = &index[0]["passkeys"][0];
    assert_eq!(b64(&listed["credentialId"]), id);
    assert_eq!(listed["rpId"], "example.com");
    assert_eq!(listed["userName"], "nyu@example.com");
    assert_eq!(listed["userDisplayName"], "Nyu");
    assert_eq!(b64(&listed["userHandle"]), b"user-1234");
    assert_eq!(listed["discoverable"], true);
    assert_eq!(listed["counter"], 0);
    assert_eq!(index[0]["uris"][0]["uri"], "https://example.com/login");
    assert_eq!(index[0]["hasPassword"], true);
    assert!(!index.to_string().contains("hunter2"));

    let client_data_hash = Sha256::digest(b"{\"type\":\"webauthn.get\"}");
    let request = json!({
        "itemId": "i1", "credentialId": created["credentialId"], "rpId": "example.com",
        "clientDataHash": URL_SAFE_NO_PAD.encode(client_data_hash), "userVerified": false,
    });
    let asserted = parse(&passkeys::assert(&request.to_string()).unwrap());
    let auth_data = b64(&asserted["authenticatorData"]);
    assert_eq!(auth_data.len(), 37);
    assert_eq!(auth_data[..32], Sha256::digest(b"example.com")[..]);
    assert_eq!(auth_data[32], passkey::UP | passkey::BE | passkey::BS);
    assert_eq!(auth_data[33..], [0, 0, 0, 0]);
    verify(
        &public_key,
        &auth_data,
        &client_data_hash,
        &b64(&asserted["signature"]),
    );
    assert_eq!(b64(&asserted["userHandle"]), b"user-1234");
    assert_eq!(asserted["credentialId"], created["credentialId"]);
    // A counter at 0 stays there, and nothing needs saving.
    assert_eq!(asserted["cipher"], Value::Null);

    // Another site, or another id, finds nothing.
    let mut other = request.clone();
    other["rpId"] = "example.net".into();
    assert_eq!(kind(passkeys::assert(&other.to_string())), "not-found");
    let mut other = request.clone();
    other["credentialId"] = URL_SAFE_NO_PAD.encode([0u8; 16]).into();
    assert_eq!(kind(passkeys::assert(&other.to_string())), "not-found");
    let mut other = request;
    other["clientDataHash"] = "AAAA".into();
    assert_eq!(kind(passkeys::assert(&other.to_string())), "invalid");
}

#[test]
fn a_passkey_in_a_new_login() {
    let account = account();
    unlocked(&account, vec![]);
    let request = json!({
        "itemId": null, "name": "", "folderId": "f1",
        "rpId": "example.com", "rpName": "Example", "userHandle": null,
        "userName": "nyu", "userDisplayName": null,
        "discoverable": false, "userVerified": false, "now": NOW,
    });
    let created = parse(&passkeys::create(&request.to_string()).unwrap());
    assert_eq!(created["itemId"], Value::Null);
    let public_key = b64(&created["publicKey"]);
    let id = check_attested(
        &b64(&created["authenticatorData"]),
        passkey::UP | passkey::BE | passkey::BS | passkey::AT,
        &public_key,
    );

    let mut stored = created["cipher"].clone();
    assert_eq!(stored["folderId"], "f1");
    stored["id"] = "new".into();
    session::open(&sync(&account, vec![stored])).unwrap();
    let detail = parse(&view::item("new").unwrap());
    assert_eq!(detail["summary"]["name"], "Example");
    assert_eq!(detail["login"]["username"], "nyu");
    assert_eq!(detail["login"]["uris"][0]["uri"], "https://example.com");
    assert_eq!(detail["login"]["uris"][0]["match"], Value::Null);
    assert_eq!(detail["login"]["passkeys"], 1);

    let client_data_hash = [9u8; 32];
    let request = json!({
        "itemId": "new", "credentialId": URL_SAFE_NO_PAD.encode(&id), "rpId": "example.com",
        "clientDataHash": URL_SAFE_NO_PAD.encode(client_data_hash), "userVerified": true,
    });
    let asserted = parse(&passkeys::assert(&request.to_string()).unwrap());
    let auth_data = b64(&asserted["authenticatorData"]);
    assert_eq!(
        auth_data[32],
        passkey::UP | passkey::UV | passkey::BE | passkey::BS
    );
    verify(
        &public_key,
        &auth_data,
        &client_data_hash,
        &b64(&asserted["signature"]),
    );
    assert_eq!(asserted["userHandle"], Value::Null);
}

#[test]
fn a_counting_passkey_counts_and_is_saved() {
    let account = account();
    let mut passkey = Passkey::generate(
        "example.com",
        None,
        Some(b"u"),
        Some("nyu"),
        None,
        true,
        NOW,
    )
    .unwrap();
    passkey.counter = 5;
    let mut item = login("Example", "nyu", "hunter2");
    item.reprompt = true;
    item.login.as_mut().unwrap().passkeys = Some(vec![
        json!({ "credentialId": "2.broken" }),
        passkey.seal(&account.user_key),
    ]);
    unlocked(&account, vec![cipher(&item, "i1", &account.user_key)]);

    let id = URL_SAFE_NO_PAD.encode(passkey.credential_id_bytes().unwrap());
    let request = json!({
        "itemId": "i1", "credentialId": id, "rpId": "example.com",
        "clientDataHash": URL_SAFE_NO_PAD.encode([1u8; 32]),
    })
    .to_string();
    assert_eq!(kind(passkeys::assert(&request)), "reprompt");
    let create = json!({ "itemId": "i1", "rpId": "example.com", "now": NOW }).to_string();
    assert_eq!(kind(passkeys::create(&create)), "reprompt");
    session::verify_reprompt("i1", PASSWORD).unwrap();

    // The one that doesn't open is left out of the index, the other listed.
    let index = parse(&autofill::index().unwrap());
    assert_eq!(index[0]["passkeys"].as_array().unwrap().len(), 1);
    assert_eq!(index[0]["passkeys"][0]["counter"], 5);

    let asserted = parse(&passkeys::assert(&request).unwrap());
    assert_eq!(b64(&asserted["authenticatorData"])[33..], [0, 0, 0, 6]);
    let cipher = &asserted["cipher"];
    let saved = &cipher["login"]["fido2Credentials"];
    assert_eq!(saved.as_array().unwrap().len(), 2);
    assert_eq!(saved[0], json!({ "credentialId": "2.broken" }));
    assert_eq!(
        Passkey::open(&saved[1], &account.user_key).unwrap().counter,
        6
    );
    assert_eq!(cipher["reprompt"], 1);

    // The vault in here counts along, before the next sync.
    let again = parse(&passkeys::assert(&request).unwrap());
    assert_eq!(b64(&again["authenticatorData"])[33..], [0, 0, 0, 7]);
}

// ── UwULock Server's extras ───────────────────────────────

/// An RSA key pair for the account (PKCS#8), the one of uwulock-core's tests.
const PRIVATE: &str = "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC2EmCSTyx6YUpNZSRI60oly0VN2cZ9Z4LFw3CuK6zIyVdFW4nGS3R4Ml5X5pdIc7lVn2FNmpi2j/1/TKFZymm/Kb4cgTmRiImF1Gc2OO9v5xlcFyJHDW0Jl8kL3fHNZvz+8ajCtXcVa29GuqCQIdoPgLEYTfCzqhSQc5T77X3QD1+DoO2nY3kXU7t+1GeXMgfUfcEJv0YPjGoofJZMP8GzMJijJTVTJc+M0WhRLPmm6XCr9E0m3OXZxNynYz6euGtPAmfm/ld5QQ+Gu+XYMnZvthAdmzwy9HyMv4cqtFVB8Q2IJOemKymg1ADNLQ453gZMXRDRSUG7kLsA2Ddsusp9AgMBAAECggEAOckbXVRDiZPXQTkYiwwiPFyHYm370VFI7/tXh+/UpuVADYM/9u97x6o0xzEoUpZn/ATZnQez8D1C92Qa0aSsaz+UVvesjcQH4bHIEC2B0MJICjJNbr+UG7dQ17NZSxektEV+ik2Nvf6bEpeo3hXgX4s4qb4S5vLUFASbBFob1CyDtjXZyL+vXiyKd8VxIF4tkG/3E/BUkJ5WANWa0psTxNbFfuOnV87lFo7Cycmx0ynZGIYdfzhz1Aq4f6cQiPdy3cDRuYWw0gKqwwrpFrMpAo2O+x8cVkS2oltZP0GkNoTZLIgcwVAhEE7K2qX1w46Hc0wjf/WlWGjoIvmBnQgDAQKBgQDhwEp56NCveTWfBSav6ZMy1YM44eJXtbUhTMRgSQnkloThnH2MXZmpX/OE8SBmmZgBCnVbwO/b90NkVDEYDawVGbEYfycfX74moakkm1zqB5P6qjacihFxwZXsudD8eDygbwvff/YKheeUGPPDwgJHqGGO3V+4b+bXiVCQnwFFZwKBgQDOd8xou+WXoCMf94N3aOcg1BRu5smHJs1bYRcubaxVPHFMa8LCOJ8XiS0iFqPdhxHgPdzNGT/n3DjZFV4P64jX6qNu+6N/fOojHSBj4FOVMq+nESblG3uyF1jrPu65xULOKgRwAjV8cPsOdWUzHTiOkZ/a0CgXseNiI57VAAS+ewKBgDWQaJtwcEOSYPSwRjOrGjAPlSkj/46MIMQb8ORfsCc6x6C4ftmVQ+Z6S8+ZXvS5MOXeU2ZH6yGoE6d0iomIhPIkvG5xjRjWoMmNxhJXgr5MugHZ7UdLQ0RYiHg4xquA4/G1J34KYJiymPX8zan/GIdkHnHFePbMJluxyxnlgGm1AoGBAJoEU79tKv/IvWsDQFa7Mm8SxYtVLdBb6aTY8Gn59iw/QmU3nbk0c7ki40Aik2qVb4hPnX6B72IOrXmCrwBBO3uV1QTdQkG/9QjsmVTn6nHJta5y5QjTT5qyP+p8r6h0tjkErvq/KxcBUMagXDWc/qubhhu8W6wRTwXOfJV3xhIxAoGAdB22F2D27iY55uz8VCQRzS4DehPx0eyipYy5KmT9MRv2FV9877tH/3wQ8amvHVBvJ/sQ4A7M1BcL/yUy1P6GD7DvM1KGXA1eybnCK+HINYr57A1rKA78kXPrXuogGYkUiy8LbMP3UvqV1q+BTlzJqfJqr20Fs6pjCEqoUu/uols=";

/// Another account's key pair (RSA-1024, only to be another one).
const OTHER_PRIVATE: &str = "MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBAK97Tqp336NvtZYtBTUPt8TYMq6+jMntikTj9+s2tnT+vVt8EX+6GDH8jkN6E1wLbrHp2Qy5qxEmMxiE8rX6NkRMpWLhTJc128QA+MC5k929V4cId/luNh2piCw5O4bL4pINj5MJbDCvNNLrirNA/NkryjUU4vOWRsNrc32x0RqNAgMBAAECgYBwBRsWnydYQbt9fofQc5QwSIMyIdnmHYkiqRReRrL6xJNEj1LsYnOHlV2LnaY2H+YuFMXF5dBaRjRf9p6ppGx25c0kz0eVG7o87LyG0xty313GL6dn0MQpYmmSbbONdQrdyYK/aue71nBOHe1qXSl84FgmTLkFL9fYneZARrQygQJBAORFaMQUHEjlZZiVS/jZgSv9anuKJTsosKLo3qc5cGE78JFL/sc+yH0Cmyv6xQVDc9kO7C25NovVS121yJQYwZkCQQDEzEtM2XwBvzgbuBk8wtfD8Objo73gMxUEWjNcRSJIuXXdrWKu9MztnK/wWMh/y1leDtTau64nXkCHvS4fSKEVAkACcQ+e0UxAJ1v/1tD6N3FfRBWofqDJUjUZeP4wsbeXAqofE74E6ZIBbE62mLcUyFTr5HH4RzvjIQPuW6xqkR05AkBkU0mn+c9wDI2MBARJp4LbjvoF3rmzjBcQyvMX/N6HeJSP2A5Q5td54sEGpBxCmeYLP0Bf6gHUbAY1rMnQhPQpAkEApSSl85X17/XhfTqOLcMekigcAqD0Iwlp5ykCAJKgWpNGryb/PC0RRQcdAlZYucJTkavFxup2THe8tKxhQpGknA==";

fn private_key() -> crypto::PrivateKey {
    use base64::engine::general_purpose::STANDARD;
    crypto::PrivateKey::from_der(&STANDARD.decode(PRIVATE).unwrap()).unwrap()
}

/// A sync with the account's private key and one organisation.
fn sync_with_keys(account: &Account, org_key: &SymmetricKey, ciphers: Vec<Value>) -> String {
    let private = private_key();
    let der = private.to_der().unwrap();
    json!({
        "profile": {
            "email": EMAIL,
            "key": account.protected,
            "privateKey": EncString::encrypt(&der, &account.user_key).to_string(),
            "organizations": [{
                "id": "o1",
                "name": "Family",
                "key": crypto::wrap_for(&private.public(), org_key).unwrap().to_string(),
            }],
        },
        "folders": [],
        "ciphers": ciphers,
    })
    .to_string()
}

/// `GET /uwu/v1/keys` with the extras key under the user key (or not) and
/// under the private key's wrap key.
fn keys(account: &Account, extras: &SymmetricKey, under_user: bool) -> String {
    let wrapped = uwulock_core::extras::wrap(extras, &account.user_key, &private_key()).unwrap();
    json!({
        "object": "uwuKeys",
        "extrasKey": {
            "userKeyWrapped": under_user.then_some(wrapped.user_key_wrapped),
            "privateKeyWrapped": wrapped.private_key_wrapped,
            "revisionDate": "2026-09-28T12:00:00.000000Z",
        },
        "lost": false,
    })
    .to_string()
}

/// The smallest thing `png_size` takes for a PNG of `side` × `side`.
fn png(side: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    png.extend(side.to_be_bytes());
    png.extend(side.to_be_bytes());
    png.extend([8, 6, 0, 0, 0]);
    png
}

fn state(answer: String) -> String {
    parse(&answer)["state"].as_str().unwrap().to_string()
}

#[test]
fn the_extras_key_opens_under_the_user_key_or_the_private_key() {
    let account = account();
    let org_key = SymmetricKey::generate();
    let extras_key = SymmetricKey::generate();
    session::unlock_with_password(EMAIL, KDF, &account.protected, PASSWORD).unwrap();
    session::open(&sync_with_keys(&account, &org_key, vec![])).unwrap();

    assert_eq!(
        state(extras::open_extras(&keys(&account, &extras_key, true)).unwrap()),
        "open"
    );
    // After an official client rotated the user key only the private key's wrap is left.
    assert_eq!(
        state(extras::open_extras(&keys(&account, &extras_key, false)).unwrap()),
        "open"
    );
    crate::with_unlocked(|u| {
        assert_eq!(u.extras.as_ref().unwrap().to_bytes(), extras_key.to_bytes());
        Ok(())
    })
    .unwrap();

    let none = r#"{"object":"uwuKeys","extrasKey":null,"lost":false}"#;
    assert_eq!(state(extras::open_extras(none).unwrap()), "none");
    let lost = r#"{"object":"uwuKeys","extrasKey":null,"lost":true}"#;
    assert_eq!(state(extras::open_extras(lost).unwrap()), "lost");
    crate::with_unlocked(|u| {
        assert!(u.extras.is_none());
        Ok(())
    })
    .unwrap();

    // Without a private key in the sync, a key that lost its user wrap stays shut.
    session::open(&sync(&account, vec![])).unwrap();
    assert_eq!(
        state(extras::open_extras(&keys(&account, &extras_key, false)).unwrap()),
        "lost"
    );
    // A wrap that isn't under this user key is an error, and nothing is open.
    let other = Account {
        user_key: SymmetricKey::generate(),
        protected: String::new(),
    };
    assert_eq!(
        kind(extras::open_extras(&keys(&other, &extras_key, true))),
        "crypto"
    );

    // An RSA wrap anyone with the public key could make is never taken:
    // 0.3's beta field is not read, and the new one has to be type 2.
    session::open(&sync_with_keys(&account, &org_key, vec![])).unwrap();
    let rsa = crypto::wrap_for(&private_key().public(), &SymmetricKey::generate())
        .unwrap()
        .to_string();
    let legacy = json!({ "extrasKey": { "userKeyWrapped": null, "publicKeyWrapped": rsa } });
    assert_eq!(
        state(extras::open_extras(&legacy.to_string()).unwrap()),
        "lost"
    );
    let forged = json!({ "extrasKey": { "userKeyWrapped": null, "privateKeyWrapped": rsa } });
    assert_eq!(kind(extras::open_extras(&forged.to_string())), "crypto");
    // Two wraps of different keys: an error, and nothing is open.
    let mut mixed = parse(&keys(&account, &extras_key, true));
    mixed["extrasKey"]["privateKeyWrapped"] =
        parse(&keys(&account, &SymmetricKey::generate(), true))["extrasKey"]["privateKeyWrapped"]
            .clone();
    assert_eq!(kind(extras::open_extras(&mixed.to_string())), "crypto");
    crate::with_unlocked(|u| {
        assert!(u.extras.is_none());
        Ok(())
    })
    .unwrap();

    session::lock();
    assert_eq!(kind(extras::open_extras(none)), "locked");
}

#[test]
fn own_icons_open_with_the_extras_or_the_organisations_key() {
    let account = account();
    let org_key = SymmetricKey::generate();
    let extras_key = SymmetricKey::generate();
    let mut shared = login("Shared", "nyu", "pw");
    shared.organization_id = Some("o1".into());
    let mut org_cipher = cipher(&shared, "shared", &org_key);
    org_cipher["organizationId"] = "o1".into();
    let ciphers = vec![
        cipher(&login("Mine", "nyu", "pw"), "mine", &account.user_key),
        org_cipher,
    ];
    session::unlock_with_password(EMAIL, KDF, &account.protected, PASSWORD).unwrap();
    session::open(&sync_with_keys(&account, &org_key, ciphers)).unwrap();

    let icon = |id: &str, key_type: &str, key: &SymmetricKey| {
        json!({
            "object": "ownIcon",
            "cipherId": id,
            "keyType": key_type,
            "data": uwulock_core::extras::seal_icon(&png(64), key).unwrap(),
            "revisionDate": "2026-09-28T12:00:00.000000Z",
        })
    };
    let list = json!([
        icon("mine", "extras", &extras_key),
        icon("shared", "organization", &org_key),
        // The key type has to fit the item; unknown items are left out.
        icon("mine", "organization", &org_key),
        icon("shared", "extras", &extras_key),
        icon("gone", "extras", &extras_key),
    ])
    .to_string();

    // Before the extras key is open only the organisation's icon opens.
    let opened = parse(&extras::open_icons(&list).unwrap());
    assert_eq!(opened.as_array().unwrap().len(), 1);
    assert_eq!(opened[0]["cipherId"], "shared");

    extras::open_extras(&keys(&account, &extras_key, true)).unwrap();
    let opened = parse(&extras::open_icons(&list).unwrap());
    let ids: Vec<&str> = opened
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["cipherId"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["mine", "shared"]);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(opened[0]["png"].as_str().unwrap())
        .unwrap();
    assert_eq!(bytes, png(64));

    // A value that doesn't open is left out too.
    let broken = json!([icon("mine", "extras", &org_key)]).to_string();
    assert_eq!(parse(&extras::open_icons(&broken).unwrap()), json!([]));
}

#[test]
fn file_requests_show_their_labels_and_links() {
    use uwulock_core::file_request::{self, LinkSecret, PublicInfo};
    let account = account();
    let extras_key = SymmetricKey::generate();
    session::unlock_with_password(EMAIL, KDF, &account.protected, PASSWORD).unwrap();
    session::open(&sync_with_keys(&account, &SymmetricKey::generate(), vec![])).unwrap();
    let secret = LinkSecret::generate();
    let info = |key: &crypto::PublicKey| {
        PublicInfo::new("Passport", None, None, key)
            .unwrap()
            .seal(&secret)
            .unwrap()
    };
    let id = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";
    let access_id = file_request::access_id(id).unwrap();
    let request = json!({
        "object": "fileRequest",
        "id": id,
        "accessId": access_id,
        "name": file_request::seal_label("Passport for the bank", &extras_key),
        "linkSecret": secret.seal(&extras_key),
        "publicInfo": info(&private_key().public()),
        "submissionCount": 1,
    });
    let unnamed = json!({ "id": "r2", "name": null, "linkSecret": null });
    let list = json!([request, unnamed]).to_string();

    assert_eq!(kind(extras::file_request_labels(&list)), "unsupported");
    extras::open_extras(&keys(&account, &extras_key, true)).unwrap();
    assert_eq!(
        parse(&extras::file_request_labels(&list).unwrap()),
        json!([{ "id": id, "label": "Passport for the bank" }, { "id": "r2", "label": null }])
    );

    let part = secret.to_link_part();
    assert_eq!(
        extras::file_request_link(&request.to_string(), "https://lock.example.com/", false)
            .unwrap(),
        format!("https://lock.example.com/#/request/{access_id}/{part}")
    );
    assert_eq!(
        extras::file_request_link(&request.to_string(), "https://send.example.com", true).unwrap(),
        format!("https://send.example.com/r/{access_id}#{part}")
    );
    // Without `accessId` it comes from the id.
    let bare = json!({ "id": id, "linkSecret": secret.seal(&extras_key), "publicInfo": request["publicInfo"] }).to_string();
    assert!(
        extras::file_request_link(&bare, "https://lock.example.com", false)
            .unwrap()
            .contains(&access_id)
    );
    assert_eq!(
        kind(extras::file_request_link(
            &unnamed.to_string(),
            "https://lock.example.com",
            false
        )),
        "invalid"
    );
    // Details that encrypt for another key (the server knew the secret and
    // made its own): no link to hand out.
    let other = crypto::PrivateKey::from_der(
        &base64::engine::general_purpose::STANDARD
            .decode(OTHER_PRIVATE)
            .unwrap(),
    )
    .unwrap();
    let mut foreign = request.clone();
    foreign["publicInfo"] = info(&other.public()).into();
    assert_eq!(
        kind(extras::file_request_link(
            &foreign.to_string(),
            "https://lock.example.com",
            false
        )),
        "crypto"
    );
    let mut without = request.clone();
    without["publicInfo"] = Value::Null;
    assert_eq!(
        kind(extras::file_request_link(
            &without.to_string(),
            "https://lock.example.com",
            false
        )),
        "invalid"
    );
}

#[test]
fn an_item_is_shared_as_a_send_without_its_authenticator_key() {
    use uwulock_core::vault::{Field, FieldKind};
    let account = account();
    let mut item = login("Router", "admin", "hunter2");
    item.login.as_mut().unwrap().totp = Some(Zeroizing::new("JBSWY3DPEHPK3PXP".into()));
    item.fields = vec![Field {
        name: Some(Zeroizing::new("PIN".into())),
        value: Some(Zeroizing::new("1234".into())),
        kind: FieldKind::Hidden,
        linked_id: None,
    }];
    let mut guarded = login("Guarded", "g", "secret");
    guarded.reprompt = true;
    unlocked(
        &account,
        vec![
            cipher(&item, "r1", &account.user_key),
            cipher(&guarded, "g1", &account.user_key),
        ],
    );

    let fields = parse(&extras::shareable_fields("r1").unwrap());
    assert_eq!(
        fields,
        json!([
            { "name": "username" },
            { "name": "password" },
            { "name": "uri:0" },
            { "name": "field:0", "label": "PIN" },
        ])
    );
    assert_eq!(kind(extras::shareable_fields("g1")), "reprompt");
    assert_eq!(kind(extras::shareable_fields("nope")), "not-found");

    let options = json!({
        "fields": [["username", "Username"], ["password", "Password"], ["totp", "Code"], ["field:0", "Field"]],
        "deletionDate": "2026-09-29T12:00:00.000Z",
        "maxAccessCount": 1,
        "password": "open sesame",
    });
    let request = parse(&extras::seal_share("r1", &options.to_string()).unwrap());
    assert_eq!(request["type"], 0);
    assert_eq!(request["maxAccessCount"], 1);
    assert_eq!(request["deletionDate"], "2026-09-29T12:00:00.000Z");
    assert_eq!(request["authType"], 1);
    assert!(request["password"]
        .as_str()
        .is_some_and(|p| p != "open sesame"));

    let key = request["key"].as_str().unwrap();
    let seed = uwulock_core::send::open_seed(key, &account.user_key).unwrap();
    let send_key = crypto::send_key(&seed).unwrap();
    let open = |value: &Value| {
        value
            .as_str()
            .unwrap()
            .parse::<EncString>()
            .unwrap()
            .decrypt_string(&send_key)
            .unwrap()
            .to_string()
    };
    assert_eq!(open(&request["name"]), "Router");
    let text = open(&request["text"]["text"]);
    assert_eq!(
        text,
        "Router\nUsername: admin\nPassword: hunter2\nPIN: 1234"
    );
    assert!(!text.contains("JBSWY3DP"));

    let link = extras::send_link(key, "AccessId1", "https://lock.example.com", false).unwrap();
    assert_eq!(
        link,
        format!(
            "https://lock.example.com/#/send/AccessId1/{}",
            URL_SAFE_NO_PAD.encode(&*seed)
        )
    );
    assert!(
        extras::send_link(key, "AccessId1", "https://send.example.com", true)
            .unwrap()
            .starts_with("https://send.example.com/AccessId1#")
    );

    // Nothing with a value chosen, and the re-prompt first.
    let empty = json!({ "fields": [["totp", "Code"]], "deletionDate": "2026-09-29T12:00:00.000Z" });
    assert_eq!(
        kind(extras::seal_share("r1", &empty.to_string())),
        "invalid"
    );
    assert_eq!(
        kind(extras::seal_share("g1", &options.to_string())),
        "reprompt"
    );
    session::verify_reprompt("g1", PASSWORD).unwrap();
    assert!(extras::seal_share("g1", &options.to_string()).is_ok());
}

/// A Wi-Fi network (docs/wifi.md): listed as `wifi` with its SSID, never offered
/// for filling, and saved as the secure note it is, every other field kept.
#[test]
fn a_wifi_network_is_listed_as_one_and_saved_as_a_note() {
    use uwulock_core::vault::{Field, FieldKind};
    let field = |name: &str, value: &str, kind: FieldKind| Field {
        name: Some(Zeroizing::new(name.into())),
        value: Some(Zeroizing::new(value.into())),
        kind,
        linked_id: None,
    };
    let account = account();
    let mut item = Item::new(ItemKind::Note);
    item.name = Zeroizing::new("Home".into());
    item.fields = vec![
        field("uwulock:type", "wifi", FieldKind::Text),
        field("SSID", " uwu-net ", FieldKind::Text),
        field("Password", "correct; horse", FieldKind::Hidden),
        field("Security", "WPA2", FieldKind::Text),
        field("Hidden network", "false", FieldKind::Boolean),
        field("Router admin", "http://192.0.2.1", FieldKind::Text),
    ];
    let mut note = Item::new(ItemKind::Note);
    note.name = Zeroizing::new("Plain".into());
    unlocked(
        &account,
        vec![
            cipher(&item, "w1", &account.user_key),
            cipher(&note, "n1", &account.user_key),
        ],
    );

    let items = parse(&view::items().unwrap());
    let wifi = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "w1")
        .unwrap();
    assert_eq!(wifi["kind"], "wifi");
    assert_eq!(wifi["subtitle"], "uwu-net");
    let plain = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "n1")
        .unwrap();
    assert_eq!(plain["kind"], "note");

    // Autofill knows it only as the note it is, and offers notes nowhere.
    let index = parse(&autofill::index().unwrap());
    let entry = index
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "w1")
        .unwrap();
    assert_eq!(entry["kind"], "note");

    assert_eq!(view::reveal("w1", "field:2", 0).unwrap(), "correct; horse");

    // The editor's draft: the SSID changed, the password left alone (`from`).
    let draft = json!({
        "kind": "note", "name": "Home", "notes": null, "favorite": false, "reprompt": false,
        "folderId": null,
        "fields": [
            { "name": "uwulock:type", "kind": "text", "value": "wifi", "from": 0 },
            { "name": "SSID", "kind": "text", "value": "uwu-net-5g", "from": 1 },
            { "name": "Password", "kind": "hidden", "value": null, "from": 2 },
            { "name": "Security", "kind": "text", "value": "WPA2", "from": 3 },
            { "name": "Hidden network", "kind": "boolean", "value": "false", "from": 4 },
            { "name": "Router admin", "kind": "text", "value": "http://192.0.2.1", "from": 5 },
        ],
    });
    let mut sealed = parse(&draft::seal_draft("w1", &draft.to_string(), NOW).unwrap());
    assert_eq!(
        sealed["type"], 2,
        "still a secure note for Bitwarden's apps"
    );
    sealed["id"] = "w1".into();
    session::open(&sync(&account, vec![sealed])).unwrap();
    let detail = parse(&view::item("w1").unwrap());
    assert_eq!(detail["summary"]["kind"], "wifi");
    assert_eq!(detail["summary"]["subtitle"], "uwu-net-5g");
    assert_eq!(detail["fields"].as_array().unwrap().len(), 6);
    assert_eq!(detail["fields"][5]["name"], "Router admin");
    assert_eq!(view::reveal("w1", "field:2", 0).unwrap(), "correct; horse");

    // `wifi` is no kind of the vault's: a draft has to say `note`.
    let wrong = json!({ "kind": "wifi", "name": "Home", "fields": [] });
    assert!(draft::seal_draft("", &wrong.to_string(), NOW).is_err());
}
