//! Passkeys that sign in: a WebAuthn authenticator, in Bitwarden's format.
//!
//! Bitwarden keeps a passkey with a login (`login.fido2Credentials`): a P-256
//! key pair for ECDSA, and what it was made for. Every value is an
//! [`EncString`] under the item's key (or the user or organisation key, for
//! an item without a key of its own), even the counter and `discoverable`;
//! only `creationDate` is plain. [`crate::vault::Login::passkeys`] holds them
//! as they came, and [`Passkey::open`] reads one of them.
//!
//! A passkey made here is one Bitwarden's apps can use, and the other way
//! round:
//!
//! - The **credential id** is a GUID, like `b2a5d2c1-…`, and the site gets
//!   its 16 bytes. Imported passkeys may have any other id; Bitwarden writes
//!   those as `b64.` and the bytes in URL-safe base64.
//! - The **private key** is PKCS#8 DER, in URL-safe base64 without padding.
//! - The **user handle** is the site's `user.id`, in URL-safe base64 too.
//!
//! What goes to the site is built here as well: the authenticator data, the
//! attestation object (format `none`, like Bitwarden) and the signature. The
//! little CBOR it takes is written by hand.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64_URL;
use base64::Engine as _;
use p256::ecdsa::signature::Signer;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey};
use rand::RngCore;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::crypto::{EncString, SymmetricKey};
use crate::vault::Secret;
use crate::Error;

/// UwULock's AAGUID, `4d0c2e23-4c15-c411-9bd1-f265e4266ad6`: which kind of
/// authenticator made a passkey, in its attested data. Sites can show a name
/// for it, or refuse it; it says nothing about the person. Random bytes, picked
/// once, and never Bitwarden's own.
pub const AAGUID: [u8; 16] = [
    0x4d, 0x0c, 0x2e, 0x23, 0x4c, 0x15, 0xc4, 0x11, 0x9b, 0xd1, 0xf2, 0x65, 0xe4, 0x26, 0x6a, 0xd6,
];

// The flags of the authenticator data.

/// User present: somebody confirmed.
pub const UP: u8 = 0x01;
/// User verified: with a PIN, the master password, a fingerprint.
pub const UV: u8 = 0x04;
/// Backup eligible: the passkey can be synced, as every one in a vault is.
pub const BE: u8 = 0x08;
/// Backed up: it is synced.
pub const BS: u8 = 0x10;
/// Attested credential data follows: only when a passkey is made.
pub const AT: u8 = 0x40;

/// A passkey, opened. Its private key zeroizes itself when dropped.
#[derive(Clone)]
pub struct Passkey {
    /// As Bitwarden stores it: a GUID, or `b64.` and URL-safe base64 for an
    /// imported id. [`Passkey::credential_id_bytes`] is what the site knows.
    pub credential_id: String,
    /// PKCS#8 DER of the P-256 private key, in URL-safe base64 without padding.
    pub key_value: Secret,
    pub rp_id: String,
    pub rp_name: Option<String>,
    /// The site's `user.id`, in URL-safe base64.
    pub user_handle: Option<String>,
    pub user_name: Option<String>,
    pub user_display_name: Option<String>,
    /// How often it signed. Bitwarden's passkeys stay at 0, which tells the
    /// site the authenticator doesn't count.
    pub counter: u32,
    /// Whether the site can find it without being told its id first.
    pub discoverable: bool,
    /// An ISO date, the one value that isn't encrypted.
    pub creation_date: String,
}

impl std::fmt::Debug for Passkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Passkey")
            .field("credential_id", &self.credential_id)
            .field("key_value", &"…")
            .field("rp_id", &self.rp_id)
            .field("rp_name", &self.rp_name)
            .field("user_handle", &self.user_handle)
            .field("user_name", &self.user_name)
            .field("user_display_name", &self.user_display_name)
            .field("counter", &self.counter)
            .field("discoverable", &self.discoverable)
            .field("creation_date", &self.creation_date)
            .finish()
    }
}

/// The value under `name` in Bitwarden's camelCase, or all in lower case as
/// an older UwULock stored it.
fn field<'a>(raw: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    raw.get(name)
        .or_else(|| raw.get(&name.to_lowercase()))
        .filter(|value| !value.is_null())
}

