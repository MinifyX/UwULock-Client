//! The broker's rules, without a kernel or systemd, so they are tested on
//! their own. `main.rs` puts them on `/dev/uhid` and the connection.
//!
//! - Who: only the user of the active session on seat0
//!   ([`active_uid`]), checked when connecting and again while it runs.
//! - What: the device is UwULock's FIDO key ([`uhid::create`]: fixed name,
//!   descriptor, bus and ids, all compiled in). The app never holds
//!   `/dev/uhid`; it sends 64-byte reports and the broker writes the
//!   `UHID_INPUT2` itself ([`from_app`]), so no `UHID_DESTROY` +
//!   `UHID_CREATE2` with another descriptor can ever reach the kernel.
//! - From the kernel only output reports of 64 bytes, opens and closes
//!   reach the app ([`from_kernel`]); feature report requests are answered
//!   here.

use std::io::{self, Read, Write};
use std::path::Path;

use uwulock_authenticator::broker::{self, kind, REPORT};
use uwulock_authenticator::uhid::{self, Incoming};

/// The user of the active session on a seat, from logind's seat file
/// (`/run/systemd/seats/seat0`, what `sd_seat_get_active` reads).
pub fn active_uid(seat_file: &str) -> Option<u32> {
    seat_file
        .lines()
        .find_map(|line| line.strip_prefix("ACTIVE_UID="))
        .and_then(|uid| uid.trim().parse().ok())
}

/// Why the app's frame ends the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Not a report: the app only ever sends reports.
    Kind(u8),
    /// A report that isn't 64 bytes.
    Length(usize),
}

/// A frame from the app as the event the broker writes to `/dev/uhid`.
pub fn from_app(kind: u8, payload: &[u8]) -> Result<Vec<u8>, Refused> {
    if kind != kind::REPORT {
        return Err(Refused::Kind(kind));
    }
    if payload.len() != REPORT {
        return Err(Refused::Length(payload.len()));
    }
    Ok(uhid::input(payload))
}

/// What to do with an event from the kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A frame for the app.
    ToApp(Vec<u8>),
    /// An event for the kernel (a feature report request's "no such").
    ToKernel(Vec<u8>),
    /// The device is gone (`UHID_STOP`).
    Stop,
    Nothing,
}

/// The kernel's event, filtered. `hidraw` names the device's hidraw node
/// for [`kind::OPEN`], when it is known.
pub fn from_kernel(event: &[u8], hidraw: impl FnOnce() -> Option<String>) -> Action {
    match uhid::parse(event) {
        Some(Incoming::Output(report)) if report.len() == REPORT => {
            Action::ToApp(broker::frame(kind::REPORT, &report))
        }
        Some(Incoming::Open) => Action::ToApp(broker::frame(
            kind::OPEN,
            hidraw().unwrap_or_default().as_bytes(),
        )),
        Some(Incoming::Close) => Action::ToApp(broker::frame(kind::CLOSE, &[])),
        Some(Incoming::GetReport { id }) => Action::ToKernel(uhid::get_report_reply(id)),
        Some(Incoming::SetReport { id }) => Action::ToKernel(uhid::set_report_reply(id)),
        Some(Incoming::Stop) => Action::Stop,
        _ => Action::Nothing,
    }
}

