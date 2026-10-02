//! An item shared as a Send that UwULock's Send page shows as an entry:
//! fields with copy buttons and live one-time codes, rather than raw text.
//!
//! It is still a plain text Send, so official Bitwarden clients open it too.
//! The text is the human-readable lines of [`crate::send::share_text`] —
//! never the authenticator key — followed by one last line
//!
//! ```text
//! uwulock-entry:v1:<base64url(JSON)>
//! ```
//!
//! with the JSON `{name, username?, password?, websites[], notes?,
//! fields[{name, value, hidden}], totp?}` ([`Entry`]). `totp` is the
//! authenticator key (a secret or an `otpauth://` URI), there only so the
//! page can make the codes; a page shows the live codes, never the key nor a
//! QR code of it.
//!
//! [`decode`] reads such a text back. A text without the marker, with a
//! marker of another version, or with one that doesn't decode, is no entry:
//! the page shows it as the plain text it is.

use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::send::{share_text, shareable_value, withheld};
use crate::vault::{FieldKind, Item};

/// What the last line of an entry Send starts with.
pub const MARKER: &str = "uwulock-entry:v1:";

/// An entry as it travels in a Send.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default)]
    pub websites: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub fields: Vec<EntryField>,
    /// The authenticator key, only for making the codes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp: Option<String>,
}

/// A value without a place of its own in [`Entry`]: a custom field, or a
/// card's, an identity's or an SSH key's value, under the label the sharer
/// read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryField {
    pub name: String,
    pub value: String,
    /// Shown as dots until a click, like a hidden field.
    #[serde(default)]
    pub hidden: bool,
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
                .map(|t| t.to_string());
            continue;
        }
        let Some(value) = shareable_value(item, name) else {
            continue;
        };
        let value = value.to_string();
        match name.as_str() {
            "username" => entry.username = Some(value),
            "password" => entry.password = Some(value),
            "notes" => entry.notes = Some(value),
            _ if name.starts_with("uri:") => entry.websites.push(value),
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

/// `readable` with the marker line of `entry` below it: the whole text of an
/// entry Send.
pub fn encode(readable: &str, entry: &Entry) -> Zeroizing<String> {
    let json = Zeroizing::new(serde_json::to_vec(entry).expect("an entry is JSON"));
    let mut out = Zeroizing::new(String::with_capacity(
        readable.len() + MARKER.len() + json.len() * 4 / 3 + 4,
    ));
    out.push_str(readable.trim_end_matches(['\n', '\r']));
    out.push('\n');
    out.push_str(MARKER);
    URL_SAFE_NO_PAD.encode_string(json.as_slice(), &mut out);
    out
}

/// The text of a Send that shares an item as an entry: the lines of
/// [`share_text`] (no authenticator key among them), then the marker line
/// of [`from_item`].
pub fn share_entry_text(item: &Item, fields: &[(String, String)]) -> Zeroizing<String> {
    encode(&share_text(item, fields), &from_item(item, fields))
}

/// The entry in a Send's text, if its last line is a marker this version
/// reads; `None` means: show the text as it is.
pub fn decode(text: &str) -> Option<Entry> {
    let last = text
        .trim_end_matches(['\n', '\r', ' ', '\t'])
        .rsplit('\n')
        .next()?
        .trim();
    let encoded = last.strip_prefix(MARKER)?;
    let json = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(encoded.trim_end_matches('='))
            .or_else(|_| URL_SAFE.decode(encoded))
            .ok()?,
    );
    serde_json::from_slice(&json).ok()
}

/// The human-readable part of an entry Send's text: everything above the
/// marker line. A text without a valid marker comes back whole.
pub fn readable_part(text: &str) -> &str {
    if decode(text).is_none() {
        return text;
    }
    let trimmed = text.trim_end_matches(['\n', '\r', ' ', '\t']);
    match trimmed.rfind('\n') {
        Some(at) => trimmed[..at].trim_end_matches('\r'),
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{Field, ItemKind, Login, LoginUri};

    const SECRET: &str = "JBSWY3DPEHPK3PXP";

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
        let text = share_entry_text(&item, &fields);
        let entry = decode(&text).expect("an entry");
        assert_eq!(entry.name, "Shop");
        assert_eq!(entry.username.as_deref(), Some("nyu"));
        assert_eq!(entry.password.as_deref(), Some("hunter2"));
        assert_eq!(
            entry.websites,
            ["https://shop.example.com", "https://login.example.com"]
        );
        assert_eq!(entry.notes.as_deref(), Some("two\nlines"));
        assert_eq!(
            entry.fields,
            vec![
                EntryField {
                    name: "PIN".into(),
                    value: "1234".into(),
                    hidden: true
                },
                EntryField {
                    name: "Customer".into(),
                    value: "42".into(),
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
        let text = share_entry_text(&item, &fields);
        let readable = readable_part(&text);
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
        let without = share_entry_text(&item, &chosen(&["username"]));
        assert!(decode(&without).unwrap().totp.is_none());
    }

    #[test]
    fn withheld_values_stay_out_of_the_entry() {
        let mut item = login();
        item.view_password = false;
        let entry = from_item(
            &item,
            &chosen(&["username", "password", "totp", "field:0", "field:1"]),
        );
        assert_eq!(entry.username.as_deref(), Some("nyu"));
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
                    value: "Nyu".into(),
                    hidden: false
                },
                EntryField {
                    name: "Number".into(),
                    value: "4111111111111111".into(),
                    hidden: true
                },
            ]
        );
    }

    #[test]
    fn plain_and_garbled_texts_are_plain_text() {
        let good = encode(
            "Shop",
            &Entry {
                name: "Shop".into(),
                ..Entry::default()
            },
        );
        assert!(decode(&good).is_some());
        let payload = good.rsplit_once(MARKER).unwrap().1.to_string();
        for text in [
            String::new(),
            "just a note".to_string(),
            format!("Shop\nuwulock-entry:v2:{payload}"),
            format!("Shop\nuwulock-entry:v1:{payload}!!"),
            "Shop\nuwulock-entry:v1:".to_string(),
            "Shop\nuwulock-entry:v1:bm90IGpzb24".to_string(),
            // Valid JSON, but no name.
            format!(
                "Shop\n{MARKER}{}",
                URL_SAFE_NO_PAD.encode(br#"{"websites":[]}"#)
            ),
            // The marker, but not on the last line.
            format!("Shop\n{MARKER}{payload}\nmore"),
            format!("{MARKER}{payload}x"),
        ] {
            assert!(decode(&text).is_none(), "{text:?}");
            assert_eq!(readable_part(&text), text);
        }
        // A trailing newline, padding and the marker alone on one line are fine.
        assert!(decode(&format!("{}\n", good.as_str())).is_some());
        assert!(decode(&format!("{}=", good.as_str())).is_some());
        let alone = format!("{MARKER}{payload}");
        assert_eq!(decode(&alone).unwrap().name, "Shop");
        assert_eq!(readable_part(&alone), "");
        // Unknown keys of a later v1 are ignored.
        let later = format!(
            "x\n{MARKER}{}",
            URL_SAFE_NO_PAD.encode(br#"{"name":"N","colour":"pink"}"#)
        );
        assert_eq!(decode(&later).unwrap().name, "N");
    }

    #[test]
    fn json_shape_is_the_contract() {
        let entry = Entry {
            name: "N".into(),
            username: Some("u".into()),
            websites: vec!["https://a.example.com".into()],
            fields: vec![EntryField {
                name: "f".into(),
                value: "v".into(),
                hidden: true,
            }],
            totp: Some(SECRET.into()),
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
