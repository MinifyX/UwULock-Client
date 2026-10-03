//! CTAPHID: CTAP2 over a HID device, in 64-byte reports.
//!
//! A message starts with an initialization packet (channel id, command with
//! the high bit set, a two-byte length, 57 bytes of data) and goes on in
//! continuation packets (channel id, sequence 0–127, 59 bytes). A browser
//! first asks for a channel of its own (`INIT` on the broadcast channel),
//! then sends `CBOR` messages on it; while the person decides, the device
//! sends `KEEPALIVE`s, and the browser may send `CANCEL`.
//!
//! [`Hid`] puts the packets together and answers what needs no person
//! (`INIT`, `PING`, errors) by itself; a whole `CBOR` request comes out as
//! [`Event::Cbor`] for the authenticator.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const PACKET: usize = 64;
const INIT_DATA: usize = PACKET - 7;
const CONT_DATA: usize = PACKET - 5;
/// The longest message: one initialization packet and 128 continuations.
pub const MAX_PAYLOAD: usize = INIT_DATA + 128 * CONT_DATA;
pub const BROADCAST: u32 = 0xffff_ffff;
/// A message whose next packet doesn't come within this is dropped (the
/// spec's transaction timeout), so one half-sent message can't block the
/// device for everybody else.
pub const TRANSACTION_TIMEOUT: Duration = Duration::from_secs(3);
/// Channels handed out at once; the least recently used one goes first.
const MAX_CHANNELS: usize = 32;

pub type Packet = [u8; PACKET];

pub mod cmd {
    pub const PING: u8 = 0x01;
    pub const MSG: u8 = 0x03;
    pub const LOCK: u8 = 0x04;
    pub const INIT: u8 = 0x06;
    pub const WINK: u8 = 0x08;
    pub const CBOR: u8 = 0x10;
    pub const CANCEL: u8 = 0x11;
    pub const KEEPALIVE: u8 = 0x3b;
    pub const ERROR: u8 = 0x3f;
}

pub mod err {
    pub const INVALID_CMD: u8 = 0x01;
    pub const INVALID_PAR: u8 = 0x02;
    pub const INVALID_LEN: u8 = 0x03;
    pub const INVALID_SEQ: u8 = 0x04;
    pub const MSG_TIMEOUT: u8 = 0x05;
    pub const CHANNEL_BUSY: u8 = 0x06;
    pub const INVALID_CHANNEL: u8 = 0x0b;
}

/// What a keepalive says the device is doing.
pub mod keepalive {
    pub const PROCESSING: u8 = 1;
    /// Waiting for the person: in UwULock, for the dialog.
    pub const UP_NEEDED: u8 = 2;
}

/// The capabilities INIT announces: CBOR, and no U2F messages (NMSG).
const CAPABILITIES: u8 = 0x04 | 0x08;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Packets to send back, already answered here.
    Reply(Vec<Packet>),
    /// A whole CTAP2 request on `cid`. The channel is busy until
    /// [`Hid::finish`].
    Cbor { cid: u32, request: Vec<u8> },
    /// The browser gave up on the request of `cid`.
    Cancel { cid: u32 },
    /// `cid` was initialised anew while its request is with the
    /// authenticator: send `reply` and cancel that request. The channel
    /// stays busy until [`Hid::finish`], so there is never more than one.
    Resync { cid: u32, reply: Vec<Packet> },
    /// A packet of a longer message; nothing to do yet.
    Pending,
}

/// A message being put together.
struct Partial {
    cid: u32,
    cmd: u8,
    length: usize,
    data: Vec<u8>,
    next_seq: u8,
    /// When its last packet came.
    last: Instant,
}

/// The device's side of CTAPHID.
pub struct Hid {
    partial: Option<Partial>,
    /// The channel whose CBOR request is with the authenticator.
    busy: Option<u32>,
    /// Channels handed out by INIT, the most recently used last.
    channels: VecDeque<u32>,
    next_channel: u32,
    /// Version bytes INIT reports: major, minor, build.
    pub version: [u8; 3],
}

impl Default for Hid {
    fn default() -> Self {
        Hid::new(rand::random())
    }
}

