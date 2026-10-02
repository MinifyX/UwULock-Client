//! An item shared as a Send that UwULock's Send page shows as an entry:
//! fields with copy buttons and live one-time codes, rather than raw text.
//!
//! It is still a plain text Send, so official Bitwarden clients open it too.
//! The text is the human-readable lines of [`crate::send::share_text`] —
//! never the authenticator key — followed by one last line
//!
//! ```text
//! uwulock-entry:v2:<base64url(JSON)>.<base64url(tag)>
//! ```
//!
//! with the JSON `{name, username?, password?, websites[], notes?,
//! fields[{name, value, hidden}], totp?}` ([`Entry`]). `totp` is the
//! authenticator key (a secret or an `otpauth://` URI), there only so the
//! page can make the codes; a page shows the live codes, never the key nor a
//! QR code of it. **The key itself is in the Send**: whoever opens the Send
//! (or reads its raw text in a Bitwarden app) can take it out and make codes
//! for good, also after the Send is gone. The apps say so and ask before it
//! goes in.
//!
//! `tag` is HMAC-SHA256 over `uwulock-entry:v2:<base64url(JSON)>` with a key
//! made from the Send's 16-byte seed (the part of the link after `#`):
//! HKDF-SHA256, salt `uwulock-entry-send`, info `v2 tag`, 32 bytes
//! ([`tag_key`]). Only whoever made the Send had the seed when the text was
//! written, so a marker line someone put into an item's notes beforehand (and
//! that then ended up last in a plain Send) has no valid tag and the text
//! stays plain text. That is why [`share_entry_text`], [`decode`] and
//! [`readable_part`] take the seed, and why the Send must be sealed with that
//! same seed ([`crate::send::TextSend::seal_with_seed`]).
//!
//! [`decode`] reads such a text back. A text without the marker, with a
//! marker of another version, with a wrong tag, or one that doesn't decode
//! (or is too big), is no entry: the page shows it as the plain text it is.

use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::send::{share_text, shareable_value, withheld};
use crate::vault::{FieldKind, Item};

/// What the last line of an entry Send starts with.
pub const MARKER: &str = "uwulock-entry:v2:";

/// The longest Send text [`decode`] looks at (bytes).
pub const MAX_TEXT: usize = 256 * 1024;
/// At most this many websites, and this many fields, in a decoded entry.
pub const MAX_WEBSITES: usize = 100;
pub const MAX_FIELDS: usize = 200;

/// An entry as it travels in a Send. Secret values are wiped when dropped
/// and `Debug` doesn't show them.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<Zeroizing<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Zeroizing<String>>,
    #[serde(default)]
    pub websites: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Zeroizing<String>>,
    #[serde(default)]
    pub fields: Vec<EntryField>,
    /// The authenticator key, only for making the codes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp: Option<Zeroizing<String>>,
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hidden = |o: &Option<Zeroizing<String>>| o.as_ref().map(|_| "…");
        f.debug_struct("Entry")
            .field("name", &self.name)
            .field("username", &hidden(&self.username))
            .field("password", &hidden(&self.password))
            .field("websites", &self.websites)
            .field("notes", &hidden(&self.notes))
            .field("fields", &self.fields)
            .field("totp", &hidden(&self.totp))
            .finish()
    }
}

/// A value without a place of its own in [`Entry`]: a custom field, or a
/// card's, an identity's or an SSH key's value, under the label the sharer
/// read.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryField {
    pub name: String,
    pub value: Zeroizing<String>,
    /// Shown as dots until a click, like a hidden field.
    #[serde(default)]
    pub hidden: bool,
}

impl std::fmt::Debug for EntryField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntryField")
            .field("name", &self.name)
            .field("value", &"…")
            .field("hidden", &self.hidden)
            .finish()
    }
}

/// Whether a website of an entry may be a link on a page: `http` and `https`
/// only (no `javascript:`, `data:`, …). Every renderer uses this one rule.
pub fn openable(website: &str) -> bool {
    url::Url::parse(website.trim()).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

/// Values that are secret wherever they show: dots until a click.
fn sensitive(item: &Item, name: &str) -> bool {
    match name {
        "card-number" | "card-code" | "ssh-private" => true,
        "identity:ssn" | "identity:passportNumber" | "identity:licenseNumber" => true,
        _ => name
            .strip_prefix("field:")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|i| item.fields.get(i))
            .is_some_and(|f| f.kind == FieldKind::Hidden),
    }
}

