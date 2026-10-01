//! Joining a Wi-Fi network from the vault: the phone app's _Connect_ button
//! (docs/wifi.md).
//!
//! The network's fields (the contract in docs/wifi.md) are read here, in
//! Rust, and handed straight to the phone plugin: the password never goes
//! through the page. [`network`] turns an item into what Android needs, or
//! says why Android can't take it — WEP, EAP-TLS without a client
//! certificate, an Enterprise network without the server's domain — so the
//! page can explain instead of failing in the system's dialog.
//!
//! Only Android joins networks. iOS would need `NEHotspotConfiguration` and
//! with it the _Hotspot Configuration_ entitlement, which a sideloaded IPA
//! signed with a free Apple ID never gets (docs/mobile.md); there and on the
//! desktop the command answers `unsupported` and the page hides the button.

use crate::vault::{self, Result, VaultState};
use serde::{Serialize, Serializer};
use tauri::State;
use uwulock_bitwarden::vault::{Item, Secret};

/// What Android's `WifiNetworkSuggestion` gets: the contract's values, sorted
/// into what the system distinguishes.
#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Network {
    pub ssid: String,
    /// `open`, `wpa2` (also WPA and WPA2/WPA3: Android joins a mixed
    /// network with its WPA2 settings), `wpa3`, `wpa2-enterprise`,
    /// `wpa3-enterprise`.
    pub security: &'static str,
    #[serde(serialize_with = "secret")]
    pub password: Option<Secret>,
    pub hidden: bool,
    /// Enterprise only: `PEAP`, `TTLS` or `PWD`.
    pub eap: Option<&'static str>,
    /// Enterprise only: `MSCHAPV2`, `PAP`, `GTC` or `NONE`.
    pub phase2: Option<&'static str>,
    pub identity: Option<String>,
    pub anonymous_identity: Option<String>,
    /// The RADIUS server's domain, checked against the phone's system
    /// certificates (PEAP and TTLS).
    pub domain: Option<String>,
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("ssid", &self.ssid)
            .field("security", &self.security)
            .field("password", &self.password.as_ref().map(|_| "…"))
            .field("hidden", &self.hidden)
            .field("eap", &self.eap)
            .field("phase2", &self.phase2)
            .field("identity", &self.identity)
            .field("anonymous_identity", &self.anonymous_identity)
            .field("domain", &self.domain)
            .finish()
    }
}

fn secret<S: Serializer>(
    value: &Option<Secret>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_str(value),
        None => serializer.serialize_none(),
    }
}

/// Why a network can't be joined from UwULock; the page explains each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// Not a network at all.
    NotWifi,
    NoSsid,
    /// WEP: Android lets no app add WEP networks any more.
    Wep,
    /// A secured network without a password.
    NoPassword,
    /// WPA passwords are 8 to 63 ASCII characters; Android takes nothing else.
    Password,
    /// EAP-TLS needs a client certificate, which the item doesn't have.
    EapTls,
    /// No EAP method, or one Android doesn't know.
    Eap,
    /// PEAP and TTLS: Android insists on checking the server's certificate,
    /// and UwULock can only point it at a domain (with the system's CAs).
    CaDomain,
    /// An Enterprise network without an identity.
    Identity,
}

impl Unsupported {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Unsupported::NotWifi => "not-wifi",
            Unsupported::NoSsid => "no-ssid",
            Unsupported::Wep => "wep",
            Unsupported::NoPassword => "no-password",
            Unsupported::Password => "password",
            Unsupported::EapTls => "eap-tls",
            Unsupported::Eap => "eap",
            Unsupported::CaDomain => "ca-domain",
            Unsupported::Identity => "identity",
        }
    }
}