impl Hid {
    /// `seed` picks where channel ids start, so a new device doesn't hand out
    /// the ids an old one did.
    pub fn new(seed: u32) -> Self {
        Hid {
            partial: None,
            busy: None,
            channels: VecDeque::new(),
            next_channel: seed,
            version: [0, 0, 0],
        }
    }

    fn allocate(&mut self) -> u32 {
        loop {
            self.next_channel = self.next_channel.wrapping_add(1);
            let cid = self.next_channel;
            if cid != 0 && cid != BROADCAST && !self.channels.contains(&cid) {
                // Only so many at once; the least recently used one goes
                // first, never the one whose request is in flight.
                if self.channels.len() >= MAX_CHANNELS {
                    let busy = self.busy;
                    if let Some(at) = self.channels.iter().position(|c| Some(*c) != busy) {
                        self.channels.remove(at);
                    }
                }
                self.channels.push_back(cid);
                return cid;
            }
        }
    }

    /// Whether `cid` was handed out; marks it as used.
    fn known(&mut self, cid: u32) -> bool {
        let Some(at) = self.channels.iter().position(|c| *c == cid) else {
            return false;
        };
        if let Some(cid) = self.channels.remove(at) {
            self.channels.push_back(cid);
        }
        true
    }

    /// The request of `cid` is answered: the channel takes new ones.
    pub fn finish(&mut self, cid: u32) {
        if self.busy == Some(cid) {
            self.busy = None;
        }
    }

    /// One report from the browser.
    pub fn receive(&mut self, packet: &[u8]) -> Event {
        self.receive_at(packet, Instant::now())
    }

    /// One report from the browser, arriving at `now`.
    pub fn receive_at(&mut self, packet: &[u8], now: Instant) -> Event {
        if packet.len() < PACKET {
            return Event::Pending;
        }
        // A message left half-sent for too long is dropped.
        if self
            .partial
            .as_ref()
            .is_some_and(|p| now.saturating_duration_since(p.last) > TRANSACTION_TIMEOUT)
        {
            self.partial = None;
        }
        let cid = u32::from_be_bytes(packet[..4].try_into().unwrap());
        if cid == 0 {
            return Event::Reply(vec![error(cid, err::INVALID_CHANNEL)]);
        }
        let first = packet[4];
        if first & 0x80 != 0 {
            let cmd = first & 0x7f;
            let length = usize::from(u16::from_be_bytes([packet[5], packet[6]]));
            // CANCEL stops nothing that is being put together; it only goes
            // to the request in flight.
            if cmd == cmd::CANCEL {
                return if self.busy == Some(cid) {
                    Event::Cancel { cid }
                } else {
                    Event::Pending
                };
            }
            // A new INIT on a channel resets what it was sending.
            if let Some(partial) = &self.partial {
                if partial.cid != cid {
                    return Event::Reply(vec![error(cid, err::CHANNEL_BUSY)]);
                }
                if cmd != cmd::INIT {
                    self.partial = None;
                    return Event::Reply(vec![error(cid, err::INVALID_SEQ)]);
                }
                self.partial = None;
            }
            if length > MAX_PAYLOAD {
                return Event::Reply(vec![error(cid, err::INVALID_LEN)]);
            }
            let take = length.min(INIT_DATA);
            let partial = Partial {
                cid,
                cmd,
                length,
                data: packet[7..7 + take].to_vec(),
                next_seq: 0,
                last: now,
            };
            if partial.data.len() == length {
                return self.complete(partial);
            }
            self.partial = Some(partial);
            Event::Pending
        } else {
            let seq = first;
            let Some(mut partial) = self.partial.take() else {
                // A continuation of nothing: ignored, as the spec says.
                return Event::Pending;
            };
            if partial.cid != cid {
                self.partial = Some(partial);
                return Event::Reply(vec![error(cid, err::CHANNEL_BUSY)]);
            }
            if seq != partial.next_seq {
                return Event::Reply(vec![error(cid, err::INVALID_SEQ)]);
            }
            partial.next_seq += 1;
            partial.last = now;
            let take = (partial.length - partial.data.len()).min(CONT_DATA);
            partial.data.extend_from_slice(&packet[5..5 + take]);
            if partial.data.len() == partial.length {
                return self.complete(partial);
            }
            self.partial = Some(partial);
            Event::Pending
        }
    }

