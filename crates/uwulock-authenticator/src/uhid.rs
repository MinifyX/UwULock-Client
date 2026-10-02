//! Linux's `/dev/uhid`: a HID device made by a program. What is written
//! there are `struct uhid_event`s (linux/uhid.h, packed): a 32-bit type and
//! a union, the largest member being `uhid_create2_req`. The kernel reads
//! and writes them whole; this module only puts them together and takes
//! them apart, so it is tested without a kernel. The app opens the file.
//!
//! The device is a FIDO one: usage page 0xF1D0, 64-byte reports in and out.
//! Browsers find it like a USB security key, through `/dev/hidraw*`.

/// sizeof(struct uhid_event): the type and `uhid_create2_req`.
pub const EVENT_SIZE: usize = 4 + 128 + 64 + 64 + 2 + 2 + 4 + 4 + 4 + 4 + DATA_MAX;
const DATA_MAX: usize = 4096;

// enum uhid_event_type
const DESTROY: u32 = 1;
const START: u32 = 2;
const STOP: u32 = 3;
const OPEN: u32 = 4;
const CLOSE: u32 = 5;
const OUTPUT: u32 = 6;
const GET_REPORT: u32 = 9;
const GET_REPORT_REPLY: u32 = 10;
const CREATE2: u32 = 11;
const INPUT2: u32 = 12;
const SET_REPORT: u32 = 13;
const SET_REPORT_REPLY: u32 = 14;

const BUS_USB: u16 = 0x03;

/// The device's name, as `lsusb`-like tools and the browser's picker show it.
pub const NAME: &str = "UwULock Passkeys";
/// No USB vendor: the device is virtual. The udev rule matches on these.
pub const VENDOR: u32 = 0x0000;
pub const PRODUCT: u32 = 0x0000;

/// A FIDO authenticator's report descriptor (CTAP 2, 11.2.8.1): 64 bytes in,
/// 64 bytes out, no report ids.
pub const REPORT_DESCRIPTOR: &[u8] = &[
    0x06, 0xd0, 0xf1, // Usage Page (FIDO Alliance)
    0x09, 0x01, // Usage (CTAPHID)
    0xa1, 0x01, // Collection (Application)
    0x09, 0x20, //   Usage (Input Report Data)
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xff, 0x00, //   Logical Maximum (255)
    0x75, 0x08, //   Report Size (8)
    0x95, 0x40, //   Report Count (64)
    0x81, 0x02, //   Input (Data, Var, Abs)
    0x09, 0x21, //   Usage (Output Report Data)
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xff, 0x00, //   Logical Maximum (255)
    0x75, 0x08, //   Report Size (8)
    0x95, 0x40, //   Report Count (64)
    0x91, 0x02, //   Output (Data, Var, Abs)
    0xc0, // End Collection
];

fn event(kind: u32) -> Vec<u8> {
    let mut out = vec![0u8; EVENT_SIZE];
    out[..4].copy_from_slice(&kind.to_ne_bytes());
    out
}

/// `UHID_CREATE2`: the FIDO device appears.
pub fn create() -> Vec<u8> {
    let mut out = event(CREATE2);
    let mut at = 4;
    let mut put = |out: &mut Vec<u8>, bytes: &[u8], width: usize| {
        out[at..at + bytes.len()].copy_from_slice(bytes);
        at += width;
    };
    put(&mut out, NAME.as_bytes(), 128);
    put(&mut out, b"uwulock", 64); // phys
    put(&mut out, b"", 64); // uniq
    put(&mut out, &(REPORT_DESCRIPTOR.len() as u16).to_ne_bytes(), 2);
    put(&mut out, &BUS_USB.to_ne_bytes(), 2);
    put(&mut out, &VENDOR.to_ne_bytes(), 4);
    put(&mut out, &PRODUCT.to_ne_bytes(), 4);
    put(&mut out, &1u32.to_ne_bytes(), 4); // version
    put(&mut out, &0u32.to_ne_bytes(), 4); // country
    put(&mut out, REPORT_DESCRIPTOR, DATA_MAX);
    out
}

/// `UHID_DESTROY`: the device goes away.
pub fn destroy() -> Vec<u8> {
    event(DESTROY)
}

/// `UHID_INPUT2`: a report to the browser.
pub fn input(report: &[u8]) -> Vec<u8> {
    let report = &report[..report.len().min(DATA_MAX)];
    let mut out = event(INPUT2);
    out[4..6].copy_from_slice(&(report.len() as u16).to_ne_bytes());
    out[6..6 + report.len()].copy_from_slice(report);
    out
}

/// What the kernel says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Start,
    Stop,
    /// Somebody opened the hidraw device.
    Open,
    Close,
    /// A report from the browser, without the report number.
    Output(Vec<u8>),
    /// A feature report was asked for; FIDO has none. Answer with
    /// [`get_report_reply`].
    GetReport {
        id: u32,
    },
    SetReport {
        id: u32,
    },
    Other(u32),
}

