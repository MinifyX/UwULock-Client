//! What UwULock keeps on disk, in the app's data folder
//! (`%APPDATA%\app.uwulock.desktop` on Windows).
//!
//! Every account has a folder of its own under `accounts/`, named by an id
//! this app made up, so a private Vaultwarden and one at work never share a
//! file:
//!
//! - `accounts/<id>/account.json` — the server, the email, how the master key
//!   is derived, and three encrypted values: the user key (wrapped by the
//!   server, under the master password), and the refresh token and the
//!   "remember this device" token, both sealed here under the user key.
//!   Without that account's master password, nothing in it opens a session.
//! - `accounts/<id>/vault.json` — the last sync, exactly as the server sent
//!   it. Every name, username, password and note in it is still encrypted by
//!   Bitwarden; that is what lets the vault open offline.
//! - `accounts.json` — which accounts there are, in which order, and which one
//!   was open last. Nothing secret.
//! - `device-id` — this installation's device id, kept across logouts and
//!   shared by every account, so Bitwarden doesn't meet a "new device" every
//!   time.
//!
//! A folder from before UwULock knew more than one account (`account.json` and
//! `vault.json` next to `device-id`) is moved into `accounts/` on the first
//! start, so nobody has to log in again.
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
const INDEX: &str = "accounts.json";
const ACCOUNTS: &str = "accounts";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub version: u32,
    pub server: Server,
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    /// What this account is called in the app — "Privat", "Arbeit". Chosen by
    /// whoever added it; the server's address when they didn't.
    #[serde(default)]
    pub label: Option<String>,
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

    /// What to call this account in the window.
    pub fn title(&self) -> String {
        self.label
            .clone()
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| self.server.label())
    }
}

/// One account, with the id its folder is named after.
#[derive(Debug, Clone)]
pub struct Stored {
    pub id: String,
    pub account: Account,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Index {
    #[serde(default)]
    version: u32,
    /// The account that was open last.
    #[serde(default)]
    active: Option<String>,
    /// The order they are shown in.
    #[serde(default)]
    order: Vec<String>,
}

pub struct Storage {
    dir: PathBuf,
}

impl Storage {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir.join(ACCOUNTS))?;
        let storage = Storage { dir };
        storage.adopt_single_account()?;
        Ok(storage)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The account folder from before this app knew more than one, moved in.
    fn adopt_single_account(&self) -> std::io::Result<()> {
        let old = self.dir.join(ACCOUNT);
        if !old.exists() {
            return Ok(());
        }
        let id = new_id();
        let home = self.dir.join(ACCOUNTS).join(&id);
        std::fs::create_dir_all(&home)?;
        std::fs::rename(&old, home.join(ACCOUNT))?;
        let cache = self.dir.join(CACHE);
        if cache.exists() {
            std::fs::rename(&cache, home.join(CACHE))?;
        }
        self.save_index(&Index {
            version: 1,
            active: Some(id.clone()),
            order: vec![id],
        })?;
        tracing::info!("moved the account into its own folder");
        Ok(())
    }

    fn index(&self) -> Index {
        std::fs::read_to_string(self.dir.join(INDEX))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_index(&self, index: &Index) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(index).map_err(std::io::Error::other)?;
        write_atomic(&self.dir.join(INDEX), text.as_bytes())
    }

    fn home(&self, id: &str) -> PathBuf {
        self.dir.join(ACCOUNTS).join(id)
    }

