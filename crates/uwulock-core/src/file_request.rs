//! File requests: Sends the other way round.
//!
//! The owner makes a link; somebody without an account uploads files and a
//! message to it, encrypted in their browser for the owner's RSA public key.
//! The server keeps only ciphertext.
//!
//! - The link carries a 16-byte **link secret** after the `#` ([`LinkSecret`]).
//!   From it comes the **link key**, HKDF-SHA256 with the salt
//!   `bitwarden-filerequest` and the info `filerequest` ([`LinkSecret::key`]),
//!   which opens the request's [`PublicInfo`]: title, note, the owner's name
//!   and the public key to encrypt for. Taking the key from this blob, not from
//!   the server, keeps a server from swapping in a key of its own.
//! - Each submission has its own 64-byte key `K` ([`SubmissionKey`]), wrapped
//!   RSA-OAEP-SHA1 for the owner's public key (type 4). The message, the
//!   sender and every file's name and key are type 2 under `K`.
//! - Each file has its own 64-byte key `Kf` ([`FileKey`]) and its contents are
//!   an EncArrayBuffer under it ([`crate::crypto::encrypt_file`]), so taking a
//!   file into an item only wraps `Kf` and the name again
//!   ([`FileKey::for_item`]); the bytes stay as they are.
//!
//! The owner's label for the request and the link secret itself are kept
//! under the account's extras key ([`crate::extras`]), so the link can be shown
//! again and survives any rotation that keeps the key pair.

use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as B64_URL};
use base64::Engine as _;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto::{
    derive_shareable_key, send_password_hash, wrap_for, EncString, PrivateKey, PublicKey,
    SymmetricKey,
};
use crate::Error;

/// The longest message an uploader may leave, in characters.
pub const TEXT_MAX_CHARS: usize = 100_000;

/// The 16 random bytes a file request's link carries after the `#`.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct LinkSecret([u8; 16]);

impl std::fmt::Debug for LinkSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LinkSecret(…)")
    }
}

impl LinkSecret {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        LinkSecret(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let bytes: [u8; 16] = bytes.try_into().map_err(|_| {
            Error::Crypto(format!(
                "a file request's secret has 16 bytes, this one {}",
                bytes.len()
            ))
        })?;
        Ok(LinkSecret(bytes))
    }

    /// From the link: URL-safe base64 without padding (padding and the
    /// standard alphabet are taken too, as people copy links around).
    pub fn from_link_part(text: &str) -> Result<Self, Error> {
        let clean: String = text
            .trim()
            .trim_end_matches('=')
            .chars()
            .map(|c| match c {
                '+' => '-',
                '/' => '_',
                other => other,
            })
            .collect();
        let bytes = Zeroizing::new(
            B64_URL
                .decode(clean)
                .map_err(|_| Error::Crypto("the link's secret isn't valid base64".into()))?,
        );
        LinkSecret::from_bytes(&bytes)
    }

    /// For the link, after the `#`.
    pub fn to_link_part(&self) -> String {
        B64_URL.encode(self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// The link key: `derive_shareable_key(s, "filerequest", "filerequest")`.
    pub fn key(&self) -> SymmetricKey {
        derive_shareable_key(&self.0, "filerequest", Some("filerequest"))
    }

    /// What an optional password becomes before it goes to the server: the
    /// same construction as a Send's, with the link secret as the salt.
    pub fn password_hash(&self, password: &str) -> String {
        send_password_hash(password, &self.0)
    }

    /// `linkSecret` of the request object: the secret under the extras key.
    pub fn seal(&self, extras: &SymmetricKey) -> String {
        EncString::encrypt(&self.0, extras).to_string()
    }

    /// Opens a request's `linkSecret`.
    pub fn open(sealed: &str, extras: &SymmetricKey) -> Result<Self, Error> {
        let bytes = sealed.parse::<EncString>()?.decrypt(extras)?;
        LinkSecret::from_bytes(&bytes)
    }
}

/// What the uploader's page shows and encrypts for, under the link key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInfo {
    /// Always 1 for now.
    pub v: u32,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    /// The owner's RSA public key, SPKI DER in standard base64.
    #[serde(rename = "publicKey")]
    pub public_key: String,
    #[serde(default)]
    pub owner: Option<String>,
}

impl PublicInfo {
    pub fn new(
        title: &str,
        note: Option<&str>,
        owner: Option<&str>,
        public_key: &PublicKey,
    ) -> Result<Self, Error> {
        Ok(PublicInfo {
            v: 1,
            title: title.to_string(),
            note: note.filter(|n| !n.is_empty()).map(str::to_string),
            public_key: B64.encode(public_key.to_der()?),
            owner: owner.filter(|n| !n.is_empty()).map(str::to_string),
        })
    }

