//! Bitwarden's crypto, as the official clients do it.
//!
//! - The **master key** comes from the master password and the account's email
//!   with PBKDF2-SHA256 or Argon2id, as the server's prelogin says.
//! - The **master password hash** is one more PBKDF2 round over the master key,
//!   salted with the password. It is the only thing that goes to the server.
//! - The master key is **stretched** with HKDF into an encryption and a MAC
//!   key, which open the account's **user key** (64 bytes: 32 to encrypt,
//!   32 to authenticate).
//! - Every field of every item is an [`EncString`]: AES-256-CBC with an
//!   HMAC-SHA256 over IV and ciphertext, checked before anything is decrypted.
//! - Organisation keys come RSA-OAEP-wrapped with the account's public key; the
//!   private key is itself an EncString under the user key. The same wrapping
//!   hands the user key to someone else's key pair: an emergency contact, a
//!   device asking to log in, a passkey that unlocks ([`PrfKeySet`]).
//! - Attachments and file Sends are the same type 2, in binary
//!   ([`encrypt_file`]). A Send's key comes from a 16-byte seed
//!   ([`send_key`]), the one its link carries.
//! - A [`fingerprint`] phrase lets two people check they see the same public
//!   key.
//!
//! Keys zeroize themselves when dropped.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use rand::RngCore;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::Error;

type HmacSha256 = Hmac<Sha256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

/// How the server wants the master key derived. From the prelogin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Kdf {
    Pbkdf2 {
        iterations: u32,
    },
    Argon2id {
        iterations: u32,
        /// In MiB, as Bitwarden counts it.
        memory_mib: u32,
        parallelism: u32,
    },
}

impl Kdf {
    /// Bitwarden's own floors. A server asking for less is either very old or
    /// trying to make the master password cheap to guess from the hash.
    pub fn check(&self) -> Result<(), Error> {
        match *self {
            Kdf::Pbkdf2 { iterations } if iterations < 5_000 => Err(Error::Unsupported(format!(
                "PBKDF2 with only {iterations} iterations"
            ))),
            Kdf::Argon2id {
                iterations,
                memory_mib,
                parallelism,
            } if iterations < 2 || memory_mib < 15 || parallelism < 1 => {
                Err(Error::Unsupported(format!(
                    "Argon2id with {iterations} iterations, {memory_mib} MiB, {parallelism} lanes"
                )))
            }
            Kdf::Argon2id { memory_mib, .. } if memory_mib > 1024 => Err(Error::Unsupported(
                format!("Argon2id with {memory_mib} MiB of memory"),
            )),
            _ => Ok(()),
        }
    }

    /// Ceilings for what a server may ask for, so a prelogin can't keep the
    /// app deriving for hours. Bitwarden's own server allows up to 2 000 000
    /// PBKDF2 rounds, 10 Argon2 passes and 16 lanes; Vaultwarden caps only
    /// the lanes. The rounds and passes get room above Bitwarden's maximum,
    /// so an account set up by hand on a Vaultwarden still logs in.
    ///
    /// Only for what comes from the server: a KDF this device already has
    /// stored was accepted before and stays usable.
    pub fn check_ceilings(&self) -> Result<(), Error> {
        match *self {
            Kdf::Pbkdf2 { iterations } if iterations > MAX_PBKDF2_ITERATIONS => Err(
                Error::Unsupported(format!("PBKDF2 with {iterations} iterations")),
            ),
            Kdf::Argon2id {
                iterations,
                parallelism,
                ..
            } if iterations > MAX_ARGON2_ITERATIONS || parallelism > MAX_ARGON2_PARALLELISM => {
                Err(Error::Unsupported(format!(
                    "Argon2id with {iterations} iterations and {parallelism} lanes"
                )))
            }
            _ => Ok(()),
        }
    }

    /// Whether a master key derived like this is cheaper to guess than one
    /// derived like `other`: fewer PBKDF2 rounds, fewer Argon2 passes or less
    /// memory, or PBKDF2 where it was Argon2id. Fewer Argon2 lanes aren't
    /// cheaper for whoever guesses, and PBKDF2 to Argon2id is the upgrade
    /// Bitwarden recommends.
    pub fn is_weaker_than(&self, other: &Kdf) -> bool {
        match (*self, *other) {
            (Kdf::Pbkdf2 { iterations }, Kdf::Pbkdf2 { iterations: before }) => iterations < before,
            (
                Kdf::Argon2id {
                    iterations,
                    memory_mib,
                    ..
                },
                Kdf::Argon2id {
                    iterations: before,
                    memory_mib: memory_before,
                    ..
                },
            ) => iterations < before || memory_mib < memory_before,
            (Kdf::Pbkdf2 { .. }, Kdf::Argon2id { .. }) => true,
            (Kdf::Argon2id { .. }, Kdf::Pbkdf2 { .. }) => false,
        }
    }
}