/// The entry of an item with the chosen values: `fields` pairs a value's
/// name ([`shareable_value`], plus `totp` for the authenticator key) with
/// the label the person reads. Values without content, and what
/// [`withheld`] names, are left out.
pub fn from_item(item: &Item, fields: &[(String, String)]) -> Entry {
    let mut entry = Entry {
        name: item.name.to_string(),
        ..Entry::default()
    };
    for (name, label) in fields {
        if withheld(item, name) {
            continue;
        }
        if name == "totp" {
            entry.totp = item
                .login
                .as_ref()
                .and_then(|l| l.totp.as_ref())
                .filter(|t| !t.trim().is_empty())
                .map(|t| Zeroizing::new(t.to_string()));
            continue;
        }
        let Some(value) = shareable_value(item, name) else {
            continue;
        };
        let value = Zeroizing::new(value.to_string());
        match name.as_str() {
            "username" => entry.username = Some(value),
            "password" => entry.password = Some(value),
            "notes" => entry.notes = Some(value),
            _ if name.starts_with("uri:") => entry.websites.push(value.to_string()),
            _ => {
                // A custom field keeps its own name, the others the label.
                let own = name
                    .strip_prefix("field:")
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|i| item.fields.get(i))
                    .and_then(|f| f.name.as_ref())
                    .filter(|n| !n.is_empty())
                    .map(|n| n.to_string());
                entry.fields.push(EntryField {
                    name: own.unwrap_or_else(|| label.clone()),
                    value,
                    hidden: sensitive(item, name),
                });
            }
        }
    }
    entry
}

type HmacSha256 = Hmac<Sha256>;

/// The key of a marker's tag, from the Send's 16-byte seed: HKDF-SHA256,
/// salt `uwulock-entry-send`, info `v2 tag`, 32 bytes.
pub fn tag_key(seed: &[u8]) -> Zeroizing<[u8; 32]> {
    let hkdf = hkdf::Hkdf::<Sha256>::new(Some(b"uwulock-entry-send"), seed);
    let mut key = Zeroizing::new([0u8; 32]);
    hkdf.expand(b"v2 tag", key.as_mut())
        .expect("32 bytes is a valid length");
    key
}

fn tag(seed: &[u8], signed: &str) -> HmacSha256 {
    let key = tag_key(seed);
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key.as_ref()).expect("any key length");
    mac.update(signed.as_bytes());
    mac
}

/// `readable` with the marker line of `entry` below it, tagged with the
/// Send's `seed`: the whole text of an entry Send.
pub fn encode(readable: &str, entry: &Entry, seed: &[u8]) -> Zeroizing<String> {
    let json = Zeroizing::new(serde_json::to_vec(entry).expect("an entry is JSON"));
    let mut out = Zeroizing::new(String::with_capacity(
        readable.len() + MARKER.len() + json.len() * 4 / 3 + 50,
    ));
    out.push_str(readable.trim_end_matches(['\n', '\r']));
    out.push('\n');
    let line_start = out.len();
    out.push_str(MARKER);
    URL_SAFE_NO_PAD.encode_string(json.as_slice(), &mut out);
    let mac = tag(seed, &out[line_start..]).finalize().into_bytes();
    out.push('.');
    URL_SAFE_NO_PAD.encode_string(mac, &mut out);
    out
}

/// The text of a Send that shares an item as an entry: the lines of
/// [`share_text`] (no authenticator key among them), then the marker line
/// of [`from_item`], tagged with the `seed` the Send is sealed with.
pub fn share_entry_text(
    item: &Item,
    fields: &[(String, String)],
    seed: &[u8],
) -> Zeroizing<String> {
    encode(&share_text(item, fields), &from_item(item, fields), seed)
}

