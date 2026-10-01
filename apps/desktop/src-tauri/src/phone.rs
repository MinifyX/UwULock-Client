//! Android and iOS: the plugin with UwULock's own phone code
//! (`tauri-plugin-uwulock-mobile`), and locking after a while in the
//! background.
//!
//! A phone has no screen lock to listen to (`session_lock`): the app simply
//! leaves the screen. With "lock with the system" on, UwULock locks when it
//! was away for [`AWAY`] or longer. iOS may stop the process while it is in
//! the background; then the check runs the moment it comes back.

use parking_lot::Mutex;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime};
use tauri::{AppHandle, Manager, State, Wry};
use tauri_plugin_uwulock_mobile::Mobile;

use crate::session_lock::{away_too_long, AWAY};
use crate::vault::{self, VaultState};

static APP: OnceLock<AppHandle> = OnceLock::new();
/// When UwULock left the screen, by both clocks: the monotonic one may stand
/// still while the phone sleeps.
static LEFT: Mutex<Option<(Instant, SystemTime)>> = Mutex::new(None);

pub(crate) fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// The plugin, once the app is set up. Its calls block until the phone
/// answers: only ever from a thread of their own, never a command's.
pub(crate) fn plugin() -> Option<State<'static, Mobile<Wry>>> {
    APP.get()?.try_state::<Mobile<Wry>>()
}

/// UwULock left the screen (Android `onPause`, iOS `willResignActive`).
pub(crate) fn suspended(app: &AppHandle) {
    *LEFT.lock() = Some((Instant::now(), SystemTime::now()));
    let app = app.clone();
    // On Android the process keeps running: lock once the time is up.
    std::thread::spawn(move || {
        std::thread::sleep(AWAY);
        check(&app);
    });
}

/// UwULock is back on screen.
pub(crate) fn resumed(app: &AppHandle) {
    check(app);
    *LEFT.lock() = None;
    // A finger or face enrolled or removed in the meantime.
    crate::hello::probe(app);
}

fn check(app: &AppHandle) {
    let Some((instant, wall)) = *LEFT.lock() else {
        return;
    };
    if !away_too_long(instant.elapsed(), wall.elapsed().unwrap_or_default()) {
        return;
    }
    let state = app.state::<VaultState>();
    if state.lock_for_system() {
        tracing::info!("locked after a while in the background");
        vault::emit_status(app);
    }
}