const MAX_PBKDF2_ITERATIONS: u32 = 10_000_000;
const MAX_ARGON2_ITERATIONS: u32 = 20;
const MAX_ARGON2_PARALLELISM: u32 = 16;

/// Bitwarden salts with the email as typed at registration, trimmed and in
/// lower case.
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// The 32-byte master key. Never leaves this process.
pub fn master_key(password: &str, email: &str, kdf: Kdf) -> Result<Zeroizing<[u8; 32]>, Error> {
    kdf.check()?;
    let email = normalize_email(email);
    let mut key = Zeroizing::new([0u8; 32]);
    match kdf {
        Kdf::Pbkdf2 { iterations } => {
            pbkdf2::pbkdf2_hmac::<Sha256>(
                password.as_bytes(),
                email.as_bytes(),
                iterations,
                key.as_mut(),
            );
        }
        Kdf::Argon2id {
            iterations,
            memory_mib,
            parallelism,
        } => {
            // Argon2's salt is the SHA-256 of the email, so short emails
            // still make a salt of the length Argon2 wants.
            let salt = Sha256::digest(email.as_bytes());
            let params = argon2::Params::new(memory_mib * 1024, iterations, parallelism, Some(32))
                .map_err(|e| Error::Crypto(format!("Argon2 parameters: {e}")))?;
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
                .hash_password_into(password.as_bytes(), &salt, key.as_mut())
                .map_err(|e| Error::Crypto(format!("Argon2: {e}")))?;
        }
    }
    Ok(key)
}

/// What the server gets instead of the password: base64 of one PBKDF2 round
/// over the master key, salted with the password.
pub fn master_password_hash(master_key: &[u8; 32], password: &str) -> String {
    let mut hash = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(master_key, password.as_bytes(), 1, hash.as_mut());
    B64.encode(hash.as_ref())
}

/// Opens the account's user key with the master key. Current accounts wrap it
/// under the stretched master key (type 2); accounts from before 2019 under
/// the bare master key without a MAC (type 0), which Bitwarden still accepts.
/// A wrong master password shows up here, as [`Error::WrongKey`].
pub fn decrypt_user_key(
    master_key: &[u8; 32],
    protected: &EncString,
) -> Result<SymmetricKey, Error> {
    match protected {
        EncString::AesCbc256HmacSha256 { .. } => {
            protected.decrypt_key(&SymmetricKey::stretch(master_key))
        }
        // Without a MAC, a wrong key surfaces as bad padding or a key of the
        // wrong length.
        EncString::AesCbc256 { iv, data } => aes_decrypt(master_key, iv, data)
            .and_then(|bytes| SymmetricKey::from_bytes(&bytes))
            .map_err(|_| Error::WrongKey),
        _ => Err(Error::Crypto(
            "the user key is wrapped in an unknown way".into(),
        )),
    }
}

/// A symmetric key: AES-256 for the content, HMAC-SHA256 for its integrity.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SymmetricKey {
    enc: [u8; 32],
    mac: [u8; 32],
}

impl std::fmt::Debug for SymmetricKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SymmetricKey(…)")
    }
}

impl SymmetricKey {
    /// A user, organisation or item key: 32 bytes to encrypt, 32 to authenticate.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != 64 {
            return Err(Error::Crypto(format!(
                "a key has 64 bytes, this one {}",
                bytes.len()
            )));
        }
        let mut key = SymmetricKey {
            enc: [0; 32],
            mac: [0; 32],
        };
        key.enc.copy_from_slice(&bytes[..32]);
        key.mac.copy_from_slice(&bytes[32..]);
        Ok(key)
    }

    /// The master key, stretched with HKDF-Expand into "enc" and "mac".
    pub fn stretch(master_key: &[u8; 32]) -> Self {
        let hkdf = hkdf::Hkdf::<Sha256>::from_prk(master_key).expect("32 bytes is a valid PRK");
        let mut key = SymmetricKey {
            enc: [0; 32],
            mac: [0; 32],
        };
        hkdf.expand(b"enc", &mut key.enc)
            .expect("32 bytes is a valid length");
        hkdf.expand(b"mac", &mut key.mac)
            .expect("32 bytes is a valid length");
        key
    }

    /// A fresh random key, for tests and for sealing local data.
    pub fn generate() -> Self {
        let mut bytes = Zeroizing::new([0u8; 64]);
        rand::rngs::OsRng.fill_bytes(bytes.as_mut());
        Self::from_bytes(bytes.as_ref()).expect("64 bytes")
    }

    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(64));
        out.extend_from_slice(&self.enc);
        out.extend_from_slice(&self.mac);
        out
    }

    fn mac_of(&self, iv: &[u8], data: &[u8]) -> [u8; 32] {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.mac).expect("any key length");
        mac.update(iv);
        mac.update(data);
        mac.finalize().into_bytes().into()
    }

    /// In constant time, so how much of a MAC matched tells nothing.
    fn check_mac(&self, iv: &[u8], data: &[u8], mac: &[u8]) -> Result<(), Error> {
        let expected = self.mac_of(iv, data);
        if bool::from(expected.as_slice().ct_eq(mac)) {
            Ok(())
        } else {
            Err(Error::WrongKey)
        }
    }
}

