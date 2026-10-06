//! Sends: making a text Send, sharing an item as one, and the account's own
//! Sends — opened for the list, sealed again when they are changed.
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
use serde::Deserialize;
use serde_json::{json, Value};
use zeroize::Zeroizing;

pub use crate::crypto::generate_send_seed;
use crate::crypto::{encrypt_file, send_key, send_password_hash, EncString, SymmetricKey};
use crate::vault::{FieldKind, Item, Secret};
use crate::Error;
use crate::{entry_send, wire};

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

impl SendAuth {
    /// Bitwarden's number for it.
    pub fn to_wire(self) -> u8 {
        match self {
            SendAuth::Emails => 0,
            SendAuth::Password => 1,
            SendAuth::None => 2,
        }
    }

    pub fn from_wire(value: u8) -> Option<SendAuth> {
        match value {
            0 => Some(SendAuth::Emails),
            1 => Some(SendAuth::Password),
            2 => Some(SendAuth::None),
            _ => None,
        }
    }
}

impl TextSend {
    /// Seals the Send: name, notes and text under the Send's key, the seed
    /// under the user key, the password as its hash. Addresses stay readable:
    /// the server mails the codes to them.
    pub fn seal(&self, user_key: &SymmetricKey) -> Result<SealedSend, Error> {
        self.seal_with_seed(user_key, generate_send_seed())
    }

    /// [`Self::seal`] with a seed made beforehand ([`generate_send_seed`]):
    /// for an entry Send, whose text is tagged with the seed
    /// ([`crate::entry_send::share_entry_text`]).
    pub fn seal_with_seed(
        &self,
        user_key: &SymmetricKey,
        seed: Zeroizing<[u8; 16]>,
    ) -> Result<SealedSend, Error> {
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
            "authType": auth.to_wire(),
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

// ── The account's Sends ────────────────────────────────────

/// One of the account's Sends, opened: what the list and the editor show.
#[derive(Clone)]
pub struct OpenSend {
    pub id: String,
    pub access_id: String,
    /// 0 a text, 1 a file.
    pub kind: u8,
    pub name: String,
    /// Only for the owner; the recipient never sees them.
    pub notes: Option<String>,
    pub text: Option<Zeroizing<String>>,
    /// The text shows only after a click on the recipient's page.
    pub hidden: bool,
    pub file_name: Option<String>,
    /// Of the encrypted file, in bytes.
    pub size: Option<u64>,
    pub max_access_count: Option<u32>,
    pub access_count: u32,
    pub has_password: bool,
    pub auth: SendAuth,
    /// The addresses of [`SendAuth::Emails`].
    pub emails: Vec<String>,
    pub disabled: bool,
    pub hide_email: bool,
    pub revision_date: Option<String>,
    pub expiration_date: Option<String>,
    pub deletion_date: Option<String>,
    /// An entry Send ([`crate::entry_send`]): its marker is tagged with this
    /// Send's seed. Its text isn't edited — the entry is in it a second time.
    pub entry: bool,
    /// The 16-byte seed of the Send's key: for its link, and a change.
    pub seed: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for OpenSend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenSend")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl OpenSend {
    /// The text without an entry Send's marker line; the text itself for a
    /// plain one.
    pub fn readable(&self) -> Option<&str> {
        self.text
            .as_deref()
            .map(|text| entry_send::readable_part(text, &self.seed))
    }
}

/// Opens one of the account's Sends from the sync with the user key.
pub fn open(send: &wire::Send, user_key: &SymmetricKey) -> Result<OpenSend, Error> {
    let seed = open_seed(
        send.key
            .as_deref()
            .ok_or_else(|| Error::Crypto("a Send without its key".into()))?,
        user_key,
    )?;
    let key = send_key(&seed)?;
    let open_text = |value: &Option<String>| -> Result<Option<Zeroizing<String>>, Error> {
        match value.as_deref() {
            None | Some("") => Ok(None),
            Some(text) => text.parse::<EncString>()?.decrypt_string(&key).map(Some),
        }
    };
    let text = match &send.text {
        Some(text) => open_text(&text.text)?,
        None => None,
    };
    let emails: Vec<String> = send
        .emails
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .collect();
    let auth = match send.auth_type.and_then(SendAuth::from_wire) {
        Some(auth) => auth,
        None if !emails.is_empty() => SendAuth::Emails,
        None if send.password.is_some() => SendAuth::Password,
        None => SendAuth::None,
    };
    let entry = text
        .as_deref()
        .is_some_and(|text| entry_send::decode(text, &seed).is_some());
    Ok(OpenSend {
        id: send.id.clone(),
        access_id: send.access_id.clone().unwrap_or_default(),
        kind: send.kind,
        name: open_text(&send.name)?
            .map(|n| n.to_string())
            .unwrap_or_default(),
        notes: open_text(&send.notes)?.map(|n| n.to_string()),
        text,
        hidden: send.text.as_ref().and_then(|t| t.hidden).unwrap_or(false),
        file_name: match &send.file {
            Some(file) => open_text(&file.file_name)?.map(|n| n.to_string()),
            None => None,
        },
        size: send
            .file
            .as_ref()
            .and_then(|f| f.size.as_deref())
            .and_then(|s| s.parse().ok()),
        max_access_count: send.max_access_count,
        access_count: send.access_count.unwrap_or(0),
        has_password: send.password.is_some(),
        auth,
        emails,
        disabled: send.disabled.unwrap_or(false),
        hide_email: send.hide_email.unwrap_or(false),
        revision_date: send.revision_date.clone(),
        expiration_date: send.expiration_date.clone(),
        deletion_date: send.deletion_date.clone(),
        entry,
        seed,
    })
}

/// What the Send editor gives — the web vault's `SendDraft`, field for field.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendDraft {
    /// 0 a text, 1 a file.
    pub kind: u8,
    pub name: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub text: Option<Zeroizing<String>>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub file_name: Option<String>,
    /// A new password; none keeps the one there is.
    #[serde(default)]
    pub password: Option<Zeroizing<String>>,
    #[serde(default)]
    pub max_access_count: Option<u32>,
    /// RFC 3339.
    #[serde(default)]
    pub expiration_date: Option<String>,
    /// RFC 3339.
    pub deletion_date: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub hide_email: bool,
    /// 0 only `emails`, 1 a password (the new one, or the one there is),
    /// 2 anybody. None: as the server has it.
    #[serde(default)]
    pub auth_type: Option<u8>,
    #[serde(default)]
    pub emails: Vec<String>,
}

/// A Send sealed for `POST /api/sends` (a text), `POST /api/sends/file/v2`
/// (a new file, then `file` is uploaded) or `PUT /api/sends/{id}` (a change).
pub struct SealedDraft {
    pub request: Value,
    pub seed: Zeroizing<Vec<u8>>,
    /// The file, encrypted; empty unless a new file Send.
    pub file: Vec<u8>,
}

impl std::fmt::Debug for SealedDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SealedDraft(…)")
    }
}

