//! SSH keys for UwUSSH's `key` records: a new Ed25519 key as OpenSSH text,
//! and what an imported key is.
//!
//! A `key` record (`uwussh-proto` `KeyPayload`) holds `key_type`
//! (`ssh-ed25519`), `public_key` (the `ssh-ed25519 AAAA… comment` line) and
//! points at `secret` records: the private key's text and, when it has one,
//! the passphrase. With a passphrase the private key is encrypted the way
//! `ssh-keygen` does it (bcrypt-pbkdf, 16 rounds, aes256-ctr), so the text
//! opens in OpenSSH as well; the passphrase goes into its own `secret`.
//!
//! All of it is pure Rust and works in WebAssembly; the bcrypt rounds take a
//! moment there.

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use ssh_key::private::{Ed25519Keypair, KeypairData};
use ssh_key::{HashAlg, LineEnding, PrivateKey, PublicKey};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::Error;

/// A key made by [`generate_ed25519`].
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedKey {
    /// `-----BEGIN OPENSSH PRIVATE KEY-----` …, LF line endings; encrypted
    /// when a passphrase was given.
    pub private_key: String,
    /// `ssh-ed25519 AAAA… comment`.
    #[zeroize(skip)]
    pub public_key: String,
    /// `ssh-ed25519`.
    #[zeroize(skip)]
    pub key_type: String,
    /// `SHA256:…`, as `ssh-keygen -l` shows it.
    #[zeroize(skip)]
    pub fingerprint: String,
}

impl std::fmt::Debug for GeneratedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedKey")
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

/// What a key is, read from its public half.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    /// `ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256`, …
    pub key_type: String,
    /// The public line, with the comment when there is one.
    pub public_key: String,
    pub fingerprint: String,
    pub comment: String,
    /// The private key needs a passphrase (always `false` for a public key).
    pub encrypted: bool,
}

/// A new Ed25519 key. `comment` ends up in both halves (often
/// `user@host`); with a non-empty `passphrase` the private key text is
/// encrypted.
pub fn generate_ed25519(comment: &str, passphrase: Option<&str>) -> Result<GeneratedKey, Error> {
    let pair = Ed25519Keypair::random(&mut OsRng);
    let mut key = PrivateKey::new(KeypairData::Ed25519(pair), comment).map_err(ssh_error)?;
    // Before encrypting: an encrypted key keeps its comment in the encrypted part.
    let public = key.public_key().clone();
    if let Some(passphrase) = passphrase.filter(|p| !p.is_empty()) {
        key = key.encrypt(&mut OsRng, passphrase).map_err(ssh_error)?;
    }
    Ok(GeneratedKey {
        private_key: key
            .to_openssh(LineEnding::LF)
            .map_err(ssh_error)?
            .to_string(),
        public_key: public.to_openssh().map_err(ssh_error)?,
        key_type: public.algorithm().as_str().to_owned(),
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
    })
}

/// An OpenSSH private key's public half, type and fingerprint, read without
/// its passphrase (OpenSSH keeps the public key in the clear; the comment of
/// an encrypted key is in the encrypted part, so it is empty here). PEM and
/// PuTTY keys aren't read here.
pub fn inspect_private_key(text: &str) -> Result<KeyInfo, Error> {
    let key = PrivateKey::from_openssh(text.trim()).map_err(ssh_error)?;
    let mut info = info(key.public_key())?;
    info.encrypted = key.is_encrypted();
    Ok(info)
}

/// A public line (`ssh-ed25519 AAAA… comment`): type and fingerprint.
pub fn inspect_public_key(line: &str) -> Result<KeyInfo, Error> {
    info(&PublicKey::from_openssh(line.trim()).map_err(ssh_error)?)
}

/// Whether `passphrase` opens an encrypted OpenSSH private key. An
/// unencrypted one opens with any.
pub fn passphrase_opens(text: &str, passphrase: &str) -> Result<bool, Error> {
    let key = PrivateKey::from_openssh(text.trim()).map_err(ssh_error)?;
    if !key.is_encrypted() {
        return Ok(true);
    }
    Ok(key.decrypt(passphrase).is_ok())
}

fn info(public: &PublicKey) -> Result<KeyInfo, Error> {
    Ok(KeyInfo {
        key_type: public.algorithm().as_str().to_owned(),
        public_key: public.to_openssh().map_err(ssh_error)?,
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
        comment: public.comment().to_owned(),
        encrypted: false,
    })
}

fn ssh_error(error: ssh_key::Error) -> Error {
    Error::Crypto(format!("SSH key: {error}"))
}