/// An encrypted value as Bitwarden writes it: `<type>.<base64 parts separated by |>`.
#[derive(Clone, PartialEq, Eq)]
pub enum EncString {
    /// Type 0: AES-256-CBC without a MAC. Only very old accounts; opened
    /// with the master key only, and never written.
    AesCbc256 { iv: Vec<u8>, data: Vec<u8> },
    /// Type 2: AES-256-CBC, HMAC-SHA256 over IV and ciphertext. Everything
    /// current.
    AesCbc256HmacSha256 {
        iv: Vec<u8>,
        data: Vec<u8>,
        mac: Vec<u8>,
    },
    /// Types 3 and 5: RSA-2048-OAEP with SHA-256 (5 with an unused MAC).
    RsaOaepSha256 { data: Vec<u8> },
    /// Types 4 and 6: RSA-2048-OAEP with SHA-1 (6 with an unused MAC).
    /// Organisation keys come like this.
    RsaOaepSha1 { data: Vec<u8> },
}

impl std::fmt::Debug for EncString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncString(…)")
    }
}

fn b64(part: &str) -> Result<Vec<u8>, Error> {
    B64.decode(part.trim())
        .map_err(|_| Error::Crypto("an encrypted value isn't valid base64".into()))
}

impl std::str::FromStr for EncString {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let (kind, rest) = match text.split_once('.') {
            Some((kind, rest)) if kind.len() == 1 => (kind, rest),
            // Values without a type are type 2 with three parts, type 0 with two.
            _ => match text.split('|').count() {
                3 => ("2", text),
                2 => ("0", text),
                _ => return Err(Error::Crypto("not an encrypted value".into())),
            },
        };
        let parts: Vec<&str> = rest.split('|').collect();
        match (kind, parts.as_slice()) {
            ("0", [iv, data]) => Ok(EncString::AesCbc256 {
                iv: b64(iv)?,
                data: b64(data)?,
            }),
            ("2", [iv, data, mac]) => {
                let (iv, mac) = (b64(iv)?, b64(mac)?);
                if iv.len() != 16 || mac.len() != 32 {
                    return Err(Error::Crypto("an encrypted value is malformed".into()));
                }
                Ok(EncString::AesCbc256HmacSha256 {
                    iv,
                    data: b64(data)?,
                    mac,
                })
            }
            ("3", [data]) | ("5", [data, _]) => Ok(EncString::RsaOaepSha256 { data: b64(data)? }),
            ("4", [data]) | ("6", [data, _]) => Ok(EncString::RsaOaepSha1 { data: b64(data)? }),
            ("1", _) => Err(Error::Unsupported("AES-128 values (type 1)".into())),
            ("7", _) => Err(Error::Unsupported(
                "values in Bitwarden's new COSE format (type 7)".into(),
            )),
            _ => Err(Error::Crypto(format!("unknown encryption type {kind}"))),
        }
    }
}

impl std::fmt::Display for EncString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncString::AesCbc256 { iv, data } => {
                write!(f, "0.{}|{}", B64.encode(iv), B64.encode(data))
            }
            EncString::AesCbc256HmacSha256 { iv, data, mac } => write!(
                f,
                "2.{}|{}|{}",
                B64.encode(iv),
                B64.encode(data),
                B64.encode(mac)
            ),
            EncString::RsaOaepSha256 { data } => write!(f, "3.{}", B64.encode(data)),
            EncString::RsaOaepSha1 { data } => write!(f, "4.{}", B64.encode(data)),
        }
    }
}

impl EncString {
    /// Type 2 under `key`, with a fresh random IV.
    pub fn encrypt(plain: &[u8], key: &SymmetricKey) -> Self {
        let mut iv = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut iv);
        let data =
            Aes256CbcEnc::new(&key.enc.into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(plain);
        let mac = key.mac_of(&iv, &data).to_vec();
        EncString::AesCbc256HmacSha256 {
            iv: iv.to_vec(),
            data,
            mac,
        }
    }

