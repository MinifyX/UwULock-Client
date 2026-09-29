//! Making a text Send, and sharing an item as one.
//!
//! A Send's values are under its own key, which comes from a 16-byte seed
//! ([`crate::crypto::send_key`]); the seed itself is kept under the user key
//! (`key`) and travels in the link after the `#`. The server hands out the
//! Send by its `accessId`, and never sees the seed.
//!
//! Sharing an item puts the fields somebody chose into the text of a new Send
//! ([`share_text`]). An authenticator key is never among them: whoever has it
//! can make the codes for good, which is not what "share this login" means.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64_URL;
use base64::Engine as _;
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::crypto::{generate_send_seed, send_key, send_password_hash, EncString, SymmetricKey};
use crate::vault::{FieldKind, Item, Secret};
use crate::Error;

/// Bitwarden's `authType`: who may open a Send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendAuth {
    /// Only the given addresses, with a code mailed to them.
    Emails,
    Password,
    None,
}

/// A text Send to make.
#[derive(Debug, Clone)]
pub struct TextSend {
    pub name: String,
    pub notes: Option<String>,
    pub text: Zeroizing<String>,
    /// Shown only after a click on the recipient's page.
    pub hidden: bool,
    pub max_access_count: Option<u32>,
    /// RFC 3339.
    pub deletion_date: String,
    pub expiration_date: Option<String>,
    pub password: Option<Zeroizing<String>>,
    /// Only these addresses may open it (UwULock Server with mail; Bitwarden).
    pub emails: Vec<String>,
    pub hide_email: bool,
}

/// A Send ready for `POST /api/sends`, and the seed its link needs.
pub struct SealedSend {
    /// The body of `POST /api/sends` (Bitwarden's `SendRequestModel`).
    pub request: Value,
    pub seed: Zeroizing<[u8; 16]>,
}

impl std::fmt::Debug for SealedSend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SealedSend(…)")
    }
}

impl TextSend {
    /// Seals the Send: name, notes and text under the Send's key, the seed
    /// under the user key, the password as its hash. Addresses stay readable:
    /// the server mails the codes to them.
    pub fn seal(&self, user_key: &SymmetricKey) -> Result<SealedSend, Error> {
        let emails: Vec<String> = self
            .emails
            .iter()
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty())
            .collect();
        if emails.iter().any(|e| !looks_like_address(e)) {
            return Err(Error::Crypto("an address isn't one".into()));
        }
        let password = self.password.as_ref().filter(|p| !p.is_empty());
        if password.is_some() && !emails.is_empty() {
            return Err(Error::Crypto(
                "a Send has either a password or addresses, not both".into(),
            ));
        }
        let seed = generate_send_seed();
        let key = send_key(seed.as_ref())?;
        let seal = |text: &str| EncString::encrypt(text.as_bytes(), &key).to_string();
        let auth = if !emails.is_empty() {
            SendAuth::Emails
        } else if password.is_some() {
            SendAuth::Password
        } else {
            SendAuth::None
        };
        let request = json!({
            "type": 0,
            "name": seal(&self.name),
            "notes": self.notes.as_deref().filter(|n| !n.is_empty()).map(seal),
            "key": EncString::encrypt(seed.as_ref(), user_key).to_string(),
            "maxAccessCount": self.max_access_count,
            "expirationDate": self.expiration_date,
            "deletionDate": self.deletion_date,
            "text": { "text": seal(&self.text), "hidden": self.hidden },
            "file": null,
            "password": password.map(|p| send_password_hash(p, seed.as_ref())),
            "emails": (!emails.is_empty()).then(|| emails.join(",")),
            "authType": match auth { SendAuth::Emails => 0, SendAuth::Password => 1, SendAuth::None => 2 },
            "disabled": false,
            "hideEmail": self.hide_email,
        });
        Ok(SealedSend { request, seed })
    }
}

fn looks_like_address(text: &str) -> bool {
    match text.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !text.contains(char::is_whitespace)
                && !text.contains(',')
        }
        None => false,
    }
}

