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

use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Manager};

use crate::vault::{self, VaultState};

/// How often the clocks (and on Windows and macOS the lock state) are read.
const EVERY: Duration = Duration::from_secs(2);
/// A pause this much longer than [`EVERY`] was sleep, not a busy moment.
const SLEPT: Duration = Duration::from_secs(20);

pub(crate) fn start(app: &AppHandle) {
    let watcher = app.clone();
    std::thread::Builder::new()
        .name("uwulock-session-lock".into())
        .spawn(move || poll(watcher))
        .ok();
    #[cfg(target_os = "linux")]
    linux::watch(app);
}

/// The system locked or went to sleep: lock, if that's wanted and anything
/// is open.
fn system_locked(app: &AppHandle, why: &str) {
    let state = app.state::<VaultState>();
    if state.lock_for_system() {
        tracing::info!(why, "locked with the system");
        vault::emit_status(app);
    }
}

fn poll(app: AppHandle) {
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
            system_locked(&app, "sleep");
        }
        last_instant = now_instant;
        last_wall = now_wall;

        let locked = screen_locked();
        if locked && !was_locked {
            system_locked(&app, "screen lock");
        }
        was_locked = locked;
    }
}

/// On a phone (`crate::phone`): how long UwULock may be in the background
/// before it locks — long enough to paste a password in another app and come
/// back, short enough that a phone left on the table isn't open.
#[cfg_attr(not(mobile), allow(dead_code))]
pub(crate) const AWAY: Duration = Duration::from_secs(60);

/// Whether UwULock was in the background for [`AWAY`] or longer, by either
/// clock: the monotonic one may stand still while the phone sleeps.
#[cfg_attr(not(mobile), allow(dead_code))]
pub(crate) fn away_too_long(monotonic: Duration, wall: Duration) -> bool {
    monotonic.max(wall) >= AWAY
}

/// Whether the wall clock ran on while the monotonic one stood still. A clock
/// set back or forward by hand shows up too, but only as one more lock.
fn slept(monotonic: Duration, wall: Duration) -> bool {
    wall > monotonic + SLEPT
}

#[cfg(windows)]
fn screen_locked() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, SwitchDesktop, DESKTOP_SWITCHDESKTOP,
    };
    // While the workstation is locked, the input desktop is Winlogon's: a
    // normal process can't open it, or can't switch to it.
    unsafe {
        let desktop = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if desktop.is_null() {
            return true;
        }
        let switched = SwitchDesktop(desktop);
        CloseDesktop(desktop);
        switched == 0
    }
}

#[cfg(target_os = "macos")]
fn screen_locked() -> bool {
    use std::ffi::{c_char, c_void};

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            text: *const c_char,
            encoding: u32,
        ) -> *const c_void;
        fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
        fn CFGetTypeID(value: *const c_void) -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(value: *const c_void) -> u8;
        fn CFRelease(value: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;

    unsafe {
        let session = CGSessionCopyCurrentDictionary();
        if session.is_null() {
            return false;
        }
        let key =
            CFStringCreateWithCString(std::ptr::null(), c"CGSSessionScreenIsLocked".as_ptr(), UTF8);
        let mut locked = false;
        if !key.is_null() {
            let value = CFDictionaryGetValue(session, key);
            locked = !value.is_null()
                && CFGetTypeID(value) == CFBooleanGetTypeID()
                && CFBooleanGetValue(value) != 0;
            CFRelease(key);
        }
        CFRelease(session);
        locked
    }
}

/// Linux says so by signal ([`linux::watch`]).
#[cfg(not(any(windows, target_os = "macos")))]
fn screen_locked() -> bool {
    false
}

#[cfg(target_os = "linux")]
mod linux {
    use super::system_locked;
    use tauri::AppHandle;
    use zbus::blocking::{Connection, Proxy};

    /// Listens on D-Bus, each source in a thread of its own; one that isn't
    /// there (no logind, no screen saver service) is simply left out.
    pub(super) fn watch(app: &AppHandle) {
        spawn(app, "logind-sleep", |app| {
            let bus = Connection::system()?;
            let manager = Proxy::new(
                &bus,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
            )?;
            for message in manager.receive_signal("PrepareForSleep")? {
                if message.body().deserialize::<bool>().unwrap_or(false) {
                    system_locked(&app, "sleep");
                }
            }
            Ok(())
        });
        spawn(app, "logind-lock", |app| {
            let bus = Connection::system()?;
            let session = session(&bus)?;
            for _ in session.receive_signal("Lock")? {
                system_locked(&app, "screen lock");
            }
            Ok(())
        });
        spawn(app, "logind-locked-hint", |app| {
            let bus = Connection::system()?;
            let session = session(&bus)?;
            for change in session.receive_property_changed::<bool>("LockedHint") {
                if change.get().unwrap_or(false) {
                    system_locked(&app, "screen lock");
                }
            }
            Ok(())
        });
        for (name, path) in [
            (
                "org.freedesktop.ScreenSaver",
                "/org/freedesktop/ScreenSaver",
            ),
            ("org.gnome.ScreenSaver", "/org/gnome/ScreenSaver"),
        ] {
            spawn(app, name, move |app| {
                let bus = Connection::session()?;
                let saver = Proxy::new(&bus, name, path, name)?;
                for message in saver.receive_signal("ActiveChanged")? {
                    if message.body().deserialize::<bool>().unwrap_or(false) {
                        system_locked(&app, "screen saver");
                    }
                }
                Ok(())
            });
        }
    }

    /// This process's own login session.
    fn session(bus: &Connection) -> zbus::Result<Proxy<'static>> {
        Proxy::new(
            bus,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
        )
    }

    fn spawn(
        app: &AppHandle,
        name: &'static str,
        work: impl FnOnce(AppHandle) -> zbus::Result<()> + Send + 'static,
    ) {
        let app = app.clone();
        let _ = std::thread::Builder::new()
            .name(format!("uwulock-{name}"))
            .spawn(move || {
                if let Err(error) = work(app) {
                    tracing::debug!(%error, source = name, "not watching for the system's lock");
                }
            });
    }
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