    /// Opens a symmetric value. The MAC is checked first, in constant time: a
    /// value that was changed, or belongs to another key, never reaches AES.
    pub fn decrypt(&self, key: &SymmetricKey) -> Result<Zeroizing<Vec<u8>>, Error> {
        match self {
            EncString::AesCbc256HmacSha256 { iv, data, mac } => {
                key.check_mac(iv, data, mac)?;
                aes_decrypt(&key.enc, iv, data)
            }
            // No MAC means no way to tell a wrong key from a changed value.
            // Bitwarden only ever wrote these with the bare master key.
            EncString::AesCbc256 { .. } => Err(Error::Unsupported(
                "values without a MAC (type 0) from very old accounts".into(),
            )),
            _ => Err(Error::Crypto("an RSA value needs a private key".into())),
        }
    }

    pub fn decrypt_string(&self, key: &SymmetricKey) -> Result<Zeroizing<String>, Error> {
        let bytes = self.decrypt(key)?;
        String::from_utf8(bytes.to_vec())
            .map(Zeroizing::new)
            .map_err(|_| Error::Crypto("a decrypted value isn't text".into()))
    }

    /// Opens a key: the account's user key, an organisation key, an item key.
    pub fn decrypt_key(&self, key: &SymmetricKey) -> Result<SymmetricKey, Error> {
        SymmetricKey::from_bytes(&self.decrypt(key)?)
    }

    /// Opens an RSA-wrapped key: an organisation key, or a user key handed
    /// over to this key pair.
    pub fn decrypt_key_rsa(&self, private: &PrivateKey) -> Result<SymmetricKey, Error> {
        SymmetricKey::from_bytes(&self.decrypt_rsa(private)?)
    }

    /// Opens an RSA-wrapped value, an organisation key.
    pub fn decrypt_rsa(&self, private: &PrivateKey) -> Result<Zeroizing<Vec<u8>>, Error> {
        let result = match self {
            EncString::RsaOaepSha1 { data } => {
                private.0.decrypt(rsa::Oaep::new::<sha1::Sha1>(), data)
            }
            EncString::RsaOaepSha256 { data } => {
                private.0.decrypt(rsa::Oaep::new::<Sha256>(), data)
            }
            _ => return Err(Error::Crypto("not an RSA value".into())),
        };
        result.map(Zeroizing::new).map_err(|_| Error::WrongKey)
    }
}

fn aes_decrypt(key: &[u8; 32], iv: &[u8], data: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let iv: [u8; 16] = iv
        .try_into()
        .map_err(|_| Error::Crypto("an IV has 16 bytes".into()))?;
    Aes256CbcDec::new(key.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map(Zeroizing::new)
        .map_err(|_| Error::Crypto("a value didn't decrypt".into()))
}

/// An RSA-2048 private key: the account's own, for organisation keys, or one
/// made for handing over the user key.
pub struct PrivateKey(rsa::RsaPrivateKey);

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateKey(…)")
    }
}

impl PrivateKey {
    /// A fresh RSA-2048 key pair, as Bitwarden makes them: for a new account,
    /// a device asking to log in, a passkey that unlocks. In a browser this
    /// takes a moment, a second or a few.
    pub fn generate() -> Result<Self, Error> {
        rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048)
            .map(PrivateKey)
            .map_err(|e| Error::Crypto(format!("RSA: {e}")))
    }

    /// From the PKCS#8 DER the profile's `privateKey` decrypts to.
    pub fn from_der(der: &[u8]) -> Result<Self, Error> {
        rsa::RsaPrivateKey::from_pkcs8_der(der)
            .map(PrivateKey)
            .map_err(|_| Error::Crypto("the account's private key doesn't parse".into()))
    }

    /// PKCS#8 DER, what Bitwarden encrypts under a symmetric key.
    pub fn to_der(&self) -> Result<Zeroizing<Vec<u8>>, Error> {
        self.0
            .to_pkcs8_der()
            .map(|der| Zeroizing::new(der.as_bytes().to_vec()))
            .map_err(|e| Error::Crypto(format!("RSA: {e}")))
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.to_public_key())
    }
}

/// An RSA public key: someone's to wrap a key for, or one to check with a
/// [`fingerprint`].
#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey(rsa::RsaPublicKey);

impl std::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PublicKey(…)")
    }
}

impl PublicKey {
    /// From SPKI DER, as Bitwarden hands out public keys (base64 in JSON).
    pub fn from_der(der: &[u8]) -> Result<Self, Error> {
        rsa::RsaPublicKey::from_public_key_der(der)
            .map(PublicKey)
            .map_err(|_| Error::Crypto("a public key doesn't parse".into()))
    }