/// The last line of a text, and what is above it.
fn split_last(text: &str) -> (&str, &str) {
    let trimmed = text.trim_end_matches(['\n', '\r', ' ', '\t']);
    match trimmed.rfind('\n') {
        Some(at) => (
            trimmed[..at].trim_end_matches('\r'),
            trimmed[at + 1..].trim(),
        ),
        None => ("", trimmed.trim()),
    }
}

/// The entry in a Send's text, if its last line is a marker this version
/// reads with a valid tag for the Send's `seed`; `None` means: show the
/// text as it is.
pub fn decode(text: &str, seed: &[u8]) -> Option<Entry> {
    if text.len() > MAX_TEXT {
        return None;
    }
    let (_, last) = split_last(text);
    let encoded = last.strip_prefix(MARKER)?;
    let (payload, given) = encoded.rsplit_once('.')?;
    let given = URL_SAFE_NO_PAD.decode(given.trim_end_matches('=')).ok()?;
    tag(seed, &last[..MARKER.len() + payload.len()])
        .verify_slice(&given)
        .ok()?;
    let json = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(payload.trim_end_matches('='))
            .or_else(|_| URL_SAFE.decode(payload))
            .ok()?,
    );
    let entry: Entry = serde_json::from_slice(&json).ok()?;
    (entry.websites.len() <= MAX_WEBSITES && entry.fields.len() <= MAX_FIELDS).then_some(entry)
}

