//! What UwULock keeps on disk, in the app's data folder
//! (`%APPDATA%\app.uwulock.desktop` on Windows).
//!
//! Every account has a folder of its own under `accounts/`, named by an id
//! this app made up, so a private Vaultwarden and one at work never share a
//! file:
//!
//! - `accounts/<id>/account.json` — the server, the email, how the master key
//!   is derived, and two encrypted values: the user key (wrapped by the
//!   server, under the master password), and the refresh token, sealed here
//!   under the user key. Without that account's master password, nothing in
//!   it opens a session.
//! - `accounts/<id>/remember-token` — the "remember this device" token of
//!   two-step login. Not under the user key: a login after the session ended,
//!   the app restarted or the vault locked has no user key yet, and is exactly
//!   when the token is needed. Under DPAPI on Windows (this Windows user
//!   only); elsewhere readable only by this user (0600), inside the app's own
//!   data folder (on phones its sandbox) — as Bitwarden's own apps keep it.
//!   It only skips the second step: the master password is still needed.
//!   Logging out removes it with the folder; nothing else does. (Up to
//!   0.5.0-beta.1 it was sealed under the user key in `account.json`; the
//!   first unlock moves it here.)
//! - `accounts/<id>/vault.json` — the last sync, exactly as the server sent
//!   it. Every name, username, password and note in it is still encrypted by
//!   Bitwarden; that is what lets the vault open offline.
//!   On a UwULock Server with delta sync, the same file also carries
//!   `uwuLock`: the cursor of the last delta and UwULock's own state (own
//!   icons, reminders, masked addresses, travel mode…), that state sealed
//!   under the user key. The vault part stays exactly `/api/sync`'s shape.
//! - `accounts/<id>/move-journal.json` — after a move from Bitwarden into
//!   this account: which source object became which here (ids only), sealed
//!   under the user key, so a second move carries only what is new.
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
use uwulock_bitwarden::delta::{Synced, UwuState};
use uwulock_bitwarden::{EncString, Kdf, Server, SymmetricKey};
use zeroize::Zeroizing;

const ACCOUNT: &str = "account.json";
const CACHE: &str = "vault.json";
const DEVICE: &str = "device-id";
const INDEX: &str = "accounts.json";
const ACCOUNTS: &str = "accounts";
const MOVE_JOURNAL: &str = "move-journal.json";
const REMEMBER: &str = "remember-token";
/// How the remember token is kept: under DPAPI, or as it is.
const REMEMBER_DPAPI: &str = "dpapi:";
const REMEMBER_PLAIN: &str = "plain:";
/// The key of UwULock's part in the cached sync.
const UWU: &str = "uwuLock";

/// The cached sync as a [`Synced`]: its vault, and — if a delta sync wrote it
/// — the cursor and UwULock's state. A cache from a plain `/api/sync` has no
/// cursor, so the next delta sync starts with a full one.
pub fn synced_from_cache(text: &str, user_key: &SymmetricKey) -> Option<Synced> {
    let mut sync: serde_json::Value = serde_json::from_str(text).ok()?;
    let extra = sync.as_object_mut()?.remove(UWU);
    let mut synced = Synced::from_full_sync(sync);
    if let Some(extra) = extra {
        let state = extra
            .get("state")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let state = Account::unseal(&state, user_key)
            .and_then(|text| serde_json::from_str::<UwuState>(&text).ok());
        // UwULock's state that doesn't open means starting over from a full
        // sync, not a delta on top of a hole.
        if let Some(state) = state {
            synced.uwu = state;
            synced.cursor = extra
                .get("cursor")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
        }
    }
    Some(synced)
}

