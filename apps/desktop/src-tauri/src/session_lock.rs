//! The system's side of locking with it (uwulock-app's `session_lock` has
//! the rest): whether the screen is locked right now, for the host
//! (`Host::screen_locked`), and on Linux logind's and the screen saver's
//! signals, which tell the app themselves (`App::system_locked`).
//!
//! - **Screen lock**: on Linux logind's `Lock` signal and `LockedHint`, and
//!   the desktop's screen saver (`ActiveChanged`, freedesktop and GNOME); on
//!   Windows the input desktop can't be switched to while the workstation is
//!   locked. Sleep is the app's: a gap between the clocks. On Linux, logind also
//!   says so before it happens (`PrepareForSleep`).
//!
//! None of this needs a signed app or any permission.

#[cfg(windows)]
pub(crate) fn screen_locked() -> bool {
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

/// Linux says so by signal.
#[cfg(target_os = "linux")]
pub(crate) mod linux {
    use uwulock_app::App;
    use zbus::blocking::{Connection, Proxy};

    /// Listens on D-Bus, each source in a thread of its own; one that isn't
    /// there (no logind, no screen saver service) is simply left out.
    pub(crate) fn watch(app: &App) {
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
                    app.system_locked("sleep");
                }
            }
            Ok(())
        });
        spawn(app, "logind-lock", |app| {
            let bus = Connection::system()?;
            let session = session(&bus)?;
            for _ in session.receive_signal("Lock")? {
                app.system_locked("screen lock");
            }
            Ok(())
        });
        spawn(app, "logind-locked-hint", |app| {
            let bus = Connection::system()?;
            let session = session(&bus)?;
            for change in session.receive_property_changed::<bool>("LockedHint") {
                if change.get().unwrap_or(false) {
                    app.system_locked("screen lock");
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
                        app.system_locked("screen saver");
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
        app: &App,
        name: &'static str,
        work: impl FnOnce(App) -> zbus::Result<()> + Send + 'static,
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
