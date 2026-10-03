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
}
