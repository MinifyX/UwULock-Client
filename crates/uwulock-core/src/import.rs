//! Moving in from a file. The apps read every other password manager's
//! export themselves (TypeScript, `apps/desktop/src/lib/import`, the same
//! module as the web vault's) and hand over Bitwarden's unencrypted JSON
//! export; Bitwarden's own CSV comes as it is. Here they become items, ready
//! to be sealed and sent to `/api/ciphers/import`.
//!
//! The reading is the web vault's (UwULock-Server `web/wasm/src/transfer.rs`):
//! change both together. On top of that: Bitwarden's password-protected JSON
//! export ([`open_protected_export`]), and KeePass's key derivations
//! ([`kdbx_argon2`], [`kdbx_aes_kdf`]), which are too slow for JavaScript.

use crate::crypto::{EncString, Kdf, SymmetricKey};
use crate::vault::{Field, FieldKind, Item, ItemKind, LoginUri, PasswordHistory, Secret};
use crate::Error;
use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use aes::Aes256;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// What a file holds, read: the items (not sealed yet), the names of the
/// folders it brings, and which item goes into which of those folders
/// (indices into both lists).
pub struct Prepared {
    pub items: Vec<Item>,
    pub folders: Vec<String>,
    pub in_folder: Vec<(usize, usize)>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Refused(message.into())
}

/// Reads an import: `json` (Bitwarden's unencrypted JSON export) or `csv`
/// (Bitwarden's CSV export). `now` stands in for a password history entry
/// without a date.
pub fn read(format: &str, text: &str, now: &str) -> Result<Prepared, Error> {
    match format {
        "json" => read_json(text, now),
        "csv" => read_csv(text),
        other => Err(invalid(format!("unknown import format {other}"))),
    }
}

// ── Passkeys ──────────────────────────────────────────────
//
// A passkey is kept the way Bitwarden keeps it: every field encrypted, but
// `creationDate`. A file has them plain; they are sealed here.

/// The one field of a passkey that is not encrypted.
const PLAIN_PASSKEY_FIELD: &str = "creationDate";

/// Passkeys from a file, where they are plain, encrypted under `key`. A field
/// encrypted under `key` already (a file this vault wrote) stays.
pub fn seal_passkeys(passkeys: &mut [Value], key: &SymmetricKey) -> Result<(), Error> {
    for passkey in passkeys {
        let Value::Object(fields) = passkey else {
            return Err(invalid("a passkey in the file is not an object"));
        };
        seal_passkey_fields(fields, key)?;
    }
    Ok(())
}

fn seal_passkey_fields(fields: &mut Map<String, Value>, key: &SymmetricKey) -> Result<(), Error> {
    for (name, value) in fields.iter_mut() {
        if name == PLAIN_PASSKEY_FIELD {
            continue;
        }
        let plain = match value {
            Value::Null => continue,
            Value::String(text) => {
                if text
                    .parse::<EncString>()
                    .is_ok_and(|sealed| sealed.decrypt(key).is_ok())
                {
                    continue;
                }
                Zeroizing::new(text.clone())
            }
            Value::Bool(_) | Value::Number(_) => Zeroizing::new(value.to_string()),
            _ => {
                return Err(invalid(
                    "a passkey in the file has a field of an unexpected shape",
                ))
            }
        };
        *value = Value::String(EncString::encrypt(plain.as_bytes(), key).to_string());
    }
    Ok(())
}

// ── Bitwarden's JSON ──────────────────────────────────────

fn some_text(value: Option<&Value>) -> Option<Secret> {
    match value? {
        Value::String(text) if !text.is_empty() => Some(Zeroizing::new(text.clone())),
        Value::Number(number) => Some(Zeroizing::new(number.to_string())),
        Value::Bool(flag) => Some(Zeroizing::new(flag.to_string())),
        _ => None,
    }
}

fn field_kind(value: Option<&Value>) -> FieldKind {
    match value.and_then(Value::as_u64) {
        Some(1) => FieldKind::Hidden,
        Some(2) => FieldKind::Boolean,
        Some(3) => FieldKind::Linked,
        _ => FieldKind::Text,
    }
}