/// Base64 in any of the ways passkeys get written: URL-safe or standard,
/// with or without padding.
fn b64_lenient(text: &str) -> Result<Zeroizing<Vec<u8>>, Error> {
    let url: Zeroizing<String> = Zeroizing::new(
        text.trim()
            .trim_end_matches('=')
            .chars()
            .map(|c| match c {
                '+' => '-',
                '/' => '_',
                c => c,
            })
            .collect(),
    );
    B64_URL
        .decode(url.as_bytes())
        .map(Zeroizing::new)
        .map_err(|_| Error::Crypto("a passkey value isn't valid base64".into()))
}

/// Bytes the way a site reads them: URL-safe base64 without padding.
fn b64_url(bytes: &[u8]) -> String {
    B64_URL.encode(bytes)
}

impl Passkey {
    /// Reads a passkey as it is kept with a login, with the key its values
    /// are under. Only ECDSA on P-256 is refused as [`Error::Unsupported`]
    /// if it says otherwise; a value that doesn't open fails the whole passkey.
    pub fn open(raw: &Value, key: &SymmetricKey) -> Result<Passkey, Error> {
        let Value::Object(raw) = raw else {
            return Err(Error::Crypto("a passkey isn't an object".into()));
        };
        let text = |name: &str| -> Result<Option<Secret>, Error> {
            match field(raw, name) {
                None => Ok(None),
                Some(Value::String(value)) if value.is_empty() => Ok(None),
                Some(Value::String(value)) => value
                    .parse::<EncString>()?
                    .decrypt_string(key)
                    .map(|value| Some(value).filter(|v| !v.is_empty())),
                Some(_) => Err(Error::Crypto(format!("a passkey's {name} isn't text"))),
            }
        };
        let plain = |name: &str| -> Result<Option<String>, Error> {
            Ok(text(name)?.map(|value| value.to_string()))
        };
        let required = |name: &str| -> Result<Secret, Error> {
            text(name)?.ok_or_else(|| Error::Crypto(format!("a passkey without {name}")))
        };

        if let Some(algorithm) = text("keyAlgorithm")? {
            if algorithm.as_str() != "ECDSA" {
                return Err(Error::Unsupported(format!(
                    "passkeys with {} keys",
                    algorithm.as_str()
                )));
            }
        }
        if let Some(curve) = text("keyCurve")? {
            if curve.as_str() != "P-256" {
                return Err(Error::Unsupported(format!(
                    "passkeys on the curve {}",
                    curve.as_str()
                )));
            }
        }
        let counter = match text("counter")? {
            None => 0,
            Some(counter) => counter
                .trim()
                .parse()
                .map_err(|_| Error::Crypto("a passkey's counter isn't a number".into()))?,
        };
        let discoverable = text("discoverable")?.is_some_and(|d| d.trim() == "true");
        let creation_date = match field(raw, "creationDate") {
            Some(Value::String(date)) => date.clone(),
            _ => String::new(),
        };

        Ok(Passkey {
            credential_id: required("credentialId")?.to_string(),
            key_value: required("keyValue")?,
            rp_id: required("rpId")?.to_string(),
            rp_name: plain("rpName")?,
            user_handle: plain("userHandle")?,
            user_name: plain("userName")?,
            user_display_name: plain("userDisplayName")?,
            counter,
            discoverable,
            creation_date,
        })
    }

    /// The passkey as Bitwarden keeps it, in its camelCase: every value
    /// encrypted under `key` but the creation date. Absent values are `null`.
    pub fn seal(&self, key: &SymmetricKey) -> Value {
        let seal =
            |value: &str| Value::String(EncString::encrypt(value.as_bytes(), key).to_string());
        let optional = |value: &Option<String>| value.as_deref().map_or(Value::Null, seal);
        let mut out = Map::new();
        out.insert("credentialId".into(), seal(&self.credential_id));
        out.insert("keyType".into(), seal("public-key"));
        out.insert("keyAlgorithm".into(), seal("ECDSA"));
        out.insert("keyCurve".into(), seal("P-256"));
        out.insert("keyValue".into(), seal(&self.key_value));
        out.insert("rpId".into(), seal(&self.rp_id));
        out.insert("userHandle".into(), optional(&self.user_handle));
        out.insert("userName".into(), optional(&self.user_name));
        out.insert("counter".into(), seal(&self.counter.to_string()));
        out.insert("rpName".into(), optional(&self.rp_name));
        out.insert("userDisplayName".into(), optional(&self.user_display_name));
        out.insert(
            "discoverable".into(),
            seal(if self.discoverable { "true" } else { "false" }),
        );
        out.insert(
            "creationDate".into(),
            Value::String(self.creation_date.clone()),
        );
        Value::Object(out)
    }

