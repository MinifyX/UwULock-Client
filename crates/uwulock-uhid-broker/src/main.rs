//! `uwulock-uhid-broker`: started by systemd as root for each connection
//! to `/run/uwulock/uhid-broker.sock` (`uwulock-uhid-broker.socket`,
//! `Accept=yes`; the connection is fd 3). It checks who is asking, makes
//! UwULock's FIDO device and relays reports until the app goes, the person
//! at the seat changes, or a rule is broken. Ending closes `/dev/uhid`,
//! which removes the device. The rules: lib.rs and docs/passkeys.md.

#[cfg(not(target_os = "linux"))]
fn main() -> std::process::ExitCode {
    eprintln!("uwulock-uhid-broker is for Linux only.");
    std::process::ExitCode::FAILURE
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    match linux::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("uwulock-uhid-broker: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs::{File, OpenOptions};
    use std::io::{self, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::net::UnixStream;
    use std::path::Path;
    use std::time::Duration;

    use uwulock_authenticator::broker::{self, kind};
    use uwulock_authenticator::uhid;
    use uwulock_uhid_broker::{
        active_uid, drop_capabilities, holder_pid, program, relay_app, relay_kernel, uevent_uniq,
    };

    const SEAT: &str = "/run/systemd/seats/seat0";
    const LOCKS: &str = "/run/uwulock";
    /// How often the seat is looked at again while the device runs.
    const RECHECK: Duration = Duration::from_secs(2);

    fn other(text: impl Into<String>) -> io::Error {
        io::Error::other(text.into())
    }

    /// The connection systemd hands over: fd 3, `LISTEN_FDS=1` for us.
    fn connection() -> io::Result<UnixStream> {
        let pid = std::env::var("LISTEN_PID")
            .ok()
            .and_then(|p| p.parse::<u32>().ok());
        let fds = std::env::var("LISTEN_FDS").ok();
        if pid != Some(std::process::id()) || fds.as_deref() != Some("1") {
            return Err(other("start me through uwulock-uhid-broker.socket"));
        }
        // SAFETY: systemd passes exactly one socket as fd 3 (checked above),
        // and nothing else in this process owns it.
        let stream = unsafe { UnixStream::from_raw_fd(3) };
        // CLOEXEC, so nothing it might start inherits it.
        unsafe { libc::fcntl(3, libc::F_SETFD, libc::FD_CLOEXEC) };
        Ok(stream)
    }

    /// The connecting program's uid and pid, as the kernel saw them at
    /// connect time.
    fn peer(stream: &UnixStream) -> io::Result<(u32, u32)> {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: a valid socket and a ucred-sized buffer.
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut size,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((cred.uid, u32::try_from(cred.pid).unwrap_or(0)))
    }

    fn seat_user() -> Option<u32> {
        active_uid(&std::fs::read_to_string(SEAT).ok()?)
    }

    /// One device per user: an exclusive lock on `/run/uwulock/uhid-<uid>.lock`,
    /// which then names the pid it went to. When another connection holds
    /// it, the reason names that program (the person sees it in UwULock).
    fn lock(uid: u32, pid: u32) -> io::Result<File> {
        let path = format!("{LOCKS}/uhid-{uid}.lock");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)?;
        // SAFETY: a valid fd.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let holder = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| holder_pid(&text))
                .map(|pid| program(Path::new("/proc"), pid));
            eprintln!(
                "<4>uwulock-uhid-broker: uid {uid}'s security key is held by {}; refused {}",
                holder.as_deref().unwrap_or("an unknown program"),
                program(Path::new("/proc"), pid),
            );
            return Err(other(broker::held(holder.as_deref())));
        }
        file.set_len(0)?;
        writeln!(file, "{pid}")?;
        Ok(file)
    }

    /// The device's hidraw node, found by its `uniq` in sysfs.
    fn hidraw(uniq: &str) -> Option<String> {
        for device in std::fs::read_dir("/sys/bus/hid/devices").ok()?.flatten() {
            let uevent = std::fs::read_to_string(device.path().join("uevent")).unwrap_or_default();
            if uevent_uniq(&uevent) != Some(uniq) {
                continue;
            }
            let node = std::fs::read_dir(device.path().join("hidraw"))
                .ok()?
                .flatten()
                .next()?;
            return node.file_name().into_string().ok();
        }
        None
    }

    fn refuse(stream: &mut UnixStream, why: &str) -> io::Error {
        let _ = broker::send(stream, kind::ERROR, why.as_bytes());
        other(why.to_string())
    }

    pub fn run() -> io::Result<()> {
        let mut app = connection()?;
        let (uid, pid) = peer(&app)?;
        if seat_user() != Some(uid) {
            return Err(refuse(
                &mut app,
                "only the person at the seat gets a security key",
            ));
        }
        let _lock = lock(uid, pid).map_err(|e| refuse(&mut app, &e.to_string()))?;
        eprintln!(
            "uwulock-uhid-broker: uid {uid}'s security key goes to {}",
            program(Path::new("/proc"), pid)
        );
        // Looking at other programs is over: nothing beyond root's uid from
        // here on, before anything from the app is read.
        drop_capabilities().map_err(|e| refuse(&mut app, &format!("capabilities: {e}")))?;
        let kernel = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/uhid")
            .map_err(|e| refuse(&mut app, &format!("/dev/uhid: {e}")))?;
        let uniq = broker::uniq(uid);
        (&kernel)
            .write_all(&uhid::create(&uniq))
            .map_err(|e| refuse(&mut app, &format!("couldn't make the device: {e}")))?;
        broker::send(&mut app, kind::READY, &[])?;

        let kernel = std::sync::Arc::new(kernel);
        let write = |kernel: &File, event: &[u8]| {
            let mut kernel = kernel;
            kernel.write_all(event)
        };

        // The person at the seat changes: the device goes.
        std::thread::spawn({
            let kernel = std::sync::Arc::clone(&kernel);
            move || loop {
                std::thread::sleep(RECHECK);
                if seat_user() != Some(uid) {
                    let _ = write(&kernel, &uhid::destroy());
                    std::process::exit(0);
                }
            }
        });
        // The kernel's side.
        std::thread::spawn({
            let kernel = std::sync::Arc::clone(&kernel);
            let mut to_app = app.try_clone()?;
            move || {
                let mut reader: &File = &kernel;
                let result = relay_kernel(
                    &mut reader,
                    &mut |event: &[u8]| write(&kernel, event),
                    &mut to_app,
                    &|| hidraw(&uniq),
                );
                let _ = write(&kernel, &uhid::destroy());
                if let Err(error) = result {
                    eprintln!("uwulock-uhid-broker: {error}");
                }
                std::process::exit(0);
            }
        });
        // The app's side, here.
        let ended = relay_app(&mut app, &mut |event: &[u8]| write(&kernel, event));
        let result = match ended {
            Ok(refused) => Err(refuse(&mut app, &format!("not a report: {refused:?}"))),
            // The app closed the connection.
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(()),
            Err(error) => Err(error),
        };
        let _ = write(&kernel, &uhid::destroy());
        result
    }
}