    fn complete(&mut self, message: Partial) -> Event {
        let Partial { cid, cmd, data, .. } = message;
        match cmd {
            cmd::INIT => {
                if data.len() != 8 {
                    return Event::Reply(vec![error(cid, err::INVALID_LEN)]);
                }
                if cid != BROADCAST && !self.known(cid) {
                    return Event::Reply(vec![error(cid, err::INVALID_CHANNEL)]);
                }
                let new = if cid == BROADCAST {
                    self.allocate()
                } else {
                    cid
                };
                let mut answer = data;
                answer.extend_from_slice(&new.to_be_bytes());
                answer.push(2); // CTAPHID protocol version
                answer.extend_from_slice(&self.version);
                answer.push(CAPABILITIES);
                let reply = packets(cid, cmd::INIT, &answer);
                // Re-initialising a channel keeps it, and cancels its request.
                if self.busy == Some(cid) {
                    return Event::Resync { cid, reply };
                }
                Event::Reply(reply)
            }
            _ if cid == BROADCAST || !self.known(cid) => {
                Event::Reply(vec![error(cid, err::INVALID_CHANNEL)])
            }
            cmd::PING => Event::Reply(packets(cid, cmd::PING, &data)),
            cmd::LOCK => Event::Reply(packets(cid, cmd::LOCK, &[])),
            cmd::CBOR => {
                if data.is_empty() {
                    return Event::Reply(vec![error(cid, err::INVALID_LEN)]);
                }
                // One request at a time, also on the channel that sent it.
                if self.busy.is_some() {
                    return Event::Reply(vec![error(cid, err::CHANNEL_BUSY)]);
                }
                self.busy = Some(cid);
                Event::Cbor { cid, request: data }
            }
            // No U2F, no wink: NMSG says so, and WINK isn't announced.
            cmd::MSG | cmd::WINK => Event::Reply(vec![error(cid, err::INVALID_CMD)]),
            _ => Event::Reply(vec![error(cid, err::INVALID_CMD)]),
        }
    }
}

/// A message cut into reports.
pub fn packets(cid: u32, cmd: u8, payload: &[u8]) -> Vec<Packet> {
    let payload = &payload[..payload.len().min(MAX_PAYLOAD)];
    let mut out = Vec::new();
    let mut first = [0u8; PACKET];
    first[..4].copy_from_slice(&cid.to_be_bytes());
    first[4] = 0x80 | cmd;
    first[5..7].copy_from_slice(&(payload.len() as u16).to_be_bytes());
    let take = payload.len().min(INIT_DATA);
    first[7..7 + take].copy_from_slice(&payload[..take]);
    out.push(first);
    for (seq, chunk) in payload[take..].chunks(CONT_DATA).enumerate() {
        let mut packet = [0u8; PACKET];
        packet[..4].copy_from_slice(&cid.to_be_bytes());
        packet[4] = seq as u8;
        packet[5..5 + chunk.len()].copy_from_slice(chunk);
        out.push(packet);
    }
    out
}

pub fn error(cid: u32, code: u8) -> Packet {
    packets(cid, cmd::ERROR, &[code])[0]
}