    /// Sealed again over what was stored before, for a new counter: whatever
    /// else a newer client keeps with the passkey stays, and a key an older
    /// UwULock lowered is replaced by its camelCase one.
    pub fn reseal(&self, previous: &Value, key: &SymmetricKey) -> Value {
        let Value::Object(sealed) = self.seal(key) else {
            unreachable!("a sealed passkey is an object")
        };
        let mut out = match previous {
            Value::Object(previous) => previous.clone(),
            _ => Map::new(),
        };
        for (name, value) in sealed {
            let lowered = name.to_lowercase();
            if lowered != name {
                out.remove(&lowered);
            }
            out.insert(name, value);
        }
        Value::Object(out)
    }

    /// A new passkey for a site, with a fresh P-256 key and a random GUID as
    /// its id, counting from 0 like Bitwarden's. `user_handle` is the site's
    /// `user.id`; `now` the ISO date it is made on.
    pub fn generate(
        rp_id: &str,
        rp_name: Option<&str>,
        user_handle: Option<&[u8]>,
        user_name: Option<&str>,
        user_display_name: Option<&str>,
        discoverable: bool,
        now: &str,
    ) -> Result<Passkey, Error> {
        let secret = p256::SecretKey::random(&mut rand::rngs::OsRng);
        let der = secret
            .to_pkcs8_der()
            .map_err(|e| Error::Crypto(format!("P-256: {e}")))?;
        let mut guid = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut guid);
        // A version 4 GUID, like the ones Bitwarden's apps make.
        guid[6] = (guid[6] & 0x0f) | 0x40;
        guid[8] = (guid[8] & 0x3f) | 0x80;
        let owned = |value: Option<&str>| value.map(str::to_string).filter(|v| !v.is_empty());
        Ok(Passkey {
            credential_id: credential_id_from_bytes(&guid),
            key_value: Zeroizing::new(b64_url(der.as_bytes())),
            rp_id: rp_id.to_string(),
            rp_name: owned(rp_name),
            user_handle: user_handle.map(b64_url),
            user_name: owned(user_name),
            user_display_name: owned(user_display_name),
            counter: 0,
            discoverable,
            creation_date: now.to_string(),
        })
    }

    /// The credential id as the site knows it: a GUID's 16 bytes in the
    /// order they are written (Bitwarden's `guidToRawFormat`), or the bytes
    /// after `b64.`.
    pub fn credential_id_bytes(&self) -> Result<Vec<u8>, Error> {
        credential_id_bytes(&self.credential_id)
    }

    /// The user handle's bytes, the site's `user.id`.
    pub fn user_handle_bytes(&self) -> Result<Option<Vec<u8>>, Error> {
        self.user_handle
            .as_deref()
            .map(|handle| b64_lenient(handle).map(|bytes| bytes.to_vec()))
            .transpose()
    }

    fn secret_key(&self) -> Result<p256::SecretKey, Error> {
        let der = b64_lenient(&self.key_value)?;
        p256::SecretKey::from_pkcs8_der(&der)
            .map_err(|_| Error::Crypto("a passkey's private key doesn't parse".into()))
    }

    /// The public key as SubjectPublicKeyInfo DER, what `getPublicKey()`
    /// hands a site.
    pub fn public_key_spki(&self) -> Result<Vec<u8>, Error> {
        self.secret_key()?
            .public_key()
            .to_public_key_der()
            .map(|der| der.into_vec())
            .map_err(|e| Error::Crypto(format!("P-256: {e}")))
    }

    /// The public key as a COSE key, for the attested data: EC2 on P-256,
    /// for ES256.
    pub fn public_key_cose(&self) -> Result<Vec<u8>, Error> {
        let point = self.secret_key()?.public_key().to_encoded_point(false);
        let (Some(x), Some(y)) = (point.x(), point.y()) else {
            return Err(Error::Crypto(
                "a passkey's public key is the identity".into(),
            ));
        };
        // {1: 2 (EC2), 3: -7 (ES256), -1: 1 (P-256), -2: x, -3: y}, the keys
        // in CTAP2's canonical order.
        let mut out = vec![0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21];
        cbor_bytes(&mut out, x);
        out.push(0x22);
        cbor_bytes(&mut out, y);
        Ok(out)
    }

    /// The authenticator data a site checks: the SHA-256 of the RP id, the
    /// `flags`, the counter, and when `attested` (a passkey being made), the
    /// [`AAGUID`], the credential id and the public key.
    pub fn authenticator_data(&self, flags: u8, attested: bool) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity(37 + if attested { 18 + 16 + 77 } else { 0 });
        out.extend_from_slice(&rp_id_hash(&self.rp_id));
        out.push(if attested { flags | AT } else { flags & !AT });
        out.extend_from_slice(&self.counter.to_be_bytes());
        if attested {
            let id = self.credential_id_bytes()?;
            let length = u16::try_from(id.len())
                .map_err(|_| Error::Crypto("a credential id this long doesn't fit".into()))?;
            out.extend_from_slice(&AAGUID);
            out.extend_from_slice(&length.to_be_bytes());
            out.extend_from_slice(&id);
            out.extend_from_slice(&self.public_key_cose()?);
        }
        Ok(out)
    }

    /// The signature a site checks: ECDSA with SHA-256 over the authenticator
    /// data and the hash of the client data, DER-encoded.
    pub fn sign(&self, auth_data: &[u8], client_data_hash: &[u8]) -> Result<Vec<u8>, Error> {
        let signing = p256::ecdsa::SigningKey::from(&self.secret_key()?);
        let mut message = Vec::with_capacity(auth_data.len() + client_data_hash.len());
        message.extend_from_slice(auth_data);
        message.extend_from_slice(client_data_hash);
        let signature: p256::ecdsa::Signature = signing.sign(&message);
        Ok(signature.to_der().as_bytes().to_vec())
    }
}