/// The trimmed value of the item's first field `name`, if it has one.
fn value(item: &Item, name: &str) -> Option<String> {
    item.field_value(name)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// A domain as Android's `setDomainSuffixMatch` takes it: labels of
/// letters, digits and hyphens with at least one dot, a leading `*.` or `.`
/// dropped. Anything else in the CA field (a note, a certificate's name, a
/// PEM) is no domain.
pub(crate) fn domain_of(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches('.');
    let text = text
        .strip_prefix("*.")
        .or_else(|| text.strip_prefix('.'))
        .unwrap_or(text);
    let labels: Vec<&str> = text.split('.').collect();
    let label_ok = |label: &&str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    let tld_ok = labels
        .last()
        .is_some_and(|tld| tld.chars().any(|c| c.is_ascii_alphabetic()));
    (labels.len() >= 2 && text.len() <= 253 && labels.iter().all(label_ok) && tld_ok)
        .then(|| text.to_ascii_lowercase())
}

/// The network in `item`, as Android gets it — or why it can't.
pub(crate) fn network(item: &Item) -> std::result::Result<Network, Unsupported> {
    use uwulock_bitwarden::vault::WIFI_SSID;
    if !item.is_wifi() {
        return Err(Unsupported::NotWifi);
    }
    // The SSID is taken as it is: spaces can be part of a network's name.
    let ssid = item
        .field_value(WIFI_SSID)
        .map(|v| v.to_string())
        .filter(|v| !v.trim().is_empty())
        .ok_or(Unsupported::NoSsid)?;
    let password = item
        .field_value("Password")
        .filter(|v| !v.is_empty())
        .cloned();
    let hidden = value(item, "Hidden network").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let security_text = value(item, "Security")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let security = match security_text.as_str() {
        "wpa3" => "wpa3",
        "wpa2/wpa3" | "wpa2" | "wpa" => "wpa2",
        "wep" => return Err(Unsupported::Wep),
        "none" => "open",
        "wpa2-enterprise" => "wpa2-enterprise",
        "wpa3-enterprise" => "wpa3-enterprise",
        // Unknown or missing: a password means WPA2, none an open network.
        _ if password.is_some() => "wpa2",
        _ => "open",
    };
    let mut network = Network {
        ssid,
        security,
        password: None,
        hidden,
        eap: None,
        phase2: None,
        identity: None,
        anonymous_identity: None,
        domain: None,
    };
    match security {
        "open" => {}
        "wpa2" | "wpa3" => {
            let password = password.ok_or(Unsupported::NoPassword)?;
            let length = password.chars().count();
            if !password.is_ascii() || !(8..=63).contains(&length) {
                return Err(Unsupported::Password);
            }
            network.password = Some(password);
        }
        _ => {
            let eap = match value(item, "EAP method").map(|v| v.to_ascii_uppercase()) {
                Some(eap) if eap == "PEAP" => "PEAP",
                Some(eap) if eap == "TTLS" => "TTLS",
                Some(eap) if eap == "PWD" => "PWD",
                Some(eap) if eap == "TLS" => return Err(Unsupported::EapTls),
                _ => return Err(Unsupported::Eap),
            };
            network.identity = Some(value(item, "Identity").ok_or(Unsupported::Identity)?);
            network.password = Some(password.ok_or(Unsupported::NoPassword)?);
            network.eap = Some(eap);
            if eap != "PWD" {
                network.phase2 = Some(
                    match value(item, "Phase 2")
                        .map(|v| v.to_ascii_uppercase())
                        .as_deref()
                    {
                        Some("MSCHAPV2") => "MSCHAPV2",
                        Some("PAP") => "PAP",
                        Some("GTC") => "GTC",
                        _ => "NONE",
                    },
                );
                network.anonymous_identity = value(item, "Anonymous identity");
                network.domain = Some(
                    value(item, "CA certificate")
                        .as_deref()
                        .and_then(domain_of)
                        .ok_or(Unsupported::CaDomain)?,
                );
            }
        }
    }
    Ok(network)
}

/// What the page shows after _Connect_.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Joined {
    /// `saved` (Android 11+, the person confirmed: the phone joins whenever
    /// the network is in range), `already-saved`, `suggested` (Android 10:
    /// the phone suggests it once the person allowed UwULock to suggest
    /// networks), `declined` (the person said no in the sheet),
    /// `disallowed` (Android 10: UwULock may not suggest networks),
    /// `unsupported`, `failed`.
    pub outcome: &'static str,
    /// With `unsupported`: one of [`Unsupported::code`], `platform` or
    /// `android-version`.
    pub reason: Option<&'static str>,
    /// With `failed`: what the phone said.
    pub message: Option<String>,
}