fn read_json(text: &str, now: &str) -> Result<Prepared, Error> {
    let value: Value =
        serde_json::from_str(text).map_err(|_| invalid("this is not a Bitwarden JSON export"))?;
    if value.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(invalid(
            "this export is encrypted; open it first (open_protected_export)",
        ));
    }
    let folders: Vec<(String, String)> = value
        .get("folders")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|f| {
                    Some((
                        f.get("id")?.as_str()?.to_string(),
                        f.get("name")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let list = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("this is not a Bitwarden JSON export"))?;
    let mut items = Vec::new();
    let mut in_folder = Vec::new();
    for entry in list {
        let kind = match entry.get("type").and_then(Value::as_u64) {
            Some(1) => ItemKind::Login,
            Some(2) => ItemKind::Note,
            Some(3) => ItemKind::Card,
            Some(4) => ItemKind::Identity,
            Some(5) => ItemKind::SshKey,
            _ => continue,
        };
        let mut item = Item::new(kind);
        item.name = some_text(entry.get("name")).unwrap_or_else(|| Zeroizing::new("?".into()));
        item.notes = some_text(entry.get("notes"));
        item.favorite = entry
            .get("favorite")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        item.reprompt = entry.get("reprompt").and_then(Value::as_u64) == Some(1);
        if let Some(login) = entry.get("login").filter(|_| kind == ItemKind::Login) {
            let current = item.login.as_mut().expect("a login has one");
            current.username = some_text(login.get("username"));
            current.password = some_text(login.get("password"));
            current.totp = some_text(login.get("totp"));
            current.password_revision_date = login
                .get("passwordRevisionDate")
                .and_then(Value::as_str)
                .map(str::to_string);
            current.uris = login
                .get("uris")
                .and_then(Value::as_array)
                .map(|uris| {
                    uris.iter()
                        .filter_map(|u| {
                            Some(LoginUri {
                                uri: some_text(u.get("uri"))?,
                                match_kind: u
                                    .get("match")
                                    .and_then(Value::as_u64)
                                    .map(|m| m as u32)
                                    .filter(|m| *m <= 5),
                                checksum: None,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            current.passkeys = login
                .get("fido2Credentials")
                .and_then(Value::as_array)
                .filter(|keys| !keys.is_empty())
                .cloned();
        }
        if let Some(card) = entry.get("card").filter(|_| kind == ItemKind::Card) {
            let current = item.card.as_mut().expect("a card has one");
            current.cardholder_name = some_text(card.get("cardholderName"));
            current.brand = some_text(card.get("brand"));
            current.number = some_text(card.get("number"));
            current.exp_month = some_text(card.get("expMonth"));
            current.exp_year = some_text(card.get("expYear"));
            current.code = some_text(card.get("code"));
        }
        if let Some(identity) = entry.get("identity").filter(|_| kind == ItemKind::Identity) {
            let current = item.identity.as_mut().expect("an identity has one");
            let get = |name: &str| some_text(identity.get(name));
            current.title = get("title");
            current.first_name = get("firstName");
            current.middle_name = get("middleName");
            current.last_name = get("lastName");
            current.username = get("username");
            current.company = get("company");
            current.email = get("email");
            current.phone = get("phone");
            current.address1 = get("address1");
            current.address2 = get("address2");
            current.address3 = get("address3");
            current.postal_code = get("postalCode");
            current.city = get("city");
            current.state = get("state");
            current.country = get("country");
            current.ssn = get("ssn");
            current.passport_number = get("passportNumber");
            current.license_number = get("licenseNumber");
        }
        if let Some(ssh) = entry.get("sshKey").filter(|_| kind == ItemKind::SshKey) {
            let current = item.ssh_key.as_mut().expect("an SSH key has one");
            current.private_key = some_text(ssh.get("privateKey"));
            current.public_key = some_text(ssh.get("publicKey"));
            current.fingerprint = some_text(ssh.get("keyFingerprint"));
        }
        item.fields = entry
            .get("fields")
            .and_then(Value::as_array)
            .map(|fields| {
                fields
                    .iter()
                    .map(|f| Field {
                        name: some_text(f.get("name")),
                        value: some_text(f.get("value")),
                        kind: field_kind(f.get("type")),
                        linked_id: f.get("linkedId").and_then(Value::as_u64).map(|n| n as u32),
                    })
                    .collect()
            })
            .unwrap_or_default();
        item.password_history = entry
            .get("passwordHistory")
            .and_then(Value::as_array)
            .map(|history| {
                history
                    .iter()
                    .filter_map(|h| {
                        Some(PasswordHistory {
                            password: some_text(h.get("password"))?,
                            last_used: Some(
                                h.get("lastUsedDate")
                                    .and_then(Value::as_str)
                                    .unwrap_or(now)
                                    .to_string(),
                            ),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(folder) = entry.get("folderId").and_then(Value::as_str) {
            if let Some(index) = folders.iter().position(|(id, _)| id == folder) {
                in_folder.push((items.len(), index));
            }
        }
        items.push(item);
    }
    Ok(Prepared {
        items,
        folders: folders.into_iter().map(|(_, name)| name).collect(),
        in_folder,
    })
}

// ── Bitwarden's CSV ───────────────────────────────────────

/// What a spreadsheet would run as a formula: an export puts a `'` in front.
const FORMULA_START: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// The `'` an export put in front of a formula goes again.
fn formula_back(value: String) -> String {
    match value.strip_prefix('\'') {
        Some(rest) if rest.starts_with(FORMULA_START) => rest.to_string(),
        _ => value,
    }
}

/// Rows of a CSV: quoted cells, doubled quotes, line breaks inside quotes.
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if cell.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut cell)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.retain(|row| row.iter().any(|cell| !cell.trim().is_empty()));
    rows
}

fn read_csv(text: &str) -> Result<Prepared, Error> {
    let mut rows = csv_rows(text).into_iter();
    let header = rows.next().ok_or_else(|| invalid("the file is empty"))?;
    let column = |name: &str| {
        header
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(name))
    };
    let name_column = column("name")
        .ok_or_else(|| invalid("this is not a Bitwarden CSV export (no name column)"))?;
    let get = |row: &[String], name: &str| {
        column(name)
            .and_then(|index| row.get(index))
            .map(|cell| cell.trim())
            .filter(|cell| !cell.is_empty())
            .map(str::to_string)
    };
    let get_text = |row: &[String], name: &str| get(row, name).map(formula_back);
    let mut items = Vec::new();
    let mut folders: Vec<String> = Vec::new();
    let mut in_folder = Vec::new();
    for row in rows {
        let kind = match get(&row, "type").as_deref() {
            Some("note") => ItemKind::Note,
            _ => ItemKind::Login,
        };
        let mut item = Item::new(kind);
        item.name = Zeroizing::new(
            row.get(name_column)
                .cloned()
                .filter(|n| !n.trim().is_empty())
                .map(formula_back)
                .unwrap_or_else(|| "?".into()),
        );
        item.notes = get_text(&row, "notes").map(Zeroizing::new);
        item.favorite = get(&row, "favorite").as_deref() == Some("1");
        item.reprompt = get(&row, "reprompt").as_deref() == Some("1");
        if let Some(login) = item.login.as_mut() {
            login.username = get(&row, "login_username").map(Zeroizing::new);
            login.password = get(&row, "login_password").map(Zeroizing::new);
            login.totp = get(&row, "login_totp").map(Zeroizing::new);
            login.uris = get_text(&row, "login_uri")
                .map(|uris| {
                    uris.split(',')
                        .map(str::trim)
                        .filter(|uri| !uri.is_empty())
                        .map(|uri| LoginUri {
                            uri: Zeroizing::new(uri.to_string()),
                            match_kind: None,
                            checksum: None,
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        item.fields = get_text(&row, "fields")
            .map(|fields| {
                fields
                    .lines()
                    .filter_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        Some(Field {
                            name: Some(Zeroizing::new(name.trim().to_string())),
                            value: Some(Zeroizing::new(value.trim().to_string()))
                                .filter(|v| !v.is_empty()),
                            kind: FieldKind::Text,
                            linked_id: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(folder) = get_text(&row, "folder") {
            let index = match folders.iter().position(|name| *name == folder) {
                Some(index) => index,
                None => {
                    folders.push(folder);
                    folders.len() - 1
                }
            };
            in_folder.push((items.len(), index));
        }
        items.push(item);
    }
    Ok(Prepared {
        items,
        folders,
        in_folder,
    })
}

// ── Bitwarden's password-protected JSON ───────────────────

/// What an encrypted Bitwarden JSON export is: protected by a password of its
/// own (`passwordProtected`), or tied to the account that made it.
pub fn is_account_bound(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok_and(|value| {
        value.get("encrypted").and_then(Value::as_bool) == Some(true)
            && value.get("passwordProtected").and_then(Value::as_bool) != Some(true)
    })
}

/// Bitwarden's password-protected JSON export, opened with its password: the
/// unencrypted export it holds. The key is derived like Bitwarden's
/// `makePinKey` — the master key's derivation with the file's own salt (as it
/// is, not lower-cased like an email), stretched — and checked against the
/// file's `encKeyValidation_DO_NOT_EDIT` before anything else is decrypted.
/// [`Error::WrongKey`] for a wrong password.
pub fn open_protected_export(text: &str, password: &str) -> Result<Zeroizing<String>, Error> {
    let value: Value =
        serde_json::from_str(text).map_err(|_| invalid("this is not a Bitwarden JSON export"))?;
    if value.get("passwordProtected").and_then(Value::as_bool) != Some(true) {
        return Err(invalid("this export is not protected by a password"));
    }
    let number = |name: &str| value.get(name).and_then(Value::as_u64).map(|n| n as u32);
    let text_of = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(format!("the export has no {name}")))
    };
    let salt = text_of("salt")?;
    let kdf = match number("kdfType") {
        Some(0) => Kdf::Pbkdf2 {
            iterations: number("kdfIterations").ok_or_else(|| invalid("no kdfIterations"))?,
        },
        Some(1) => Kdf::Argon2id {
            iterations: number("kdfIterations").ok_or_else(|| invalid("no kdfIterations"))?,
            memory_mib: number("kdfMemory").ok_or_else(|| invalid("no kdfMemory"))?,
            parallelism: number("kdfParallelism").ok_or_else(|| invalid("no kdfParallelism"))?,
        },
        _ => return Err(Error::Unsupported("this export's key derivation".into())),
    };
    kdf.check()?;
    kdf.check_ceilings()?;
    let mut derived = Zeroizing::new([0u8; 32]);
    match kdf {
        Kdf::Pbkdf2 { iterations } => {
            pbkdf2::pbkdf2_hmac::<Sha256>(
                password.as_bytes(),
                salt.as_bytes(),
                iterations,
                derived.as_mut(),
            );
        }
        Kdf::Argon2id {
            iterations,
            memory_mib,
            parallelism,
        } => {
            let salt = Sha256::digest(salt.as_bytes());
            let params = argon2::Params::new(memory_mib * 1024, iterations, parallelism, Some(32))
                .map_err(|e| Error::Crypto(format!("Argon2 parameters: {e}")))?;
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
                .hash_password_into(password.as_bytes(), &salt, derived.as_mut())
                .map_err(|e| Error::Crypto(format!("Argon2: {e}")))?;
        }
    }
    let key = SymmetricKey::stretch(&derived);
    text_of("encKeyValidation_DO_NOT_EDIT")?
        .parse::<EncString>()?
        .decrypt(&key)
        .map_err(|_| Error::WrongKey)?;
    let plain = text_of("data")?
        .parse::<EncString>()?
        .decrypt_string(&key)?;
    Ok(plain)
}

// ── KeePass ───────────────────────────────────────────────

/// The most memory an Argon2 of a KeePass file may ask for: 1 GiB (the
/// import module checks the same before it asks).
const MAX_KDBX_MEMORY_KIB: u32 = 1024 * 1024;

/// Argon2d (`id` false) or Argon2id over a KeePass file's composite key with
/// the file's parameters; `version` is 0x10 or 0x13. 32 bytes.
pub fn kdbx_argon2(
    id: bool,
    version: u32,
    key: &[u8],
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    if memory_kib > MAX_KDBX_MEMORY_KIB {
        return Err(Error::Unsupported(
            "a KeePass file that asks for more than 1 GiB of memory".into(),
        ));
    }
    let version = match version {
        0x10 => argon2::Version::V0x10,
        0x13 => argon2::Version::V0x13,
        _ => return Err(Error::Unsupported("this Argon2 version".into())),
    };
    let params = argon2::Params::new(memory_kib, iterations, lanes, Some(32))
        .map_err(|e| invalid(format!("Argon2 parameters of the file: {e}")))?;
    let algorithm = if id {
        argon2::Algorithm::Argon2id
    } else {
        argon2::Algorithm::Argon2d
    };
    let mut out = Zeroizing::new(vec![0u8; 32]);
    argon2::Argon2::new(algorithm, version, params)
        .hash_password_into(key, salt, &mut out)
        .map_err(|e| Error::Crypto(format!("Argon2: {e}")))?;
    Ok(out)
}

/// KeePass's AES-KDF: the composite key encrypted `rounds` times with AES-256
/// in ECB mode under `seed`, then its SHA-256.
pub fn kdbx_aes_kdf(key: &[u8], seed: &[u8], rounds: u64) -> Result<Zeroizing<Vec<u8>>, Error> {
    if key.len() != 32 || seed.len() != 32 {
        return Err(invalid("AES-KDF takes a 32-byte key and seed"));
    }
    let cipher = Aes256::new(GenericArray::from_slice(seed));
    let mut blocks = [
        GenericArray::clone_from_slice(&key[..16]),
        GenericArray::clone_from_slice(&key[16..]),
    ];
    for _ in 0..rounds {
        cipher.encrypt_blocks(&mut blocks);
    }
    let mut hash = Sha256::new();
    hash.update(blocks[0]);
    hash.update(blocks[1]);
    Ok(Zeroizing::new(hash.finalize().to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_cells_with_commas_quotes_and_lines() {
        let rows = csv_rows("a,b,c\n\"one, two\",\"say \"\"hi\"\"\",\"line\nbreak\"\r\n");
        assert_eq!(
            rows,
            vec![
                vec!["a", "b", "c"],
                vec!["one, two", "say \"hi\"", "line\nbreak"]
            ]
        );
    }

    #[test]
    fn a_bitwarden_csv_reads() {
        let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                   Arbeit,1,login,Router,,\"pin: 1234\",0,\"https://a.example.com,https://b.example.com\",admin,hunter2,\n\
                   ,,note,'=Notiz,geheim,,0,,,,\n";
        let read = read("csv", csv, "").unwrap();
        assert_eq!(read.items.len(), 2);
        assert_eq!(read.folders, ["Arbeit"]);
        assert_eq!(read.in_folder, [(0, 0)]);
        let router = &read.items[0];
        assert!(router.favorite);
        assert_eq!(router.login.as_ref().unwrap().uris.len(), 2);
        assert_eq!(router.fields[0].value.as_ref().unwrap().as_str(), "1234");
        assert_eq!(read.items[1].kind, ItemKind::Note);
        assert_eq!(
            read.items[1].name.as_str(),
            "=Notiz",
            "the formula guard goes again"
        );
    }

    #[test]
    fn a_bitwarden_json_reads() {
        let json = r#"{"encrypted": false, "folders": [{"id": "f1", "name": "Privat"}], "items": [
            {"type": 1, "name": "Mail", "folderId": "f1", "favorite": true,
             "login": {"username": "nyu", "password": "pw", "uris": [{"uri": "https://mail.example.com", "match": 3}],
                       "fido2Credentials": [{"credentialId": "abc", "rpId": "example.com", "counter": 0, "creationDate": "2026-01-01T00:00:00.000Z"}]},
             "fields": [{"name": "pin", "value": "42", "type": 1}],
             "passwordHistory": [{"password": "old", "lastUsedDate": "2026-01-01T00:00:00.000Z"}]},
            {"type": 3, "name": "Karte", "card": {"number": "4111111111111111", "code": "123"}},
            {"type": 9, "name": "Unknown"}]}"#;
        let read = read("json", json, "2026-09-25T00:00:00.000Z").unwrap();
        assert_eq!(read.items.len(), 2, "an unknown type is left out");
        assert_eq!(read.folders, ["Privat"]);
        assert_eq!(read.in_folder, [(0, 0)]);
        let login = read.items[0].login.as_ref().unwrap();
        assert_eq!(login.uris[0].match_kind, Some(3));
        assert_eq!(login.passkeys.as_ref().unwrap().len(), 1);
        assert_eq!(read.items[0].fields[0].kind, FieldKind::Hidden);
        assert_eq!(read.items[0].password_history[0].password.as_str(), "old");
        assert_eq!(
            read.items[1]
                .card
                .as_ref()
                .unwrap()
                .code
                .as_ref()
                .unwrap()
                .as_str(),
            "123"
        );
        assert!(super::read("json", r#"{"encrypted": true, "items": []}"#, "").is_err());
    }

    #[test]
    fn passkeys_are_sealed_but_their_date() {
        let key = SymmetricKey::generate();
        let mut passkeys = vec![serde_json::json!({
            "credentialId": "abc", "counter": 3, "discoverable": true, "userName": null,
            "creationDate": "2026-01-01T00:00:00.000Z"
        })];
        seal_passkeys(&mut passkeys, &key).unwrap();
        let sealed = passkeys[0].as_object().unwrap();
        assert_eq!(sealed["creationDate"], "2026-01-01T00:00:00.000Z");
        assert_eq!(sealed["userName"], Value::Null);
        let open = |name: &str| {
            sealed[name]
                .as_str()
                .unwrap()
                .parse::<EncString>()
                .unwrap()
                .decrypt_string(&key)
                .unwrap()
                .to_string()
        };
        assert_eq!(open("credentialId"), "abc");
        assert_eq!(open("counter"), "3");
        assert_eq!(open("discoverable"), "true");
        // Sealed already: stays as it is.
        let again = passkeys.clone();
        seal_passkeys(&mut passkeys, &key).unwrap();
        assert_eq!(passkeys, again);
        assert!(seal_passkeys(&mut [Value::from(1)], &key).is_err());
    }

    /// A password-protected export as Bitwarden writes one (PBKDF2 with the
    /// lowest count Bitwarden allows, so the test stays fast).
    fn protected(password: &str, plain: &str) -> String {
        let salt = "c2FsdHNhbHRzYWx0c2FsdA==";
        let mut derived = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 5_000, &mut derived);
        let key = SymmetricKey::stretch(&derived);
        serde_json::json!({
            "encrypted": true,
            "passwordProtected": true,
            "salt": salt,
            "kdfType": 0,
            "kdfIterations": 5000,
            "encKeyValidation_DO_NOT_EDIT": EncString::encrypt(b"a-uuid", &key).to_string(),
            "data": EncString::encrypt(plain.as_bytes(), &key).to_string(),
        })
        .to_string()
    }

    #[test]
    fn a_password_protected_export_opens_with_its_password_only() {
        let plain = r#"{"encrypted":false,"folders":[],"items":[]}"#;
        let file = protected("Kennwort", plain);
        assert!(!is_account_bound(&file));
        assert_eq!(
            open_protected_export(&file, "Kennwort").unwrap().as_str(),
            plain
        );
        assert!(matches!(
            open_protected_export(&file, "kennwort"),
            Err(Error::WrongKey)
        ));
        assert!(is_account_bound(r#"{"encrypted":true,"data":"2.x"}"#));
        assert!(open_protected_export(r#"{"encrypted":true,"data":"2.x"}"#, "x").is_err());
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn kdbx_argon2d_matches_the_reference() {
        // `echo -n password | argon2 somesalt -d -t 2 -m 16 -p 1 -l 32 -r` (version 0x13).
        let out = kdbx_argon2(false, 0x13, b"password", b"somesalt", 1 << 16, 2, 1).unwrap();
        assert_eq!(
            hex(&out),
            "955e5d5b163a1b60bba35fc36d0496474fba4f6b59ad53628666f07fb2f93eaf"
        );
        assert!(kdbx_argon2(true, 0x13, b"k", b"saltsalt", MAX_KDBX_MEMORY_KIB + 1, 1, 1).is_err());
    }

    #[test]
    fn kdbx_aes_kdf_of_no_rounds_is_the_hash() {
        let key = [7u8; 32];
        let expected = Sha256::digest(key);
        assert_eq!(
            kdbx_aes_kdf(&key, &[1u8; 32], 0).unwrap().as_slice(),
            expected.as_slice()
        );
        assert_ne!(
            kdbx_aes_kdf(&key, &[1u8; 32], 1).unwrap().as_slice(),
            expected.as_slice()
        );
        assert!(kdbx_aes_kdf(&key[..16], &[1u8; 32], 1).is_err());
    }
}