/// The bytes of a credential id as Bitwarden stores it; see
/// [`Passkey::credential_id_bytes`]. Anything that is neither a GUID nor
/// `b64.` is refused.
pub fn credential_id_bytes(id: &str) -> Result<Vec<u8>, Error> {
    if let Some(encoded) = id.strip_prefix("b64.") {
        return b64_lenient(encoded).map(|bytes| bytes.to_vec());
    }
    let groups: Vec<&str> = id.split('-').collect();
    let shaped = groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
        && groups
            .iter()
            .all(|g| g.bytes().all(|b| b.is_ascii_hexdigit()));
    if !shaped {
        return Err(Error::Crypto(
            "a passkey's credential id is neither a GUID nor b64.".into(),
        ));
    }
    let hex: String = groups.concat();
    Ok((0..16)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("checked as hex"))
        .collect())
}

/// How Bitwarden stores the id a site knows: 16 bytes as a GUID in lower
/// case, anything else as `b64.` and URL-safe base64.
pub fn credential_id_from_bytes(bytes: &[u8]) -> String {
    if bytes.len() != 16 {
        return format!("b64.{}", b64_url(bytes));
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// SHA-256 of the RP id, the start of every authenticator data.
pub fn rp_id_hash(rp_id: &str) -> [u8; 32] {
    Sha256::digest(rp_id.as_bytes()).into()
}

/// The attestation object for a new passkey: `{"fmt": "none", "attStmt": {},
/// "authData": …}`, like Bitwarden's. The keys in CTAP2's canonical order:
/// shorter first.
pub fn attestation_object(auth_data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(auth_data.len() + 32);
    out.push(0xa3);
    cbor_text(&mut out, "fmt");
    cbor_text(&mut out, "none");
    cbor_text(&mut out, "attStmt");
    out.push(0xa0);
    cbor_text(&mut out, "authData");
    cbor_bytes(&mut out, auth_data);
    out
}

/// A CBOR head: the major type and a length.
fn cbor_head(out: &mut Vec<u8>, major: u8, length: usize) {
    let major = major << 5;
    match length {
        0..=23 => out.push(major | length as u8),
        24..=0xff => out.extend_from_slice(&[major | 24, length as u8]),
        0x100..=0xffff => {
            out.push(major | 25);
            out.extend_from_slice(&(length as u16).to_be_bytes());
        }
        _ => {
            out.push(major | 26);
            out.extend_from_slice(&(length as u32).to_be_bytes());
        }
    }
}

fn cbor_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    cbor_head(out, 2, bytes.len());
    out.extend_from_slice(bytes);
}

fn cbor_text(out: &mut Vec<u8>, text: &str) {
    cbor_head(out, 3, text.len());
    out.extend_from_slice(text.as_bytes());
}

/// What a person sees of a passkey in an item's details: no key, nothing
/// secret. [`list`] makes them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyInfo {
    /// Its place among the login's passkeys: what [`remove`] takes.
    pub index: usize,
    /// `false` for one UwULock can't open (another algorithm, a value that
    /// doesn't decrypt): it can still be deleted, the rest is empty.
    pub readable: bool,
    /// As stored ([`Passkey::credential_id`]); empty when not readable.
    pub credential_id: String,
    /// Names this stored passkey for [`remove`], readable or not: hex of the
    /// first 16 bytes of SHA-256 over its stored (encrypted) JSON
    /// ([`fingerprint`]). Nothing secret.
    pub fingerprint: String,
    pub rp_id: String,
    pub rp_name: Option<String>,
    pub user_name: Option<String>,
    pub user_display_name: Option<String>,
    /// An ISO date, or empty when the passkey has none.
    pub creation_date: String,
    pub discoverable: bool,
}

