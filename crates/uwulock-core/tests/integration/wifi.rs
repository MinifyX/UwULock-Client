//! Wi-Fi networks (docs/wifi.md): a secure note with UwULock's marker field.
//! On the wire it stays a note, so Bitwarden's apps keep showing a note with
//! fields; sealing and opening it again changes nothing, not the order of the
//! fields and not a field of another app.

use uwulock_core::crypto::SymmetricKey;
use uwulock_core::vault::{
    Field, FieldKind, Item, ItemKind, Vault, TYPE_MARKER, WIFI_SSID, WIFI_TYPE,
};
use uwulock_core::wire;
use zeroize::Zeroizing;

fn field(name: &str, value: &str, kind: FieldKind) -> Field {
    Field {
        name: Some(Zeroizing::new(name.into())),
        value: Some(Zeroizing::new(value.into())),
        kind,
        linked_id: None,
    }
}

/// A network as UwULock's apps write it, with a field of another app's after it.
fn network() -> Item {
    let mut item = Item::new(ItemKind::Note);
    item.id = "w0".into();
    item.name = Zeroizing::new("Home".into());
    item.notes = Some(Zeroizing::new("router in the hall".into()));
    item.fields = vec![
        field(TYPE_MARKER, WIFI_TYPE, FieldKind::Text),
        field(WIFI_SSID, "uwu-net", FieldKind::Text),
        field("Password", "correct; horse", FieldKind::Hidden),
        field("Security", "WPA2-Enterprise", FieldKind::Text),
        field("Hidden network", "true", FieldKind::Boolean),
        field("EAP method", "PEAP", FieldKind::Text),
        field("Phase 2", "MSCHAPV2", FieldKind::Text),
        field("Identity", "nyu@example.com", FieldKind::Text),
        field(
            "Anonymous identity",
            "anonymous@example.com",
            FieldKind::Text,
        ),
        field("CA certificate", "radius.example.com", FieldKind::Text),
        field("Router admin", "http://192.0.2.1", FieldKind::Text),
    ];
    item
}

fn fields(item: &Item) -> Vec<(String, String, FieldKind)> {
    item.fields
        .iter()
        .map(|f| {
            (
                f.name.as_deref().cloned().unwrap_or_default(),
                f.value.as_deref().cloned().unwrap_or_default(),
                f.kind,
            )
        })
        .collect()
}

/// Seals the item the way a save does and opens what the server would hand back.
fn round_trip(item: &Item, key: &SymmetricKey) -> (serde_json::Value, Item) {
    let request = serde_json::to_value(item.seal(key).unwrap()).unwrap();
    let mut answer = request.clone();
    answer["id"] = item.id.clone().into();
    let cipher: wire::Cipher = serde_json::from_value(wire::lowercase_keys(answer)).unwrap();
    let opened = Vault::default()
        .open_cipher(&cipher, key)
        .unwrap()
        .expect("a note is a kind UwULock knows");
    (request, opened)
}

#[test]
fn a_network_stays_a_secure_note_with_every_field_in_its_order() {
    let key = SymmetricKey::generate();
    let item = network();
    assert!(item.is_wifi());
    assert_eq!(item.own_type(), Some("wifi"));

    let (request, opened) = round_trip(&item, &key);
    assert_eq!(request["type"], 2, "Bitwarden's apps see a secure note");
    assert_eq!(request["secureNote"]["type"], 0);
    assert_eq!(request["fields"][0]["type"], 0);
    assert_eq!(request["fields"][2]["type"], 1, "the password stays hidden");
    assert_eq!(request["fields"][4]["type"], 2);

    assert_eq!(opened.kind, ItemKind::Note);
    assert!(opened.is_wifi());
    assert_eq!(fields(&opened), fields(&item));
    assert_eq!(
        opened.notes.as_deref().map(String::as_str),
        Some("router in the hall")
    );
    assert_eq!(
        opened.field_value(WIFI_SSID).map(|s| s.as_str()),
        Some("uwu-net")
    );

    // And once more, as another UwULock app would save it again unchanged.
    let (_, again) = round_trip(&opened, &key);
    assert_eq!(fields(&again), fields(&item));
}

#[test]
fn only_a_note_with_the_text_marker_is_a_network() {
    let mut item = network();
    item.fields[0].value = Some(Zeroizing::new("  WiFi ".into()));
    assert!(item.is_wifi(), "spaces and case don't matter");

    item.fields[0].kind = FieldKind::Hidden;
    assert!(!item.is_wifi(), "the marker is a text field");

    let mut item = network();
    item.fields[0].value = Some(Zeroizing::new("router".into()));
    assert_eq!(item.own_type(), None, "an unknown type is a plain note");

    let mut item = network();
    item.fields[0].name = Some(Zeroizing::new("UwULock:Type".into()));
    assert!(!item.is_wifi(), "the name is exact");

    let mut login = Item::new(ItemKind::Login);
    login.fields = network().fields;
    assert!(!login.is_wifi(), "a login with the field stays a login");
}
