//! UwULock as the system's AutoFill provider for passwords and passkeys:
//! whether it is switched on, and asking the system to switch it on.
//!
//! - **iOS** (17+): Settings → General → AutoFill & Passwords. iOS 18 asks in a
//!   sheet of its own (`ASSettingsHelper.requestToTurnOnCredentialProviderExtension`),
//!   iOS 17 opens those settings.
//! - **macOS** (14+, only the builds with the extension — the Mac App Store's):
//!   the same calls.
//! - **Android**: Credential Manager's provider (14+) and the autofill service,
//!   each with the system's own sheet.
//! - Windows and Linux have no such provider: nothing.
//!
//! On iOS and macOS asking also switches on UwULock's own setting, which
//! leaves the extension its sealed list (passkeys/apple.rs): the system's
//! switch alone fills nothing.
//!
//! Also iOS 26's credential exchange: what Apple Passwords hands over waits in
//! the host until the page takes it in (`credential_exchange_*`).
//!
//! Asking the system is the host's ([`Host::autofill_provider_status`](crate::host::Host::autofill_provider_status) and
//! the rest); what UwULock's own setting does with the answer is here.

use serde::Serialize;

use crate::vault::{Failure, Result};
use crate::App;

/// What the page shows: the card after the first unlock, the settings row.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    /// `ios`, `macos`, `android` or `none`.
    platform: &'static str,
    /// This system can have UwULock as its provider.
    supported: bool,
    /// Switched on in the system; `None` when it doesn't say.
    enabled: Option<bool>,
    /// The system asks the person itself (iOS 18+, macOS 15+, Android);
    /// otherwise the button opens the settings.
    direct: bool,
    /// Android: the autofill service is UwULock's too.
    autofill: Option<bool>,
    /// iOS and macOS: UwULock's own setting that leaves the extension its list.
    list: bool,
}

impl App {
    pub async fn autofill_provider_status(&self) -> ProviderView {
        let app = self.clone();
        crate::runtime::spawn_blocking(move || status(&app))
            .await
            .unwrap_or_default()
    }
}

impl App {
    /// Asks the system to make UwULock the provider. `target`: `credentials`
    /// (passwords and passkeys) or, on Android, `autofill` (the autofill
    /// service for apps and browsers without Credential Manager).
    pub async fn autofill_provider_request(&self, target: String) -> Result<ProviderView> {
        let app = self.clone();
        let target = match target.as_str() {
            "credentials" | "autofill" => target,
            _ => return Err(Failure::new("invalid", "not a provider")),
        };
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        list_on(&app)?;
        crate::runtime::spawn_blocking(move || request(&app, &target))
            .await
            .map_err(|e| Failure::new("failed", e.to_string()))?
    }
}

/// What the system says about UwULock as its provider ([`Host`](crate::host::Host)).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderState {
    /// This system can have UwULock as its provider.
    pub supported: bool,
    /// Switched on in the system; `None` when it doesn't say.
    pub enabled: Option<bool>,
    /// The system asks the person itself; otherwise the settings open.
    pub direct: bool,
    /// Android: the autofill service is UwULock's too.
    pub autofill: Option<bool>,
}

/// UwULock's own setting on: the extension gets its list.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn list_on(app: &App) -> Result<()> {
    let mut settings = app.passkeys().settings();
    if !settings.apple_extension {
        settings.apple_extension = true;
        app.set_passkey_provider(settings)?;
    }
    Ok(())
}

/// UwULock is switched on in the system's AutoFill but its own setting is
/// off (turned on in Settings → AutoFill & Passwords rather than with the
/// app's button, or settings from before): the extension would keep an old
/// list, without logins. The system's switch is the person's say: the
/// setting follows it.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn follow_system(app: &App, enabled: Option<bool>) {
    if enabled == Some(true) && !list_setting(app) {
        tracing::info!("UwULock is on in the system's AutoFill: the extension gets its list");
        if let Err(error) = list_on(app) {
            tracing::warn!(
                error = error.message(),
                "the extension's list didn't switch on"
            );
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn follow_system(_app: &App, _enabled: Option<bool>) {}

/// Once the app runs: whether the system has UwULock on (see [`follow_system`]).
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub(crate) fn check_system(app: &App) {
    let app = app.clone();
    crate::runtime::spawn_blocking(move || {
        status(&app);
    });
}

/// iOS and macOS: UwULock's own setting that leaves the extension its list.
fn list_setting(app: &App) -> bool {
    cfg!(any(target_os = "ios", target_os = "macos")) && app.passkeys().settings().apple_extension
}

fn platform() -> &'static str {
    if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "none"
    }
}

fn view(app: &App, state: ProviderState) -> ProviderView {
    ProviderView {
        platform: platform(),
        supported: state.supported,
        enabled: state.enabled,
        direct: state.direct,
        autofill: state.autofill,
        list: list_setting(app),
    }
}

fn status(app: &App) -> ProviderView {
    match app.host().autofill_provider_status() {
        Ok(state) => {
            follow_system(app, state.enabled);
            view(app, state)
        }
        Err(error) => {
            tracing::debug!(%error, "the system didn't say whether UwULock is its provider");
            view(app, ProviderState::default())
        }
    }
}

fn request(app: &App, target: &str) -> Result<ProviderView> {
    let state = app.host().autofill_provider_request(target)?;
    follow_system(app, state.enabled);
    Ok(view(app, state))
}

// ── iOS 26: credentials another app hands over ─────────────

impl App {
    /// Whether another app (Apple Passwords) handed over credentials that wait.
    pub async fn credential_exchange_pending(&self) -> bool {
        let app = self.clone();
        crate::runtime::spawn_blocking(move || app.host().credential_exchange_pending())
            .await
            .unwrap_or(false)
    }

