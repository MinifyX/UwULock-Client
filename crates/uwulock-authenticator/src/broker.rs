//! What UwULock and its uhid broker say to each other.
//!
//! `/dev/uhid` stays root's: whoever may write there can make any HID
//! device — a keyboard that types into the terminal, a device that feeds
//! a buggy kernel driver. So on Linux a small root helper,
//! `uwulock-uhid-broker` (socket-activated by systemd), opens it and makes
//! exactly one device: UwULock's FIDO security key with the descriptor in
//! [`crate::uhid`], nothing else. The app talks to it over a unix socket in
//! frames: one byte kind, two bytes length (big-endian), the payload.
//!
//! - app → broker: only [`kind::REPORT`] with 64 bytes, an input report for
//!   the browser. Anything else ends the connection, and with it the device.
//! - broker → app: [`kind::READY`] once the device is there, then
//!   [`kind::REPORT`] (64 bytes from the browser), [`kind::OPEN`] (a program
//!   opened the device; the payload names its hidraw node, e.g. `hidraw7`),
//!   [`kind::CLOSE`], and [`kind::ERROR`] (a reason, then the end).

use std::io::{self, Read, Write};

/// Where the broker listens (the systemd socket unit).
pub const SOCKET: &str = "/run/uwulock/uhid-broker.sock";

/// A report's size: CTAPHID's packet.
pub const REPORT: usize = crate::ctaphid::PACKET;

/// The longest frame either side takes.
pub const MAX_FRAME: usize = 512;

pub mod kind {
    pub const READY: u8 = 0x00;
    pub const REPORT: u8 = 0x01;
    pub const OPEN: u8 = 0x02;
    pub const CLOSE: u8 = 0x03;
    pub const ERROR: u8 = 0x7f;
}

/// The start of the broker's [`kind::ERROR`] reason when another program of
/// the same user holds the security key already (one per user). After it,
/// [`held`] names that program as the broker saw it, when it could.
pub const HELD: &str = "this user's security key is already there";

/// The reason for "already there", with the program holding the key.
pub fn held(holder: Option<&str>) -> String {
    match holder {
        Some(holder) => format!("{HELD}: held by {holder}"),
        None => HELD.to_string(),
    }
}

/// Whether `reason` (a [`kind::ERROR`] payload) says another program holds
/// the key, and which one: `Some(None)` when the broker couldn't tell.
pub fn holder_of(reason: &str) -> Option<Option<&str>> {
    let rest = reason.strip_prefix(HELD)?;
    if rest.is_empty() {
        return Some(None);
    }
    Some(rest.strip_prefix(": held by ").filter(|h| !h.is_empty()))
}

/// A frame.
pub fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let payload = &payload[..payload.len().min(MAX_FRAME)];
    let mut out = Vec::with_capacity(3 + payload.len());
    out.push(kind);
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Writes one frame whole.
pub fn send(to: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    to.write_all(&frame(kind, payload))
}

/// Reads one frame. A frame longer than [`MAX_FRAME`] is an error.
pub fn receive(from: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 3];
    from.read_exact(&mut head)?;
    let length = usize::from(u16::from_be_bytes([head[1], head[2]]));
    if length > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too long"));
    }
    let mut payload = vec![0u8; length];
    from.read_exact(&mut payload)?;
    Ok((head[0], payload))
}

/// The device's `uniq` for the user `uid`, so the broker finds its hidraw
/// node in sysfs.
pub fn uniq(uid: u32) -> String {
    format!("uwulock-{uid}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_names_the_holder() {
        assert_eq!(holder_of(&held(None)), Some(None));
        assert_eq!(
            holder_of(&held(Some("/home/nyu/.cache/x (pid 42)"))),
            Some(Some("/home/nyu/.cache/x (pid 42)"))
        );
        assert_eq!(
            holder_of("only the person at the seat gets a security key"),
            None
        );
    }

    #[test]
    fn frames_both_ways() {
        let bytes = frame(kind::REPORT, &[7; REPORT]);
        assert_eq!(bytes.len(), 3 + REPORT);
        assert_eq!(bytes[..3], [kind::REPORT, 0, 64]);
        let mut reader = &bytes[..];
        assert_eq!(
            receive(&mut reader).unwrap(),
            (kind::REPORT, vec![7; REPORT])
        );
        let mut empty = &frame(kind::READY, &[])[..];
        assert_eq!(receive(&mut empty).unwrap(), (kind::READY, vec![]));
        // Too long, or cut off: errors.
        let mut long = &[kind::REPORT, 0xff, 0xff, 0][..];
        assert!(receive(&mut long).is_err());
        let mut short = &bytes[..10];
        assert!(receive(&mut short).is_err());
        assert_eq!(uniq(1000), "uwulock-1000");
    }
}