/// The human-readable part of an entry Send's text: everything above the
/// marker line. A text that isn't an entry Send (for this `seed`) comes
/// back whole.
pub fn readable_part<'a>(text: &'a str, seed: &[u8]) -> &'a str {
    if decode(text, seed).is_none() {
        return text;
    }
    split_last(text).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{Field, ItemKind, Login, LoginUri};

    const SECRET: &str = "JBSWY3DPEHPK3PXP";
    const SEED: &[u8] = &[7u8; 16];
    const OTHER: &[u8] = &[8u8; 16];

    fn uri(text: &str) -> LoginUri {
        LoginUri {
            uri: Zeroizing::new(text.into()),
            match_kind: None,
            checksum: None,
        }
    }

    fn login() -> Item {
        let mut item = Item::new(ItemKind::Login);
        item.name = Zeroizing::new("Shop".into());
        item.notes = Some(Zeroizing::new("two\nlines".into()));
        item.login = Some(Login {
            username: Some(Zeroizing::new("nyu".into())),
            password: Some(Zeroizing::new("hunter2".into())),
            totp: Some(Zeroizing::new(format!(
                "otpauth://totp/Shop:nyu?secret={SECRET}&issuer=Shop"
            ))),
            uris: vec![
                uri("https://shop.example.com"),
                uri("https://login.example.com"),
            ],
            ..Login::default()
        });
        item.fields.push(Field {
            name: Some(Zeroizing::new("PIN".into())),
            value: Some(Zeroizing::new("1234".into())),
            kind: FieldKind::Hidden,
            linked_id: None,
        });
        item.fields.push(Field {
            name: Some(Zeroizing::new("Customer".into())),
            value: Some(Zeroizing::new("42".into())),
            kind: FieldKind::Text,
            linked_id: None,
        });
        item
    }

    fn chosen(names: &[&str]) -> Vec<(String, String)> {
        names
            .iter()
            .map(|n| (n.to_string(), format!("L-{n}")))
            .collect()
    }

    #[test]
    fn round_trip() {
        let item = login();
        let fields = chosen(&[
            "username", "password", "uri:0", "uri:1", "notes", "field:0", "field:1", "totp",
        ]);
        let text = share_entry_text(&item, &fields, SEED);
        let entry = decode(&text, SEED).expect("an entry");
        assert!(decode(&text, OTHER).is_none());
        assert_eq!(entry.name, "Shop");
        assert_eq!(entry.username.as_deref().map(|s| s.as_str()), Some("nyu"));
        assert_eq!(
            entry.password.as_deref().map(|s| s.as_str()),
            Some("hunter2")
        );
        assert_eq!(
            entry.websites,
            ["https://shop.example.com", "https://login.example.com"]
        );
        assert_eq!(
            entry.notes.as_deref().map(|s| s.as_str()),
            Some("two\nlines")
        );
        assert_eq!(
            entry.fields,
            vec![
                EntryField {
                    name: "PIN".into(),
                    value: Zeroizing::new("1234".into()),
                    hidden: true
                },
                EntryField {
                    name: "Customer".into(),
                    value: Zeroizing::new("42".into()),
                    hidden: false
                },
            ]
        );
        assert!(entry.totp.as_deref().unwrap().contains(SECRET));
        assert_eq!(entry, from_item(&item, &fields));
        // And the entry makes the codes.
        let totp = crate::totp::Totp::parse(entry.totp.as_deref().unwrap()).unwrap();
        assert_eq!(totp.code_at(59).0.len(), 6);
    }

    #[test]
    fn the_readable_part_never_holds_the_authenticator_key() {
        let item = login();
        let fields = chosen(&["username", "password", "uri:1", "totp", "field:0"]);
        let text = share_entry_text(&item, &fields, SEED);
        let readable = readable_part(&text, SEED);
        assert_eq!(
            readable,
            "Shop\nL-username: nyu\nL-password: hunter2\nL-uri:1: https://login.example.com\nPIN: 1234"
        );
        assert!(!readable.contains(SECRET));
        assert!(!readable.contains("otpauth"));
        assert!(!readable.contains(MARKER));
        let (head, last) = text.rsplit_once('\n').unwrap();
        assert_eq!(head, readable);
        assert!(last.starts_with(MARKER));
        // Without `totp` chosen the key isn't in the JSON either.
        let without = share_entry_text(&item, &chosen(&["username"]), SEED);
        assert!(decode(&without, SEED).unwrap().totp.is_none());
    }

    #[test]
    fn withheld_values_stay_out_of_the_entry() {
        let mut item = login();
        item.view_password = false;
        let entry = from_item(
            &item,
            &chosen(&["username", "password", "totp", "field:0", "field:1"]),
        );
        assert_eq!(entry.username.as_deref().map(|s| s.as_str()), Some("nyu"));
        assert!(entry.password.is_none() && entry.totp.is_none());
        assert_eq!(entry.fields.len(), 1);
        assert_eq!(entry.fields[0].name, "Customer");
    }

    #[test]
    fn other_values_become_fields_under_their_label() {
        let mut item = Item::new(ItemKind::Card);
        item.name = Zeroizing::new("Card".into());
        item.card = Some(crate::vault::Card {
            number: Some(Zeroizing::new("4111111111111111".into())),
            cardholder_name: Some(Zeroizing::new("Nyu".into())),
            ..Default::default()
        });
        let entry = from_item(
            &item,
            &[
                ("card-name".into(), "Name".into()),
                ("card-number".into(), "Number".into()),
            ],
        );
        assert_eq!(
            entry.fields,
            vec![
                EntryField {
                    name: "Name".into(),
                    value: Zeroizing::new("Nyu".into()),
                    hidden: false
                },
                EntryField {
                    name: "Number".into(),
                    value: Zeroizing::new("4111111111111111".into()),
                    hidden: true
                },
            ]
        );
    }

    fn marker(payload: &str, seed: &[u8]) -> String {
        let signed = format!("{MARKER}{payload}");
        let mac = tag(seed, &signed).finalize().into_bytes();
        format!("{signed}.{}", URL_SAFE_NO_PAD.encode(mac))
    }

    #[test]
    fn plain_and_garbled_texts_are_plain_text() {
        let good = encode(
            "Shop",
            &Entry {
                name: "Shop".into(),
                ..Entry::default()
            },
            SEED,
        );
        assert!(decode(&good, SEED).is_some());
        let line = good.rsplit_once('\n').unwrap().1.to_string();
        let payload = line
            .strip_prefix(MARKER)
            .unwrap()
            .rsplit_once('.')
            .unwrap()
            .0
            .to_string();
        let big = format!(
            "{{\"name\":\"N\",\"websites\":[{}]}}",
            vec!["\"https://a.example.com\""; MAX_WEBSITES + 1].join(",")
        );
        for text in [
            String::new(),
            "just a note".to_string(),
            format!("Shop\nuwulock-entry:v1:{payload}"),
            format!("Shop\nuwulock-entry:v3:{payload}"),
            format!("Shop\n{MARKER}{payload}"),
            format!("Shop\n{MARKER}{payload}."),
            format!("Shop\n{}!!", marker(&payload, SEED)),
            "Shop\nuwulock-entry:v2:".to_string(),
            format!("Shop\n{}", marker("bm90IGpzb24", SEED)),
            // Valid JSON, but no name.
            format!(
                "Shop\n{}",
                marker(&URL_SAFE_NO_PAD.encode(br#"{"websites":[]}"#), SEED)
            ),
            // Too many websites.
            format!("Shop\n{}", marker(&URL_SAFE_NO_PAD.encode(&big), SEED)),
            // The marker, but not on the last line.
            format!("Shop\n{line}\nmore"),
            format!("{line}x"),
            // Too long.
            format!("{}\n{line}", "x".repeat(MAX_TEXT)),
        ] {
            assert!(decode(&text, SEED).is_none(), "{text:?}");
            assert_eq!(readable_part(&text, SEED), text);
        }
        // A trailing newline and the marker alone on one line are fine.
        assert!(decode(&format!("{}\n", good.as_str()), SEED).is_some());
        assert_eq!(decode(&line, SEED).unwrap().name, "Shop");
        assert_eq!(readable_part(&line, SEED), "");
        // Unknown keys of a later v2 are ignored.
        let later = format!(
            "x\n{}",
            marker(
                &URL_SAFE_NO_PAD.encode(br#"{"name":"N","colour":"pink"}"#),
                SEED
            )
        );
        assert_eq!(decode(&later, SEED).unwrap().name, "N");
    }

    #[test]
    fn a_marker_written_before_the_send_was_made_is_plain_text() {
        // Somebody ends an item's notes with a marker line of their own; a
        // plain Send of the item then ends with it. Its tag can't be right:
        // the Send's seed didn't exist yet.
        let fake = Entry {
            name: "Bank".into(),
            websites: vec!["https://phish.example".into()],
            ..Entry::default()
        };
        let planted = encode("", &fake, OTHER);
        let mut item = login();
        item.notes = Some(Zeroizing::new(format!("hello\n{}", planted.trim())));
        let text = share_text(&item, &chosen(&["username", "notes"]));
        assert!(decode(&text, SEED).is_none());
        assert_eq!(readable_part(&text, SEED), text.as_str());
        // Tampering with a real one breaks it too.
        let real = share_entry_text(&item, &chosen(&["username"]), SEED);
        let (head, line) = real.rsplit_once('\n').unwrap();
        let (signed, mac) = line.rsplit_once('.').unwrap();
        let swapped = format!(
            "{head}\n{MARKER}{}.{mac}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&fake).unwrap())
        );
        assert!(decode(&swapped, SEED).is_none());
        assert!(signed.starts_with(MARKER));
    }

    #[test]
    fn secrets_stay_out_of_debug() {
        let entry = from_item(&login(), &chosen(&["password", "totp", "field:0"]));
        let shown = format!("{entry:?}");
        assert!(!shown.contains("hunter2") && !shown.contains(SECRET) && !shown.contains("1234"));
    }

    #[test]
    fn only_web_links_open() {
        assert!(openable("https://a.example.com/x"));
        assert!(openable(" http://a.example.com "));
        for bad in [
            "javascript:alert(1)",
            "data:text/html,x",
            "a.example.com",
            "ftp://a.example.com",
            "",
        ] {
            assert!(!openable(bad), "{bad}");
        }
    }

    #[test]
    fn json_shape_is_the_contract() {
        let entry = Entry {
            name: "N".into(),
            username: Some(Zeroizing::new("u".into())),
            websites: vec!["https://a.example.com".into()],
            fields: vec![EntryField {
                name: "f".into(),
                value: Zeroizing::new("v".into()),
                hidden: true,
            }],
            totp: Some(Zeroizing::new(SECRET.into())),
            ..Entry::default()
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "name": "N", "username": "u", "websites": ["https://a.example.com"],
                "fields": [{"name": "f", "value": "v", "hidden": true}], "totp": SECRET,
            })
        );
    }
}