/// The app's side: frames in, events to the kernel, until the app goes or
/// breaks a rule. Returns why it ended.
pub fn relay_app(
    app: &mut impl Read,
    kernel: &mut impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<Refused> {
    loop {
        let (kind, payload) = broker::receive(app)?;
        match from_app(kind, &payload) {
            Ok(event) => kernel(&event)?,
            Err(refused) => return Ok(refused),
        }
    }
}

/// The kernel's side: events in, filtered, to the app or back.
pub fn relay_kernel(
    kernel: &mut impl Read,
    to_kernel: &mut impl FnMut(&[u8]) -> io::Result<()>,
    app: &mut impl Write,
    hidraw: &impl Fn() -> Option<String>,
) -> io::Result<()> {
    let mut buffer = vec![0u8; uhid::EVENT_SIZE];
    loop {
        let read = match kernel.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        match from_kernel(&buffer[..read], hidraw) {
            Action::ToApp(frame) => app.write_all(&frame)?,
            Action::ToKernel(event) => to_kernel(&event)?,
            Action::Stop => return Ok(()),
            Action::Nothing => {}
        }
    }
}

/// The `HID_UNIQ` in a HID device's sysfs `uevent`.
pub fn uevent_uniq(uevent: &str) -> Option<&str> {
    uevent
        .lines()
        .find_map(|line| line.strip_prefix("HID_UNIQ="))
}

/// Who a lock file says holds the key: the pid and when that process
/// started (field 22 of `/proc/<pid>/stat`, clock ticks since boot). The
/// start time tells the holder from a later process that got the same pid
/// once the holder is gone (R8 C-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holder {
    pub pid: u32,
    pub start: u64,
}

impl Holder {
    /// The process `pid` as it is now in `proc`, if it is there.
    pub fn of(proc: &Path, pid: u32) -> Option<Holder> {
        let stat = std::fs::read_to_string(proc.join(pid.to_string()).join("stat")).ok()?;
        Some(Holder {
            pid,
            start: start_time(&stat)?,
        })
    }

    /// What the lock file holds: `<pid> <start>`.
    pub fn line(&self) -> String {
        format!("{} {}\n", self.pid, self.start)
    }

    /// The holder a lock file names. A file without a start time (written
    /// by an older broker) names nobody that can be checked: `None`.
    pub fn parse(lock_file: &str) -> Option<Holder> {
        let mut fields = lock_file.split_whitespace();
        let pid = fields.next()?.parse().ok().filter(|pid| *pid > 0)?;
        let start = fields.next()?.parse().ok()?;
        fields.next().is_none().then_some(Holder { pid, start })
    }
}

/// A process's start time from its `/proc/<pid>/stat`: field 22. The name
/// (field 2) is in parentheses and may hold spaces and `)` itself, so the
/// fields are counted from the last `)`.
pub fn start_time(stat: &str) -> Option<u64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    // After the name: field 3 (state) is the first, so 22 is the 20th.
    rest.split_whitespace().nth(19)?.parse().ok()
}

/// The real and effective uid from a `/proc/<pid>/status`.
pub fn uids(status: &str) -> Option<(u32, u32)> {
    let line = status.lines().find_map(|line| line.strip_prefix("Uid:"))?;
    let mut ids = line.split_whitespace().map(str::parse::<u32>);
    Some((ids.next()?.ok()?, ids.next()?.ok()?))
}

/// The program `pid` runs, for the journal and the app: the path of its
/// executable (`<proc>/<pid>/exe`), else the name it gives itself, marked as
/// such; both escaped and cut ([`broker::printable`], R8 C-1).
///
/// Named only when the process belongs to `uid` (real and effective), and,
/// with `start`, only while it is still the process that started then: the
/// broker looks with root's eyes and `CAP_SYS_PTRACE`, and must not tell one
/// user what another user's process runs (R8 C-2). `None` otherwise. Read
/// only while the broker may still look (see [`drop_capabilities`]).
pub fn program(proc: &Path, pid: u32, uid: u32, start: Option<u64>) -> Option<String> {
    let dir = proc.join(pid.to_string());
    let same = || {
        let status = std::fs::read_to_string(dir.join("status")).ok()?;
        let still = match start {
            Some(start) => Holder::of(proc, pid)?.start == start,
            None => true,
        };
        (still && uids(&status)? == (uid, uid)).then_some(())
    };
    same()?;
    let named = match std::fs::read_link(dir.join("exe")) {
        Ok(exe) => broker::printable(&exe.to_string_lossy(), broker::PATH_SHOWN),
        Err(_) => {
            let comm = std::fs::read_to_string(dir.join("comm")).ok()?;
            format!(
                "a program calling itself {:?}",
                broker::printable(comm.trim(), 64)
            )
        }
    };
    // Still the same process after the look: the pid didn't change hands
    // in between.
    same()?;
    Some(format!("{named} (pid {pid})"))
}

