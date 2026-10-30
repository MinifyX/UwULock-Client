//! UwULock's app, without a window: what the page asks for, as plain
//! methods on [`App`]. The Tauri app (apps/desktop/src-tauri) turns its IPC
//! commands into these calls, one each with the same name and arguments; a
//! native app can do the same. What it needs from the program around it —
//! events to the page, the clipboard, the system's biometrics, settings and
//! extensions — is the [`Host`] it is given.
//!
//! The protocol and the crypto are `uwulock-bitwarden`'s; this is the state
//! between: accounts on disk, the open vault, the background work.
//!
//! - [`vault`] — logging in, unlocking, syncing, items, copying
//! - [`account`] — what is kept on disk, and where
//! - [`extras`] — UwULock Server's extras: icons, versions, reminders, file
//!   requests, masked addresses, travel mode; items shared as Sends
//! - [`health`] — the password check and its review one login at a time
//! - [`suite`] — UwUSSH's and UwURDP's hosts, logins and keys
//! - [`live`] — changes from other devices as they happen
//! - [`session_lock`] — locking when the screen locks or the computer sleeps
//! - [`hello`] — unlocking with Windows Hello, Touch ID, or a phone's
//!   fingerprint or face
//! - [`moving`] — moving a vault in from Bitwarden or Vaultwarden
//! - [`importing`] — moving in from another app's export file
//! - [`clipboard`] — copies that clear themselves
//! - [`system`] — links out of the app
//! - [`phone`] — Android and iOS: locking in the background
//! - [`autofill`] — UwULock as the system's AutoFill provider (iOS, macOS,
//!   Android), and what Apple Passwords hands over (iOS 26)
//! - [`passkeys`] — the vault's passkeys for the system
//! - [`safari`] — the Safari extension the Apple builds carry: switched on?
//! - [`mello`] — counts for Mello (MelloAI-Desktop) on loopback, opt-in
//! - [`sends`], [`wifi`] — Sends; Wi-Fi networks handed to the phone
//! - [`host`] — what the program around it does; [`runtime`] — where the
//!   async work runs
//!
//! `#[cfg(desktop)]`/`#[cfg(mobile)]` (build.rs) leave out what only one of
//! them has, as in the Tauri app.

pub mod account;
pub mod autofill;
pub mod clipboard;
mod dpapi;
pub mod extras;
pub mod health;
pub mod hello;
pub mod host;
pub mod importing;
pub mod live;
pub mod mello;
pub mod moving;
pub mod passkeys;
pub mod phone;
pub mod runtime;
pub mod safari;
pub mod sends;
pub mod session_lock;
pub mod suite;
pub mod system;
pub mod vault;
pub mod wifi;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;

pub use account::Storage;
pub use host::Host;
pub use vault::{Failure, Result, VaultState};

use moving::MoveState;
use passkeys::Provider;

type Listener = Arc<dyn Fn(&serde_json::Value) + Send + Sync>;

/// One of [`App::listen`]'s listeners, for [`App::unlisten`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

/// The app: its state and its host. Cheap to clone; every clone is the same
/// app.
#[derive(Clone)]
pub struct App(Arc<Inner>);

struct Inner {
    host: Arc<dyn Host>,
    vault: VaultState,
    passkeys: Provider,
    mello: mello::Link,
    listeners: RwLock<Vec<(ListenerId, String, Listener)>>,
    next_listener: AtomicU64,
}

impl App {
    /// The app with its accounts in `storage`. Nothing runs yet: see
    /// [`App::start`].
    pub fn new(storage: Storage, host: Arc<dyn Host>) -> App {
        let passkeys = Provider::new(storage.dir());
        let mello = mello::Link::new(storage.dir());
        let vault = VaultState::new(storage);
        vault.attach(host.clone());
        App(Arc::new(Inner {
            host,
            vault,
            passkeys,
            mello,
            listeners: RwLock::new(Vec::new()),
            next_listener: AtomicU64::new(1),
        }))
    }

    /// Starts the background work: syncing, live changes, the extras, the
    /// screen lock watch, the passkey providers and the Mello link the settings
    /// switched on and,
    /// on a computer, asking what biometrics there are. On a phone the
    /// biometrics are asked once the page is there (`vault_status`).
    pub fn start(&self) {
        vault::start(self);
        live::start(self);
        extras::start(self);
        session_lock::start(self);
        passkeys::start(self);
        mello::start(self);
        #[cfg(desktop)]
        hello::probe(self);
    }

    pub fn host(&self) -> &Arc<dyn Host> {
        &self.0.host
    }

    pub fn vault(&self) -> &VaultState {
        &self.0.vault
    }

    pub fn moves(&self) -> &Arc<MoveState> {
        &self.0.vault.moves
    }

    pub fn passkeys(&self) -> &Provider {
        &self.0.passkeys
    }

    pub(crate) fn mello(&self) -> &mello::Link {
        &self.0.mello
    }

    /// Tells the app's own listeners, then the page. A payload that doesn't
    /// serialise goes as `null` (none of the app's do that).
    pub fn emit<S: Serialize>(&self, event: &str, payload: S) {
        let payload = serde_json::to_value(payload).unwrap_or_default();
        let listeners: Vec<Listener> = self
            .0
            .listeners
            .read()
            .iter()
            .filter(|(_, name, _)| name == event)
            .map(|(_, _, listener)| listener.clone())
            .collect();
        for listener in listeners {
            listener(&payload);
        }
        self.0.host.emit(event, payload);
    }

    /// Calls `listener` with the payload of every `event` the app emits.
    pub fn listen<F>(&self, event: &str, listener: F) -> ListenerId
    where
        F: Fn(&serde_json::Value) + Send + Sync + 'static,
    {
        let id = ListenerId(self.0.next_listener.fetch_add(1, Ordering::Relaxed));
        self.0
            .listeners
            .write()
            .push((id, event.to_string(), Arc::new(listener)));
        id
    }

    pub fn unlisten(&self, id: ListenerId) {
        self.0
            .listeners
            .write()
            .retain(|(other, _, _)| *other != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn listeners_hear_their_event_until_they_stop() {
        let dir = std::env::temp_dir().join(format!("uwulock-test-{}", uuid::Uuid::new_v4()));
        let app = App::new(Storage::new(dir.clone()).unwrap(), Arc::new(host::NoHost));
        let heard = Arc::new(AtomicUsize::new(0));
        let counter = heard.clone();
        let id = app.listen("vault-changed", move |payload| {
            assert!(payload.is_null());
            counter.fetch_add(1, Ordering::Relaxed);
        });
        app.emit("vault-changed", ());
        app.emit("vault-status", 1);
        assert_eq!(heard.load(Ordering::Relaxed), 1);
        app.unlisten(id);
        app.emit("vault-changed", ());
        assert_eq!(heard.load(Ordering::Relaxed), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
