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
//!   private key is itself an EncString under the user key.
//!
//! Keys zeroize themselves when dropped.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use rand::RngCore;
use rsa::pkcs8::DecodePrivateKey;
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
                let expected = key.mac_of(iv, data);
                if !bool::from(expected.as_slice().ct_eq(mac)) {
                    return Err(Error::WrongKey);
                }
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

/// The account's RSA private key, for organisation keys.
pub struct PrivateKey(rsa::RsaPrivateKey);

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateKey(…)")
    }
}

impl PrivateKey {
    /// From the PKCS#8 DER the profile's `privateKey` decrypts to.
    pub fn from_der(der: &[u8]) -> Result<Self, Error> {
        rsa::RsaPrivateKey::from_pkcs8_der(der)
            .map(PrivateKey)
            .map_err(|_| Error::Crypto("the account's private key doesn't parse".into()))
    }

    pub fn public(&self) -> rsa::RsaPublicKey {
        self.0.to_public_key()
    }
}

/// Wraps a key for someone's public key, as the server hands out organisation keys.
pub fn wrap_for(public: &rsa::RsaPublicKey, key: &SymmetricKey) -> Result<EncString, Error> {
    let data = public
        .encrypt(
            &mut rand::rngs::OsRng,
            rsa::Oaep::new::<sha1::Sha1>(),
            &key.to_bytes(),
        )
        .map_err(|e| Error::Crypto(format!("RSA: {e}")))?;
    Ok(EncString::RsaOaepSha1 { data })
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
        let private = rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap();
        let private = PrivateKey(private);
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
}