    /// The handed-over credentials as CXF JSON, for the page's import (which
    /// shows them before anything is saved); `discard` drops them. Only once.
    pub async fn credential_exchange_import(&self, discard: bool) -> Result<String> {
        let app = self.clone();
        crate::runtime::spawn_blocking(move || app.host().credential_exchange_import(discard))
            .await
            .map_err(|e| Failure::new("failed", e.to_string()))?
    }
}

// ── The AutoFill protocol ───────────────────────────────────

/// How much of the protocol is read: the extension keeps it under this
/// anyway (AutoFillLog.swift).
pub const LOG_BYTES: u64 = 64 * 1024;
/// How many lines the page shows at most, the newest.
const LOG_LINES: usize = 300;
/// A longer line is cut: the extension writes short ones, anything else
/// isn't its.
const LOG_LINE_CHARS: usize = 600;

/// What the AutoFill extension logged on this device (iOS, and the Mac
/// builds with the extension): which way the system came in, the steps, the
/// error codes. Hosts, counts and short id prefixes, never a password, a
/// user name or a full address.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutofillLog {
    /// This system has the extension, so a protocol can exist.
    supported: bool,
    /// The last lines, oldest first.
    lines: Vec<String>,
    /// Why there are none to read, when it isn't that there are none.
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

impl App {
    pub async fn autofill_log(&self) -> AutofillLog {
        let app = self.clone();
        crate::runtime::spawn_blocking(move || read_log(&app))
            .await
            .unwrap_or_default()
    }

    pub async fn autofill_log_clear(&self) -> Result<()> {
        let app = self.clone();
        crate::runtime::spawn_blocking(move || app.host().autofill_log_clear())
            .await
            .map_err(|e| Failure::new("failed", e.to_string()))?
    }
}

fn read_log(app: &App) -> AutofillLog {
    match app.host().autofill_log() {
        Some(Ok(text)) => AutofillLog {
            supported: true,
            lines: log_lines(&text),
            problem: None,
        },
        Some(Err(problem)) => {
            tracing::debug!(%problem, "the AutoFill protocol didn't read");
            AutofillLog {
                supported: true,
                lines: Vec::new(),
                problem: Some(problem),
            }
        }
        None => AutofillLog::default(),
    }
}

/// The last `max` bytes of a file, as text (a character cut in two at the
/// front becomes U+FFFD; [`log_lines`] drops that line anyway).
pub fn read_tail(path: &std::path::Path, max: u64) -> std::io::Result<String> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size > max {
        file.seek(SeekFrom::Start(size - max))?;
    }
    let mut bytes = Vec::with_capacity(size.min(max) as usize);
    file.take(max).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The protocol's lines worth showing: only whole entries (each starts with
/// its ISO 8601 time, so a line cut off at the front of a tail is left out),
/// control characters as spaces, overlong lines cut, the last [`LOG_LINES`].
fn log_lines(text: &str) -> Vec<String> {
    let entry = |line: &str| {
        let bytes = line.as_bytes();
        bytes.len() > 20 && bytes[..4].iter().all(u8::is_ascii_digit) && bytes[4] == b'-'
    };
    let kept: Vec<String> = text
        .lines()
        .filter(|line| entry(line))
        .map(|line| {
            line.chars()
                .take(LOG_LINE_CHARS)
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect()
        })
        .collect();
    let skip = kept.len().saturating_sub(LOG_LINES);
    kept.into_iter().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "2026-10-07T12:00:00.123Z [a1b2] entry prepareCredentialList(password)";

    #[test]
    fn log_lines_keep_whole_entries() {
        let text = format!("0:00.000Z [a1b2] cut off\n{LINE}\n\n  \n{LINE}\n");
        assert_eq!(log_lines(&text), vec![LINE.to_string(), LINE.to_string()]);
        assert!(log_lines("").is_empty());
        assert!(log_lines("not a log\n").is_empty());
    }

    #[test]
    fn log_lines_keep_the_newest() {
        let text: String = (0..LOG_LINES + 50)
            .map(|n| format!("2026-10-07T12:00:00.000Z [a1b2] step {n}\n"))
            .collect();
        let lines = log_lines(&text);
        assert_eq!(lines.len(), LOG_LINES);
        assert!(lines[0].ends_with("step 50"));
        assert!(lines[LOG_LINES - 1].ends_with(&format!("step {}", LOG_LINES + 49)));
    }

    #[test]
    fn log_lines_are_clean_and_short() {
        let long = format!("{LINE} {}\u{7}\r", "x".repeat(2 * LOG_LINE_CHARS));
        let lines = log_lines(&long);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].chars().count(), LOG_LINE_CHARS);
        assert!(!lines[0].chars().any(char::is_control));
        let bell = log_lines(&format!("{LINE}\u{7}"));
        assert!(bell[0].ends_with(' '));
    }

    #[test]
    fn read_tail_reads_the_end() {
        let path = std::env::temp_dir().join(format!(
            "uwulock-autofill-log-{}-{:?}.log",
            std::process::id(),
            std::thread::current().id()
        ));
        let text: String = (0..2000)
            .map(|n| format!("2026-10-07T12:00:00.000Z [a1b2] step {n}\n"))
            .collect();
        std::fs::write(&path, &text).unwrap();
        let tail = read_tail(&path, 1024).unwrap();
        assert_eq!(tail.len(), 1024);
        assert!(text.ends_with(&tail));
        let lines = log_lines(&tail);
        assert!(lines.last().unwrap().ends_with("step 1999"));
        assert!(read_tail(&path, 1 << 20).unwrap() == text);
        std::fs::remove_file(&path).unwrap();
        assert!(read_tail(&path, 1024).is_err());
    }
}