impl Joined {
    fn unsupported(reason: &'static str) -> Self {
        Joined {
            outcome: "unsupported",
            reason: Some(reason),
            message: None,
        }
    }
}

#[tauri::command]
pub(crate) async fn wifi_connect(state: State<'_, VaultState>, id: String) -> Result<Joined> {
    state.touch();
    let network = vault::with_item(&state, &id, |item| Ok(network(item)))?;
    let network = match network {
        Ok(network) => network,
        Err(why) => return Ok(Joined::unsupported(why.code())),
    };
    join(network).await
}

#[cfg(target_os = "android")]
async fn join(network: Network) -> Result<Joined> {
    use crate::vault::Failure;
    // The plugin waits for the system's dialog: never on a command's thread.
    tauri::async_runtime::spawn_blocking(move || {
        let plugin = crate::phone::plugin()
            .ok_or_else(|| Failure::new("wifi", "The phone isn't ready yet."))?;
        Ok(match plugin.connect_wifi(&network) {
            Ok(outcome) => Joined {
                outcome: match outcome.as_str() {
                    "saved" => "saved",
                    "already-saved" => "already-saved",
                    "suggested" => "suggested",
                    "declined" => "declined",
                    "disallowed" => "disallowed",
                    _ => "failed",
                },
                reason: None,
                message: None,
            },
            Err(error) if error.is("unsupported") => Joined::unsupported("android-version"),
            Err(error) => Joined {
                outcome: "failed",
                reason: None,
                message: Some(error.message),
            },
        })
    })
    .await
    .map_err(|e| Failure::new("wifi", e.to_string()))?
}

#[cfg(not(target_os = "android"))]
async fn join(network: Network) -> Result<Joined> {
    // The password is a `Zeroizing` string: wiped as it goes.
    drop(network);
    Ok(Joined::unsupported("platform"))
}

/// Opens the phone's Wi-Fi settings, for a network [`wifi_connect`] can't
/// hand over; the page copies the password first. `false` where there is
/// nothing to open (the desktop, iOS).
#[tauri::command]
pub(crate) async fn wifi_settings() -> Result<bool> {
    open_settings().await
}

#[cfg(target_os = "android")]
async fn open_settings() -> Result<bool> {
    use crate::vault::Failure;
    tauri::async_runtime::spawn_blocking(|| {
        let plugin = crate::phone::plugin()
            .ok_or_else(|| Failure::new("wifi", "The phone isn't ready yet."))?;
        plugin
            .open_wifi_settings()
            .map(|()| true)
            .map_err(|error| Failure::new("wifi", error.message))
    })
    .await
    .map_err(|e| Failure::new("wifi", e.to_string()))?
}