    /// `publicInfo`: this, as JSON, type 2 under the link key.
    pub fn seal(&self, secret: &LinkSecret) -> Result<String, Error> {
        let json = Zeroizing::new(
            serde_json::to_vec(self).map_err(|e| Error::Crypto(format!("JSON: {e}")))?,
        );
        Ok(EncString::encrypt(&json, &secret.key()).to_string())
    }

    /// Opens `publicInfo` with the link's secret.
    pub fn open(sealed: &str, secret: &LinkSecret) -> Result<Self, Error> {
        let json = sealed.parse::<EncString>()?.decrypt(&secret.key())?;
        let info: PublicInfo = serde_json::from_slice(&json)
            .map_err(|_| Error::Crypto("a file request's details don't read".into()))?;
        if info.v != 1 {
            return Err(Error::Unsupported(format!(
                "file requests of version {}",
                info.v
            )));
        }
        Ok(info)
    }

    /// The key to encrypt submissions for.
    pub fn public_key(&self) -> Result<PublicKey, Error> {
        let der = B64
            .decode(self.public_key.trim())
            .map_err(|_| Error::Crypto("the owner's public key isn't valid base64".into()))?;
        PublicKey::from_der(&der)
    }

    /// Whether the uploads go to `owner` — the owner's own public key. The
    /// owner checks this before showing or handing out a link: details that
    /// name another key mean someone who knew the link secret made them, and
    /// uploads to that link would be readable by them, not by the owner.
    pub fn is_for(&self, owner: &PublicKey) -> bool {
        self.public_key().is_ok_and(|key| key == *owner)
    }
}

/// `accessId` from the request's id: URL-safe base64 of its 16 bytes, as for
/// Sends.
pub fn access_id(request_id: &str) -> Result<String, Error> {
    let hex: String = request_id.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Crypto("a request id is a UUID".into()));
    }
    let bytes: Vec<u8> = (0..16)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex digits"))
        .collect();
    Ok(B64_URL.encode(bytes))
}

/// The link to hand out. `base` is the main host (`https://lock.example.com`)
/// or, with `send_domain`, a send domain (`https://send.example.com`).
pub fn link(base: &str, access_id: &str, secret: &LinkSecret, send_domain: bool) -> String {
    let base = base.trim_end_matches('/');
    let secret = secret.to_link_part();
    if send_domain {
        format!("{base}/r/{access_id}#{secret}")
    } else {
        format!("{base}/#/request/{access_id}/{secret}")
    }
}

/// The owner's label for a request (`name`), under the extras key.
pub fn seal_label(name: &str, extras: &SymmetricKey) -> String {
    EncString::encrypt(name.as_bytes(), extras).to_string()
}

pub fn open_label(sealed: &str, extras: &SymmetricKey) -> Result<Zeroizing<String>, Error> {
    sealed.parse::<EncString>()?.decrypt_string(extras)
}

/// Who uploaded, as they typed it. Nobody checked it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sender {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// The key of one submission, `K`.
#[derive(Debug, Clone)]
pub struct SubmissionKey(SymmetricKey);

impl SubmissionKey {
    pub fn generate() -> Self {
        SubmissionKey(SymmetricKey::generate())
    }

    /// `wrappedKey`: `K` for the owner's public key, type 4.
    pub fn wrap(&self, owner: &PublicKey) -> Result<String, Error> {
        Ok(wrap_for(owner, &self.0)?.to_string())
    }

    /// The owner opens `wrappedKey` with the account's private key.
    pub fn open(wrapped: &str, private: &PrivateKey) -> Result<Self, Error> {
        wrapped
            .parse::<EncString>()?
            .decrypt_key_rsa(private)
            .map(SubmissionKey)
    }

    /// `text`: the message, at most [`TEXT_MAX_CHARS`] characters.
    pub fn seal_text(&self, text: &str) -> Result<String, Error> {
        if text.chars().count() > TEXT_MAX_CHARS {
            return Err(Error::Crypto(format!(
                "a message is at most {TEXT_MAX_CHARS} characters"
            )));
        }
        Ok(EncString::encrypt(text.as_bytes(), &self.0).to_string())
    }

    pub fn open_text(&self, sealed: &str) -> Result<Zeroizing<String>, Error> {
        sealed.parse::<EncString>()?.decrypt_string(&self.0)
    }