/// The passkeys of an item, for its details. `key` is the item's own key, or
/// the key it is under ([`crate::vault::Vault::item_key`]).
pub fn list(item: &crate::vault::Item, key: &SymmetricKey) -> Vec<PasskeyInfo> {
    let Some(raw) = item.login.as_ref().and_then(|l| l.passkeys.as_ref()) else {
        return Vec::new();
    };
    raw.iter()
        .enumerate()
        .map(|(index, raw)| match Passkey::open(raw, key) {
            Ok(passkey) => PasskeyInfo {
                index,
                readable: true,
                fingerprint: fingerprint(raw),
                credential_id: passkey.credential_id.clone(),
                rp_id: passkey.rp_id.clone(),
                rp_name: passkey.rp_name.clone(),
                user_name: passkey.user_name.clone(),
                user_display_name: passkey.user_display_name.clone(),
                creation_date: passkey.creation_date.clone(),
                discoverable: passkey.discoverable,
            },
            Err(_) => PasskeyInfo {
                index,
                readable: false,
                fingerprint: fingerprint(raw),
                credential_id: String::new(),
                rp_id: String::new(),
                rp_name: None,
                user_name: None,
                user_display_name: None,
                creation_date: match field_of(raw, "creationDate") {
                    Some(Value::String(date)) => date.clone(),
                    _ => String::new(),
                },
                discoverable: false,
            },
        })
        .collect()
}

fn field_of<'a>(raw: &'a Value, name: &str) -> Option<&'a Value> {
    match raw {
        Value::Object(map) => field(map, name),
        _ => None,
    }
}

