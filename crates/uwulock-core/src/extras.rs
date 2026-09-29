//! The extras key: what UwULock encrypts beyond Bitwarden's own objects.
//!
//! Suite spaces (UwUSSH, UwURDP), own icons, file-request labels and the
//! health report are under one **extras key** per account, not under the user
//! key. An official Bitwarden client that rotates the user key re-encrypts
//! only what it knows; anything else under the old key would be lost. So the
//! server keeps the extras key wrapped twice (`/uwu/v1/keys`):
//!
//! - `userKeyWrapped`: the 64-byte key as a type 2 value under the user key;
//! - `privateKeyWrapped`: the same key as a type 2 value under a key derived
//!   from the account's RSA **private** key ([`private_wrap_key`]).
//!
//! Both are made with a secret the server never has, so the server can't hand
//! out an extras key of its own choosing: whatever it would learn from that
//! (suite space keys, file-request link secrets) stays out of its reach. An
//! RSA wrap for the public key (type 4, how 0.3's betas did it) can't be told
//! apart from one the server made — it knows the public key — so it is never
//! taken.
//!
//! Bitwarden's clients keep the key pair when they rotate, so after an
//! official rotation the server drops `userKeyWrapped` and the next UwULock
//! client opens the key with the private key and wraps it again for the new
//! user key ([`resolve`] says so with `rewrap`). A key made before
//! `privateKeyWrapped` existed gets it the next time a client opens it
//! (`private_wrap`). Whenever both wraps are there and the private key is at
//! hand, they must hold the same key.
//!
//! Also here: what lives under the extras key or next to it — a suite
//! space's key ([`SpaceKey`]), an own icon ([`seal_icon`]), and re-encrypting
//! entry versions for a rotation made by UwULock itself
//! ([`reencrypt_version`]).

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto::{EncString, PrivateKey, SymmetricKey};
use crate::Error;

/// HKDF-SHA256 salt of [`private_wrap_key`].
pub const PRIVATE_WRAP_SALT: &str = "uwulock-extras-key-v1";
/// HKDF-SHA256 info of [`private_wrap_key`].
pub const PRIVATE_WRAP_INFO: &str = "private-key-wrap";

/// The extras key as `GET /uwu/v1/keys` hands it out (`extrasKey`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrappedExtrasKey {
    /// `null` after an official client rotated the user key, until a UwULock
    /// client wraps it again.
    #[serde(default)]
    pub user_key_wrapped: Option<String>,
    /// `null` for a key made before this wrap existed, until a UwULock client
    /// adds it.
    #[serde(default)]
    pub private_key_wrapped: Option<String>,
    #[serde(default)]
    pub revision_date: Option<String>,
}

/// `GET /uwu/v1/keys`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keys {
    #[serde(default)]
    pub extras_key: Option<WrappedExtrasKey>,
    /// The account's key pair changed in a rotation: neither wrap opens any
    /// more. The person may start over (`DELETE /uwu/v1/keys`).
    #[serde(default)]
    pub lost: bool,
}

/// The body of `POST /uwu/v1/keys` (and of `extrasKey` in
/// `POST /uwu/v1/accounts/rotate-keys`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtrasKeyRequest {
    pub user_key_wrapped: String,
    pub private_key_wrapped: String,
}

/// The body of `PUT /uwu/v1/keys/user-wrap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserWrapRequest {
    pub user_key_wrapped: String,
}

/// The body of `PUT /uwu/v1/keys/private-wrap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateWrapRequest {
    pub private_key_wrapped: String,
}

/// A fresh extras key and both its wraps, for `POST /uwu/v1/keys`.
pub struct NewExtrasKey {
    pub key: SymmetricKey,
    pub request: ExtrasKeyRequest,
}

impl std::fmt::Debug for NewExtrasKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewExtrasKey")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