pub fn keepalive(cid: u32, status: u8) -> Packet {
    packets(cid, cmd::KEEPALIVE, &[status])[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Puts reports back together, as a browser does.
    fn join(packets: &[Packet]) -> (u32, u8, Vec<u8>) {
        let cid = u32::from_be_bytes(packets[0][..4].try_into().unwrap());
        let cmd = packets[0][4] & 0x7f;
        let length = usize::from(u16::from_be_bytes([packets[0][5], packets[0][6]]));
        let mut data = packets[0][7..].to_vec();
        for (seq, packet) in packets[1..].iter().enumerate() {
            assert_eq!(packet[4] as usize, seq);
            data.extend_from_slice(&packet[5..]);
        }
        data.truncate(length);
        (cid, cmd, data)
    }

    fn reply(event: Event) -> (u32, u8, Vec<u8>) {
        match event {
            Event::Reply(packets) => join(&packets),
            other => panic!("expected a reply, got {other:?}"),
        }
    }

    fn init(hid: &mut Hid) -> u32 {
        let nonce = [1, 2, 3, 4, 5, 6, 7, 8];
        let request = packets(BROADCAST, cmd::INIT, &nonce);
        let (cid, cmd, data) = reply(hid.receive(&request[0]));
        assert_eq!((cid, cmd), (BROADCAST, cmd::INIT));
        assert_eq!(data.len(), 17);
        assert_eq!(data[..8], nonce);
        assert_eq!(data[12], 2);
        assert_eq!(data[16], CAPABILITIES);
        u32::from_be_bytes(data[8..12].try_into().unwrap())
    }

    #[test]
    fn init_hands_out_channels() {
        let mut hid = Hid::new(7);
        let a = init(&mut hid);
        let b = init(&mut hid);
        assert_ne!(a, b);
        assert!(a != 0 && a != BROADCAST);
        // A channel nobody got is refused.
        let ping = packets(0x1234_5678, cmd::PING, b"hi");
        let (_, cmd, data) = reply(hid.receive(&ping[0]));
        assert_eq!((cmd, data), (cmd::ERROR, vec![err::INVALID_CHANNEL]));
    }

    #[test]
    fn a_long_ping_comes_back_whole() {
        let mut hid = Hid::new(1);
        let cid = init(&mut hid);
        let payload: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        let request = packets(cid, cmd::PING, &payload);
        assert_eq!(request.len(), 1 + (1000 - 57usize).div_ceil(59));
        for packet in &request[..request.len() - 1] {
            assert_eq!(hid.receive(packet), Event::Pending);
        }
        let (got_cid, cmd, data) = reply(hid.receive(request.last().unwrap()));
        assert_eq!((got_cid, cmd), (cid, cmd::PING));
        assert_eq!(data, payload);
    }

    #[test]
    fn cbor_requests_and_cancel() {
        let mut hid = Hid::new(1);
        let cid = init(&mut hid);
        let other = init(&mut hid);
        let request = packets(cid, cmd::CBOR, &[0x04]);
        assert_eq!(
            hid.receive(&request[0]),
            Event::Cbor {
                cid,
                request: vec![0x04]
            }
        );
        // Another channel waits until this one is answered.
        let (_, cmd, data) = reply(hid.receive(&packets(other, cmd::CBOR, &[0x04])[0]));
        assert_eq!((cmd, data), (cmd::ERROR, vec![err::CHANNEL_BUSY]));
        assert_eq!(
            hid.receive(&packets(cid, cmd::CANCEL, &[])[0]),
            Event::Cancel { cid }
        );
        hid.finish(cid);
        assert_eq!(
            hid.receive(&packets(cid, cmd::CANCEL, &[])[0]),
            Event::Pending
        );
        assert!(matches!(
            hid.receive(&packets(other, cmd::CBOR, &[0x04])[0]),
            Event::Cbor { .. }
        ));
    }

    #[test]
    fn one_request_at_a_time_and_no_stuck_messages() {
        let mut hid = Hid::new(1);
        let cid = init(&mut hid);
        let other = init(&mut hid);
        assert!(matches!(
            hid.receive(&packets(cid, cmd::CBOR, &[0x04])[0]),
            Event::Cbor { .. }
        ));
        // The same channel can't start a second request (R3-8) ...
        let (_, cmd, data) = reply(hid.receive(&packets(cid, cmd::CBOR, &[0x04])[0]));
        assert_eq!((cmd, data), (cmd::ERROR, vec![err::CHANNEL_BUSY]));
        // ... and re-initialising it cancels the one in flight, but the
        // channel stays busy until that one is answered.
        let mut again = [0u8; PACKET];
        again[..4].copy_from_slice(&cid.to_be_bytes());
        again[4] = 0x80 | cmd::INIT;
        again[6] = 8;
        match hid.receive(&again) {
            Event::Resync { cid: c, reply } => {
                assert_eq!(c, cid);
                assert_eq!(join(&reply).1, cmd::INIT);
            }
            other => panic!("{other:?}"),
        }
        let (_, cmd, _) = reply(hid.receive(&packets(cid, cmd::CBOR, &[0x04])[0]));
        assert_eq!(cmd, cmd::ERROR);
        hid.finish(cid);

        // A message left half-sent blocks others only until it times out.
        let start = Instant::now();
        let long = packets(cid, cmd::PING, &[9; 200]);
        assert_eq!(hid.receive_at(&long[0], start), Event::Pending);
        let (_, cmd, data) = reply(hid.receive_at(&packets(other, cmd::PING, &[1])[0], start));
        assert_eq!((cmd, data), (cmd::ERROR, vec![err::CHANNEL_BUSY]));
        let later = start + TRANSACTION_TIMEOUT + Duration::from_millis(1);
        let (_, cmd, data) = reply(hid.receive_at(&packets(other, cmd::PING, &[1])[0], later));
        assert_eq!((cmd, data), (cmd::PING, vec![1]));
        // A continuation in time keeps it going.
        assert_eq!(hid.receive_at(&long[0], later), Event::Pending);
        let step = later + Duration::from_secs(2);
        assert_eq!(hid.receive_at(&long[1], step), Event::Pending);
        let step = step + Duration::from_secs(2);
        assert_eq!(hid.receive_at(&long[2], step), Event::Pending);
        let (_, cmd, data) = reply(hid.receive_at(&long[3], step + Duration::from_secs(2)));
        assert_eq!((cmd, data.len()), (cmd::PING, 200));
    }

    #[test]
    fn busy_channels_survive_many_inits() {
        let mut hid = Hid::new(1);
        let browser = init(&mut hid);
        assert!(matches!(
            hid.receive(&packets(browser, cmd::CBOR, &[0x04])[0]),
            Event::Cbor { .. }
        ));
        for _ in 0..100 {
            init(&mut hid);
        }
        assert_eq!(
            hid.receive(&packets(browser, cmd::CANCEL, &[])[0]),
            Event::Cancel { cid: browser }
        );
        hid.finish(browser);
        // In use, it stays among the most recently used.
        assert!(matches!(
            hid.receive(&packets(browser, cmd::CBOR, &[0x04])[0]),
            Event::Cbor { .. }
        ));
        hid.finish(browser);
        for _ in 0..(MAX_CHANNELS - 1) {
            init(&mut hid);
        }
        let (_, cmd, _) = reply(hid.receive(&packets(browser, cmd::PING, &[1])[0]));
        assert_eq!(cmd, cmd::PING);
    }

    #[test]
    fn broken_sequences_are_refused() {
        let mut hid = Hid::new(1);
        let cid = init(&mut hid);
        let request = packets(cid, cmd::PING, &[9; 200]);
        assert_eq!(hid.receive(&request[0]), Event::Pending);
        // Skips sequence 0.
        let (_, cmd, data) = reply(hid.receive(&request[2]));
        assert_eq!((cmd, data), (cmd::ERROR, vec![err::INVALID_SEQ]));
        // Too long a message.
        let mut huge = packets(cid, cmd::PING, &[])[0];
        huge[5..7].copy_from_slice(&(MAX_PAYLOAD as u16 + 1).to_be_bytes());
        let (_, cmd, data) = reply(hid.receive(&huge));
        assert_eq!((cmd, data), (cmd::ERROR, vec![err::INVALID_LEN]));
        // U2F isn't spoken.
        let (_, cmd, data) = reply(hid.receive(&packets(cid, cmd::MSG, &[0; 10])[0]));
        assert_eq!((cmd, data), (cmd::ERROR, vec![err::INVALID_CMD]));
        // A continuation of nothing is dropped.
        let mut stray = [0u8; PACKET];
        stray[..4].copy_from_slice(&cid.to_be_bytes());
        assert_eq!(hid.receive(&stray), Event::Pending);
    }

    #[test]
    fn keepalives_and_errors_are_one_packet() {
        let packet = keepalive(0xaabb_ccdd, keepalive::UP_NEEDED);
        assert_eq!(
            packet[..8],
            [0xaa, 0xbb, 0xcc, 0xdd, 0x80 | cmd::KEEPALIVE, 0, 1, 2]
        );
        let packet = error(5, err::INVALID_PAR);
        assert_eq!(packet[4..8], [0x80 | cmd::ERROR, 0, 1, err::INVALID_PAR]);
    }
}