/// Seals what the editor gives, as the web vault does: a new Send (`existing`
/// none, a fresh seed) or a change to one of the account's Sends, under its
/// own seed. A new file Send brings its file (`file`), which is encrypted
/// here. An entry Send keeps its text whatever the draft says: its entry is
/// tagged with its seed, and a changed text would no longer match it.
pub fn seal_draft(
    draft: &SendDraft,
    existing: Option<&OpenSend>,
    user_key: &SymmetricKey,
    file: Option<&[u8]>,
) -> Result<SealedDraft, Error> {
    if draft.kind > 1 {
        return Err(Error::Crypto("a Send is a text or a file".into()));
    }
    if let Some(send) = existing {
        if send.kind != draft.kind {
            return Err(Error::Crypto("a Send stays a text or a file".into()));
        }
    }
    if draft.name.trim().is_empty() {
        return Err(Error::Crypto("a Send needs a name".into()));
    }
    let seed: Zeroizing<Vec<u8>> = match existing {
        Some(send) => send.seed.clone(),
        None => Zeroizing::new(generate_send_seed().to_vec()),
    };
    let key = send_key(&seed)?;
    let seal = |text: &str| EncString::encrypt(text.as_bytes(), &key).to_string();
    let mut request = json!({
        "type": draft.kind,
        "key": EncString::encrypt(&seed, user_key).to_string(),
        "name": seal(&draft.name),
        "notes": draft.notes.as_deref().filter(|n| !n.is_empty()).map(seal),
        "maxAccessCount": draft.max_access_count.filter(|n| *n > 0),
        "expirationDate": draft.expiration_date,
        "deletionDate": draft.deletion_date,
        "disabled": draft.disabled,
        "hideEmail": draft.hide_email,
        "password": draft
            .password
            .as_deref()
            .filter(|p| !p.is_empty())
            .map(|p| send_password_hash(p, &seed)),
    });
    if let Some(auth) = draft.auth_type {
        let auth = SendAuth::from_wire(auth)
            .ok_or_else(|| Error::Crypto("no such way to open a Send".into()))?;
        request["authType"] = json!(auth.to_wire());
        match auth {
            SendAuth::Emails => {
                let emails: Vec<String> = draft
                    .emails
                    .iter()
                    .map(|e| e.trim().to_lowercase())
                    .filter(|e| !e.is_empty())
                    .collect();
                if emails.is_empty() {
                    return Err(Error::Crypto("name at least one address".into()));
                }
                if emails.iter().any(|e| !looks_like_address(e)) {
                    return Err(Error::Crypto("an address isn't one".into()));
                }
                request["emails"] = json!(emails.join(","));
                request["password"] = Value::Null;
            }
            SendAuth::Password => {
                let keeps = existing.is_some_and(|s| s.has_password);
                if request["password"].is_null() && !keeps {
                    return Err(Error::Crypto("a Send with a password needs one".into()));
                }
            }
            SendAuth::None => request["password"] = Value::Null,
        }
    }
    let mut encrypted = Vec::new();
    if draft.kind == 0 {
        let text = match existing.filter(|s| s.entry) {
            Some(send) => send.text.clone().unwrap_or_default(),
            None => draft.text.clone().unwrap_or_default(),
        };
        if text.trim().is_empty() {
            return Err(Error::Crypto("a text Send needs its text".into()));
        }
        request["text"] = json!({ "text": seal(&text), "hidden": draft.hidden });
    } else {
        let name = draft
            .file_name
            .as_deref()
            .filter(|n| !n.is_empty())
            .or(existing.and_then(|s| s.file_name.as_deref()))
            .unwrap_or("file");
        request["file"] = json!({ "fileName": seal(name) });
        match (existing, file) {
            (None, Some(data)) => {
                encrypted = encrypt_file(data, &key);
                request["fileLength"] = json!(encrypted.len());
            }
            (None, None) => return Err(Error::Crypto("a file Send needs its file".into())),
            // A Send's file stays as it is.
            (Some(_), _) => {}
        }
    }
    if let Some(send) = existing {
        request["id"] = json!(send.id);
    }
    Ok(SealedDraft {
        request,
        seed,
        file: encrypted,
    })
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

    /// What a server's sync gives back for a sealed request: its keys in
    /// lower case (as `parse_sync` reads them), with an id and access id.
    fn synced(request: &Value, id: &str) -> wire::Send {
        fn lower(value: &Value) -> Value {
            match value {
                Value::Object(map) => Value::Object(
                    map.iter()
                        .map(|(k, v)| (k.to_lowercase(), lower(v)))
                        .collect(),
                ),
                other => other.clone(),
            }
        }
        let mut value = lower(request);
        value["id"] = json!(id);
        value["accessid"] = json!(format!("access-{id}"));
        value["accesscount"] = json!(2);
        serde_json::from_value(value).unwrap()
    }

    fn draft(kind: u8) -> SendDraft {
        SendDraft {
            kind,
            name: "Plan".into(),
            notes: Some("only mine".into()),
            text: Some(Zeroizing::new("the text".into())),
            file_name: Some("plan.pdf".into()),
            deletion_date: "2026-10-13T12:00:00.000Z".into(),
            ..SendDraft::default()
        }
    }

    #[test]
    fn a_text_send_opens_and_changes_under_its_own_seed() {
        let user = SymmetricKey::generate();
        let mut new = draft(0);
        new.hidden = true;
        new.max_access_count = Some(3);
        new.auth_type = Some(1);
        new.password = Some(Zeroizing::new("pw".into()));
        let sealed = seal_draft(&new, None, &user, None).unwrap();
        assert!(sealed.request.get("id").is_none());
        let opened = open(&synced(&sealed.request, "s1"), &user).unwrap();
        assert_eq!(opened.name, "Plan");
        assert_eq!(opened.notes.as_deref(), Some("only mine"));
        assert_eq!(opened.text.as_deref().map(|t| t.as_str()), Some("the text"));
        assert_eq!(opened.readable(), Some("the text"));
        assert!(opened.hidden && opened.has_password && !opened.entry);
        assert_eq!(opened.auth, SendAuth::Password);
        assert_eq!((opened.max_access_count, opened.access_count), (Some(3), 2));
        assert_eq!(opened.seed.as_slice(), sealed.seed.as_slice());

        // A change keeps the seed (the link stays) and, without a new one, the password.
        let mut change = draft(0);
        change.text = Some(Zeroizing::new("new text".into()));
        change.auth_type = Some(1);
        change.disabled = true;
        let changed = seal_draft(&change, Some(&opened), &user, None).unwrap();
        assert_eq!(changed.request["id"], "s1");
        assert!(changed.request["password"].is_null());
        assert_eq!(changed.seed.as_slice(), opened.seed.as_slice());
        let again = open(&synced(&changed.request, "s1"), &user).unwrap();
        assert_eq!(again.text.as_deref().map(|t| t.as_str()), Some("new text"));
        assert!(again.disabled);

        // A password Send without any password, and anybody: no password.
        let mut none = draft(0);
        none.auth_type = Some(1);
        assert!(seal_draft(&none, None, &user, None).is_err());
        none.auth_type = Some(2);
        none.password = Some(Zeroizing::new("ignored".into()));
        let open_to_all = seal_draft(&none, Some(&opened), &user, None).unwrap();
        assert!(open_to_all.request["password"].is_null());
        assert_eq!(open_to_all.request["authType"], 2);
    }

    #[test]
    fn addresses_and_the_auth_type_come_back_from_the_sync() {
        let user = SymmetricKey::generate();
        let mut only = draft(0);
        only.auth_type = Some(0);
        only.emails = vec![" A@Example.com".into(), "b@example.org ".into()];
        let sealed = seal_draft(&only, None, &user, None).unwrap();
        assert_eq!(sealed.request["emails"], "a@example.com,b@example.org");
        let opened = open(&synced(&sealed.request, "s2"), &user).unwrap();
        assert_eq!(opened.auth, SendAuth::Emails);
        assert_eq!(opened.emails, ["a@example.com", "b@example.org"]);
        only.emails = vec!["nope".into()];
        assert!(seal_draft(&only, None, &user, None).is_err());
        only.emails.clear();
        assert!(seal_draft(&only, None, &user, None).is_err());
    }

    #[test]
    fn a_file_send_carries_its_file_encrypted_once() {
        let user = SymmetricKey::generate();
        assert!(seal_draft(&draft(1), None, &user, None).is_err());
        let sealed = seal_draft(&draft(1), None, &user, Some(b"%PDF-1.7")).unwrap();
        assert_eq!(sealed.request["fileLength"], sealed.file.len());
        assert!(sealed.request.get("text").is_none());
        let key = send_key(&sealed.seed).unwrap();
        assert_eq!(
            crate::crypto::decrypt_file(&sealed.file, &key)
                .unwrap()
                .as_slice(),
            b"%PDF-1.7"
        );
        let opened = open(&synced(&sealed.request, "s3"), &user).unwrap();
        assert_eq!(opened.file_name.as_deref(), Some("plan.pdf"));
        // Changed: the file stays, nothing to upload; a text it can't become.
        let mut change = draft(1);
        change.file_name = None;
        let changed = seal_draft(&change, Some(&opened), &user, Some(b"other")).unwrap();
        assert!(changed.file.is_empty() && changed.request.get("fileLength").is_none());
        let again = open(&synced(&changed.request, "s3"), &user).unwrap();
        assert_eq!(again.file_name.as_deref(), Some("plan.pdf"));
        assert!(seal_draft(&draft(0), Some(&opened), &user, None).is_err());
    }

    #[test]
    fn an_entry_send_keeps_its_text() {
        let user = SymmetricKey::generate();
        let item = login();
        let fields = vec![("username".to_string(), "Username".to_string())];
        let seed = generate_send_seed();
        let text = entry_send::share_entry_text(&item, &fields, seed.as_ref());
        let sealed = TextSend {
            name: "Shop".into(),
            notes: None,
            text,
            hidden: false,
            max_access_count: None,
            deletion_date: "2026-10-13T12:00:00.000Z".into(),
            expiration_date: None,
            password: None,
            emails: vec![],
            hide_email: false,
        }
        .seal_with_seed(&user, seed)
        .unwrap();
        let opened = open(&synced(&sealed.request, "s4"), &user).unwrap();
        assert!(opened.entry);
        assert_eq!(opened.readable(), Some("Shop\nUsername: nyu"));
        let mut change = draft(0);
        change.text = Some(Zeroizing::new("replaced".into()));
        let changed = seal_draft(&change, Some(&opened), &user, None).unwrap();
        let again = open(&synced(&changed.request, "s4"), &user).unwrap();
        assert!(again.entry);
        assert_eq!(again.text, opened.text);
    }

    #[test]
    fn a_send_under_another_key_does_not_open() {
        let sealed = seal_draft(&draft(0), None, &SymmetricKey::generate(), None).unwrap();
        assert!(open(&synced(&sealed.request, "s5"), &SymmetricKey::generate()).is_err());
    }
}
