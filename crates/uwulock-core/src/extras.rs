//! The extras key: what UwULock encrypts beyond Bitwarden's own objects.
//!
//! Suite spaces (UwUSSH, UwURDP), own icons, file-request labels and the
//! health report are under one **extras key** per account, not under the user
//! key. An official Bitwarden client that rotates the user key re-encrypts
//! only what it knows; anything else under the old key would be lost. So the
//! server keeps the extras key wrapped twice (`/uwu/v1/keys`):
//!
//! - `userKeyWrapped`: the 64-byte key as a type 2 value under the user key;
//! - `publicKeyWrapped`: the same key RSA-OAEP-SHA1-wrapped for the account's
//!   public key (type 4), the way an organisation key is wrapped for a member.
//!
//! Bitwarden's clients keep the key pair when they rotate, so after an
//! official rotation the server drops `userKeyWrapped` and the next UwULock
//! client opens the key with the private key and wraps it again for the new
//! user key ([`resolve`] says so with [`Resolved::rewrap`]).
//!
//! Also here: what lives under the extras key or next to it — a suite
//! space's key ([`SpaceKey`]), an own icon ([`seal_icon`]), and re-encrypting
//! entry versions for a rotation made by UwULock itself
//! ([`reencrypt_version`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto::{wrap_for, EncString, PrivateKey, PublicKey, SymmetricKey};
use crate::Error;

/// The extras key as `GET /uwu/v1/keys` hands it out (`extrasKey`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrappedExtrasKey {
    /// `null` after an official client rotated the user key, until a UwULock
    /// client wraps it again.
    #[serde(default)]
    pub user_key_wrapped: Option<String>,
    #[serde(default)]
    pub public_key_wrapped: Option<String>,
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
    pub public_key_wrapped: String,
}

/// The body of `PUT /uwu/v1/keys/user-wrap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserWrapRequest {
    pub user_key_wrapped: String,
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

/// Makes an extras key: 64 random bytes, wrapped for the user key and for the
/// account's public key (SPKI DER, from the token response's
/// `AccountKeys.publicKeyEncryptionKeyPair.publicKey` or from
/// `PrivateKey::public`).
pub fn create(user_key: &SymmetricKey, public_key: &PublicKey) -> Result<NewExtrasKey, Error> {
    let key = SymmetricKey::generate();
    let request = wrap(&key, user_key, public_key)?;
    Ok(NewExtrasKey { key, request })
}

/// Both wraps of an existing extras key: for a new user key in a rotation
/// made by UwULock (`POST /uwu/v1/accounts/rotate-keys`), or when the key
/// pair changed.
pub fn wrap(
    extras: &SymmetricKey,
    user_key: &SymmetricKey,
    public_key: &PublicKey,
) -> Result<ExtrasKeyRequest, Error> {
    Ok(ExtrasKeyRequest {
        user_key_wrapped: EncString::encrypt(&extras.to_bytes(), user_key).to_string(),
        public_key_wrapped: wrap_for(public_key, extras)?.to_string(),
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
    },
    /// Neither wrap opens any more (`lost`): the key pair was replaced. The
    /// person is asked whether to start over.
    Lost,
}

/// Opens the extras key, or says how to get one.
///
/// `private_key` is the account's own (the profile's `privateKey` under the
/// user key); it is only needed after an official rotation, and to make a new
/// key (its public half is what the second wrap is for). Without it, a key
/// that exists but has no user wrap is [`Resolved::Lost`] for this client
/// only — nothing is written.
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
        return Ok(Resolved::Create(create(user_key, &private.public())?));
    };
    if let Some(under_user) = &wrapped.user_key_wrapped {
        let key = under_user.parse::<EncString>()?.decrypt_key(user_key)?;
        return Ok(Resolved::Open { key, rewrap: None });
    }
    let (Some(under_public), Some(private)) = (&wrapped.public_key_wrapped, private_key) else {
        return Ok(Resolved::Lost);
    };
    let key = under_public
        .parse::<EncString>()?
        .decrypt_key_rsa(private)
        .map_err(|_| {
            Error::Crypto("the extras key doesn't open with this account's private key".into())
        })?;
    let rewrap = UserWrapRequest {
        user_key_wrapped: EncString::encrypt(&key.to_bytes(), user_key).to_string(),
    };
    Ok(Resolved::Open {
        key,
        rewrap: Some(rewrap),
    })
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