    /// SPKI DER.
    pub fn to_der(&self) -> Result<Vec<u8>, Error> {
        self.0
            .to_public_key_der()
            .map(|der| der.into_vec())
            .map_err(|e| Error::Crypto(format!("RSA: {e}")))
    }
}

/// Wraps a key for someone's public key, RSA-OAEP with SHA-1 (type 4), as
/// Bitwarden's apps do it: organisation keys for members, the user key for
/// an emergency contact, a device asking to log in, a passkey's key set.
pub fn wrap_for(public: &PublicKey, key: &SymmetricKey) -> Result<EncString, Error> {
    let data = public
        .0
        .encrypt(
            &mut rand::rngs::OsRng,
            rsa::Oaep::new::<sha1::Sha1>(),
            &key.to_bytes(),
        )
        .map_err(|e| Error::Crypto(format!("RSA: {e}")))?;
    Ok(EncString::RsaOaepSha1 { data })
}

// ── Files: attachments and Sends ───────────────────────────
//
// A file's contents are type 2 like any other value, but in binary, without
// base64 (Bitwarden's `EncArrayBuffer`): the type byte 2, the IV (16 bytes),
// the MAC (32), then the ciphertext. An attachment is under its own key
// (`attachment.key`, itself under the item key, or the user or organisation
// key), or under that key directly if it is older than attachment keys. A
// file Send is under the Send's key.

/// Encrypts a file's contents under `key`, with a fresh random IV.
pub fn encrypt_file(plain: &[u8], key: &SymmetricKey) -> Vec<u8> {
    let mut iv = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut iv);
    // Encrypted in place, in the buffer that is returned: attachments can be
    // large, and a browser tab has no memory to spare for a second copy.
    let padded = (plain.len() / 16 + 1) * 16;
    let mut out = vec![0u8; FILE_HEADER + padded];
    out[0] = 2;
    out[1..17].copy_from_slice(&iv);
    out[FILE_HEADER..FILE_HEADER + plain.len()].copy_from_slice(plain);
    Aes256CbcEnc::new(&key.enc.into(), &iv.into())
        .encrypt_padded_mut::<Pkcs7>(&mut out[FILE_HEADER..], plain.len())
        .expect("room for the padding");
    let mac = key.mac_of(&iv, &out[FILE_HEADER..]);
    out[17..FILE_HEADER].copy_from_slice(&mac);
    out
}

/// Opens a file's contents. Anything but type 2 is refused, and so is
/// anything too short to hold one AES block; the MAC is checked first, in
/// constant time.
pub fn decrypt_file(bytes: &[u8], key: &SymmetricKey) -> Result<Zeroizing<Vec<u8>>, Error> {
    match bytes.first() {
        Some(2) => {}
        Some(kind) => {
            return Err(Error::Crypto(format!(
                "an encrypted file of unknown type {kind}"
            )))
        }
        None => return Err(Error::Crypto("an encrypted file is empty".into())),
    }
    if bytes.len() < FILE_HEADER + 16 {
        return Err(Error::Crypto("an encrypted file is too short".into()));
    }
    let (iv, mac, data) = (
        &bytes[1..17],
        &bytes[17..FILE_HEADER],
        &bytes[FILE_HEADER..],
    );
    key.check_mac(iv, data, mac)?;
    aes_decrypt(&key.enc, iv, data)
}

/// Type byte, IV and MAC.
const FILE_HEADER: usize = 1 + 16 + 32;

/// A key made from a short random secret, for sharing it in a link: HKDF-SHA256
/// with the salt `bitwarden-<name>` and `info`, 64 bytes (Bitwarden's
/// `derive_shareable_key`).
pub fn derive_shareable_key(secret: &[u8; 16], name: &str, info: Option<&str>) -> SymmetricKey {
    let salt = format!("bitwarden-{name}");
    let hkdf = hkdf::Hkdf::<Sha256>::new(Some(salt.as_bytes()), secret);
    let mut bytes = Zeroizing::new([0u8; 64]);
    hkdf.expand(info.unwrap_or_default().as_bytes(), bytes.as_mut())
        .expect("64 bytes is a valid length");
    SymmetricKey::from_bytes(bytes.as_ref()).expect("64 bytes")
}

/// A Send's key, from its 16-byte seed: what the Send's `key` decrypts to
/// under the user key, and what its link carries after the `#`, in URL-safe
/// base64 without padding.
pub fn send_key(seed: &[u8]) -> Result<SymmetricKey, Error> {
    let seed: &[u8; 16] = seed.try_into().map_err(|_| {
        Error::Crypto(format!(
            "a Send's seed has 16 bytes, this one {}",
            seed.len()
        ))
    })?;
    Ok(derive_shareable_key(seed, "send", Some("send")))
}

