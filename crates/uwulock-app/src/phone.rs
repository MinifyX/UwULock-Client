//! Android and iOS: locking after a while in the background.
//!
//! A phone has no screen lock to listen to (`session_lock`): the app simply
//! leaves the screen. With "lock with the system" on, UwULock locks when it
//! was away for [`AWAY`] or longer. iOS may stop the process while it is in
//! the background; then the check runs the moment it comes back. The host
//! says when it leaves and comes back ([`App::suspended`], [`App::resumed`]).

use parking_lot::Mutex;
use std::time::{Instant, SystemTime};

use crate::session_lock::{away_too_long, AWAY};
use crate::vault;
use crate::App;

/// When UwULock left the screen, by both clocks: the monotonic one may stand
/// still while the phone sleeps.
static LEFT: Mutex<Option<(Instant, SystemTime)>> = Mutex::new(None);

impl App {
    /// UwULock left the screen (Android `onPause`, iOS `willResignActive`).
    pub fn suspended(&self) {
        *LEFT.lock() = Some((Instant::now(), SystemTime::now()));
        let app = self.clone();
        // On Android the process keeps running: lock once the time is up.
        std::thread::spawn(move || {
            std::thread::sleep(AWAY);
            check(&app);
        });
    }

    /// UwULock is back on screen.
    pub fn resumed(&self) {
        check(self);
        *LEFT.lock() = None;
        // The auto-lock time may have passed while the phone slept: lock before
        // the vault shows, not at the next round of the timer.
        if self.vault().lock_if_idle() {
            vault::emit_status(self);
        }
        // A finger or face enrolled or removed in the meantime.
        crate::hello::probe(self);
    }
}

fn check(app: &App) {
    let Some((instant, wall)) = *LEFT.lock() else {
        return;
    };
    if !away_too_long(instant.elapsed(), wall.elapsed().unwrap_or_default()) {
        return;
    }
    if app.vault().lock_for_system() {
        tracing::info!("locked after a while in the background");
        vault::emit_status(app);
    }
}
