//! Linux: UwULock as a FIDO2 security key, made through `/dev/uhid`.
//!
//! The kernel turns the device into a `/dev/hidraw*` like a USB key's, and
//! browsers (Firefox, Chromium, anything with libfido2) talk CTAP2 to it.
//! Every request that needs the person opens UwULock's dialog; meanwhile the
//! key sends keepalives ("waiting for the user"), and the browser may cancel.
//!
//! `/dev/uhid` stays root's: whoever writes there can make any HID device,
//! a keyboard too. The packages bring a small root helper instead,
//! `uwulock-uhid-broker` (crates/uwulock-uhid-broker, socket-activated by
//! systemd): it makes only UwULock's FIDO device, only for the person at the
//! seat, and relays 64-byte reports over [`broker::SOCKET`]. Off until
//! switched on in the settings. docs/passkeys.md has the design.

use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use uwulock_authenticator::broker::{self, kind};
use uwulock_authenticator::ctap2::Authenticator;
use uwulock_authenticator::ctaphid::{self, Event, Hid};
use uwulock_authenticator::flight::Slot;

use super::{Client, DesktopBackend, Provider, Warning};

/// The running key: the connection to the broker and the flag that stops it.
pub(crate) struct Device {
    link: Link,
    stop: Arc<AtomicBool>,
}

/// The connection to the broker, written to from several threads: one
/// frame at a time.
#[derive(Clone)]
struct Link(Arc<Mutex<UnixStream>>);

impl Link {
    fn send(&self, packets: &[ctaphid::Packet]) {
        let mut stream = self.0.lock();
        for packet in packets {
            if let Err(error) = broker::send(&mut *stream, kind::REPORT, packet) {
                tracing::warn!(%error, "couldn't answer the browser");
                return;
            }
        }
    }
}

fn not_installed() -> String {
    "UwULock's helper for the security key isn't there: install UwULock's .deb, .rpm or AUR package, then run `sudo systemctl enable --now uwulock-uhid-broker.socket` once."
        .into()
}

/// Why the key can't run here, in words for the settings.
pub(crate) fn problem() -> Option<String> {
    (!Path::new(broker::SOCKET).exists()).then(not_installed)
}

/// Why there is no key.
#[derive(Debug)]
enum Failed {
    /// Another program of this user holds the one key the broker makes per
    /// user; the broker named it when it could.
    Held(Option<String>),
    Other(String),
}

impl From<String> for Failed {
    fn from(text: String) -> Self {
        Failed::Other(text)
    }
}