/// The link of a Send. `base` is the web vault (`https://lock.example.com`,
/// Bitwarden's `https://vault.bitwarden.com`) or, with `send_domain`, a UwULock
/// send domain (`https://send.example.com`).
pub fn link(base: &str, access_id: &str, seed: &[u8], send_domain: bool) -> String {
    let base = base.trim_end_matches('/');
    let key = B64_URL.encode(seed);
    if send_domain {
        format!("{base}/{access_id}#{key}")
    } else {
        format!("{base}/#/send/{access_id}/{key}")
    }
}

/// The seed of a Send this account made, from its `key`: for showing its
/// link again.
pub fn open_seed(key: &str, user_key: &SymmetricKey) -> Result<Zeroizing<Vec<u8>>, Error> {
    let seed = key.parse::<EncString>()?.decrypt(user_key)?;
    if seed.len() != 16 {
        return Err(Error::Crypto("a Send's seed has 16 bytes".into()));
    }
    Ok(seed)
}

/// One value of an item by its name — the names the desktop app and the
/// extension use: `username`, `password`, `notes`, `uri:<n>`, `card-name`,
/// `card-number`, `card-expiry`, `card-code`, `identity:<name>`, `ssh-public`,
/// `ssh-private`, `ssh-fingerprint`, `field:<n>`. `totp` and anything else
/// is `None`.
pub fn shareable_value(item: &Item, name: &str) -> Option<Secret> {
    let filled = |value: Option<&Secret>| value.filter(|v| !v.is_empty()).cloned();
    let index = |prefix: &str| name.strip_prefix(prefix)?.parse::<usize>().ok();
    let login = item.login.as_ref();
    let card = item.card.as_ref();
    let ssh = item.ssh_key.as_ref();
    let identity = item.identity.as_ref();
    match name {
        "username" => filled(login.and_then(|l| l.username.as_ref())),
        "password" => filled(login.and_then(|l| l.password.as_ref())),
        "notes" => filled(item.notes.as_ref()),
        "card-name" => filled(card.and_then(|c| c.cardholder_name.as_ref())),
        "card-number" => filled(card.and_then(|c| c.number.as_ref())),
        "card-code" => filled(card.and_then(|c| c.code.as_ref())),
        "card-expiry" => {
            let c = card?;
            let month = c.exp_month.as_ref().map_or("", |v| v.as_str());
            let year = c.exp_year.as_ref().map_or("", |v| v.as_str());
            if month.is_empty() && year.is_empty() {
                return None;
            }
            Some(Zeroizing::new(format!("{month:0>2}/{year}")))
        }
        "ssh-public" => filled(ssh.and_then(|s| s.public_key.as_ref())),
        "ssh-private" => filled(ssh.and_then(|s| s.private_key.as_ref())),
        "ssh-fingerprint" => filled(ssh.and_then(|s| s.fingerprint.as_ref())),
        _ if name.starts_with("uri:") => filled(
            login
                .and_then(|l| l.uris.get(index("uri:")?))
                .map(|u| &u.uri),
        ),
        _ if name.starts_with("field:") => {
            let field = item.fields.get(index("field:")?)?;
            if field.kind == FieldKind::Linked {
                return None;
            }
            filled(field.value.as_ref())
        }
        _ if name.starts_with("identity:") => {
            let i = identity?;
            let value = match name.trim_start_matches("identity:") {
                "title" => i.title.as_ref(),
                "firstName" => i.first_name.as_ref(),
                "middleName" => i.middle_name.as_ref(),
                "lastName" => i.last_name.as_ref(),
                "username" => i.username.as_ref(),
                "company" => i.company.as_ref(),
                "email" => i.email.as_ref(),
                "phone" => i.phone.as_ref(),
                "address1" => i.address1.as_ref(),
                "address2" => i.address2.as_ref(),
                "address3" => i.address3.as_ref(),
                "postalCode" => i.postal_code.as_ref(),
                "city" => i.city.as_ref(),
                "state" => i.state.as_ref(),
                "country" => i.country.as_ref(),
                "ssn" => i.ssn.as_ref(),
                "passportNumber" => i.passport_number.as_ref(),
                "licenseNumber" => i.license_number.as_ref(),
                _ => None,
            };
            filled(value)
        }
        // The authenticator key, the password history and anything unknown.
        _ => None,
    }
}