    /// Every account on this device, in the order they are shown. An id in the
    /// index without a folder is dropped; a folder the index doesn't know is
    /// kept, at the end.
    pub fn accounts(&self) -> Vec<Stored> {
        let mut ids = self.index().order;
        if let Ok(entries) = std::fs::read_dir(self.dir.join(ACCOUNTS)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !ids.contains(&name) {
                    ids.push(name);
                }
            }
        }
        ids.into_iter()
            .filter_map(|id| {
                let account = self.load_account(&id)?;
                Some(Stored { id, account })
            })
            .collect()
    }

    pub fn load_account(&self, id: &str) -> Option<Account> {
        let text = std::fs::read_to_string(self.home(id).join(ACCOUNT)).ok()?;
        match serde_json::from_str(&text) {
            Ok(account) => Some(account),
            Err(error) => {
                tracing::warn!(%error, account = %id, "account.json doesn't parse");
                None
            }
        }
    }

    pub fn save_account(&self, id: &str, account: &Account) -> std::io::Result<()> {
        let home = self.home(id);
        std::fs::create_dir_all(&home)?;
        let text = serde_json::to_string_pretty(account).map_err(std::io::Error::other)?;
        write_atomic(&home.join(ACCOUNT), text.as_bytes())?;
        let mut index = self.index();
        if !index.order.iter().any(|known| known == id) {
            index.order.push(id.to_string());
            index.version = 1;
            self.save_index(&index)?;
        }
        Ok(())
    }

    /// A fresh id for an account that isn't on this device yet. The same
    /// server and email keep the id they already have, so logging in again
    /// doesn't leave a second copy behind.
    pub fn id_for(&self, server: &Server, email: &str) -> String {
        self.accounts()
            .into_iter()
            .find(|stored| &stored.account.server == server && stored.account.email == email)
            .map(|stored| stored.id)
            .unwrap_or_else(new_id)
    }

    pub fn active(&self) -> Option<String> {
        let index = self.index();
        let active = index.active?;
        self.load_account(&active).is_some().then_some(active)
    }

    pub fn set_active(&self, id: Option<&str>) -> std::io::Result<()> {
        let mut index = self.index();
        index.version = 1;
        index.active = id.map(str::to_string);
        self.save_index(&index)
    }

    pub fn load_cache(&self, id: &str) -> Option<String> {
        std::fs::read_to_string(self.home(id).join(CACHE)).ok()
    }

    pub fn save_cache(&self, id: &str, sync: &str) -> std::io::Result<()> {
        let home = self.home(id);
        std::fs::create_dir_all(&home)?;
        write_atomic(&home.join(CACHE), sync.as_bytes())
    }

    /// Logging out of one account: its folder goes, the device id and the
    /// other accounts stay.
    pub fn forget(&self, id: &str) -> std::io::Result<()> {
        match std::fs::remove_dir_all(self.home(id)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut index = self.index();
        index.version = 1;
        index.order.retain(|known| known != id);
        if index.active.as_deref() == Some(id) {
            index.active = index.order.first().cloned();
        }
        self.save_index(&index)
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

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
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

    fn sample(email: &str) -> Account {
        Account {
            version: 1,
            server: Server::self_hosted("vault.example.org").unwrap(),
            email: email.into(),
            name: None,
            label: None,
            kdf: Kdf::Pbkdf2 {
                iterations: 600_000,
            },
            protected_user_key: "2.x|y|z".into(),
            protected_refresh_token: None,
            protected_remember_token: None,
            last_sync: None,
        }
    }

    fn scratch() -> PathBuf {
        std::env::temp_dir().join(format!("uwulock-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn accounts_live_side_by_side() {
        let dir = scratch();
        let storage = Storage::new(dir.clone()).unwrap();
        let key = SymmetricKey::generate();

        let mut private = sample("nyu@example.org");
        private.protected_refresh_token = Some(Account::seal("refresh", &key));
        private.label = Some("Privat".into());
        let private_id = storage.id_for(&private.server, &private.email);
        storage.save_account(&private_id, &private).unwrap();
        storage.save_cache(&private_id, "{\"private\":1}").unwrap();

        let work = sample("lorin@work.example");
        let work_id = storage.id_for(&work.server, &work.email);
        storage.save_account(&work_id, &work).unwrap();
        storage.save_cache(&work_id, "{\"work\":1}").unwrap();
        storage.set_active(Some(&work_id)).unwrap();

        assert_ne!(private_id, work_id);
        // Logging in again with the same email keeps the same folder.
        assert_eq!(storage.id_for(&private.server, &private.email), private_id);

        let ids: Vec<String> = storage.accounts().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, [private_id.clone(), work_id.clone()]);
        assert_eq!(storage.active().as_deref(), Some(work_id.as_str()));
        assert_eq!(
            Account::unseal(
                &storage
                    .load_account(&private_id)
                    .unwrap()
                    .protected_refresh_token,
                &key
            )
            .unwrap()
            .as_str(),
            "refresh"
        );
        assert_eq!(storage.load_cache(&work_id).unwrap(), "{\"work\":1}");
        assert_eq!(storage.load_account(&private_id).unwrap().title(), "Privat");

        // Logging out of the open one leaves the other, and opens it.
        let device = storage.device_id();
        storage.forget(&work_id).unwrap();
        assert!(storage.load_account(&work_id).is_none());
        assert!(storage.load_cache(&work_id).is_none());
        assert_eq!(storage.active().as_deref(), Some(private_id.as_str()));
        assert_eq!(storage.accounts().len(), 1);
        assert_eq!(storage.device_id(), device);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_older_single_account_folder_is_moved_in() {
        let dir = scratch();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(ACCOUNT),
            serde_json::to_string(&sample("old@example.org")).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join(CACHE), "{\"old\":1}").unwrap();
        std::fs::write(dir.join(DEVICE), uuid::Uuid::new_v4().to_string()).unwrap();

        let storage = Storage::new(dir.clone()).unwrap();
        let accounts = storage.accounts();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].account.email, "old@example.org");
        assert_eq!(storage.active().as_deref(), Some(accounts[0].id.as_str()));
        assert_eq!(storage.load_cache(&accounts[0].id).unwrap(), "{\"old\":1}");
        assert!(!dir.join(ACCOUNT).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
