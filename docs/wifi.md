# Wi-Fi networks

UwULock keeps Wi-Fi networks as an item type of their own: in the list with their own icon and
filter (_Wi-Fi_), with an editor for the network's settings and a QR code that phones join the
network with. Bitwarden has no such type, so a network is stored the way every Bitwarden app can
read it: as a **secure note** (cipher type 2) with **custom fields**. Bitwarden's official apps
show a note with fields; every UwULock app (the desktop app, the phone apps, the browser
extension and UwULock Server's web vault) shows a Wi-Fi network. The server never knows the
difference — the fields are encrypted like every other, and the official apps keep working with
the same vault.

## The contract

The same in every app (UwULock Server has the same page as `docs/wifi.md`). Field names are
stable English and never translated; field types are Bitwarden's (`0` text, `1` hidden,
`2` boolean).

| Field                | Type | Value                                                                                          |
| -------------------- | ---- | ---------------------------------------------------------------------------------------------- |
| `uwulock:type`       | 0    | `wifi` — the marker. UwULock's apps hide it.                                                   |
| `SSID`               | 0    | the network's name                                                                             |
| `Password`           | 1    | the network's password (empty for an open network)                                             |
| `Security`           | 0    | one of `WPA3`, `WPA2/WPA3`, `WPA2`, `WPA`, `WEP`, `None`, `WPA2-Enterprise`, `WPA3-Enterprise` |
| `Hidden network`     | 2    | `true` or `false`                                                                              |
| `EAP method`         | 0    | Enterprise only: `PEAP`, `TTLS`, `TLS` or `PWD`                                                |
| `Phase 2`            | 0    | Enterprise only: `MSCHAPV2`, `PAP`, `GTC` or `none`                                            |
| `Identity`           | 0    | Enterprise only                                                                                |
| `Anonymous identity` | 0    | Enterprise only                                                                                |
| `CA certificate`     | 0    | Enterprise only: the server's domain, or a note on the certificate                             |

The item's notes are the cipher's notes; its name defaults to the SSID.

Rules for every app that reads or writes networks:

- **Recognising**: an item is a network when it is a secure note and has a text field named
  exactly `uwulock:type` whose value is `wifi` (surrounding spaces and case don't matter). A
  login or card with the field stays what it is.
- **Reading**: the first field of each name counts. A field the app doesn't expect for the
  network's security (an `Identity` on a WPA2 network) is not the network's: it stays an
  ordinary custom field.
- **Writing**: the marker and the network's fields first, in the order of the table; the
  Enterprise fields only for an Enterprise security and only with a value. **Every other field
  stays as it is** — name, value, type and order — after the network's fields. A password the
  app never showed is kept: the editor sends the field's index, not the value.
- **Unknown values** (a security from another app, an EAP method not in the list) are kept and
  shown as they are; the editor offers them as an extra choice.

## In the code

- `crates/uwulock-core` — `Item::own_type()` / `Item::is_wifi()` recognise the marker
  (`TYPE_MARKER`, `WIFI_TYPE`). `ItemKind` has no Wi-Fi kind on purpose: on the wire and in
  every save the item stays a note (type 2), so nothing about Bitwarden compatibility changes.
  `tests/integration/wifi.rs` seals a network and opens it again: still a note, every field in
  its order.
- `apps/desktop/src-tauri/src/vault.rs` and `crates/uwulock-wasm/src/view.rs` (the extension) —
  the list shows such a note with the kind `wifi` and the SSID as its subtitle. A draft is
  always a `note`; `wifi` as a draft's kind is refused.
- `apps/desktop/src/lib/wifi.ts` — reading and writing the fields and the QR code's text. It is
  the same file as the web vault's (`web/src/lib/wifi.ts` in UwULock-Server); change both
  together. The extension's popup imports it (`@desktop/lib/wifi`); its tests are in
  `apps/extension/test/wifi.test.ts`.

## In the desktop app

- _New → Wi-Fi_ makes a network; the filter _Types → Wi-Fi_ appears once there is one.
- The editor has the network's name (the item's name follows it as long as it was the SSID),
  the security as a list, the password with the generator, _Hidden network_, and — only for
  WPA2-/WPA3-Enterprise — EAP method, phase 2, identity, anonymous identity and CA certificate.
  Other fields stay under _Custom fields_.
- The details show the network with copy buttons, the password behind the eye, and
  **Share as QR code**: a dialog with the code and the network's name, security and password
  (hidden until the eye is clicked). The password comes from Rust for the code and is forgotten
  when the dialog closes.