/// What [`PasskeyInfo::fingerprint`] holds for a stored passkey.
pub fn fingerprint(raw: &Value) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(serde_json::to_vec(raw).unwrap_or_default());
    hash[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// Deletes the passkey at `index` from an item (then seal and save the
/// item), but only if the passkey there still is the one `which` names —
/// its credential id, or its [`fingerprint`] (the only name an unreadable
/// one has) — so a sync or an earlier delete in between doesn't delete
/// another: otherwise [`Error::Conflict`]. Without `which` nothing is
/// deleted ([`Error::Refused`]). A login without passkeys left has
/// `passkeys: None`.
pub fn remove(
    item: &mut crate::vault::Item,
    key: &SymmetricKey,
    index: usize,
    which: Option<&str>,
) -> Result<(), Error> {
    let passkeys = item
        .login
        .as_mut()
        .and_then(|l| l.passkeys.as_mut())
        .filter(|p| index < p.len())
        .ok_or_else(|| Error::Refused("this item has no such passkey".into()))?;
    let wanted = which
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::Refused("say which passkey to delete".into()))?;
    let raw = &passkeys[index];
    let same = fingerprint(raw) == wanted
        || Passkey::open(raw, key).is_ok_and(|there| there.credential_id == wanted);
    if !same {
        return Err(Error::Conflict);
    }
    passkeys.remove(index);
    if passkeys.is_empty() {
        if let Some(login) = item.login.as_mut() {
            login.passkeys = None;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as B64;
    use p256::ecdsa::signature::Verifier;
    use p256::pkcs8::DecodePublicKey;

    fn passkey() -> Passkey {
        Passkey::generate(
            "example.com",
            Some("Example"),
            Some(b"user-1234"),
            Some("nyu@example.com"),
            Some("Nyu"),
            true,
            "2026-09-28T12:00:00.000Z",
        )
        .unwrap()
    }

    #[test]
    fn a_new_passkey_signs_and_verifies() {
        let passkey = passkey();
        assert_eq!(passkey.counter, 0);
        assert_eq!(passkey.user_handle.as_deref(), Some("dXNlci0xMjM0"));
        let id = passkey.credential_id_bytes().unwrap();
        assert_eq!(id.len(), 16);
        assert_eq!(id[6] >> 4, 4, "a version 4 GUID");
        assert_eq!(credential_id_from_bytes(&id), passkey.credential_id);
        assert!(!passkey.key_value.contains(['=', '+', '/']));

        let auth_data = passkey.authenticator_data(UP | UV, false).unwrap();
        let client_data_hash = Sha256::digest(b"{\"type\":\"webauthn.get\"}");
        let signature = passkey.sign(&auth_data, &client_data_hash).unwrap();

        let spki = passkey.public_key_spki().unwrap();
        let public = p256::PublicKey::from_public_key_der(&spki).unwrap();
        let verifying = p256::ecdsa::VerifyingKey::from(&public);
        let signature = p256::ecdsa::Signature::from_der(&signature).unwrap();
        let mut message = auth_data.clone();
        message.extend_from_slice(&client_data_hash);
        assert!(verifying.verify(&message, &signature).is_ok());
        message[0] ^= 1;
        assert!(verifying.verify(&message, &signature).is_err());
    }

    #[test]
    fn seal_and_open() {
        let key = SymmetricKey::generate();
        let passkey = passkey();
        let sealed = passkey.seal(&key);
        assert_eq!(sealed["creationDate"], "2026-09-28T12:00:00.000Z");
        for name in [
            "credentialId",
            "keyValue",
            "counter",
            "discoverable",
            "keyCurve",
        ] {
            assert!(sealed[name].as_str().unwrap().starts_with("2."), "{name}");
        }
        let opened = Passkey::open(&sealed, &key).unwrap();
        assert_eq!(opened.credential_id, passkey.credential_id);
        assert_eq!(opened.key_value, passkey.key_value);
        assert_eq!(opened.rp_id, "example.com");
        assert_eq!(opened.rp_name.as_deref(), Some("Example"));
        assert_eq!(opened.user_handle, passkey.user_handle);
        assert_eq!(opened.user_name.as_deref(), Some("nyu@example.com"));
        assert_eq!(opened.user_display_name.as_deref(), Some("Nyu"));
        assert!(opened.discoverable);
        assert_eq!(opened.creation_date, passkey.creation_date);

        assert!(matches!(
            Passkey::open(&sealed, &SymmetricKey::generate()),
            Err(Error::WrongKey)
        ));
    }

    #[test]
    fn debug_leaves_the_private_key_out() {
        let passkey = passkey();
        let shown = format!("{passkey:?}");
        assert!(!shown.contains(passkey.key_value.as_str()));
        assert!(shown.contains("example.com"));
    }

    #[test]
    fn resealing_keeps_what_it_doesnt_know() {
        let key = SymmetricKey::generate();
        let mut passkey = passkey();
        let mut stored = passkey.seal(&key);
        // As an older UwULock stored it: every key lowered, plus something newer.
        let Value::Object(map) = &stored else {
            unreachable!()
        };
        stored = Value::Object(
            map.iter()
                .map(|(k, v)| (k.to_lowercase(), v.clone()))
                .chain([("somethingNewer".into(), Value::Bool(true))])
                .collect(),
        );
        passkey.counter = 8;
        let resealed = passkey.reseal(&stored, &key);
        assert_eq!(resealed["somethingNewer"], true);
        assert!(resealed.get("credentialid").is_none() && resealed.get("counter").is_some());
        assert_eq!(Passkey::open(&resealed, &key).unwrap().counter, 8);
    }

    #[test]
    fn a_passkey_as_bitwarden_writes_it() {
        let key = SymmetricKey::generate();
        let enc = |text: &str| EncString::encrypt(text.as_bytes(), &key).to_string();
        let der = p256::SecretKey::from_slice(&[7u8; 32])
            .unwrap()
            .to_pkcs8_der()
            .unwrap();
        let camel = serde_json::json!({
            "credentialId": enc("b2a5d2c1-5e3f-4a7b-9c1d-0e2f4a6b8c9d"),
            "keyType": enc("public-key"),
            "keyAlgorithm": enc("ECDSA"),
            "keyCurve": enc("P-256"),
            "keyValue": enc(&b64_url(der.as_bytes())),
            "rpId": enc("example.com"),
            "userHandle": enc("dXNlci0xMjM0"),
            "userName": enc("nyu@example.com"),
            "counter": enc("5"),
            "rpName": enc("Example"),
            "userDisplayName": null,
            "discoverable": enc("false"),
            "creationDate": "2026-01-01T00:00:00.000Z",
        });
        let Value::Object(map) = &camel else {
            unreachable!()
        };
        let lower = Value::Object(
            map.iter()
                .map(|(k, v)| (k.to_lowercase(), v.clone()))
                .collect(),
        );
        for raw in [camel, lower] {
            let passkey = Passkey::open(&raw, &key).unwrap();
            assert_eq!(
                passkey.credential_id_bytes().unwrap(),
                [
                    0xb2, 0xa5, 0xd2, 0xc1, 0x5e, 0x3f, 0x4a, 0x7b, 0x9c, 0x1d, 0x0e, 0x2f, 0x4a,
                    0x6b, 0x8c, 0x9d
                ]
            );
            assert_eq!(passkey.counter, 5);
            assert!(!passkey.discoverable);
            assert_eq!(passkey.user_display_name, None);
            assert_eq!(passkey.user_handle_bytes().unwrap().unwrap(), b"user-1234");
            assert_eq!(passkey.creation_date, "2026-01-01T00:00:00.000Z");
            assert!(passkey.public_key_spki().is_ok());
        }

        // Only what is needed to sign: no algorithm, no counter, no names.
        let bare = serde_json::json!({
            "credentialId": enc("b64.AQID"),
            "keyValue": enc(&B64.encode(der.as_bytes())),
            "rpId": enc("example.com"),
        });
        let passkey = Passkey::open(&bare, &key).unwrap();
        assert_eq!(passkey.credential_id_bytes().unwrap(), [1, 2, 3]);
        assert_eq!(passkey.counter, 0);
        assert!(!passkey.discoverable && passkey.creation_date.is_empty());
        // Standard base64 with padding works as well as Bitwarden's URL-safe.
        assert!(passkey.sign(b"data", &[0; 32]).is_ok());

        for (name, value) in [("keyAlgorithm", "RSA"), ("keyCurve", "P-384")] {
            let mut other = bare.clone();
            other[name] = enc(value).into();
            assert!(matches!(
                Passkey::open(&other, &key),
                Err(Error::Unsupported(_))
            ));
        }
        let mut no_rp = bare.clone();
        no_rp.as_object_mut().unwrap().remove("rpId");
        assert!(Passkey::open(&no_rp, &key).is_err());
    }

    #[test]
    fn credential_ids_both_ways() {
        let guid = "B2A5D2C1-5E3F-4A7B-9C1D-0E2F4A6B8C9D";
        let bytes = credential_id_bytes(guid).unwrap();
        assert_eq!(credential_id_from_bytes(&bytes), guid.to_lowercase());

        let imported = credential_id_from_bytes(&[0xfb; 20]);
        assert_eq!(imported, "b64.-_v7-_v7-_v7-_v7-_v7-_v7-_s");
        assert_eq!(credential_id_bytes(&imported).unwrap(), [0xfb; 20]);
        // Padded standard base64 after `b64.` is read too.
        assert_eq!(
            credential_id_bytes("b64.+/v7+/v7+/v7+/v7+/v7+/v7+/s=").unwrap(),
            [0xfb; 20]
        );

        for wrong in [
            "",
            "not-a-guid",
            "b2a5d2c15e3f4a7b9c1d0e2f4a6b8c9d",
            "g2a5d2c1-5e3f-4a7b-9c1d-0e2f4a6b8c9d",
        ] {
            assert!(credential_id_bytes(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn authenticator_data_layout() {
        let passkey = passkey();
        let plain = passkey.authenticator_data(UP | BE | BS, false).unwrap();
        assert_eq!(plain.len(), 37);
        assert_eq!(plain[..32], rp_id_hash("example.com"));
        assert_eq!(plain[32], UP | BE | BS);
        assert_eq!(plain[33..37], [0, 0, 0, 0]);

        let attested = passkey.authenticator_data(UP | UV, true).unwrap();
        assert_eq!(attested[32], UP | UV | AT);
        assert_eq!(attested[37..53], AAGUID);
        assert_eq!(attested[53..55], [0, 16]);
        assert_eq!(attested[55..71], passkey.credential_id_bytes().unwrap());
        assert_eq!(attested[71..], passkey.public_key_cose().unwrap());
        assert_eq!(attested.len(), 71 + 77);
    }

    #[test]
    fn the_attestation_object() {
        let object = attestation_object(&[0xaa; 300]);
        let mut start = vec![0xa3, 0x63];
        start.extend_from_slice(b"fmt");
        start.push(0x64);
        start.extend_from_slice(b"none");
        start.push(0x67);
        start.extend_from_slice(b"attStmt");
        start.push(0xa0);
        start.push(0x68);
        start.extend_from_slice(b"authData");
        // 300 bytes need a two-byte length.
        start.extend_from_slice(&[0x59, 0x01, 0x2c]);
        assert_eq!(object[..start.len()], start);
        assert_eq!(object.len(), start.len() + 300);

        let short = attestation_object(&[0xaa; 37]);
        assert_eq!(short[short.len() - 39..short.len() - 37], [0x58, 37]);
    }

    #[test]
    fn list_and_remove_passkeys() {
        use crate::vault::{Item, ItemKind};
        let key = SymmetricKey::generate();
        let first = passkey();
        let second = Passkey::generate(
            "login.example.com",
            None,
            None,
            Some("other"),
            None,
            false,
            "2026-09-29T12:00:00.000Z",
        )
        .unwrap();
        let mut item = Item::new(ItemKind::Login);
        let other_key = SymmetricKey::generate();
        item.login.as_mut().unwrap().passkeys = Some(vec![
            first.seal(&key),
            second.seal(&key),
            // Under another key: not readable here, still listed.
            first.seal(&other_key),
        ]);
        let shown = list(&item, &key);
        assert_eq!(shown.len(), 3);
        assert_eq!(shown[0].rp_id, "example.com");
        assert_eq!(shown[0].rp_name.as_deref(), Some("Example"));
        assert_eq!(shown[0].user_name.as_deref(), Some("nyu@example.com"));
        assert_eq!(shown[0].creation_date, "2026-09-28T12:00:00.000Z");
        assert!(shown[0].discoverable && shown[0].readable);
        assert_eq!(shown[1].credential_id, second.credential_id);
        assert!(!shown[2].readable && shown[2].credential_id.is_empty());
        assert_eq!(shown[2].creation_date, "2026-09-28T12:00:00.000Z");
        let json = serde_json::to_value(&shown[0]).unwrap();
        assert_eq!(json["rpId"], "example.com");
        assert!(json.get("keyValue").is_none());

        // The wrong id for that place: nothing is deleted.
        assert!(matches!(
            remove(&mut item, &key, 0, Some(&second.credential_id)),
            Err(Error::Conflict)
        ));
        assert!(remove(&mut item, &key, 9, Some("x")).is_err());
        // Without saying which: refused, even for an unreadable one.
        assert!(matches!(
            remove(&mut item, &key, 2, None),
            Err(Error::Refused(_))
        ));
        // The broken one by its fingerprint; a stale index after a delete
        // hits nothing else.
        let broken = shown[2].fingerprint.clone();
        assert_eq!(broken.len(), 32);
        assert!(shown.iter().filter(|p| p.fingerprint == broken).count() == 1);
        remove(&mut item, &key, 0, Some(&first.credential_id)).unwrap();
        assert!(matches!(
            remove(&mut item, &key, 2, Some(&broken)),
            Err(Error::Refused(_))
        ));
        assert!(matches!(
            remove(&mut item, &key, 0, Some(&broken)),
            Err(Error::Conflict)
        ));
        remove(&mut item, &key, 1, Some(&broken)).unwrap();
        let left = list(&item, &key);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].credential_id, second.credential_id);
        remove(&mut item, &key, 0, Some(&left[0].fingerprint)).unwrap();
        assert!(item.login.as_ref().unwrap().passkeys.is_none());
        assert!(list(&item, &key).is_empty());
    }
}
