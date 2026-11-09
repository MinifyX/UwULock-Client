//! Locking when the computer does: when the screen locks, and when the system
//! goes to sleep. Whoever walks up to the computer next finds UwULock locked
//! too, whatever the auto-lock time says. On by default; the settings can
//! switch it off.
//!
//! - **Sleep**, everywhere: the monotonic clock stands still while the system
//!   sleeps (`CLOCK_MONOTONIC` on Linux, `CLOCK_UPTIME_RAW` on macOS), the
//!   wall clock doesn't. A gap between the two after a pause means the
//!   computer slept. On Linux, logind also says so before it happens
//!   (`PrepareForSleep`).
//! - **Screen lock**: on Linux logind's `Lock` signal and `LockedHint`, and
//!   the desktop's screen saver (`ActiveChanged`, freedesktop and GNOME); on
//!   Windows the input desktop can't be switched to while the workstation is
//!   locked; on macOS the session dictionary has `CGSSessionScreenIsLocked`.
//!
//! None of this needs a signed app or any permission.
//!
//! The clocks are read here; whether the screen is locked is the host's
//! ([`Host::screen_locked`](crate::host::Host::screen_locked)), and a host told by signal (Linux) calls
//! [`App::system_locked`] itself.

use std::time::{Duration, Instant, SystemTime};

use crate::vault;
use crate::App;

/// How often the clocks (and on Windows and macOS the lock state) are read.
const EVERY: Duration = Duration::from_secs(2);
/// A pause this much longer than [`EVERY`] was sleep, not a busy moment.
const SLEPT: Duration = Duration::from_secs(20);

pub fn start(app: &App) {
    let watcher = app.clone();
    std::thread::Builder::new()
        .name("uwulock-session-lock".into())
        .spawn(move || poll(watcher))
        .ok();
}

impl App {
    /// The system locked or went to sleep: lock, if that's wanted and
    /// anything is open. `why` goes to the log.
    pub fn system_locked(&self, why: &str) {
        if self.vault().lock_for_system() {
            tracing::info!(why, "locked with the system");
            vault::emit_status(self);
        }
    }
}

fn poll(app: App) {
    let mut last_instant = Instant::now();
    let mut last_wall = SystemTime::now();
    let mut was_locked = false;
    loop {
        std::thread::sleep(EVERY);
        let now_instant = Instant::now();
        let now_wall = SystemTime::now();
        let monotonic = now_instant.duration_since(last_instant);
        let wall = now_wall.duration_since(last_wall).unwrap_or_default();
        if slept(monotonic, wall) {
            app.system_locked("sleep");
        }
        last_instant = now_instant;
        last_wall = now_wall;

        let locked = app.host().screen_locked();
        if locked && !was_locked {
            app.system_locked("screen lock");
        }
        was_locked = locked;
    }
}

/// On a phone (`crate::phone`): how long UwULock may be in the background
/// before it locks — long enough to paste a password in another app and come
/// back, short enough that a phone left on the table isn't open.
pub const AWAY: Duration = Duration::from_secs(60);

/// Whether UwULock was in the background for [`AWAY`] or longer, by either
/// clock: the monotonic one may stand still while the phone sleeps.
pub fn away_too_long(monotonic: Duration, wall: Duration) -> bool {
    monotonic.max(wall) >= AWAY
}

/// Whether the wall clock ran on while the monotonic one stood still. A clock
/// set back or forward by hand shows up too, but only as one more lock.
fn slept(monotonic: Duration, wall: Duration) -> bool {
    wall > monotonic + SLEPT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gap_between_the_clocks_is_sleep() {
        let s = Duration::from_secs;
        assert!(!slept(s(2), s(2)));
        // A busy moment: both clocks ran.
        assert!(!slept(s(15), s(15)));
        assert!(!slept(s(2), s(10)));
        assert!(slept(s(2), s(3600)));
    }

    #[test]
    fn a_short_trip_to_another_app_keeps_the_vault_open() {
        let s = Duration::from_secs;
        assert!(!away_too_long(s(5), s(5)));
        assert!(away_too_long(AWAY, AWAY));
        // The phone slept: the monotonic clock stood still, the wall clock didn't.
        assert!(away_too_long(s(1), s(3600)));
    }
}