/// The key `privateKeyWrapped` is under: HKDF-SHA256 over the account's
/// private key as PKCS#8 DER ([`PrivateKey::to_der`], so it doesn't depend on
/// how the client that made the key pair encoded it), salt
/// [`PRIVATE_WRAP_SALT`], info [`PRIVATE_WRAP_INFO`], 64 bytes (32 for
/// AES-256, 32 for HMAC-SHA256).
pub fn private_wrap_key(private: &PrivateKey) -> Result<SymmetricKey, Error> {
    let der = private.to_der()?;
    let hkdf = hkdf::Hkdf::<Sha256>::new(Some(PRIVATE_WRAP_SALT.as_bytes()), &der);
    let mut bytes = Zeroizing::new([0u8; 64]);
    hkdf.expand(PRIVATE_WRAP_INFO.as_bytes(), bytes.as_mut())
        .expect("64 bytes is a valid length");
    SymmetricKey::from_bytes(bytes.as_ref())
}

/// Tells extras keys apart without showing them: the first 16 bytes of
/// HMAC-SHA256 under the key over `uwulock-extras-key-id-v1`, in hex. A
/// client keeps it to notice when the account's extras key is a different one
/// than last time (someone started over).
pub fn key_id(extras: &SymmetricKey) -> String {
    let bytes = extras.to_bytes();
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&bytes).expect("any key length");
    mac.update(b"uwulock-extras-key-id-v1");
    mac.finalize().into_bytes()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Makes an extras key: 64 random bytes, wrapped for the user key and for the
/// account's private key (the profile's `privateKey`, opened).
pub fn create(user_key: &SymmetricKey, private_key: &PrivateKey) -> Result<NewExtrasKey, Error> {
    let key = SymmetricKey::generate();
    let request = wrap(&key, user_key, private_key)?;
    Ok(NewExtrasKey { key, request })
}

/// Both wraps of an existing extras key: for a new user key in a rotation
/// made by UwULock (`POST /uwu/v1/accounts/rotate-keys`).
pub fn wrap(
    extras: &SymmetricKey,
    user_key: &SymmetricKey,
    private_key: &PrivateKey,
) -> Result<ExtrasKeyRequest, Error> {
    Ok(ExtrasKeyRequest {
        user_key_wrapped: EncString::encrypt(&extras.to_bytes(), user_key).to_string(),
        private_key_wrapped: EncString::encrypt(
            &extras.to_bytes(),
            &private_wrap_key(private_key)?,
        )
        .to_string(),
    })
}

/// What a client does with the answer of `GET /uwu/v1/keys`.
#[derive(Debug)]
pub enum Resolved {
    /// There is none yet: `POST /uwu/v1/keys` with `request`. On 409 `exists`
    /// another client was quicker — fetch the keys again and resolve those.
    Create(NewExtrasKey),
    /// The key, opened.
    Open {
        key: SymmetricKey,
        /// An official client rotated the user key: the key was opened with
        /// the private key, and `PUT /uwu/v1/keys/user-wrap` should get this.
        /// A failed PUT costs nothing but doing it again next time.
        rewrap: Option<UserWrapRequest>,
        /// The key has no `privateKeyWrapped` yet (made before it existed):
        /// `PUT /uwu/v1/keys/private-wrap` should get this, so it survives
        /// an official rotation. Best effort too.
        private_wrap: Option<PrivateWrapRequest>,
    },
    /// Nothing opens it here: the key pair was replaced (`lost`), or only a
    /// wrap is left that this client can't open or mustn't trust. The person
    /// is asked whether to start over.
    Lost,
}

