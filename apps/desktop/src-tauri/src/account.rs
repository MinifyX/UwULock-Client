//! What UwULock keeps on disk, in the app's data folder
//! (`%APPDATA%\app.uwulock.desktop` on Windows):
//!
//! - `account.json` — the server, the email, how the master key is derived,
//!   and three encrypted values: the user key (wrapped by the server, under
//!   the master password), and the refresh token and the "remember this
//!   device" token, both sealed here under the user key. Without the master
//!   password, nothing in it opens a session.
//! - `vault.json` — the last sync, exactly as the server sent it. Every name,
//!   username, password and note in it is still encrypted by Bitwarden; that
//!   is what lets the vault open offline.
//! - `device-id` — this installation's device id, kept across logouts so
//!   Bitwarden doesn't meet a "new device" every time.
//!
//! `UWULOCK_DATA_DIR` points all of it elsewhere, so trying things out never
//! touches the real account.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uwulock_bitwarden::{EncString, Kdf, Server, SymmetricKey};
use zeroize::Zeroizing;

const ACCOUNT: &str = "account.json";
const CACHE: &str = "vault.json";
const DEVICE: &str = "device-id";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub version: u32,
    pub server: Server,
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    pub kdf: Kdf,
    /// The user key as the server wraps it, under the stretched master key.
    pub protected_user_key: String,
    /// Sealed under the user key.
    #[serde(default)]
    pub protected_refresh_token: Option<String>,
    /// Sealed under the user key.
    #[serde(default)]
    pub protected_remember_token: Option<String>,
    /// Unix seconds of the last successful sync.
    #[serde(default)]
    pub last_sync: Option<u64>,
}

impl Account {
    pub fn seal(value: &str, key: &SymmetricKey) -> String {
        EncString::encrypt(value.as_bytes(), key).to_string()
    }

    pub fn unseal(value: &Option<String>, key: &SymmetricKey) -> Option<Zeroizing<String>> {
        let text = value.as_ref()?;
        match text
            .parse::<EncString>()
            .and_then(|e| e.decrypt_string(key))
        {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(%error, "a sealed value didn't open");
                None
            }
        }
    }
}

pub struct Storage {
    dir: PathBuf,
}

impl Storage {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Storage { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn load_account(&self) -> Option<Account> {
        let text = std::fs::read_to_string(self.dir.join(ACCOUNT)).ok()?;
        match serde_json::from_str(&text) {
            Ok(account) => Some(account),
            Err(error) => {
                tracing::warn!(%error, "account.json doesn't parse");
                None
            }
        }
    }

    pub fn save_account(&self, account: &Account) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(account).map_err(std::io::Error::other)?;
        write_atomic(&self.dir.join(ACCOUNT), text.as_bytes())
    }

    pub fn load_cache(&self) -> Option<String> {
        std::fs::read_to_string(self.dir.join(CACHE)).ok()
    }

    pub fn save_cache(&self, sync: &str) -> std::io::Result<()> {
        write_atomic(&self.dir.join(CACHE), sync.as_bytes())
    }

    /// Logging out: the account and the cached vault go, the device id stays.
    pub fn forget(&self) -> std::io::Result<()> {
        for name in [ACCOUNT, CACHE] {
            match std::fs::remove_file(self.dir.join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    pub fn device_id(&self) -> String {
        let path = self.dir.join(DEVICE);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(id) = uuid::Uuid::parse_str(text.trim()) {
                return id.to_string();
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        if let Err(error) = write_atomic(&path, id.as_bytes()) {
            tracing::warn!(%error, "couldn't keep the device id");
        }
        id
    }
}

/// Written next to the target, then renamed over it: a crash in between
/// leaves the old file, never half a new one.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_round_trip_and_forget() {
        let dir = std::env::temp_dir().join(format!("uwulock-test-{}", uuid::Uuid::new_v4()));
        let storage = Storage::new(dir.clone()).unwrap();
        let key = SymmetricKey::generate();
        let account = Account {
            version: 1,
            server: Server::self_hosted("vault.example.org").unwrap(),
            email: "nyu@example.org".into(),
            name: None,
            kdf: Kdf::Pbkdf2 {
                iterations: 600_000,
            },
            protected_user_key: "2.x|y|z".into(),
            protected_refresh_token: Some(Account::seal("refresh", &key)),
            protected_remember_token: None,
            last_sync: None,
        };
        storage.save_account(&account).unwrap();
        storage.save_cache("{}").unwrap();
        let id = storage.device_id();
        let loaded = storage.load_account().unwrap();
        assert_eq!(
            Account::unseal(&loaded.protected_refresh_token, &key)
                .unwrap()
                .as_str(),
            "refresh"
        );
        storage.forget().unwrap();
        assert!(storage.load_account().is_none());
        assert!(storage.load_cache().is_none());
        assert_eq!(storage.device_id(), id);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
