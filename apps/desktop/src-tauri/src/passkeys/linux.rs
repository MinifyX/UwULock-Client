//! Linux: UwULock as a FIDO2 security key, made with `/dev/uhid`.
//!
//! The kernel turns the device into a `/dev/hidraw*` like a USB key's, and
//! browsers (Firefox, Chromium, anything with libfido2) talk CTAP2 to it.
//! Every request that needs the person opens UwULock's dialog; meanwhile the
//! key sends keepalives ("waiting for the user"), and the browser may cancel.
//!
//! Who may open `/dev/uhid` decides who can make security keys: root by
//! default. The packages bring a udev rule (`60-uwulock-passkeys.rules`)
//! that gives the person at the seat access (`uaccess`), and load the
//! `uhid` module at boot. Off until switched on in the settings.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use uwulock_authenticator::ctap2::Authenticator;
use uwulock_authenticator::ctaphid::{self, Event, Hid};
use uwulock_authenticator::uhid::{self, Incoming};

use super::{DesktopBackend, Provider};

const UHID: &str = "/dev/uhid";

/// The running key: the open `/dev/uhid` and the flag that stops it.
pub(crate) struct Device {
    file: Arc<File>,
    stop: Arc<AtomicBool>,
}

fn open() -> std::io::Result<File> {
    OpenOptions::new().read(true).write(true).open(UHID)
}

/// Why the key can't run here, in words for the settings.
pub(crate) fn problem() -> Option<String> {
    match open() {
        Ok(_) => None,
        Err(error) if error.kind() == ErrorKind::NotFound => Some(
            "Linux has no /dev/uhid: the uhid kernel module isn't loaded (sudo modprobe uhid)."
                .into(),
        ),
        Err(error) if error.kind() == ErrorKind::PermissionDenied => Some(
            "No access to /dev/uhid: the udev rule from UwULock's package is missing, or you need to log in again once after installing."
                .into(),
        ),
        Err(error) => Some(format!("/dev/uhid: {error}")),
    }
}

/// Writes one event; uhid takes each in a single write.
fn send(file: &File, event: &[u8]) -> std::io::Result<()> {
    let mut writer = file;
    writer.write_all(event)
}

fn send_packets(file: &File, packets: &[ctaphid::Packet]) {
    for packet in packets {
        if let Err(error) = send(file, &uhid::input(packet)) {
            tracing::warn!(%error, "couldn't answer the browser");
            return;
        }
    }
}

pub(crate) fn start(app: &AppHandle) -> Result<(), String> {
    let provider = app.state::<Provider>();
    let mut device = provider.device.lock();
    if device.is_some() {
        return Ok(());
    }
    let file = Arc::new(open().map_err(|e| problem().unwrap_or_else(|| e.to_string()))?);
    send(&file, &uhid::create()).map_err(|e| format!("couldn't make the security key: {e}"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let reader = (Arc::clone(&file), Arc::clone(&stop), app.clone());
    std::thread::Builder::new()
        .name("uwulock-security-key".into())
        .spawn(move || run(reader.0, reader.1, reader.2))
        .map_err(|e| e.to_string())?;
    *device = Some(Device { file, stop });
    tracing::info!("the virtual security key is there");
    Ok(())
}

pub(crate) fn stop(app: &AppHandle) {
    let Some(device) = app.state::<Provider>().device.lock().take() else {
        return;
    };
    device.stop.store(true, Ordering::Relaxed);
    // The kernel answers with UHID_STOP, which wakes the reader.
    let _ = send(&device.file, &uhid::destroy());
    tracing::info!("the virtual security key is gone");
}

/// The request in flight: its channel, and the flag that cancels it.
struct InFlight {
    cid: u32,
    cancelled: Arc<AtomicBool>,
}

fn run(file: Arc<File>, stop: Arc<AtomicBool>, app: AppHandle) {
    let hid = Arc::new(Mutex::new(Hid::default()));
    let in_flight: Arc<Mutex<Option<InFlight>>> = Arc::default();
    let mut buffer = vec![0u8; uhid::EVENT_SIZE];
    loop {
        let read = (&*file).read(&mut buffer);
        let event = match read {
            Ok(0) => break,
            Ok(n) => uhid::parse(&buffer[..n]),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                tracing::warn!(%error, "reading /dev/uhid failed");
                break;
            }
        };
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match event {
            Some(Incoming::Output(report)) => {
                let event = hid.lock().receive(&report);
                match event {
                    Event::Reply(packets) => send_packets(&file, &packets),
                    Event::Pending => {}
                    Event::Cancel { cid } => {
                        if let Some(flight) = in_flight.lock().as_ref().filter(|f| f.cid == cid) {
                            flight.cancelled.store(true, Ordering::Relaxed);
                        }
                    }
                    Event::Cbor { cid, request } => {
                        let cancelled = Arc::new(AtomicBool::new(false));
                        *in_flight.lock() = Some(InFlight {
                            cid,
                            cancelled: Arc::clone(&cancelled),
                        });
                        let (file, worker_hid, in_flight, app) = (
                            Arc::clone(&file),
                            Arc::clone(&hid),
                            Arc::clone(&in_flight),
                            app.clone(),
                        );
                        let spawned = std::thread::Builder::new()
                            .name("uwulock-security-key-request".into())
                            .spawn(move || {
                                let answer = answer(&app, &file, cid, &request, &cancelled);
                                send_packets(
                                    &file,
                                    &ctaphid::packets(cid, ctaphid::cmd::CBOR, &answer),
                                );
                                worker_hid.lock().finish(cid);
                                let mut flight = in_flight.lock();
                                if flight.as_ref().is_some_and(|f| f.cid == cid) {
                                    *flight = None;
                                }
                            });
                        if spawned.is_err() {
                            hid.lock().finish(cid);
                        }
                    }
                }
            }
            Some(Incoming::GetReport { id }) => {
                let _ = send(&file, &uhid::get_report_reply(id));
            }
            Some(Incoming::SetReport { id }) => {
                let _ = send(&file, &uhid::set_report_reply(id));
            }
            _ => {}
        }
    }
    tracing::debug!("the security key's reader stopped");
}

/// One CTAP2 request through the authenticator, with keepalives while the
/// person decides.
fn answer(
    app: &AppHandle,
    file: &File,
    cid: u32,
    request: &[u8],
    cancelled: &AtomicBool,
) -> Vec<u8> {
    let mut tick = || {
        let packet = ctaphid::keepalive(cid, ctaphid::keepalive::UP_NEEDED);
        let _ = send(file, &uhid::input(&packet));
    };
    let mut authenticator = Authenticator::new(DesktopBackend {
        app: app.clone(),
        client: "browser".into(),
        cancelled,
        tick: &mut tick,
    });
    authenticator.handle(request)
}