/// Opens the extras key, or says how to get one.
///
/// `private_key` is the account's own (the profile's `privateKey` under the
/// user key). It is needed after an official rotation, to make a new key, and
/// to add or check `privateKeyWrapped`. Without it, a key that exists but has
/// no user wrap is [`Resolved::Lost`] for this client only — nothing is
/// written.
///
/// An error, not `Lost`, when a wrap is there but wrong: the user wrap doesn't
/// open under the user key, `privateKeyWrapped` isn't type 2, or the two
/// wraps hold different keys. A server can do that, an honest one doesn't.
pub fn resolve(
    keys: &Keys,
    user_key: &SymmetricKey,
    private_key: Option<&PrivateKey>,
) -> Result<Resolved, Error> {
    if keys.lost {
        return Ok(Resolved::Lost);
    }
    let Some(wrapped) = &keys.extras_key else {
        let private = private_key.ok_or_else(|| {
            Error::Crypto("the account has no key pair to wrap an extras key for".into())
        })?;
        return Ok(Resolved::Create(create(user_key, private)?));
    };
    let bound = private_key.map(private_wrap_key).transpose()?;
    let under_private = wrapped
        .private_key_wrapped
        .as_deref()
        .map(parse_private_wrap)
        .transpose()?;
    if let Some(under_user) = &wrapped.user_key_wrapped {
        let key = under_user.parse::<EncString>()?.decrypt_key(user_key)?;
        let private_wrap = match (&under_private, &bound) {
            (Some(under_private), Some(bound)) => {
                let other = under_private.decrypt_key(bound).map_err(|_| disagree())?;
                if !bool::from(other.to_bytes().as_slice().ct_eq(key.to_bytes().as_slice())) {
                    return Err(disagree());
                }
                None
            }
            (None, Some(bound)) => Some(PrivateWrapRequest {
                private_key_wrapped: EncString::encrypt(&key.to_bytes(), bound).to_string(),
            }),
            (_, None) => None,
        };
        return Ok(Resolved::Open {
            key,
            rewrap: None,
            private_wrap,
        });
    }
    let (Some(under_private), Some(bound)) = (&under_private, &bound) else {
        return Ok(Resolved::Lost);
    };
    let key = under_private.decrypt_key(bound).map_err(|_| {
        Error::Crypto("the extras key doesn't open with this account's private key".into())
    })?;
    let rewrap = UserWrapRequest {
        user_key_wrapped: EncString::encrypt(&key.to_bytes(), user_key).to_string(),
    };
    Ok(Resolved::Open {
        key,
        rewrap: Some(rewrap),
        private_wrap: None,
    })
}

/// `privateKeyWrapped`, which has to be type 2: a type 4 value is what
/// anyone with the public key can make.
fn parse_private_wrap(text: &str) -> Result<EncString, Error> {
    match text.parse::<EncString>()? {
        enc @ EncString::AesCbc256HmacSha256 { .. } => Ok(enc),
        _ => Err(Error::Crypto(
            "the server offered an extras key that isn't bound to this account's private key"
                .into(),
        )),
    }
}

fn disagree() -> Error {
    Error::Crypto(
        "the extras key's wraps don't match this account's keys; the server may have changed \
         one of them"
            .into(),
    )
}

// ── Suite spaces ───────────────────────────────────────────

/// The key of one suite space (UwUSSH's `ssh`, UwURDP's `rdp`, …): 32 random
/// bytes for XChaCha20-Poly1305, kept by the server as a type 2 value under
/// the extras key (`suiteSpace.key`).
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SpaceKey([u8; 32]);

impl std::fmt::Debug for SpaceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SpaceKey(…)")
    }
}

impl SpaceKey {
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        SpaceKey(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
            Error::Crypto(format!(
                "a space key has 32 bytes, this one {}",
                bytes.len()
            ))
        })?;
        Ok(SpaceKey(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The `key` of `PUT /uwu/v1/suite/spaces/{space}`.
    pub fn wrap(&self, extras: &SymmetricKey) -> String {
        EncString::encrypt(&self.0, extras).to_string()
    }

    /// Opens a space's `key`.
    pub fn unwrap(wrapped: &str, extras: &SymmetricKey) -> Result<Self, Error> {
        let bytes = wrapped.parse::<EncString>()?.decrypt(extras)?;
        SpaceKey::from_bytes(&bytes)
    }
}

// ── Own icons ──────────────────────────────────────────────

/// The largest own icon the server takes, as the text of its EncString.
pub const ICON_MAX_TEXT: usize = 96 * 1024;
/// Own icons are at most this many pixels wide and high.
pub const ICON_MAX_PIXELS: u32 = 128;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Width and height of a PNG, from its header. `None` if it isn't one.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || !png.starts_with(PNG_SIGNATURE) || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((width, height))
}