- There is no _Connect_ button on the computer. The details have a slot for it
  (`ItemDetail`'s `wifiActions`), which the Android app fills (below).

## Connecting on the phone

On Android a network's details have **Connect**. `apps/desktop/src-tauri/src/wifi.rs` reads the
item in Rust and turns it into what Android takes (`wifi::network`, tested there); the plugin
builds a `WifiNetworkSuggestion` from it ([mobile.md](mobile.md#the-phone-plugin)).

| Security                   | Android gets                                   |
| -------------------------- | ---------------------------------------------- |
| `WPA3`                     | WPA3 (SAE) passphrase                          |
| `WPA2/WPA3`, `WPA2`, `WPA` | WPA2 passphrase (a mixed network takes it)     |
| `None`                     | open network, no password                      |
| `WEP`                      | — Android lets no app add WEP networks         |
| `WPA2-Enterprise`          | WPA2-Enterprise with the EAP settings below    |
| `WPA3-Enterprise`          | WPA3-Enterprise (standard mode on Android 12+) |
| missing or unknown         | WPA2 with a password, open without             |

- `Hidden network` = `true` makes it a hidden SSID. WPA passwords must be 8–63 ASCII characters.
- Enterprise: `EAP method` PEAP, TTLS or PWD (TLS needs a client certificate UwULock doesn't
  have), `Phase 2` (MSCHAPV2, PAP, GTC, anything else = none), `Identity` (required),
  `Anonymous identity`, `Password`.
- `CA certificate`: Android requires PEAP and TTLS networks to check the RADIUS server. If the
  field looks like a domain (`radius.example.org`, `*.example.org`), UwULock uses it as the
  server's domain and the phone's **system** CA certificates — what Android's settings call
  _Use system certificates_. Anything else in the field (a note, a certificate's name) can't be
  checked, so UwULock explains that instead of adding a network that would never connect. PWD
  needs no certificate.
- **Android 11+** shows its own sheet; the person confirms or declines. **Android 10** gets a
  suggestion: the system asks once in a notification whether UwULock may suggest networks.
- The answer appears as a short message: saved, already saved, suggested, declined. Where
  Android can't take the network (WEP, EAP-TLS, no domain, a password Android refuses), the
  details say why and offer **Copy password & open Wi-Fi settings** to add it by hand.
- **iPhone**: no Connect button. iOS's `NEHotspotConfiguration` needs an entitlement a
  sideloaded app signed with a free Apple ID doesn't get ([mobile.md](mobile.md#the-phone-plugin));
  the QR code (the camera joins the network) and copying remain.

## In the browser extension

- The popup lists networks with their icon and under _Types → Wi-Fi_; the details show the
  network with copy buttons, the password behind the eye, and **Show QR code** right in the
  card. The password is fetched for the code and dropped when the code is hidden.
- Networks are **never offered for filling**: autofill only sees the note they are, and the
  extension fills logins, cards and identities only.
- Networks are edited in the desktop app or the web vault; the popup has no editor for them.

## The QR code

The format phones and most QR scanners understand:

```
WIFI:T:WPA;S:<ssid>;P:<password>;H:true;;
WIFI:T:WPA2-EAP;S:<ssid>;E:<eap>;PH2:<phase 2>;A:<anonymous>;I:<identity>;P:<password>;;
```

- `T` is `WPA` for every WPA, WPA2 and WPA3 personal security, `WEP` for WEP, `nopass` for
  `None` (then without `P`), and `WPA2-EAP` for both Enterprise securities.
- `\`, `;`, `,`, `:` and `"` in a value get a backslash. An SSID made only of hex digits (an
  even number of them) is put in double quotes, or a phone would read it as bytes.
- Parts without a value are left out, `PH2` also for `none`; `H:true` only for a hidden network.

The code is drawn on the device with [uqr](https://github.com/unjs/uqr) (MIT), the same library
as the web vault's. Nothing about the network leaves the app or the extension for it.

## Imports and moving

The client has no importer of its own: imports from other password managers happen in the web
vault, which turns 1Password's _Wireless Router_, Proton Pass's Wi-Fi items, LastPass's _Wi-Fi
Password_ form and every entry that already carries the marker into networks. Moving a vault
from Bitwarden ([moving-from-bitwarden.md](moving-from-bitwarden.md)) copies every field as it
is, so networks arrive as networks.

---

# WLAN-Netze

UwULock führt WLAN-Netze als eigenen Eintragstyp: in der Liste mit eigenem Symbol und Filter
(_WLAN_), mit einem Editor für die Einstellungen des Netzes und einem QR-Code, mit dem Handys
dem Netz beitreten. Bitwarden kennt diesen Typ nicht, deshalb wird ein Netz so gespeichert, dass
jede Bitwarden-App es lesen kann: als **sichere Notiz** mit **eigenen Feldern** (Tabelle oben;
die Feldnamen bleiben immer Englisch). Bitwardens Apps zeigen eine Notiz mit Feldern, jede
UwULock-App zeigt ein WLAN.

- **Desktop-App**: _Neu → WLAN_ legt ein Netz an. Der Editor hat Netzwerkname (SSID),
  Sicherheit, Passwort mit Generator, _Verstecktes Netzwerk_ und bei WPA2-/WPA3-Enterprise
  EAP-Methode, Phase 2, Identität, anonyme Identität und CA-Zertifikat. In den Details:
  Kopieren, Passwort hinter dem Auge und **Als QR-Code teilen**.
- **Browser-Erweiterung**: ansehen, kopieren und **QR-Code zeigen** im Popup. Zum Ausfüllen
  werden WLAN-Netze nie angeboten; bearbeitet werden sie in der App oder im Web-Tresor.
- **QR-Code**: im Standardformat `WIFI:…`, auf dem Gerät gezeichnet (uqr, MIT) – nichts über
  das Netz verlässt dafür App oder Erweiterung.
- **Verbinden** gibt es in der Android-App: ab Android 11 bestätigst du das Netz im Fenster des
  Systems, Android 10 schlägt es vor. WPA2/WPA3, offene und versteckte Netze gehen; WEP lässt
  Android nicht zu. Enterprise-Netze mit PEAP, TTLS oder PWD gehen, wenn im Feld
  _CA-Zertifikat_ eine Domain steht (z. B. `radius.example.org`) – dann prüft das Handy den
  Server mit den Zertifikaten des Systems. Geht es nicht, sagt UwULock warum und bietet
  _Passwort kopieren & WLAN-Einstellungen öffnen_ an. Am Computer gibt es den Knopf nicht, auf
  dem iPhone auch nicht: iOS erlaubt das nur mit einer Berechtigung, die eine selbst signierte
  App nicht bekommt.
- Felder anderer Apps im selben Eintrag bleiben unverändert, mit Namen, Typ und Reihenfolge.
