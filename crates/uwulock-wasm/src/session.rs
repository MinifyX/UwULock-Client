//! Unlocking and locking: with the master password, a PIN, or the user key
//! the extension kept while the browser runs.
//!
//! The server never sees the master password, only its hash. A PIN never
//! leaves the device at all: it opens a copy of the user key the extension
//! keeps, the way Bitwarden's "unlock with PIN" does.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::collections::HashSet;
use uwulock_core::crypto::{self, EncString, Kdf, SymmetricKey};
use uwulock_core::vault::Vault;
use uwulock_core::wire;
use zeroize::Zeroizing;

use crate::{kdf_from, with_unlocked, Failure, Result, Unlocked, PENDING, UNLOCKED};

fn wrong_password() -> Failure {
    Failure::new("wrong-password", "The master password is wrong.")
}

/// The master key from the password, kept for [`unlock`]; the hash the
/// server gets.
pub fn derive_login(email: &str, password: &str, kdf: &str) -> Result<String> {
    let kdf = kdf_from(kdf)?;
    let master = crypto::master_key(password, email, kdf)?;
    let hash = crypto::master_password_hash(&master, password);
    PENDING.with(|cell| *cell.borrow_mut() = Some(master));
    Ok(hash)
}

/// Opens the user key with the master key [`derive_login`] kept.
pub fn unlock(email: &str, kdf: &str, protected_key: &str) -> Result<()> {
    let kdf = kdf_from(kdf)?;
    let master = PENDING
        .with(|cell| cell.borrow_mut().take())
        .ok_or_else(|| Failure::new("locked", "Log in first."))?;
    let user_key = open_user_key(&master, protected_key)?;
    unlock_with(email, kdf, protected_key, user_key);
    Ok(())
}

pub fn unlock_with_password(
    email: &str,
    kdf: &str,
    protected_key: &str,
    password: &str,
) -> Result<String> {
    let kdf = kdf_from(kdf)?;
    let master = crypto::master_key(password, email, kdf)?;
    let user_key = open_user_key(&master, protected_key)?;
    unlock_with(email, kdf, protected_key, user_key);
    Ok(crypto::master_password_hash(&master, password))
}

pub fn unlock_with_key(email: &str, kdf: &str, protected_key: &str, user_key: &str) -> Result<()> {
    let kdf = kdf_from(kdf)?;
    let bytes = STANDARD
        .decode(user_key.trim())
        .map(Zeroizing::new)
        .map_err(|_| Failure::new("invalid", "The user key is not base64."))?;
    let user_key = SymmetricKey::from_bytes(&bytes)?;
    unlock_with(email, kdf, protected_key, user_key);
    Ok(())
}

pub fn user_key() -> Result<String> {
    with_unlocked(|unlocked| Ok(STANDARD.encode(unlocked.user_key.to_bytes().as_slice())))
}

/// The key a PIN makes: derived like a master key, with the account's
/// address and KDF, and stretched. What Bitwarden calls the PIN key.
fn pin_key(pin: &str, email: &str, kdf: Kdf) -> Result<SymmetricKey> {
    if pin.is_empty() {
        return Err(Failure::new("invalid", "A PIN can't be empty."));
    }
    let master = crypto::master_key(pin, email, kdf)?;
    Ok(SymmetricKey::stretch(&master))
}

pub fn pin_protect(pin: &str) -> Result<String> {
    with_unlocked(|unlocked| {
        let key = pin_key(pin, &unlocked.email, unlocked.kdf)?;
        Ok(EncString::encrypt(&unlocked.user_key.to_bytes(), &key).to_string())
    })
}

pub fn unlock_with_pin(
    email: &str,
    kdf: &str,
    protected_key: &str,
    pin: &str,
    protected: &str,
) -> Result<()> {
    let kdf = kdf_from(kdf)?;
    let key = pin_key(pin, email, kdf)?;
    let user_key = protected
        .parse::<EncString>()?
        .decrypt_key(&key)
        .map_err(|_| Failure::new("wrong-password", "The PIN is wrong."))?;
    unlock_with(email, kdf, protected_key, user_key);
    Ok(())
}

fn open_user_key(master: &[u8; 32], protected_key: &str) -> Result<SymmetricKey> {
    let protected: EncString = protected_key.parse()?;
    crypto::decrypt_user_key(master, &protected).map_err(|_| wrong_password())
}

/// The vault is open with `user_key`, however it was got. What was open
/// before is gone, the master key of a login in between too.
fn unlock_with(email: &str, kdf: Kdf, protected_key: &str, user_key: SymmetricKey) {
    PENDING.with(|cell| *cell.borrow_mut() = None);
    UNLOCKED.with(|cell| {
        *cell.borrow_mut() = Some(Unlocked {
            email: crypto::normalize_email(email),
            kdf,
            protected_key: protected_key.to_string(),
            user_key,
            vault: Vault::default(),
            reprompt_ok: HashSet::new(),
            private_key: None,
            extras: None,
        })
    });
}

pub fn lock() {
    PENDING.with(|cell| *cell.borrow_mut() = None);
    UNLOCKED.with(|cell| *cell.borrow_mut() = None);
}

pub fn is_unlocked() -> bool {
    UNLOCKED.with(|cell| cell.borrow().is_some())
}

/// The hash of `password`, if it is the master password: it has to open the
/// user key.
pub fn check_password(unlocked: &Unlocked, password: &str) -> Result<String> {
    let master = crypto::master_key(password, &unlocked.email, unlocked.kdf)?;
    open_user_key(&master, &unlocked.protected_key)?;
    Ok(crypto::master_password_hash(&master, password))
}

pub fn verify_reprompt(id: &str, password: &str) -> Result<()> {
    with_unlocked(|unlocked| {
        check_password(unlocked, password)?;
        unlocked.reprompt_ok.insert(id.to_string());
        Ok(())
    })
}

/// Opens a sync. Answered re-prompts stay answered for items that are still
/// there.
pub fn open(sync: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(sync)?;
    let sync: wire::Sync = serde_json::from_value(wire::lowercase_keys(value))?;
    with_unlocked(|unlocked| {
        unlocked.vault = Vault::open(&sync, &unlocked.user_key)?;
        if let Some(key) = sync.profile.key.clone() {
            unlocked.protected_key = key;
        }
        unlocked.private_key = sync.profile.private_key.clone();
        let vault = &unlocked.vault;
        unlocked.reprompt_ok.retain(|id| vault.item(id).is_some());
        Ok(())
    })
}

pub fn encrypt_text(text: &str) -> Result<String> {
    with_unlocked(
        |unlocked| Ok(EncString::encrypt(text.as_bytes(), &unlocked.user_key).to_string()),
    )
}