/// What [`Storage::save_cache`] writes for a [`Synced`].
pub fn synced_to_cache(synced: &Synced, user_key: &SymmetricKey) -> String {
    let mut sync = synced.sync.clone();
    if let Some(object) = sync.as_object_mut() {
        let state = serde_json::to_string(&synced.uwu).unwrap_or_default();
        object.insert(
            UWU.into(),
            serde_json::json!({
                "cursor": synced.cursor,
                "state": Account::seal(&state, user_key),
            }),
        );
    }
    sync.to_string()
}

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
    /// Up to 0.5.0-beta.1: the remember token, sealed under the user key.
    /// Now in `remember-token` ([`Storage::load_remember_token`]); an old
    /// one is moved there at the next unlock and this stays empty.
    #[serde(default)]
    pub protected_remember_token: Option<String>,
    /// Unix seconds of the last successful sync.
    #[serde(default)]
    pub last_sync: Option<u64>,
    /// The user key sealed under what Windows Hello signs (`hello`), when
    /// unlocking with Windows Hello is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hello_user_key: Option<String>,
    /// Which extras key this device last took for the account
    /// (`extras::key_id`, nothing secret). A different one means somebody
    /// started over, or the server lost it: the app says so before taking it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras_key_id: Option<String>,
    /// This device's id in the clocks of UwUSSH's and UwURDP's records that
    /// UwULock edits (random, never 0; nothing secret).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite_device: Option<u32>,
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

    /// An account's folder. Only an id this app could have made gets one:
    /// the id comes back from the page, and `..`, an empty id or an absolute
    /// path would point `forget` at a folder that isn't an account's.
    fn home(&self, id: &str) -> std::io::Result<PathBuf> {
        if !is_account_id(id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "not an account id",
            ));
        }
        Ok(self.dir.join(ACCOUNTS).join(id))
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
        let text = std::fs::read_to_string(self.home(id).ok()?.join(ACCOUNT)).ok()?;
        match serde_json::from_str(&text) {
            Ok(account) => Some(account),
            Err(error) => {
                tracing::warn!(%error, account = %id, "account.json doesn't parse");
                None
            }
        }
    }

    pub fn save_account(&self, id: &str, account: &Account) -> std::io::Result<()> {
        let home = self.home(id)?;
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
        std::fs::read_to_string(self.home(id).ok()?.join(CACHE)).ok()
    }

    pub fn save_cache(&self, id: &str, sync: &str) -> std::io::Result<()> {
        let home = self.home(id)?;
        std::fs::create_dir_all(&home)?;
        write_atomic(&home.join(CACHE), sync.as_bytes())
    }

    /// The journal of moves into this account (`move-journal.json`): which
    /// object of which source became which here, sealed under the user key.
    pub fn load_move_journal(&self, id: &str) -> Option<String> {
        std::fs::read_to_string(self.home(id).ok()?.join(MOVE_JOURNAL)).ok()
    }

    pub fn save_move_journal(&self, id: &str, sealed: &str) -> std::io::Result<()> {
        let home = self.home(id)?;
        std::fs::create_dir_all(&home)?;
        write_atomic(&home.join(MOVE_JOURNAL), sealed.as_bytes())
    }

    /// The "remember this device" token of two-step login for this account,
    /// readable without its user key (see the top of this file).
    pub fn load_remember_token(&self, id: &str) -> Option<Zeroizing<String>> {
        let text =
            Zeroizing::new(std::fs::read_to_string(self.home(id).ok()?.join(REMEMBER)).ok()?);
        let token = if let Some(plain) = text.strip_prefix(REMEMBER_PLAIN) {
            Zeroizing::new(plain.to_string())
        } else {
            let sealed = text.strip_prefix(REMEMBER_DPAPI)?;
            match unprotect(sealed.trim()) {
                Ok(token) => token,
                Err(error) => {
                    tracing::warn!(%error, "the remember token didn't open");
                    return None;
                }
            }
        };
        (!token.is_empty()).then_some(token)
    }

    /// Keeps a new remember token, replacing the old one.
    pub fn save_remember_token(&self, id: &str, token: &str) -> std::io::Result<()> {
        let home = self.home(id)?;
        std::fs::create_dir_all(&home)?;
        let text = Zeroizing::new(protect(token)?);
        write_private(&home.join(REMEMBER), text.as_bytes())
    }

    /// The server no longer takes it: the next login asks for the code again.
    pub fn forget_remember_token(&self, id: &str) -> std::io::Result<()> {
        match std::fs::remove_file(self.home(id)?.join(REMEMBER)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Logging out of one account: its folder goes (its remember token with
    /// it), the device id and the other accounts stay.
    pub fn forget(&self, id: &str) -> std::io::Result<()> {
        // The token first, on its own: should the folder only partly go (a
        // file held open on Windows), the device is still forgotten.
        self.forget_remember_token(id)?;
        match std::fs::remove_dir_all(self.home(id)?) {
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

/// What `new_id` makes: a UUID in its 36-character form with hyphens, and
/// nothing a path could be built from.
fn is_account_id(id: &str) -> bool {
    id.len() == 36 && uuid::Uuid::try_parse(id).is_ok()
}

/// What `remember-token` holds for `token`: under DPAPI on Windows.
#[cfg(windows)]
fn protect(token: &str) -> std::io::Result<String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let sealed = crate::hello::protect_at_rest(token.as_bytes()).map_err(std::io::Error::other)?;
    Ok(format!("{REMEMBER_DPAPI}{}", B64.encode(sealed)))
}

/// Elsewhere the file itself is the protection (0600, the app's own folder).
#[cfg(not(windows))]
fn protect(token: &str) -> std::io::Result<String> {
    Ok(format!("{REMEMBER_PLAIN}{token}"))
}

#[cfg(windows)]
fn unprotect(sealed: &str) -> Result<Zeroizing<String>, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let bytes = B64.decode(sealed).map_err(|e| e.to_string())?;
    let opened = crate::hello::unprotect_at_rest(&bytes)?;
    String::from_utf8(opened.to_vec())
        .map(Zeroizing::new)
        .map_err(|e| e.to_string())
}

/// A DPAPI file brought over from Windows doesn't open anywhere else.
#[cfg(not(windows))]
fn unprotect(_sealed: &str) -> Result<Zeroizing<String>, String> {
    Err("DPAPI is only on Windows".into())
}

/// Like [`write_atomic`], readable by this user only where files have modes.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
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
            hello_user_key: None,
            extras_key_id: None,
            suite_device: None,
        }
    }

    fn scratch() -> PathBuf {
        std::env::temp_dir().join(format!("uwulock-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn a_move_journal_lives_and_goes_with_its_account() {
        let dir = scratch();
        let storage = Storage::new(dir.clone()).unwrap();
        let account = sample("nyu@example.org");
        let id = storage.id_for(&account.server, &account.email);
        storage.save_account(&id, &account).unwrap();
        assert!(storage.load_move_journal(&id).is_none());
        storage.save_move_journal(&id, "2.x|y|z").unwrap();
        assert_eq!(storage.load_move_journal(&id).as_deref(), Some("2.x|y|z"));
        assert!(storage
            .save_move_journal("../elsewhere", "2.x|y|z")
            .is_err());
        storage.forget(&id).unwrap();
        assert!(storage.load_move_journal(&id).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_remember_token_is_kept_privately_and_goes_with_its_account() {
        let dir = scratch();
        let storage = Storage::new(dir.clone()).unwrap();
        let account = sample("nyu@example.org");
        let id = storage.id_for(&account.server, &account.email);
        storage.save_account(&id, &account).unwrap();
        assert!(storage.load_remember_token(&id).is_none());
        storage.save_remember_token(&id, "remember-1").unwrap();
        storage.save_remember_token(&id, "remember-2").unwrap();
        assert_eq!(
            storage.load_remember_token(&id).unwrap().as_str(),
            "remember-2"
        );
        // Not in account.json, and only this user may read the file.
        let path = dir.join(ACCOUNTS).join(&id).join(REMEMBER);
        assert!(
            !std::fs::read_to_string(dir.join(ACCOUNTS).join(&id).join(ACCOUNT))
                .unwrap()
                .contains("remember-2")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // Something this build can't read is no token, not an error.
        std::fs::write(&path, "garbage").unwrap();
        assert!(storage.load_remember_token(&id).is_none());
        storage.save_remember_token(&id, "remember-3").unwrap();
        storage.forget_remember_token(&id).unwrap();
        storage.forget_remember_token(&id).unwrap();
        assert!(storage.load_remember_token(&id).is_none());
        assert!(storage.save_remember_token("../elsewhere", "x").is_err());
        // Logging out takes it along with the folder.
        storage.save_remember_token(&id, "remember-4").unwrap();
        storage.forget(&id).unwrap();
        assert!(storage.load_remember_token(&id).is_none());
        let _ = std::fs::remove_dir_all(dir);
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

    /// Byte for byte what 0.1.0-beta.1 wrote, so a field that quietly gets
    /// another name is caught before an update loses somebody's login.
    const ACCOUNT_0_1: &str = r#"{
  "version": 1,
  "server": {
    "kind": "self-hosted",
    "url": "https://vault.example.org"
  },
  "email": "nyu@example.org",
  "name": "Nyu",
  "kdf": {
    "type": "pbkdf2",
    "iterations": 600000
  },
  "protectedUserKey": "2.aXY=|ZGF0YQ==|bWFj",
  "protectedRefreshToken": "2.aXY=|cmVmcmVzaA==|bWFj",
  "protectedRememberToken": null,
  "lastSync": 1758556800
}"#;

    #[test]
    fn an_account_from_the_first_beta_still_opens() {
        let dir = scratch();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(ACCOUNT), ACCOUNT_0_1).unwrap();
        std::fs::write(dir.join(CACHE), "{\"ciphers\":[]}").unwrap();

        let storage = Storage::new(dir.clone()).unwrap();
        let accounts = storage.accounts();
        assert_eq!(accounts.len(), 1);
        let account = &accounts[0].account;
        assert_eq!(account.email, "nyu@example.org");
        assert_eq!(account.name.as_deref(), Some("Nyu"));
        assert_eq!(
            account.server,
            Server::self_hosted("vault.example.org").unwrap()
        );
        assert_eq!(
            account.kdf,
            Kdf::Pbkdf2 {
                iterations: 600_000
            }
        );
        assert_eq!(account.protected_user_key, "2.aXY=|ZGF0YQ==|bWFj");
        assert!(account.protected_refresh_token.is_some());
        assert_eq!(account.last_sync, Some(1_758_556_800));
        // No label yet in that version: the server's address stands in.
        assert_eq!(account.label, None);
        assert_eq!(account.title(), "vault.example.org");
        // And the same account keeps its folder when it logs in again.
        assert_eq!(
            storage.id_for(&account.server, &account.email),
            accounts[0].id
        );
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

    #[test]
    fn the_delta_cursor_and_uwulocks_state_travel_with_the_vault() {
        let key = SymmetricKey::generate();
        let mut synced =
            Synced::from_full_sync(serde_json::json!({ "ciphers": [], "profile": {} }));
        synced.cursor = Some("c7".into());
        synced.uwu.unseen.security_notices = 3;
        let text = synced_to_cache(&synced, &key);
        // The vault part still reads as a sync; the state is sealed.
        assert!(uwulock_bitwarden::api::parse_sync(&text).is_ok());
        assert!(!text.contains("securityNotices"));
        assert_eq!(synced_from_cache(&text, &key).unwrap(), synced);
        // Under another key the state doesn't open: no cursor, a full sync next.
        let other = synced_from_cache(&text, &SymmetricKey::generate()).unwrap();
        assert_eq!(other.cursor, None);
        assert_eq!(other.sync, synced.sync);
        // A plain sync from before.
        let plain = synced_from_cache("{\"ciphers\":[]}", &key).unwrap();
        assert_eq!(plain.cursor, None);
    }

    #[test]
    fn only_account_ids_are_forgotten() {
        let dir = scratch();
        let storage = Storage::new(dir.join("data")).unwrap();
        let sample = sample("nyu@example.org");
        let id = storage.id_for(&sample.server, &sample.email);
        storage.save_account(&id, &sample).unwrap();
        // Something next to the data folder that must survive.
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep"), "1").unwrap();

        let absolute = outside.to_string_lossy().to_string();
        let braced = format!("{{{id}}}");
        for bad in ["", ".", "..", "../..", absolute.as_str(), braced.as_str()] {
            assert!(storage.forget(bad).is_err(), "{bad:?}");
            assert!(storage.save_cache(bad, "{}").is_err(), "{bad:?}");
            assert!(storage.load_account(bad).is_none(), "{bad:?}");
        }
        assert!(outside.join("keep").exists());
        assert!(dir.join("data").join(ACCOUNTS).join(&id).exists());
        assert_eq!(storage.accounts().len(), 1);

        // A well-formed id nobody has is fine: nothing to remove.
        storage.forget(&new_id()).unwrap();
        storage.forget(&id).unwrap();
        assert!(storage.accounts().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