/// An event read from `/dev/uhid`.
pub fn parse(event: &[u8]) -> Option<Incoming> {
    let kind = u32::from_ne_bytes(event.get(..4)?.try_into().ok()?);
    Some(match kind {
        START => Incoming::Start,
        STOP => Incoming::Stop,
        OPEN => Incoming::Open,
        CLOSE => Incoming::Close,
        OUTPUT => {
            // struct uhid_output_req { data[4096]; u16 size; u8 rtype; }
            let size = usize::from(u16::from_ne_bytes(
                event.get(4 + DATA_MAX..6 + DATA_MAX)?.try_into().ok()?,
            ));
            let data = event.get(4..4 + size.min(DATA_MAX))?;
            // hidraw hands over the report number first: 0 for a device
            // without numbered reports, then the 64 bytes.
            let data = match data.len() {
                65 if data[0] == 0 => &data[1..],
                _ => data,
            };
            Incoming::Output(data.to_vec())
        }
        GET_REPORT => Incoming::GetReport {
            id: u32::from_ne_bytes(event.get(4..8)?.try_into().ok()?),
        },
        SET_REPORT => Incoming::SetReport {
            id: u32::from_ne_bytes(event.get(4..8)?.try_into().ok()?),
        },
        other => Incoming::Other(other),
    })
}

/// "No such report" for a `GET_REPORT`, so the asker doesn't wait.
pub fn get_report_reply(id: u32) -> Vec<u8> {
    let mut out = event(GET_REPORT_REPLY);
    out[4..8].copy_from_slice(&id.to_ne_bytes());
    out[8..10].copy_from_slice(&5u16.to_ne_bytes()); // EIO
    out
}

/// "Not taken" for a `SET_REPORT`.
pub fn set_report_reply(id: u32) -> Vec<u8> {
    let mut out = event(SET_REPORT_REPLY);
    out[4..8].copy_from_slice(&id.to_ne_bytes());
    out[8..10].copy_from_slice(&5u16.to_ne_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_event_is_as_large_as_the_kernels() {
        // linux/uhid.h: 4 + sizeof(struct uhid_create2_req) = 4 + 4372.
        assert_eq!(EVENT_SIZE, 4376);
        let created = create();
        assert_eq!(created.len(), EVENT_SIZE);
        assert_eq!(
            u32::from_ne_bytes(created[..4].try_into().unwrap()),
            CREATE2
        );
        assert_eq!(&created[4..4 + NAME.len()], NAME.as_bytes());
        // rd_size after name, phys, uniq.
        let rd_size = u16::from_ne_bytes(created[260..262].try_into().unwrap());
        assert_eq!(usize::from(rd_size), REPORT_DESCRIPTOR.len());
        assert_eq!(
            u16::from_ne_bytes(created[262..264].try_into().unwrap()),
            BUS_USB
        );
        assert_eq!(
            &created[280..280 + REPORT_DESCRIPTOR.len()],
            REPORT_DESCRIPTOR
        );
    }

    #[test]
    fn reports_both_ways() {
        let report = [7u8; 64];
        let event = input(&report);
        assert_eq!(u32::from_ne_bytes(event[..4].try_into().unwrap()), INPUT2);
        assert_eq!(u16::from_ne_bytes(event[4..6].try_into().unwrap()), 64);
        assert_eq!(event[6..70], report);

        // From hidraw: the report number 0, then the report.
        let mut output = vec![0u8; EVENT_SIZE];
        output[..4].copy_from_slice(&OUTPUT.to_ne_bytes());
        output[5..69].copy_from_slice(&report);
        output[4 + DATA_MAX..6 + DATA_MAX].copy_from_slice(&65u16.to_ne_bytes());
        assert_eq!(parse(&output), Some(Incoming::Output(report.to_vec())));
        // Without it, as it is.
        output[4..68].copy_from_slice(&report);
        output[4 + DATA_MAX..6 + DATA_MAX].copy_from_slice(&64u16.to_ne_bytes());
        assert_eq!(parse(&output), Some(Incoming::Output(report.to_vec())));
    }

    #[test]
    fn other_events() {
        assert_eq!(parse(&event(OPEN)), Some(Incoming::Open));
        assert_eq!(parse(&event(START)), Some(Incoming::Start));
        let mut asked = event(GET_REPORT);
        asked[4..8].copy_from_slice(&42u32.to_ne_bytes());
        assert_eq!(parse(&asked), Some(Incoming::GetReport { id: 42 }));
        let reply = get_report_reply(42);
        assert_eq!(u32::from_ne_bytes(reply[4..8].try_into().unwrap()), 42);
        assert_eq!(parse(&[1, 2]), None);
        assert_eq!(destroy().len(), EVENT_SIZE);
    }
}