/// Connects to the broker and waits for the device.
fn connect() -> Result<UnixStream, Failed> {
    let mut stream = UnixStream::connect(broker::SOCKET).map_err(|error| {
        if matches!(
            error.kind(),
            ErrorKind::NotFound | ErrorKind::ConnectionRefused
        ) {
            not_installed()
        } else {
            format!("{}: {error}", broker::SOCKET)
        }
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    match broker::receive(&mut stream) {
        Ok((kind::READY, _)) => {}
        Ok((kind::ERROR, why)) => {
            let why = String::from_utf8_lossy(&why);
            if let Some(holder) = broker::holder_of(&why) {
                return Err(Failed::Held(holder.map(super::shown)));
            }
            return Err(format!("the security key helper said no: {why}").into());
        }
        Ok((other, _)) => return Err(format!("the security key helper said {other}").into()),
        Err(error) => return Err(format!("the security key helper didn't answer: {error}").into()),
    }
    stream.set_read_timeout(None).map_err(|e| e.to_string())?;
    Ok(stream)
}

/// Connects, and keeps the connection only if the key is still `wanted`
/// once it is there: switching off during the up to 5 s of `connect` found
/// no device to stop (R7 L-4). Called under the device lock, which `stop`
/// takes after the setting is saved.
fn connect_if<T, E>(
    wanted: impl Fn() -> bool,
    connect: impl FnOnce() -> Result<T, E>,
) -> Result<Option<T>, E> {
    let connection = connect()?;
    Ok(wanted().then_some(connection))
}

pub(crate) fn start(app: &AppHandle) -> Result<(), String> {
    keep_up(app);
    let provider = app.state::<Provider>();
    let mut device = provider.device.lock();
    if device.is_some() {
        return Ok(());
    }
    let connected = connect_if(|| provider.settings().security_key, connect);
    let stream = match connected {
        Ok(Some(stream)) => stream,
        // Switched off meanwhile: dropping the connection removes the device.
        Ok(None) => return Ok(()),
        Err(Failed::Held(holder)) => {
            super::warn(
                app,
                Some(Warning {
                    kind: "held",
                    holder: holder.clone(),
                }),
            );
            return Err(format!(
                "another program holds this user's security key: {}",
                holder
                    .as_deref()
                    .unwrap_or("the helper couldn't tell which")
            ));
        }
        Err(Failed::Other(error)) => return Err(error),
    };
    let reader = stream.try_clone().map_err(|e| e.to_string())?;
    let link = Link(Arc::new(Mutex::new(stream)));
    let stop = Arc::new(AtomicBool::new(false));
    let state = (link.clone(), Arc::clone(&stop), app.clone());
    let ours = link.clone();
    std::thread::Builder::new()
        .name("uwulock-security-key".into())
        .spawn(move || {
            let (link, stop, app) = state;
            run(reader, link, Arc::clone(&stop), app.clone());
            if !stop.load(Ordering::Relaxed) {
                // The broker went (the seat changed hands, it was restarted):
                // forget this device, so `keep_up` connects again.
                let provider = app.state::<Provider>();
                let mut device = provider.device.lock();
                if device
                    .as_ref()
                    .is_some_and(|d| Arc::ptr_eq(&d.link.0, &ours.0))
                {
                    *device = None;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    *device = Some(Device { link, stop });
    drop(device);
    super::warn(app, None);
    tracing::info!("the virtual security key is there");
    Ok(())
}

/// While the setting is on and no key is there (the broker said no, went
/// away, or the person wasn't at the seat yet), tries again: after 5 s, then
/// up to every minute. One such thread per process.
fn keep_up(app: &AppHandle) {
    static RUNNING: AtomicBool = AtomicBool::new(false);
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("uwulock-security-key-keeper".into())
        .spawn(move || {
            let mut wait = Duration::from_secs(5);
            loop {
                std::thread::sleep(wait);
                let provider = app.state::<Provider>();
                if !provider.settings().security_key || provider.device.lock().is_some() {
                    wait = Duration::from_secs(5);
                    continue;
                }
                match start(&app) {
                    Ok(()) => wait = Duration::from_secs(5),
                    Err(error) => {
                        tracing::debug!(%error, "the virtual security key still isn't there");
                        wait = (wait * 2).min(Duration::from_secs(60));
                    }
                }
            }
        });
    if spawned.is_err() {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn stop(app: &AppHandle) {
    // Off: nothing to warn about any more.
    super::warn(app, None);
    let Some(device) = app.state::<Provider>().device.lock().take() else {
        return;
    };
    device.stop.store(true, Ordering::Relaxed);
    // The broker sees the connection end and removes the device; the reader
    // wakes up with it.
    let _ = device.link.0.lock().shutdown(std::net::Shutdown::Both);
    tracing::info!("the virtual security key is gone");
}

fn run(mut reader: UnixStream, link: Link, stop: Arc<AtomicBool>, app: AppHandle) {
    let hid = Arc::new(Mutex::new(Hid::default()));
    // The request with the person, by channel; cleared only by its own
    // worker (by identity, R7 L-3).
    let in_flight: Arc<Slot<u32>> = Arc::default();
    // The key's hidraw node, as the broker named it when it was opened.
    let mut node = String::new();
    loop {
        let frame = broker::receive(&mut reader);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let (kind, payload) = match frame {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(%error, "the security key helper went away");
                break;
            }
        };
        match kind {
            kind::OPEN => node = String::from_utf8_lossy(&payload).into_owned(),
            kind::ERROR => {
                tracing::warn!(
                    reason = %String::from_utf8_lossy(&payload),
                    "the security key helper stopped"
                );
                break;
            }
            kind::REPORT => {
                // Held while the report is handled, so a worker's answer
                // never falls between a report and what it starts.
                let mut channels = hid.lock();
                match channels.receive(&payload) {
                    Event::Reply(packets) => link.send(&packets),
                    Event::Pending => {}
                    Event::Cancel { cid } => {
                        in_flight.cancel(&cid, false);
                    }
                    Event::Resync { cid, reply } => {
                        in_flight.cancel(&cid, true);
                        link.send(&reply);
                    }
                    Event::Cbor { cid, request } => {
                        let flight = in_flight.replace(cid);
                        let (link, worker_hid, slot, app, client) = (
                            link.clone(),
                            Arc::clone(&hid),
                            Arc::clone(&in_flight),
                            app.clone(),
                            // Who holds the key open as the request comes in.
                            who_has(&node),
                        );
                        let ours = flight.clone();
                        let spawned = std::thread::Builder::new()
                            .name("uwulock-security-key-request".into())
                            .spawn(move || {
                                let answer =
                                    answer(&app, &link, client, cid, &request, ours.cancelled());
                                // Slot and channel free first, then the
                                // answer, all before the next report.
                                let mut channels = worker_hid.lock();
                                if let Some(packets) = channels.answered(&slot, &ours, cid, &answer)
                                {
                                    link.send(&packets);
                                }
                            });
                        if spawned.is_err() {
                            in_flight.release(&flight);
                            channels.finish(cid);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    tracing::debug!("the security key's reader stopped");
}

/// Browsers UwULock knows, by their program's name, and how to call them.
const BROWSERS: &[(&str, &str)] = &[
    ("firefox", "Firefox"),
    ("firefox-bin", "Firefox"),
    ("firefox-esr", "Firefox"),
    ("librewolf", "LibreWolf"),
    ("waterfox", "Waterfox"),
    ("floorp", "Floorp"),
    ("zen", "Zen"),
    ("zen-bin", "Zen"),
    ("chrome", "Chrome"),
    ("google-chrome", "Chrome"),
    ("chromium", "Chromium"),
    ("chromium-browser", "Chromium"),
    ("brave", "Brave"),
    ("msedge", "Edge"),
    ("vivaldi-bin", "Vivaldi"),
    ("opera", "Opera"),
    ("thorium", "Thorium"),
];

/// A program holding the key open: its name, and whether it runs from where
/// the system installs programs (`/usr`, `/opt`, `/snap`, Flatpak's `/app`),
/// owned by root and not writable by others. Only such a browser counts as
/// one: a name alone anybody can take.
struct Program {
    name: String,
    installed: bool,
}

/// The programs holding the key's hidraw node open, as one [`Client`]:
/// trusted when all of them are browsers UwULock knows, installed by the
/// system. This tells the person who asks; it isn't proof (a program of the
/// same user can still drive an installed browser).
fn describe(programs: &[Program]) -> Client {
    if programs.is_empty() {
        return Client::default();
    }
    let mut names: Vec<String> = programs
        .iter()
        .map(|Program { name: program, .. }| {
            BROWSERS
                .iter()
                .find(|(name, _)| name == program)
                .map_or_else(|| program.clone(), |(_, shown)| (*shown).to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    Client {
        name: names.join(", "),
        trusted: programs.iter().all(|program| {
            program.installed && BROWSERS.iter().any(|(name, _)| *name == program.name)
        }),
    }
}

/// Whether `exe` lies where the system installs programs and only root may
/// change it (the file and every folder up to `/`).
fn installed(exe: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let system = ["/usr/", "/opt/", "/snap/", "/app/"]
        .iter()
        .any(|prefix| exe.starts_with(prefix));
    system
        && exe.ancestors().all(|part| {
            part.as_os_str().is_empty()
                || std::fs::metadata(part).is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0)
        })
}

/// Who holds `/dev/<hidraw>` open: every process of this user whose open
/// files include it (`/proc/*/fd`), by its program's name.
fn who_has(hidraw: &str) -> Client {
    let valid = hidraw.starts_with("hidraw") && hidraw[6..].bytes().all(|b| b.is_ascii_digit());
    if !valid || hidraw.len() == 6 {
        return Client::default();
    }
    let node = Path::new("/dev").join(hidraw);
    let mut programs = Vec::new();
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return Client::default();
    };
    for process in processes.flatten() {
        let Ok(fds) = std::fs::read_dir(process.path().join("fd")) else {
            continue;
        };
        let holds = fds
            .flatten()
            .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|target| target == node));
        if !holds {
            continue;
        }
        let exe = std::fs::read_link(process.path().join("exe")).ok();
        let program = exe
            .as_ref()
            .and_then(|exe| exe.file_name().map(|n| n.to_string_lossy().into_owned()))
            .map(|name| Program {
                installed: exe.as_deref().is_some_and(installed),
                name,
            })
            .or_else(|| {
                std::fs::read_to_string(process.path().join("comm"))
                    .ok()
                    .map(|comm| Program {
                        name: comm.trim().to_string(),
                        installed: false,
                    })
            });
        if let Some(program) = program {
            programs.push(program);
        }
    }
    describe(&programs)
}

/// One CTAP2 request through the authenticator, with keepalives while the
/// person decides.
fn answer(
    app: &AppHandle,
    link: &Link,
    client: Client,
    cid: u32,
    request: &[u8],
    cancelled: &AtomicBool,
) -> Vec<u8> {
    let mut tick = || link.send(&[ctaphid::keepalive(cid, ctaphid::keepalive::UP_NEEDED)]);
    let mut authenticator = Authenticator::new(DesktopBackend {
        app: app.clone(),
        client,
        cancelled,
        tick: &mut tick,
    });
    authenticator.handle(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(name: &str, installed: bool) -> Program {
        Program {
            name: name.into(),
            installed,
        }
    }

    #[test]
    fn callers_by_name() {
        let firefox = describe(&[at("firefox", true)]);
        assert_eq!(firefox.name, "Firefox");
        assert!(firefox.trusted);
        let both = describe(&[at("chrome", true), at("chromium", true), at("chrome", true)]);
        assert_eq!(both.name, "Chrome, Chromium");
        assert!(both.trusted);
        // Anything else is named as it is, and not trusted.
        let odd = describe(&[at("firefox", true), at("python3", true)]);
        assert_eq!(odd.name, "Firefox, python3");
        assert!(!odd.trusted);
        // A "firefox" from the home folder is named, but not trusted.
        let home = describe(&[at("firefox", false)]);
        assert_eq!(home.name, "Firefox");
        assert!(!home.trusted);
        let nobody = describe(&[]);
        assert!(nobody.name.is_empty() && !nobody.trusted);
        // Only hidraw node names are looked up.
        assert!(who_has("../uhid").name.is_empty());
        assert!(who_has("hidraw").name.is_empty());
    }

    #[test]
    fn switched_off_while_connecting() {
        use std::cell::Cell;
        let on = Cell::new(true);
        // The setting goes off during the connect: the connection is dropped.
        let kept = connect_if(
            || on.get(),
            || {
                on.set(false);
                Ok::<_, ()>("connection")
            },
        );
        assert_eq!(kept, Ok(None));
        on.set(true);
        assert_eq!(connect_if(|| on.get(), || Ok::<_, ()>(1)), Ok(Some(1)));
        assert_eq!(connect_if(|| on.get(), || Err::<u8, _>("no")), Err("no"));
    }

    #[test]
    fn installed_programs() {
        assert!(installed(Path::new("/usr/bin/env")) || !Path::new("/usr/bin/env").exists());
        assert!(!installed(Path::new("/tmp/firefox")));
        assert!(!installed(Path::new("/home/someone/firefox")));
        assert!(!installed(Path::new("/usr/../tmp/firefox")));
    }
}