/// An own icon for `PUT /uwu/v1/icons/own/{cipherId}`: a PNG of at most 128 ×
/// 128 pixels as a type 2 value under the extras key (a personal item) or the
/// organisation key (an organisation's item). Resizing is the caller's
/// business; this only refuses what the server would.
pub fn seal_icon(png: &[u8], key: &SymmetricKey) -> Result<String, Error> {
    let (width, height) =
        png_size(png).ok_or_else(|| Error::Crypto("an own icon has to be a PNG".into()))?;
    if width == 0 || height == 0 || width > ICON_MAX_PIXELS || height > ICON_MAX_PIXELS {
        return Err(Error::Crypto(format!(
            "an own icon is at most {ICON_MAX_PIXELS} × {ICON_MAX_PIXELS} pixels, this one \
             {width} × {height}"
        )));
    }
    let sealed = EncString::encrypt(png, key).to_string();
    if sealed.len() > ICON_MAX_TEXT {
        return Err(Error::Crypto(
            "the icon is too large; make it smaller".into(),
        ));
    }
    Ok(sealed)
}

/// Opens an own icon's `data`: the PNG, checked to be one.
pub fn open_icon(data: &str, key: &SymmetricKey) -> Result<Zeroizing<Vec<u8>>, Error> {
    let png = data.parse::<EncString>()?.decrypt(key)?;
    if png_size(&png).is_none() {
        return Err(Error::Crypto("an own icon isn't a PNG".into()));
    }
    Ok(png)
}

// ── Entry versions in a rotation ───────────────────────────

/// Re-encrypts one personal entry version (`cipherVersion.cipher`) from the
/// old user key to the new one, for `POST /uwu/v1/accounts/rotate-keys`.
///
/// A version with its own item key (`key`) only needs that key wrapped again;
/// the fields stay as they are. Without one, every value in it is under the
/// user key, and each is opened and sealed again — whatever the item type,
/// since every string that is a type 2 value is one of its fields (dates,
/// numbers and plain flags are not). A value that doesn't open under the old
/// key is an error: sending it on would lose it.
pub fn reencrypt_version(
    cipher: &Value,
    old_user_key: &SymmetricKey,
    new_user_key: &SymmetricKey,
) -> Result<Value, Error> {
    let mut out = cipher.clone();
    let item_key = ["key", "Key"]
        .iter()
        .find_map(|name| out.get(*name).and_then(Value::as_str).map(str::to_owned));
    if let Some(wrapped) = item_key {
        let key = wrapped.parse::<EncString>()?.decrypt_key(old_user_key)?;
        let again = EncString::encrypt(&key.to_bytes(), new_user_key).to_string();
        let slot = if out.get("key").is_some() {
            "key"
        } else {
            "Key"
        };
        out[slot] = Value::String(again);
        return Ok(out);
    }
    reencrypt_values(&mut out, old_user_key, new_user_key)?;
    Ok(out)
}

fn reencrypt_values(
    value: &mut Value,
    old: &SymmetricKey,
    new: &SymmetricKey,
) -> Result<(), Error> {
    match value {
        Value::String(text) if text.starts_with("2.") => {
            if let Ok(enc) = text.parse::<EncString>() {
                let plain = enc.decrypt(old)?;
                *text = EncString::encrypt(&plain, new).to_string();
            }
        }
        Value::Array(list) => {
            for entry in list {
                reencrypt_values(entry, old, new)?;
            }
        }
        Value::Object(map) => {
            for entry in map.values_mut() {
                reencrypt_values(entry, old, new)?;
            }
        }
        _ => {}
    }
    Ok(())
}