/// A seed for a new Send.
pub fn generate_send_seed() -> Zeroizing<[u8; 16]> {
    let mut seed = Zeroizing::new([0u8; 16]);
    rand::rngs::OsRng.fill_bytes(seed.as_mut());
    seed
}

/// What a Send's password becomes before it goes to the server, when the Send
/// is made and when it is opened: base64 of PBKDF2-SHA256 over the password,
/// salted with the seed, 100 000 rounds.
pub fn send_password_hash(password: &str, seed: &[u8]) -> String {
    let mut hash = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), seed, 100_000, hash.as_mut());
    B64.encode(hash.as_ref())
}

// ── Passkeys that unlock (WebAuthn PRF) ────────────────────
//
// A passkey whose authenticator has the PRF extension gives 32 secret bytes
// each time it signs in, for the same salt. Bitwarden's web vault stretches
// them into a key and keeps a key set with the passkey: a key pair of its own,
// the user key wrapped for its public half, the private half under the PRF
// key. Logging in with the passkey opens the private half, and with it the
// user key. The public half is under the user key, so a key rotation can wrap
// the new user key without the passkey.

/// The salt the PRF extension is asked to evaluate, the same as in
/// Bitwarden's web vault: SHA-256 of `passwordless-login`. Another salt gives
/// other bytes, and a passkey set up there wouldn't unlock here.
pub fn prf_salt() -> [u8; 32] {
    Sha256::digest(b"passwordless-login").into()
}

/// The key a passkey's PRF output makes: its first 32 bytes, stretched like a
/// master key. Output that is too short or all zeros is refused.
pub fn prf_key(prf: &[u8]) -> Result<SymmetricKey, Error> {
    let secret: &[u8; 32] = prf
        .get(..32)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| Error::Crypto("a PRF output has at least 32 bytes".into()))?;
    if secret.iter().all(|byte| *byte == 0) {
        return Err(Error::Crypto("the PRF output is all zeros".into()));
    }
    Ok(SymmetricKey::stretch(secret))
}

/// The keys kept with a passkey that unlocks, as the server takes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrfKeySet {
    /// `encryptedUserKey`: the user key, wrapped for the set's public key (type 4).
    pub encrypted_user_key: EncString,
    /// `encryptedPublicKey`: the set's public key as SPKI DER, under the user key.
    pub encrypted_public_key: EncString,
    /// `encryptedPrivateKey`: the set's private key as PKCS#8 DER, under the PRF key.
    pub encrypted_private_key: EncString,
}

impl PrfKeySet {
    /// A key set for the passkey that gave `prf`, with a fresh key pair.
    pub fn create(prf: &[u8], user_key: &SymmetricKey) -> Result<Self, Error> {
        let prf_key = prf_key(prf)?;
        let private = PrivateKey::generate()?;
        let public = private.public();
        Ok(PrfKeySet {
            encrypted_user_key: wrap_for(&public, user_key)?,
            encrypted_public_key: EncString::encrypt(&public.to_der()?, user_key),
            encrypted_private_key: EncString::encrypt(&private.to_der()?, &prf_key),
        })
    }

    /// The user key, with the PRF output of the passkey this set belongs to.
    pub fn open(&self, prf: &[u8]) -> Result<SymmetricKey, Error> {
        open_prf_key_set(prf, &self.encrypted_private_key, &self.encrypted_user_key)
    }
}

/// The user key, from what a passkey login gets back (`webAuthnPrfOption`:
/// the private key and the user key of the set, not the public key). Another
/// passkey's PRF output fails as [`Error::WrongKey`].
pub fn open_prf_key_set(
    prf: &[u8],
    encrypted_private_key: &EncString,
    encrypted_user_key: &EncString,
) -> Result<SymmetricKey, Error> {
    let der = encrypted_private_key.decrypt(&prf_key(prf)?)?;
    encrypted_user_key.decrypt_key_rsa(&PrivateKey::from_der(&der)?)
}

// ── Fingerprint phrases ────────────────────────────────────

/// EFF's long word list (<https://www.eff.org/dice>, CC BY 3.0 US), one word a
/// line, in its order: 7776 words. Passphrases come from it too
/// ([`crate::generator::passphrase`]).
const WORDS: &str = include_str!("eff_large_wordlist.txt");
pub(crate) const WORD_COUNT: u32 = 7776;

/// The word at `index` of the list, counted from 0.
pub(crate) fn word(index: usize) -> &'static str {
    WORDS.lines().nth(index).expect("the list has 7776 words")
}