/// Gives up every capability of this thread for good (the broker is one
/// thread when it calls this). The unit grants `CAP_SYS_PTRACE` only so the
/// broker can name the program it serves or refuses (`/proc/<pid>/exe`);
/// it is dropped before the first byte from the app is read.
#[cfg(target_os = "linux")]
pub fn drop_capabilities() -> io::Result<()> {
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Data {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    // _LINUX_CAPABILITY_VERSION_3: two 32-bit halves.
    let header = Header {
        version: 0x2008_0522,
        pid: 0,
    };
    let data = [Data::default(); 2];
    // SAFETY: capset with a valid header and two data structs, as v3 wants.
    let result =
        unsafe { libc::syscall(libc::SYS_capset, &header as *const Header, data.as_ptr()) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn output(report: &[u8]) -> Vec<u8> {
        // struct uhid_output_req: data[4096], u16 size, u8 rtype.
        let mut event = vec![0u8; uhid::EVENT_SIZE];
        event[..4].copy_from_slice(&6u32.to_ne_bytes());
        event[4..4 + report.len()].copy_from_slice(report);
        event[4 + 4096..6 + 4096].copy_from_slice(&(report.len() as u16).to_ne_bytes());
        event
    }

    fn kind_of(event: &[u8]) -> u32 {
        u32::from_ne_bytes(event[..4].try_into().unwrap())
    }

    #[test]
    fn only_the_person_at_the_seat() {
        let seat = "# This is private data. Do not parse.\nIS_SEAT0=1\nACTIVE=3\nACTIVE_UID=1000\nSESSIONS=3 c1\nUIDS=1000 120\n";
        assert_eq!(active_uid(seat), Some(1000));
        assert_eq!(active_uid("IS_SEAT0=1\nSESSIONS=c1\n"), None);
        assert_eq!(active_uid("ACTIVE_UID=nobody\n"), None);
    }

    #[test]
    fn the_app_sends_reports_and_nothing_else() {
        let report = [0x5a; REPORT];
        let event = from_app(kind::REPORT, &report).unwrap();
        assert_eq!(event, uhid::input(&report));
        assert_eq!(kind_of(&event), 12); // UHID_INPUT2
                                         // A whole uhid event — a DESTROY, a CREATE2 with a keyboard's
                                         // descriptor — is no report: refused, never written.
        assert_eq!(
            from_app(kind::REPORT, &uhid::destroy()[..REPORT + 1]),
            Err(Refused::Length(REPORT + 1))
        );
        assert_eq!(from_app(kind::REPORT, &[1; 8]), Err(Refused::Length(8)));
        for other in [
            kind::READY,
            kind::OPEN,
            kind::CLOSE,
            kind::ERROR,
            11,
            1 | 0x80,
        ] {
            assert_eq!(from_app(other, &report), Err(Refused::Kind(other)));
        }
    }

    #[test]
    fn a_rule_broken_ends_the_relay() {
        let mut stream = broker::frame(kind::REPORT, &[1; REPORT]);
        stream.extend(broker::frame(kind::REPORT, &[2; REPORT]));
        // Then a try to slip a raw CREATE2 through.
        stream.extend(broker::frame(kind::REPORT, &uhid::create("x")[..300]));
        stream.extend(broker::frame(kind::REPORT, &[3; REPORT]));
        let mut written = Vec::new();
        let ended = relay_app(&mut Cursor::new(stream), &mut |event: &[u8]| {
            written.push(event.to_vec());
            Ok(())
        })
        .unwrap();
        assert_eq!(ended, Refused::Length(300));
        assert_eq!(written.len(), 2);
        assert!(written.iter().all(|event| kind_of(event) == 12));
        assert_eq!(&written[1][6..6 + REPORT], &[2; REPORT]);
        // An app that just goes: an error, the broker ends too.
        let mut gone = Cursor::new(Vec::new());
        assert!(relay_app(&mut gone, &mut |_: &[u8]| Ok(())).is_err());
    }

    #[test]
    fn the_kernel_side_is_filtered() {
        let report = [9u8; REPORT];
        // Reports from hidraw come with report number 0 in front.
        let mut numbered = vec![0];
        numbered.extend_from_slice(&report);
        let mut get_report = vec![0u8; uhid::EVENT_SIZE];
        get_report[..4].copy_from_slice(&9u32.to_ne_bytes());
        get_report[4..8].copy_from_slice(&42u32.to_ne_bytes());
        let mut open = vec![0u8; uhid::EVENT_SIZE];
        open[..4].copy_from_slice(&4u32.to_ne_bytes());
        let mut start = vec![0u8; uhid::EVENT_SIZE];
        start[..4].copy_from_slice(&2u32.to_ne_bytes());
        let mut stop = vec![0u8; uhid::EVENT_SIZE];
        stop[..4].copy_from_slice(&3u32.to_ne_bytes());

        let events = [
            start,
            open,
            output(&numbered),
            output(&[1; 10]), // not a FIDO report: dropped
            get_report,
            stop,
            output(&report), // after STOP: never read
        ];
        // A reader that hands out one event per read, as /dev/uhid does.
        struct Events(std::vec::IntoIter<Vec<u8>>);
        impl Read for Events {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let Some(event) = self.0.next() else {
                    return Ok(0);
                };
                buf[..event.len()].copy_from_slice(&event);
                Ok(event.len())
            }
        }
        let mut to_kernel = Vec::new();
        let mut app = Vec::new();
        relay_kernel(
            &mut Events(Vec::from(events).into_iter()),
            &mut |event: &[u8]| {
                to_kernel.push(event.to_vec());
                Ok(())
            },
            &mut app,
            &|| Some("hidraw7".into()),
        )
        .unwrap();
        let mut frames = Cursor::new(app);
        assert_eq!(
            broker::receive(&mut frames).unwrap(),
            (kind::OPEN, b"hidraw7".to_vec())
        );
        assert_eq!(
            broker::receive(&mut frames).unwrap(),
            (kind::REPORT, report.to_vec())
        );
        assert!(broker::receive(&mut frames).is_err());
        assert_eq!(to_kernel, vec![uhid::get_report_reply(42)]);
    }

    #[test]
    fn sysfs_uniq() {
        let uevent = "DRIVER=hid-generic\nHID_ID=0003:00000000:00000000\nHID_NAME=UwULock Passkeys\nHID_PHYS=uwulock\nHID_UNIQ=uwulock-1000\nMODALIAS=hid:b0003g0001v00000000p00000000\n";
        assert_eq!(uevent_uniq(uevent), Some("uwulock-1000"));
        assert_eq!(uevent_uniq("HID_NAME=x\n"), None);
    }

    #[cfg(unix)]
    /// A `/proc/<pid>` with `stat`, `status` and an `exe` link, in a fresh
    /// directory under the system's temp dir.
    struct FakeProc(std::path::PathBuf);

    #[cfg(unix)]
    impl FakeProc {
        fn new(name: &str) -> FakeProc {
            let dir =
                std::env::temp_dir().join(format!("uwulock-broker-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            FakeProc(dir)
        }

        fn process(&self, pid: u32, start: u64, uid: u32, exe: &str) {
            let dir = self.0.join(pid.to_string());
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            // A name with spaces and a ")" of its own.
            let stat = format!(
                "{pid} (evil) 1 2 3) S 1 {pid} {pid} 0 -1 4194560 100 0 0 0 1 1 0 0 20 0 1 0 {start} 1000 100 18446744073709551615\n"
            );
            std::fs::write(dir.join("stat"), stat).unwrap();
            std::fs::write(
                dir.join("status"),
                format!("Name:\tx\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\nGid:\t1\t1\t1\t1\n"),
            )
            .unwrap();
            std::fs::write(dir.join("comm"), "x\n").unwrap();
            std::os::unix::fs::symlink(exe, dir.join("exe")).unwrap();
        }
    }

    #[cfg(unix)]
    impl Drop for FakeProc {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn who_holds_the_key() {
        let holder = Holder {
            pid: 4242,
            start: 987654,
        };
        assert_eq!(Holder::parse(&holder.line()), Some(holder));
        assert_eq!(Holder::parse(""), None);
        assert_eq!(Holder::parse("0 5"), None);
        assert_eq!(Holder::parse("nope 5"), None);
        // An older broker's file (pid only): nobody that can be checked.
        assert_eq!(Holder::parse("4242\n"), None);
        assert_eq!(Holder::parse("4242 5 6"), None);

        let stat = "42 (a) b) c) R 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 777 19 20";
        assert_eq!(start_time(stat), Some(777));
        assert_eq!(start_time("42 (x) R 1"), None);
        assert_eq!(uids("Name:\tx\nUid:\t1000\t0\t0\t0\n"), Some((1000, 0)));
        assert_eq!(uids("Name:\tx\n"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_process_is_named() {
        let me = std::process::id();
        let uid = unsafe { libc::getuid() };
        let proc = Path::new("/proc");
        let start = Holder::of(proc, me).unwrap().start;
        let named = program(proc, me, uid, Some(start)).unwrap();
        assert!(named.ends_with(&format!("(pid {me})")), "{named}");
        // Another start time: a later process with the same pid.
        assert_eq!(program(proc, me, uid, Some(start + 1)), None);
        // Another user's.
        assert_eq!(program(proc, me, uid.wrapping_add(1), None), None);
        assert_eq!(program(Path::new("/nonexistent"), 7, uid, None), None);
    }

    #[cfg(unix)]
    #[test]
    fn only_the_same_users_holder_is_named() {
        let proc = FakeProc::new("holder");
        proc.process(4242, 500, 1000, "/home/nyu/.local/bin/thing");
        let holder = Holder::of(&proc.0, 4242).unwrap();
        assert_eq!(holder.start, 500);
        assert_eq!(
            program(&proc.0, 4242, 1000, Some(holder.start)).as_deref(),
            Some("/home/nyu/.local/bin/thing (pid 4242)")
        );
        // The holder went, and the pid went to root's (or anybody's)
        // process: not named, whoever's it is.
        proc.process(4242, 900, 0, "/usr/sbin/secret-daemon");
        assert_eq!(program(&proc.0, 4242, 1000, Some(holder.start)), None);
        // Same user, but a later process with the holder's pid.
        proc.process(4242, 900, 1000, "/usr/bin/other");
        assert_eq!(program(&proc.0, 4242, 1000, Some(holder.start)), None);
        // Another user's process with the right start time: not named.
        proc.process(4242, 500, 1001, "/home/other/bin/x");
        assert_eq!(program(&proc.0, 4242, 1000, Some(500)), None);
        // Real uid ours, effective root (a setuid program): not named.
        std::fs::write(proc.0.join("4242/status"), "Uid:\t1000\t0\t0\t0\n").unwrap();
        assert_eq!(program(&proc.0, 4242, 1000, Some(500)), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_path_cannot_forge_journal_lines() {
        let proc = FakeProc::new("forge");
        proc.process(
            77,
            1,
            1000,
            "/tmp/x\n<0>uwulock-uhid-broker: uid 1000's security key goes to /usr/bin/firefox",
        );
        let named = program(&proc.0, 77, 1000, Some(1)).unwrap();
        assert!(!named.contains('\n'), "{named}");
        assert!(named.starts_with("/tmp/x\\n<0>"), "{named}");
        // A very long path: cut.
        let long = format!("/tmp/{}", "a".repeat(250));
        let deep = format!("{long}/{long}/{long}");
        proc.process(78, 1, 1000, &deep);
        let named = program(&proc.0, 78, 1000, Some(1)).unwrap();
        assert!(
            named.len() <= broker::PATH_SHOWN + " (pid 78)".len(),
            "{}",
            named.len()
        );
        assert!(named.ends_with("… (pid 78)"), "{named}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn capabilities_go() {
        // Giving up what one has always works, also without any.
        std::thread::spawn(|| drop_capabilities().unwrap())
            .join()
            .unwrap();
    }
}