/// Whether a value is kept out of a Send because the organisation hides the
/// item's passwords from this member (`viewPassword: false`): the password,
/// the authenticator key, hidden custom fields, a card's number and code, and
/// an SSH private key.
/// Official Bitwarden clients have no way to take these off the device, so
/// neither does a Send.
pub fn withheld(item: &Item, name: &str) -> bool {
    if item.view_password {
        return false;
    }
    match name {
        "password" | "totp" | "card-number" | "card-code" | "ssh-private" => true,
        _ => match name
            .strip_prefix("field:")
            .and_then(|n| n.parse::<usize>().ok())
        {
            Some(index) => item
                .fields
                .get(index)
                .is_some_and(|f| f.kind == FieldKind::Hidden),
            None => false,
        },
    }
}

/// The text of a Send that shares an item: its name, then one line per chosen
/// field, `label: value` (a custom field is labelled with its own name).
/// `fields` pairs a field's name ([`shareable_value`]) with the label the
/// person reads, in their language. Fields without a value are left out; the
/// authenticator key is never shared, whatever is asked for, and neither is
/// what [`withheld`] names.
pub fn share_text(item: &Item, fields: &[(String, String)]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::new());
    out.push_str(&item.name);
    for (name, label) in fields {
        if withheld(item, name) {
            continue;
        }
        let Some(value) = shareable_value(item, name) else {
            continue;
        };
        let label = match name
            .strip_prefix("field:")
            .and_then(|n| n.parse::<usize>().ok())
        {
            Some(index) => item
                .fields
                .get(index)
                .and_then(|f| f.name.as_ref())
                .filter(|n| !n.is_empty())
                .map(|n| n.to_string())
                .unwrap_or_else(|| label.clone()),
            None => label.clone(),
        };
        out.push('\n');
        // Several lines (notes, a key) start on a line of their own.
        if value.contains('\n') {
            out.push_str(&format!("{label}:\n{}", value.as_str()));
        } else {
            out.push_str(&format!("{label}: {}", value.as_str()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{Field, ItemKind, Login, LoginUri};

    fn login() -> Item {
        let mut item = Item::new(ItemKind::Login);
        item.name = Zeroizing::new("Shop".into());
        item.login = Some(Login {
            username: Some(Zeroizing::new("nyu".into())),
            password: Some(Zeroizing::new("hunter2".into())),
            totp: Some(Zeroizing::new("JBSWY3DPEHPK3PXP".into())),
            uris: vec![LoginUri {
                uri: Zeroizing::new("https://shop.example.com".into()),
                match_kind: None,
                checksum: None,
            }],
            ..Login::default()
        });
        item.fields.push(Field {
            name: Some(Zeroizing::new("PIN".into())),
            value: Some(Zeroizing::new("1234".into())),
            kind: FieldKind::Hidden,
            linked_id: None,
        });
        item
    }

    #[test]
    fn sharing_never_takes_the_authenticator_key() {
        let item = login();
        let fields: Vec<(String, String)> = [
            ("username", "Username"),
            ("password", "Password"),
            ("totp", "Authenticator key"),
            ("uri:0", "Website"),
            ("field:0", "Field"),
            ("history:0", "Old password"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let text = share_text(&item, &fields);
        assert_eq!(
            text.as_str(),
            "Shop\nUsername: nyu\nPassword: hunter2\nWebsite: https://shop.example.com\nPIN: 1234"
        );
        assert!(!text.contains("JBSWY3DPEHPK3PXP"));
        assert!(shareable_value(&item, "totp").is_none());
    }

    #[test]
    fn hidden_passwords_stay_out_of_a_send() {
        let mut item = login();
        item.fields.push(Field {
            name: Some(Zeroizing::new("Note".into())),
            value: Some(Zeroizing::new("plain".into())),
            kind: FieldKind::Text,
            linked_id: None,
        });
        item.card = Some(crate::vault::Card {
            number: Some(Zeroizing::new("4111111111111111".into())),
            code: Some(Zeroizing::new("123".into())),
            cardholder_name: Some(Zeroizing::new("Nyu".into())),
            ..Default::default()
        });
        item.view_password = false;
        let fields: Vec<(String, String)> = [
            ("username", "Username"),
            ("password", "Password"),
            ("field:0", "Field"),
            ("field:1", "Field"),
            ("card-name", "Name"),
            ("card-number", "Number"),
            ("card-code", "Code"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let text = share_text(&item, &fields);
        assert_eq!(text.as_str(), "Shop\nUsername: nyu\nNote: plain\nName: Nyu");
        for name in [
            "password",
            "totp",
            "field:0",
            "card-number",
            "card-code",
            "ssh-private",
        ] {
            assert!(withheld(&item, name), "{name}");
        }
        assert!(!withheld(&item, "field:1") && !withheld(&item, "username"));
        item.view_password = true;
        assert!(!withheld(&item, "password"));
    }

    #[test]
    fn a_sealed_send_opens_with_its_link() {
        let user = SymmetricKey::generate();
        let send = TextSend {
            name: "Shop".into(),
            notes: None,
            text: Zeroizing::new("secret text".into()),
            hidden: true,
            max_access_count: Some(1),
            deletion_date: "2026-09-29T12:00:00.000Z".into(),
            expiration_date: None,
            password: Some(Zeroizing::new("pw".into())),
            emails: vec![],
            hide_email: false,
        };
        let sealed = send.seal(&user).unwrap();
        let r = &sealed.request;
        assert_eq!(r["authType"], 1);
        assert_eq!(r["maxAccessCount"], 1);
        let seed = open_seed(r["key"].as_str().unwrap(), &user).unwrap();
        assert_eq!(seed.as_slice(), sealed.seed.as_ref());
        let key = send_key(&seed).unwrap();
        let text: EncString = r["text"]["text"].as_str().unwrap().parse().unwrap();
        assert_eq!(text.decrypt_string(&key).unwrap().as_str(), "secret text");
        assert_eq!(
            r["password"].as_str().unwrap(),
            send_password_hash("pw", &seed)
        );
        assert_eq!(
            link("https://lock.example.com", "abc", &[0; 16], false),
            "https://lock.example.com/#/send/abc/AAAAAAAAAAAAAAAAAAAAAA"
        );
        assert_eq!(
            link("https://send.example.com/", "abc", &[0; 16], true),
            "https://send.example.com/abc#AAAAAAAAAAAAAAAAAAAAAA"
        );
    }

    #[test]
    fn addresses_exclude_a_password() {
        let user = SymmetricKey::generate();
        let mut send = TextSend {
            name: "x".into(),
            notes: None,
            text: Zeroizing::new("t".into()),
            hidden: false,
            max_access_count: None,
            deletion_date: "2026-09-29T12:00:00.000Z".into(),
            expiration_date: None,
            password: None,
            emails: vec![" A@Example.com ".into(), "b@example.org".into()],
            hide_email: false,
        };
        let sealed = send.seal(&user).unwrap();
        assert_eq!(sealed.request["authType"], 0);
        assert_eq!(sealed.request["emails"], "a@example.com,b@example.org");
        assert!(sealed.request["password"].is_null());
        send.password = Some(Zeroizing::new("pw".into()));
        assert!(send.seal(&user).is_err());
        send.password = None;
        send.emails = vec!["nope".into()];
        assert!(send.seal(&user).is_err());
    }
}