/// Bitwarden's fingerprint phrase for a public key (SPKI DER): five words
/// joined with `-`, which both sides read out to check they see the same key
/// before one hands a key over. `material` ties the phrase to someone: the
/// email in lower case when a device asks to log in, the user id for
/// emergency access and an account's own fingerprint.
///
/// SHA-256 of the key, HKDF-Expand with `material` to 32 bytes, read as one
/// big-endian number; each word is the remainder by 7776, the number divided
/// by 7776 for the next. Five words are the fewest with 64 bits between them.
pub fn fingerprint(material: &str, public_key: &[u8]) -> String {
    let hkdf = hkdf::Hkdf::<Sha256>::from_prk(&Sha256::digest(public_key))
        .expect("32 bytes is a valid PRK");
    let mut number = [0u8; 32];
    hkdf.expand(material.as_bytes(), &mut number)
        .expect("32 bytes is a valid length");
    let words: Vec<&str> = (0..5)
        .map(|_| {
            // Long division of the 256-bit number, a byte at a time.
            let mut remainder = 0u32;
            for byte in number.iter_mut() {
                let part = (remainder << 8) | u32::from(*byte);
                *byte = (part / WORD_COUNT) as u8;
                remainder = part % WORD_COUNT;
            }
            word(remainder as usize)
        })
        .collect();
    words.join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_type_2() {
        let key = SymmetricKey::generate();
        let enc = EncString::encrypt(b"hello nyu", &key);
        let text = enc.to_string();
        assert!(text.starts_with("2."));
        let back: EncString = text.parse().unwrap();
        assert_eq!(back, enc);
        assert_eq!(back.decrypt(&key).unwrap().as_slice(), b"hello nyu");
    }

    #[test]
    fn a_changed_value_or_another_key_is_refused() {
        let key = SymmetricKey::generate();
        let other = SymmetricKey::generate();
        let enc = EncString::encrypt(b"secret", &key);
        assert!(matches!(enc.decrypt(&other), Err(Error::WrongKey)));
        let EncString::AesCbc256HmacSha256 { iv, mut data, mac } = enc else {
            unreachable!()
        };
        data[0] ^= 1;
        let tampered = EncString::AesCbc256HmacSha256 { iv, data, mac };
        assert!(matches!(tampered.decrypt(&key), Err(Error::WrongKey)));
    }

    #[test]
    fn values_without_a_type_prefix() {
        let key = SymmetricKey::generate();
        let text = EncString::encrypt(b"x", &key).to_string();
        let bare = text.trim_start_matches("2.");
        assert!(bare.parse::<EncString>().unwrap().decrypt(&key).is_ok());
    }

    #[test]
    fn refuses_cheap_kdf_settings() {
        assert!(Kdf::Pbkdf2 { iterations: 100 }.check().is_err());
        assert!(Kdf::Pbkdf2 {
            iterations: 600_000
        }
        .check()
        .is_ok());
        assert!(Kdf::Argon2id {
            iterations: 1,
            memory_mib: 64,
            parallelism: 4
        }
        .check()
        .is_err());
    }

    #[test]
    fn what_counts_as_a_weaker_kdf() {
        let pbkdf2 = |iterations| Kdf::Pbkdf2 { iterations };
        let argon2 = |iterations, memory_mib, parallelism| Kdf::Argon2id {
            iterations,
            memory_mib,
            parallelism,
        };
        assert!(pbkdf2(5_000).is_weaker_than(&pbkdf2(600_000)));
        assert!(!pbkdf2(600_000).is_weaker_than(&pbkdf2(600_000)));
        assert!(!pbkdf2(2_000_000).is_weaker_than(&pbkdf2(600_000)));
        assert!(argon2(2, 64, 4).is_weaker_than(&argon2(3, 64, 4)));
        assert!(argon2(3, 32, 4).is_weaker_than(&argon2(3, 64, 4)));
        assert!(!argon2(3, 64, 1).is_weaker_than(&argon2(3, 64, 4)));
        assert!(!argon2(4, 128, 4).is_weaker_than(&argon2(3, 64, 4)));
        // Leaving Argon2id for PBKDF2 is a downgrade however many rounds;
        // the other way is Bitwarden's recommended upgrade.
        assert!(pbkdf2(2_000_000).is_weaker_than(&argon2(2, 15, 1)));
        assert!(!argon2(2, 15, 1).is_weaker_than(&pbkdf2(600_000)));
    }

    #[test]
    fn rsa_round_trip() {
        let private = PrivateKey::generate().unwrap();
        let org = SymmetricKey::generate();
        let wrapped = wrap_for(&private.public(), &org).unwrap();
        let text = wrapped.to_string();
        assert!(text.starts_with("4."));
        let opened = text
            .parse::<EncString>()
            .unwrap()
            .decrypt_rsa(&private)
            .unwrap();
        assert_eq!(opened.as_slice(), org.to_bytes().as_slice());
    }

    #[test]
    fn handing_the_user_key_to_another_key_pair() {
        // Emergency access and a device asking to log in go the same way: the
        // other side's public key arrives as SPKI DER, the user key goes back
        // wrapped for it, and only its private key opens that.
        let theirs = PrivateKey::generate().unwrap();
        let der = theirs.public().to_der().unwrap();
        let public = PublicKey::from_der(&der).unwrap();
        let user = SymmetricKey::generate();
        let wrapped = wrap_for(&public, &user).unwrap().to_string();
        assert!(wrapped.starts_with("4."));
        let wrapped: EncString = wrapped.parse().unwrap();
        let opened = wrapped.decrypt_key_rsa(&theirs).unwrap();
        assert_eq!(opened.to_bytes().as_slice(), user.to_bytes().as_slice());

        // The private key survives the way through PKCS#8.
        let again = PrivateKey::from_der(&theirs.to_der().unwrap()).unwrap();
        assert!(wrapped.decrypt_key_rsa(&again).is_ok());
        assert!(PublicKey::from_der(b"not a key").is_err());
    }

    #[test]
    fn files_round_trip() {
        let key = SymmetricKey::generate();
        for length in [0, 1, 15, 16, 17, 1000] {
            let plain: Vec<u8> = (0..length).map(|i| i as u8).collect();
            let file = encrypt_file(&plain, &key);
            assert_eq!(file[0], 2);
            assert_eq!(file.len(), FILE_HEADER + (length / 16 + 1) * 16);
            assert_eq!(decrypt_file(&file, &key).unwrap().as_slice(), plain);
        }
    }

    #[test]
    fn files_that_were_changed_or_are_not_type_2_are_refused() {
        let key = SymmetricKey::generate();
        let file = encrypt_file(b"a cat picture", &key);
        assert!(matches!(
            decrypt_file(&file, &SymmetricKey::generate()),
            Err(Error::WrongKey)
        ));
        for at in [1, 17, file.len() - 1] {
            let mut changed = file.clone();
            changed[at] ^= 1;
            assert!(matches!(decrypt_file(&changed, &key), Err(Error::WrongKey)));
        }
        for kind in [0, 1, 4, 7] {
            let mut other = file.clone();
            other[0] = kind;
            assert!(matches!(decrypt_file(&other, &key), Err(Error::Crypto(_))));
        }
        assert!(decrypt_file(&[], &key).is_err());
        assert!(decrypt_file(&file[..FILE_HEADER + 15], &key).is_err());
    }

    #[test]
    fn a_passkey_key_set_round_trip() {
        let prf = [7u8; 32];
        let user = SymmetricKey::generate();
        let set = PrfKeySet::create(&prf, &user).unwrap();
        assert!(set.encrypted_user_key.to_string().starts_with("4."));
        assert!(set.encrypted_private_key.to_string().starts_with("2."));
        let opened = set.open(&prf).unwrap();
        assert_eq!(opened.to_bytes().as_slice(), user.to_bytes().as_slice());

        // What a passkey login gets back is enough, as text.
        let opened = open_prf_key_set(
            &prf,
            &set.encrypted_private_key.to_string().parse().unwrap(),
            &set.encrypted_user_key.to_string().parse().unwrap(),
        )
        .unwrap();
        assert_eq!(opened.to_bytes().as_slice(), user.to_bytes().as_slice());

        // A key rotation reads the public key with the user key alone.
        let public = set.encrypted_public_key.decrypt(&user).unwrap();
        assert!(PublicKey::from_der(&public).is_ok());

        assert!(matches!(set.open(&[8u8; 32]), Err(Error::WrongKey)));
        // Longer output counts with its first 32 bytes; shorter or empty doesn't.
        assert!(set.open(&[7u8; 64]).is_ok());
        assert!(prf_key(&[7u8; 31]).is_err());
        assert!(prf_key(&[0u8; 32]).is_err());
    }

    #[test]
    fn the_word_list_is_effs() {
        let words: Vec<&str> = WORDS.lines().collect();
        assert_eq!(words.len(), WORD_COUNT as usize);
        assert_eq!(words.first(), Some(&"abacus"));
        assert_eq!(words.last(), Some(&"zoom"));
        assert!(words.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn the_prf_salt_is_bitwardens() {
        // SHA-256 of "passwordless-login", worked out with Python's hashlib.
        assert_eq!(
            B64.encode(prf_salt()),
            "l7AG39jbWXuJLJpQNDtSSLpsMoBZ74Es7DR84Nd0OQs="
        );
    }
}
