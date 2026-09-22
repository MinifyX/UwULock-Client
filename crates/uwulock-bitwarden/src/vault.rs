//! The sync, opened: every item, folder and collection decrypted with the
//! account's keys.
//!
//! An item whose key or name doesn't open is kept, marked `broken`, so it
//! doesn't silently vanish from the list; a single field that doesn't open is
//! left empty and marks the item too.

use std::collections::HashMap;
use zeroize::Zeroizing;

use crate::crypto::{EncString, PrivateKey, SymmetricKey};
use crate::wire;
use crate::Error;

pub type Secret = Zeroizing<String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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
}

#[derive(Debug, Default)]
pub struct Login {
    pub username: Option<Secret>,
    pub password: Option<Secret>,
    /// The authenticator key: a base32 secret, an `otpauth://` or `steam://` URI.
    pub totp: Option<Secret>,
    pub uris: Vec<LoginUri>,
    pub password_revision_date: Option<String>,
    /// Passkeys stored with the login. Counted only: UwULock can't use them yet.
    pub passkeys: usize,
}

#[derive(Debug)]
pub struct LoginUri {
    pub uri: Secret,
    /// Bitwarden's match detection: 0 domain, 1 host, 2 starts with, 3 exact,
    /// 4 regex, 5 never. `None` follows the account's default.
    pub match_kind: Option<u32>,
}

#[derive(Debug, Default)]
pub struct Card {
    pub cardholder_name: Option<Secret>,
    pub brand: Option<Secret>,
    pub number: Option<Secret>,
    pub exp_month: Option<Secret>,
    pub exp_year: Option<Secret>,
    pub code: Option<Secret>,
}

#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
pub struct SshKey {
    pub private_key: Option<Secret>,
    pub public_key: Option<Secret>,
    pub fingerprint: Option<Secret>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldKind {
    Text,
    Hidden,
    Boolean,
    /// Points at another field of the item (username, password); has no value of its own.
    Linked,
}

#[derive(Debug)]
pub struct Field {
    pub name: Option<Secret>,
    pub value: Option<Secret>,
    pub kind: FieldKind,
}

#[derive(Debug)]
pub struct PasswordHistory {
    pub password: Secret,
    pub last_used: Option<String>,
}

#[derive(Debug)]
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
}

impl Vault {
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
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
        let mut org_keys: HashMap<&str, SymmetricKey> = HashMap::new();
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
                    org_keys.insert(org.id.as_str(), key);
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
                let key = org_keys.get(collection.organization_id.as_str())?;
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
                Some(org) => match org_keys.get(org.as_str()) {
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
                })
            })
            .collect(),
        password_revision_date: login.password_revision_date.clone(),
        passkeys: login.fido2_credentials.as_ref().map_or(0, Vec::len),
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
            kind: match field.kind {
                Some(1) => FieldKind::Hidden,
                Some(2) => FieldKind::Boolean,
                Some(3) => FieldKind::Linked,
                _ => FieldKind::Text,
            },
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
    }
}
