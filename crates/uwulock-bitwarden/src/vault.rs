//! The sync, opened: every item, folder and collection decrypted with the
//! account's keys — and sealed again when something is saved.
//!
//! An item whose key or name doesn't open is kept, marked `broken`, so it
//! doesn't silently vanish from the list; a single field that doesn't open is
//! left empty and marks the item too.
//!
//! An item carries what it takes to put it back: the key its values live
//! under, and the pieces UwULock doesn't show (passkeys, linked fields, the
//! checksum of an address). [`Item::seal`] hands all of it back, so saving a
//! name never costs a passkey.

use std::collections::HashMap;
use zeroize::Zeroizing;

use crate::crypto::{EncString, PrivateKey, SymmetricKey};
use crate::wire;
use crate::Error;

pub type Secret = Zeroizing<String>;

/// A value the server never saw as empty: Bitwarden leaves such a field out.
fn some(value: &Option<Secret>) -> Option<&Secret> {
    value.as_ref().filter(|v| !v.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    Login,
    Note,
    Card,
    Identity,
    SshKey,
}

impl ItemKind {
    fn from_wire(kind: u8) -> Option<Self> {
        Some(match kind {
            1 => ItemKind::Login,
            2 => ItemKind::Note,
            3 => ItemKind::Card,
            4 => ItemKind::Identity,
            5 => ItemKind::SshKey,
            _ => return None,
        })
    }

    pub fn to_wire(self) -> u8 {
        match self {
            ItemKind::Login => 1,
            ItemKind::Note => 2,
            ItemKind::Card => 3,
            ItemKind::Identity => 4,
            ItemKind::SshKey => 5,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Login {
    pub username: Option<Secret>,
    pub password: Option<Secret>,
    /// The authenticator key: a base32 secret, an `otpauth://` or `steam://` URI.
    pub totp: Option<Secret>,
    pub uris: Vec<LoginUri>,
    pub password_revision_date: Option<String>,
    /// Passkeys stored with the login, as the server sent them. UwULock can't
    /// use them yet, and hands them back untouched.
    pub passkeys: Option<Vec<serde_json::Value>>,
    pub autofill_on_page_load: Option<bool>,
}

impl Login {
    pub fn passkey_count(&self) -> usize {
        self.passkeys.as_ref().map_or(0, Vec::len)
    }
}

#[derive(Debug, Clone)]
pub struct LoginUri {
    pub uri: Secret,
    /// Bitwarden's match detection: 0 domain, 1 host, 2 starts with, 3 exact,
    /// 4 regex, 5 never. `None` follows the account's default.
    pub match_kind: Option<u32>,
    /// Only valid for this exact address; dropped when the address changes.
    pub checksum: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Card {
    pub cardholder_name: Option<Secret>,
    pub brand: Option<Secret>,
    pub number: Option<Secret>,
    pub exp_month: Option<Secret>,
    pub exp_year: Option<Secret>,
    pub code: Option<Secret>,
}

#[derive(Debug, Default, Clone)]
pub struct Identity {
    pub title: Option<Secret>,
    pub first_name: Option<Secret>,
    pub middle_name: Option<Secret>,
    pub last_name: Option<Secret>,
    pub username: Option<Secret>,
    pub company: Option<Secret>,
    pub email: Option<Secret>,
    pub phone: Option<Secret>,
    pub address1: Option<Secret>,
    pub address2: Option<Secret>,
    pub address3: Option<Secret>,
    pub postal_code: Option<Secret>,
    pub city: Option<Secret>,
    pub state: Option<Secret>,
    pub country: Option<Secret>,
    pub ssn: Option<Secret>,
    pub passport_number: Option<Secret>,
    pub license_number: Option<Secret>,
}

#[derive(Debug, Default, Clone)]
pub struct SshKey {
    pub private_key: Option<Secret>,
    pub public_key: Option<Secret>,
    pub fingerprint: Option<Secret>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldKind {
    Text,
    Hidden,
    Boolean,
    /// Points at another field of the item (username, password); has no value of its own.
    Linked,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: Option<Secret>,
    pub value: Option<Secret>,
    pub kind: FieldKind,
    /// Which value of the item a linked field points at. Kept as it came.
    pub linked_id: Option<u32>,
}

impl FieldKind {
    fn from_wire(kind: Option<u32>) -> Self {
        match kind {
            Some(1) => FieldKind::Hidden,
            Some(2) => FieldKind::Boolean,
            Some(3) => FieldKind::Linked,
            _ => FieldKind::Text,
        }
    }

    pub fn to_wire(self) -> u32 {
        match self {
            FieldKind::Text => 0,
            FieldKind::Hidden => 1,
            FieldKind::Boolean => 2,
            FieldKind::Linked => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PasswordHistory {
    pub password: Secret,
    pub last_used: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    pub kind: ItemKind,
    pub name: Secret,
    pub notes: Option<Secret>,
    pub folder_id: Option<String>,
    pub organization_id: Option<String>,
    pub collection_ids: Vec<String>,
    pub favorite: bool,
    /// Asks for the master password again before showing or copying secrets.
    pub reprompt: bool,
    pub revision_date: Option<String>,
    pub creation_date: Option<String>,
    /// In the trash.
    pub deleted: bool,
    pub login: Option<Login>,
    pub card: Option<Card>,
    pub identity: Option<Identity>,
    pub ssh_key: Option<SshKey>,
    pub fields: Vec<Field>,
    pub password_history: Vec<PasswordHistory>,
    pub attachments: usize,
    /// Something in it didn't decrypt.
    pub broken: bool,
    /// The item's own key, if it has one: newer items keep their values under
    /// a key of their own, which itself is wrapped under the user or
    /// organisation key.
    pub key: Option<SymmetricKey>,
    /// That key as the server keeps it, to hand back on a save.
    pub wrapped_key: Option<String>,
    /// Set while the item is archived. A save that leaves it out un-archives.
    pub archived_date: Option<String>,
    /// Which kind of note this is (0 is the only one Bitwarden has).
    pub note_kind: Option<u32>,
}

impl Item {
    /// A new item of `kind`, with nothing in it yet.
    pub fn new(kind: ItemKind) -> Item {
        Item {
            id: String::new(),
            kind,
            name: Secret::default(),
            notes: None,
            folder_id: None,
            organization_id: None,
            collection_ids: Vec::new(),
            favorite: false,
            reprompt: false,
            revision_date: None,
            creation_date: None,
            deleted: false,
            login: (kind == ItemKind::Login).then(Login::default),
            card: (kind == ItemKind::Card).then(Card::default),
            identity: (kind == ItemKind::Identity).then(Identity::default),
            ssh_key: (kind == ItemKind::SshKey).then(SshKey::default),
            fields: Vec::new(),
            password_history: Vec::new(),
            attachments: 0,
            broken: false,
            key: None,
            wrapped_key: None,
            archived_date: None,
            note_kind: (kind == ItemKind::Note).then_some(0),
        }
    }

    /// The item as the server takes it back, every value encrypted again.
    ///
    /// `outer` is the key the item belongs under — the account's user key, or
    /// the organisation's. An item with a key of its own keeps using that one
    /// for its values; everything else UwULock doesn't touch travels along.
    ///
    /// A broken item is never sealed: half of it didn't open, and saving it
    /// would write that half away. See [`Item::can_save`].
    pub fn seal(&self, outer: &SymmetricKey) -> Result<wire::CipherRequest, Error> {
        if self.broken {
            return Err(Error::Refused(
                "this item didn't fully decrypt, so UwULock won't write it back".into(),
            ));
        }
        let key = self.key.as_ref().unwrap_or(outer);
        let seal = |value: &Option<Secret>| {
            some(value).map(|v| EncString::encrypt(v.as_bytes(), key).to_string())
        };
        let login = self.login.as_ref().map(|l| wire::LoginRequest {
            username: seal(&l.username),
            password: seal(&l.password),
            totp: seal(&l.totp),
            uris: l
                .uris
                .iter()
                .filter(|u| !u.uri.is_empty())
                .map(|u| wire::LoginUriRequest {
                    uri: Some(EncString::encrypt(u.uri.as_bytes(), key).to_string()),
                    match_kind: u.match_kind,
                    uri_checksum: u.checksum.clone(),
                })
                .collect(),
            password_revision_date: l.password_revision_date.clone(),
            fido2_credentials: l.passkeys.clone(),
            autofill_on_page_load: l.autofill_on_page_load,
        });
        let card = self.card.as_ref().map(|c| wire::CardRequest {
            cardholder_name: seal(&c.cardholder_name),
            brand: seal(&c.brand),
            number: seal(&c.number),
            exp_month: seal(&c.exp_month),
            exp_year: seal(&c.exp_year),
            code: seal(&c.code),
        });
        let identity = self.identity.as_ref().map(|i| wire::IdentityRequest {
            title: seal(&i.title),
            first_name: seal(&i.first_name),
            middle_name: seal(&i.middle_name),
            last_name: seal(&i.last_name),
            username: seal(&i.username),
            company: seal(&i.company),
            email: seal(&i.email),
            phone: seal(&i.phone),
            address1: seal(&i.address1),
            address2: seal(&i.address2),
            address3: seal(&i.address3),
            postal_code: seal(&i.postal_code),
            city: seal(&i.city),
            state: seal(&i.state),
            country: seal(&i.country),
            ssn: seal(&i.ssn),
            passport_number: seal(&i.passport_number),
            license_number: seal(&i.license_number),
        });
        let ssh_key = self.ssh_key.as_ref().map(|s| wire::SshKeyRequest {
            private_key: seal(&s.private_key),
            public_key: seal(&s.public_key),
            key_fingerprint: seal(&s.fingerprint),
        });
        let fields = self
            .fields
            .iter()
            .map(|f| wire::FieldRequest {
                name: seal(&f.name),
                value: seal(&f.value),
                kind: f.kind.to_wire(),
                linked_id: f.linked_id,
            })
            .collect::<Vec<_>>();
        let history = self
            .password_history
            .iter()
            .filter(|h| !h.password.is_empty())
            .map(|h| wire::PasswordHistoryRequest {
                password: EncString::encrypt(h.password.as_bytes(), key).to_string(),
                last_used_date: h
                    .last_used
                    .clone()
                    .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".into()),
            })
            .collect::<Vec<_>>();

        Ok(wire::CipherRequest {
            kind: self.kind.to_wire(),
            name: EncString::encrypt(self.name.as_bytes(), key).to_string(),
            notes: seal(&self.notes),
            favorite: self.favorite,
            reprompt: u8::from(self.reprompt),
            folder_id: self.folder_id.clone(),
            organization_id: self.organization_id.clone(),
            key: self.wrapped_key.clone(),
            login,
            card,
            identity,
            secure_note: (self.kind == ItemKind::Note).then(|| wire::SecureNoteRequest {
                kind: self.note_kind.unwrap_or(0),
            }),
            ssh_key,
            fields: (!fields.is_empty()).then_some(fields),
            password_history: (!history.is_empty()).then_some(history),
            last_known_revision_date: self.revision_date.clone(),
            archived_date: self.archived_date.clone(),
        })
    }

    /// Sets a login's password and keeps the one before it, the way
    /// Bitwarden's clients do: newest first, five at most. `now` is an ISO
    /// date, the one the item's password revision gets too.
    pub fn set_password(&mut self, password: Secret, now: &str) {
        let Some(login) = self.login.as_mut() else {
            return;
        };
        let previous = login.password.replace(password.clone());
        match previous {
            Some(old) if !old.is_empty() && old != password => {
                login.password_revision_date = Some(now.to_string());
                self.password_history.insert(
                    0,
                    PasswordHistory {
                        password: old,
                        last_used: Some(now.to_string()),
                    },
                );
                self.password_history.truncate(5);
            }
            // Nothing was there, or nothing changed: no history entry.
            _ => {
                if login.password_revision_date.is_none() && !password.is_empty() {
                    login.password_revision_date = Some(now.to_string());
                }
            }
        }
    }

    /// Whether UwULock may write this item back. A server that keeps the SSH
    /// key material only when all three parts are there would otherwise empty
    /// the item.
    pub fn can_save(&self) -> Result<(), Error> {
        if self.broken {
            return Err(Error::Refused(
                "this item didn't fully decrypt, so UwULock won't write it back".into(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(Error::Refused("an item needs a name".into()));
        }
        if let Some(ssh) = &self.ssh_key {
            let parts = [&ssh.private_key, &ssh.public_key, &ssh.fingerprint];
            if parts.iter().any(|part| some(part).is_none()) {
                return Err(Error::Refused(
                    "an SSH key needs its private key, its public key and its fingerprint".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub organization_id: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct Vault {
    pub email: String,
    pub name: Option<String>,
    pub items: Vec<Item>,
    pub folders: Vec<Folder>,
    pub collections: Vec<Collection>,
    pub organizations: Vec<Organization>,
    /// Items of a kind UwULock doesn't know, or organisations whose key didn't open.
    pub skipped: usize,
    /// The organisations' keys, kept for saving items that belong to one.
    org_keys: HashMap<String, SymmetricKey>,
}

impl Vault {
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    /// The key an item belongs under: the organisation's, or the account's own.
    pub fn outer_key<'a>(
        &'a self,
        organization_id: Option<&str>,
        user_key: &'a SymmetricKey,
    ) -> Result<&'a SymmetricKey, Error> {
        match organization_id {
            None => Ok(user_key),
            Some(id) => self.org_keys.get(id).ok_or_else(|| {
                Error::Refused("this item belongs to an organisation UwULock has no key for".into())
            }),
        }
    }

    /// Opens a sync with the account's user key.
    pub fn open(sync: &wire::Sync, user_key: &SymmetricKey) -> Result<Vault, Error> {
        let private = match &sync.profile.private_key {
            Some(text) => {
                let der = text.parse::<EncString>()?.decrypt(user_key)?;
                Some(PrivateKey::from_der(&der)?)
            }
            None => None,
        };

        let mut skipped = 0;
        let mut org_keys: HashMap<String, SymmetricKey> = HashMap::new();
        let mut organizations = Vec::new();
        for org in &sync.profile.organizations {
            let key = match (&org.key, &private) {
                (Some(key), Some(private)) => key
                    .parse::<EncString>()
                    .and_then(|enc| enc.decrypt_rsa(private))
                    .and_then(|bytes| SymmetricKey::from_bytes(&bytes)),
                _ => Err(Error::Crypto("no organisation key".into())),
            };
            match key {
                Ok(key) => {
                    org_keys.insert(org.id.clone(), key);
                    organizations.push(Organization {
                        id: org.id.clone(),
                        name: org.name.clone().unwrap_or_default(),
                    });
                }
                Err(error) => {
                    tracing::warn!(org = %org.id, %error, "organisation key didn't open");
                    skipped += 1;
                }
            }
        }

        let folders = sync
            .folders
            .iter()
            .map(|folder| Folder {
                id: folder.id.clone(),
                name: open_text(&folder.name, user_key)
                    .ok()
                    .flatten()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "?".into()),
            })
            .collect();

        let collections = sync
            .collections
            .iter()
            .filter_map(|collection| {
                let key = org_keys.get(&collection.organization_id)?;
                Some(Collection {
                    id: collection.id.clone(),
                    organization_id: collection.organization_id.clone(),
                    name: open_text(&collection.name, key)
                        .ok()
                        .flatten()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "?".into()),
                })
            })
            .collect();

        let mut items = Vec::with_capacity(sync.ciphers.len());
        for cipher in &sync.ciphers {
            let Some(kind) = ItemKind::from_wire(cipher.kind) else {
                skipped += 1;
                continue;
            };
            let outer = match &cipher.organization_id {
                Some(org) => match org_keys.get(org) {
                    Some(key) => key,
                    None => {
                        skipped += 1;
                        continue;
                    }
                },
                None => user_key,
            };
            items.push(open_item(cipher, kind, outer));
        }

        Ok(Vault {
            email: sync.profile.email.clone(),
            name: sync.profile.name.clone(),
            items,
            folders,
            collections,
            organizations,
            skipped,
            org_keys,
        })
    }
}

fn open_text(value: &Option<String>, key: &SymmetricKey) -> Result<Option<Secret>, Error> {
    match value {
        None => Ok(None),
        Some(text) if text.is_empty() => Ok(None),
        Some(text) => text.parse::<EncString>()?.decrypt_string(key).map(Some),
    }
}

fn open_item(cipher: &wire::Cipher, kind: ItemKind, outer: &SymmetricKey) -> Item {
    let mut broken = false;
    let item_key = match &cipher.key {
        Some(text) => match text.parse::<EncString>().and_then(|e| e.decrypt_key(outer)) {
            Ok(key) => Some(key),
            Err(error) => {
                tracing::warn!(item = %cipher.id, %error, "item key didn't open");
                broken = true;
                None
            }
        },
        None => None,
    };
    let key = item_key.as_ref().unwrap_or(outer);
    let mut open = |value: &Option<String>| match open_text(value, key) {
        Ok(value) => value,
        Err(_) => {
            broken = true;
            None
        }
    };

    let name = open(&cipher.name).unwrap_or_else(|| Zeroizing::new(String::new()));
    let notes = open(&cipher.notes);
    let login = cipher.login.as_ref().map(|login| Login {
        username: open(&login.username),
        password: open(&login.password),
        totp: open(&login.totp),
        uris: login
            .uris
            .iter()
            .flatten()
            .filter_map(|uri| {
                Some(LoginUri {
                    uri: open(&uri.uri)?,
                    match_kind: uri.match_kind,
                    checksum: uri.checksum.clone(),
                })
            })
            .collect(),
        password_revision_date: login.password_revision_date.clone(),
        passkeys: login.fido2_credentials.clone(),
        autofill_on_page_load: login.autofill_on_page_load,
    });
    let card = cipher.card.as_ref().map(|card| Card {
        cardholder_name: open(&card.cardholder_name),
        brand: open(&card.brand),
        number: open(&card.number),
        exp_month: open(&card.exp_month),
        exp_year: open(&card.exp_year),
        code: open(&card.code),
    });
    let identity = cipher.identity.as_ref().map(|id| Identity {
        title: open(&id.title),
        first_name: open(&id.first_name),
        middle_name: open(&id.middle_name),
        last_name: open(&id.last_name),
        username: open(&id.username),
        company: open(&id.company),
        email: open(&id.email),
        phone: open(&id.phone),
        address1: open(&id.address1),
        address2: open(&id.address2),
        address3: open(&id.address3),
        postal_code: open(&id.postal_code),
        city: open(&id.city),
        state: open(&id.state),
        country: open(&id.country),
        ssn: open(&id.ssn),
        passport_number: open(&id.passport_number),
        license_number: open(&id.license_number),
    });
    let ssh_key = cipher.ssh_key.as_ref().map(|ssh| SshKey {
        private_key: open(&ssh.private_key),
        public_key: open(&ssh.public_key),
        fingerprint: open(&ssh.fingerprint),
    });
    let fields = cipher
        .fields
        .iter()
        .flatten()
        .map(|field| Field {
            name: open(&field.name),
            value: open(&field.value),
            kind: FieldKind::from_wire(field.kind),
            linked_id: field.linked_id,
        })
        .collect();
    let password_history = cipher
        .password_history
        .iter()
        .flatten()
        .filter_map(|entry| {
            Some(PasswordHistory {
                password: open(&entry.password)?,
                last_used: entry.last_used_date.clone(),
            })
        })
        .collect();

    Item {
        id: cipher.id.clone(),
        kind,
        name,
        notes,
        folder_id: cipher.folder_id.clone(),
        organization_id: cipher.organization_id.clone(),
        collection_ids: cipher.collection_ids.clone(),
        favorite: cipher.favorite,
        reprompt: cipher.reprompt.unwrap_or(0) == 1,
        revision_date: cipher.revision_date.clone(),
        creation_date: cipher.creation_date.clone(),
        deleted: cipher.deleted_date.is_some(),
        login,
        card,
        identity,
        ssh_key,
        fields,
        password_history,
        attachments: cipher.attachments.as_ref().map_or(0, Vec::len),
        broken,
        key: item_key,
        wrapped_key: cipher.key.clone(),
        archived_date: cipher.archived_date.clone(),
        note_kind: cipher.secure_note.as_ref().and_then(|note| note.kind),
    }
}