    /// `sender`: name and address as JSON.
    pub fn seal_sender(&self, sender: &Sender) -> Result<String, Error> {
        let json = serde_json::to_vec(sender).map_err(|e| Error::Crypto(format!("JSON: {e}")))?;
        Ok(EncString::encrypt(&json, &self.0).to_string())
    }

    pub fn open_sender(&self, sealed: &str) -> Result<Sender, Error> {
        let json = sealed.parse::<EncString>()?.decrypt(&self.0)?;
        serde_json::from_slice(&json).map_err(|_| Error::Crypto("the sender doesn't read".into()))
    }

    /// A new file in this submission: its key, and `{fileName, key}` as they
    /// go into the submission. The contents go through
    /// [`FileKey::encrypt`].
    pub fn new_file(&self, file_name: &str) -> (FileKey, SealedFile) {
        let key = FileKey(SymmetricKey::generate());
        let sealed = SealedFile {
            file_name: EncString::encrypt(file_name.as_bytes(), &self.0).to_string(),
            key: EncString::encrypt(&key.0.to_bytes(), &self.0).to_string(),
        };
        (key, sealed)
    }

    /// The owner opens a file's name and key.
    pub fn open_file(&self, file: &SealedFile) -> Result<(Zeroizing<String>, FileKey), Error> {
        let name = file
            .file_name
            .parse::<EncString>()?
            .decrypt_string(&self.0)?;
        let key = file.key.parse::<EncString>()?.decrypt_key(&self.0)?;
        Ok((name, FileKey(key)))
    }
}

/// A file's name and key, both under the submission's key (or, after
/// [`FileKey::for_item`], under an item's key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedFile {
    pub file_name: String,
    pub key: String,
}

/// The key of one file, `Kf`.
#[derive(Debug, Clone)]
pub struct FileKey(SymmetricKey);

impl FileKey {
    /// The contents, as the EncArrayBuffer that is uploaded; its length is the
    /// file's `size`.
    pub fn encrypt(&self, contents: &[u8]) -> Vec<u8> {
        crate::crypto::encrypt_file(contents, &self.0)
    }

    pub fn decrypt(&self, encrypted: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
        crate::crypto::decrypt_file(encrypted, &self.0)
    }

    /// Taking the file into an item (`…/attach`): the name and `Kf` under the
    /// item's key — the cipher's own key if it has one, else the user or
    /// organisation key — as for a Bitwarden attachment.
    pub fn for_item(&self, file_name: &str, item_key: &SymmetricKey) -> SealedFile {
        SealedFile {
            file_name: EncString::encrypt(file_name.as_bytes(), item_key).to_string(),
            key: EncString::encrypt(&self.0.to_bytes(), item_key).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_secret_goes_through_the_link_and_back() {
        let secret = LinkSecret::generate();
        let part = secret.to_link_part();
        assert_eq!(part.len(), 22);
        assert_eq!(
            LinkSecret::from_link_part(&part).unwrap().as_bytes(),
            secret.as_bytes()
        );
        assert!(LinkSecret::from_link_part("abc").is_err());
    }

    #[test]
    fn links_on_the_main_host_and_a_send_domain() {
        let secret = LinkSecret::from_bytes(&[7; 16]).unwrap();
        let id = access_id("5b0c1e2d-0000-4000-8000-00000000abcd").unwrap();
        assert_eq!(id, "WwweLQAAQACAAAAAAACrzQ");
        assert_eq!(
            link("https://lock.example.com/", &id, &secret, false),
            "https://lock.example.com/#/request/WwweLQAAQACAAAAAAACrzQ/BwcHBwcHBwcHBwcHBwcHBw"
        );
        assert_eq!(
            link("https://send.example.com", &id, &secret, true),
            "https://send.example.com/r/WwweLQAAQACAAAAAAACrzQ#BwcHBwcHBwcHBwcHBwcHBw"
        );
        assert!(access_id("not-a-uuid").is_err());
    }

    #[test]
    fn the_owner_keeps_label_and_secret_under_the_extras_key() {
        let extras = SymmetricKey::generate();
        let secret = LinkSecret::generate();
        let sealed = secret.seal(&extras);
        assert_eq!(
            LinkSecret::open(&sealed, &extras).unwrap().as_bytes(),
            secret.as_bytes()
        );
        let label = seal_label("Passport for the bank", &extras);
        assert_eq!(
            open_label(&label, &extras).unwrap().as_str(),
            "Passport for the bank"
        );
        assert!(LinkSecret::open(&sealed, &SymmetricKey::generate()).is_err());
    }
}
