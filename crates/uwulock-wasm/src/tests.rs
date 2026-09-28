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

use crate::{autofill, draft, generator, passkeys, session, view, Failure};

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