#[cfg(not(target_os = "android"))]
async fn open_settings() -> Result<bool> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uwulock_bitwarden::vault::{Field, FieldKind, ItemKind};
    use zeroize::Zeroizing;

    /// An item's fields: name, value, kind.
    type Fields<'a> = &'a [(&'a str, &'a str, FieldKind)];

    fn wifi(fields: &[(&str, &str, FieldKind)]) -> Item {
        let mut item = Item::new(ItemKind::Note);
        item.fields = std::iter::once(("uwulock:type", "wifi", FieldKind::Text))
            .chain(fields.iter().copied())
            .map(|(name, value, kind)| Field {
                name: Some(Zeroizing::new(name.into())),
                value: Some(Zeroizing::new(value.into())),
                kind,
                linked_id: None,
            })
            .collect();
        item
    }

    use FieldKind::{Boolean, Hidden, Text};

    #[test]
    fn a_home_network_goes_to_android_as_wpa2() {
        for security in ["WPA2", "WPA", "WPA2/WPA3", "wpa2"] {
            let item = wifi(&[
                ("SSID", "uwu home", Text),
                ("Password", "correct horse", Hidden),
                ("Security", security, Text),
                ("Hidden network", "true", Boolean),
            ]);
            let network = network(&item).unwrap();
            assert_eq!(network.ssid, "uwu home");
            assert_eq!(network.security, "wpa2", "{security}");
            assert_eq!(
                network.password.as_deref().map(String::as_str),
                Some("correct horse")
            );
            assert!(network.hidden);
            assert_eq!(network.eap, None);
        }
        let wpa3 = wifi(&[
            ("SSID", "uwu", Text),
            ("Password", "correct horse", Hidden),
            ("Security", "WPA3", Text),
        ]);
        let network = network(&wpa3).unwrap();
        assert_eq!(network.security, "wpa3");
        assert!(!network.hidden);
    }

    #[test]
    fn the_request_is_what_the_kotlin_side_reads() {
        let item = wifi(&[
            ("SSID", "uwu", Text),
            ("Password", "correct horse", Hidden),
            ("Security", "WPA2", Text),
        ]);
        let json = serde_json::to_value(network(&item).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "ssid": "uwu", "security": "wpa2", "password": "correct horse",
                "hidden": false, "eap": null, "phase2": null, "identity": null,
                "anonymousIdentity": null, "domain": null,
            })
        );
        // Debug output never shows the password.
        assert!(!format!("{:?}", network(&item).unwrap()).contains("horse"));
    }

    #[test]
    fn an_open_network_has_no_password() {
        let item = wifi(&[
            ("SSID", "Café", Text),
            ("Password", "left over", Hidden),
            ("Security", "None", Text),
        ]);
        let network = network(&item).unwrap();
        assert_eq!(network.security, "open");
        assert_eq!(network.password, None);
        // No security at all: open without a password, WPA2 with one.
        assert_eq!(
            super::network(&wifi(&[("SSID", "x", Text)]))
                .unwrap()
                .security,
            "open"
        );
        let guessed = wifi(&[("SSID", "x", Text), ("Password", "12345678", Hidden)]);
        assert_eq!(super::network(&guessed).unwrap().security, "wpa2");
    }

    #[test]
    fn what_android_cannot_take_is_said_up_front() {
        let cases: &[(Fields, Unsupported)] = &[
            (&[("Password", "12345678", Hidden)], Unsupported::NoSsid),
            (&[("SSID", "  ", Text)], Unsupported::NoSsid),
            (
                &[
                    ("SSID", "old", Text),
                    ("Security", "WEP", Text),
                    ("Password", "abcde", Hidden),
                ],
                Unsupported::Wep,
            ),
            (
                &[("SSID", "x", Text), ("Security", "WPA2", Text)],
                Unsupported::NoPassword,
            ),
            (
                &[
                    ("SSID", "x", Text),
                    ("Security", "WPA2", Text),
                    ("Password", "short", Hidden),
                ],
                Unsupported::Password,
            ),
            (
                &[
                    ("SSID", "x", Text),
                    ("Security", "WPA3", Text),
                    ("Password", "pässwörter", Hidden),
                ],
                Unsupported::Password,
            ),
            (
                &[
                    ("SSID", "x", Text),
                    ("Security", "WPA2", Text),
                    ("Password", &"a".repeat(64), Hidden),
                ],
                Unsupported::Password,
            ),
        ];
        for (fields, why) in cases {
            assert_eq!(network(&wifi(fields)).unwrap_err(), *why, "{fields:?}");
        }
        let mut login = wifi(&[("SSID", "x", Text)]);
        login.kind = ItemKind::Login;
        assert_eq!(network(&login).unwrap_err(), Unsupported::NotWifi);
    }

    fn enterprise(extra: &[(&str, &str, FieldKind)]) -> Item {
        let mut fields = vec![
            ("SSID", "eduroam", Text),
            ("Password", "secret", Hidden),
            ("Security", "WPA2-Enterprise", Text),
            ("EAP method", "PEAP", Text),
            ("Phase 2", "MSCHAPV2", Text),
            ("Identity", "nyu@example.org", Text),
            ("Anonymous identity", "anonymous@example.org", Text),
            ("CA certificate", "radius.example.org", Text),
        ];
        for (name, value, kind) in extra {
            fields.retain(|(n, _, _)| n != name);
            fields.push((name, value, *kind));
        }
        wifi(&fields)
    }

    #[test]
    fn an_enterprise_network_carries_its_eap_settings() {
        let network = network(&enterprise(&[])).unwrap();
        assert_eq!(network.security, "wpa2-enterprise");
        assert_eq!(network.eap, Some("PEAP"));
        assert_eq!(network.phase2, Some("MSCHAPV2"));
        assert_eq!(network.identity.as_deref(), Some("nyu@example.org"));
        assert_eq!(
            network.anonymous_identity.as_deref(),
            Some("anonymous@example.org")
        );
        assert_eq!(network.domain.as_deref(), Some("radius.example.org"));
        assert_eq!(
            network.password.as_deref().map(String::as_str),
            Some("secret")
        );

        let ttls = super::network(&enterprise(&[
            ("Security", "WPA3-Enterprise", Text),
            ("EAP method", "ttls", Text),
            ("Phase 2", "none", Text),
            ("Anonymous identity", "", Text),
            ("CA certificate", "*.Example.ORG", Text),
        ]))
        .unwrap();
        assert_eq!(ttls.security, "wpa3-enterprise");
        assert_eq!(ttls.eap, Some("TTLS"));
        assert_eq!(ttls.phase2, Some("NONE"));
        assert_eq!(ttls.anonymous_identity, None);
        assert_eq!(ttls.domain.as_deref(), Some("example.org"));

        // PWD checks no certificate: no domain, no phase 2.
        let pwd = super::network(&enterprise(&[
            ("EAP method", "PWD", Text),
            ("CA certificate", "ask the IT desk", Text),
        ]))
        .unwrap();
        assert_eq!(pwd.eap, Some("PWD"));
        assert_eq!(pwd.phase2, None);
        assert_eq!(pwd.domain, None);
    }

    #[test]
    fn an_enterprise_network_android_cannot_check_is_refused() {
        let cases: &[(Fields, Unsupported)] = &[
            (&[("EAP method", "TLS", Text)], Unsupported::EapTls),
            (&[("EAP method", "LEAP", Text)], Unsupported::Eap),
            (&[("EAP method", "", Text)], Unsupported::Eap),
            (&[("CA certificate", "", Text)], Unsupported::CaDomain),
            (
                &[(
                    "CA certificate",
                    "the university's CA, see the intranet",
                    Text,
                )],
                Unsupported::CaDomain,
            ),
            (
                &[("CA certificate", "-----BEGIN CERTIFICATE-----", Text)],
                Unsupported::CaDomain,
            ),
            (&[("Identity", " ", Text)], Unsupported::Identity),
            (&[("Password", "", Hidden)], Unsupported::NoPassword),
        ];
        for (fields, why) in cases {
            assert_eq!(
                network(&enterprise(fields)).unwrap_err(),
                *why,
                "{fields:?}"
            );
        }
    }

    #[test]
    fn domains_are_told_from_notes() {
        assert_eq!(
            domain_of("radius.example.org").as_deref(),
            Some("radius.example.org")
        );
        assert_eq!(domain_of(" .Example.COM. ").as_deref(), Some("example.com"));
        assert_eq!(
            domain_of("*.wifi.example.net").as_deref(),
            Some("wifi.example.net")
        );
        for not in [
            "localhost",
            "192.0.2.1",
            "a b.example.org",
            "-x.example.org",
            "",
            "example.",
        ] {
            assert_eq!(domain_of(not), None, "{not}");
        }
    }

    #[test]
    fn the_answer_for_the_page_is_camel_case() {
        assert_eq!(
            serde_json::to_value(Joined::unsupported("wep")).unwrap(),
            serde_json::json!({ "outcome": "unsupported", "reason": "wep", "message": null })
        );
    }
}
