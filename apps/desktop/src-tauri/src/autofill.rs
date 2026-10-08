//! UwULock as the system's AutoFill provider for passwords and passkeys:
//! whether it is switched on, and asking the system to switch it on.
//!
//! - **iOS** (17+): Settings → General → AutoFill & Passwords. iOS 18 asks in a
//!   sheet of its own (`ASSettingsHelper.requestToTurnOnCredentialProviderExtension`),
//!   iOS 17 opens those settings. Through the mobile plugin.
//! - **macOS** (14+, only the builds with the extension — the Mac App Store's):
//!   the same calls, made here through the Objective-C runtime.
//! - **Android**: Credential Manager's provider (14+) and the autofill service,
//!   each with the system's own sheet. Through the mobile plugin.
//! - Windows and Linux have no such provider: nothing.
//!
//! On iOS and macOS asking also switches on UwULock's own setting, which
//! leaves the extension its sealed list (passkeys/apple.rs): the system's
//! switch alone fills nothing.
//!
//! Also iOS 26's credential exchange: what Apple Passwords hands over waits in
//! the plugin until the page takes it in (`credential_exchange_*`).

use serde::Serialize;
use tauri::AppHandle;

use crate::vault::{Failure, Result};

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

#[tauri::command]
pub(crate) async fn autofill_provider_status(app: AppHandle) -> ProviderView {
    tauri::async_runtime::spawn_blocking(move || status(&app))
        .await
        .unwrap_or_default()
}

/// Asks the system to make UwULock the provider. `target`: `credentials`
/// (passwords and passkeys) or, on Android, `autofill` (the autofill
/// service for apps and browsers without Credential Manager).
#[tauri::command]
pub(crate) async fn autofill_provider_request(
    app: AppHandle,
    target: String,
) -> Result<ProviderView> {
    let target = match target.as_str() {
        "credentials" | "autofill" => target,
        _ => return Err(Failure::new("invalid", "not a provider")),
    };
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    list_on(&app)?;
    tauri::async_runtime::spawn_blocking(move || request(&app, &target))
        .await
        .map_err(|e| Failure::new("failed", e.to_string()))?
}

/// UwULock's own setting on: the extension gets its list.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn list_on(app: &AppHandle) -> Result<()> {
    use tauri::Manager as _;
    let mut settings = app.state::<crate::passkeys::Provider>().settings();
    if !settings.apple_extension {
        settings.apple_extension = true;
        crate::passkeys::set_passkey_provider(app.clone(), settings)?;
    }
    Ok(())
}

/// UwULock is switched on in the system's AutoFill but its own setting is
/// off (turned on in Settings → AutoFill & Passwords rather than with the
/// app's button, or settings from before): the extension would keep an old
/// list, without logins. The system's switch is the person's say: the
/// setting follows it.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn follow_system(app: &AppHandle, enabled: Option<bool>) {
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

/// Once the app runs: whether the system has UwULock on (see [`follow_system`]).
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub(crate) fn check_system(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        status(&app);
    });
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn list_setting(app: &AppHandle) -> bool {
    use tauri::Manager as _;
    app.state::<crate::passkeys::Provider>()
        .settings()
        .apple_extension
}

#[cfg(mobile)]
fn from_plugin(
    platform: &'static str,
    state: tauri_plugin_uwulock_mobile::ProviderState,
    list: bool,
) -> ProviderView {
    ProviderView {
        platform,
        supported: state.supported,
        enabled: state.enabled,
        direct: state.direct,
        autofill: state.autofill,
        list,
    }
}

#[cfg(mobile)]
fn platform() -> &'static str {
    if cfg!(target_os = "ios") {
        "ios"
    } else {
        "android"
    }
}

#[cfg(mobile)]
fn list_flag(app: &AppHandle) -> bool {
    #[cfg(target_os = "ios")]
    return list_setting(app);
    #[cfg(not(target_os = "ios"))]
    {
        let _ = app;
        false
    }
}

#[cfg(mobile)]
fn status(app: &AppHandle) -> ProviderView {
    let Some(plugin) = crate::phone::plugin() else {
        return ProviderView {
            platform: platform(),
            ..ProviderView::default()
        };
    };
    match plugin.provider_status() {
        Ok(state) => {
            #[cfg(target_os = "ios")]
            follow_system(app, state.enabled);
            from_plugin(platform(), state, list_flag(app))
        }
        Err(error) => {
            tracing::debug!(%error, "the phone didn't say whether UwULock is its provider");
            ProviderView {
                platform: platform(),
                list: list_flag(app),
                ..ProviderView::default()
            }
        }
    }
}

#[cfg(mobile)]
fn request(app: &AppHandle, target: &str) -> Result<ProviderView> {
    let plugin = crate::phone::plugin()
        .ok_or_else(|| Failure::new("unsupported", "the phone's plugin isn't there"))?;
    let state = plugin
        .provider_request(target)
        .map_err(|e| Failure::new("failed", e.message))?;
    Ok(from_plugin(platform(), state, list_flag(app)))
}

#[cfg(target_os = "macos")]
fn status(app: &AppHandle) -> ProviderView {
    let supported = crate::passkeys::apple::problem(app).is_none();
    let enabled = supported.then(mac::enabled).flatten();
    follow_system(app, enabled);
    ProviderView {
        platform: "macos",
        supported,
        enabled,
        direct: supported && mac::can_ask(),
        autofill: None,
        list: list_setting(app),
    }
}

#[cfg(target_os = "macos")]
fn request(app: &AppHandle, _target: &str) -> Result<ProviderView> {
    if crate::passkeys::apple::problem(app).is_some() {
        return Ok(status(app));
    }
    mac::ask(app);
    Ok(status(app))
}

#[cfg(not(any(mobile, target_os = "macos")))]
fn status(_app: &AppHandle) -> ProviderView {
    ProviderView {
        platform: "none",
        ..ProviderView::default()
    }
}

#[cfg(not(any(mobile, target_os = "macos")))]
fn request(app: &AppHandle, _target: &str) -> Result<ProviderView> {
    Ok(status(app))
}

/// macOS: AuthenticationServices through the Objective-C runtime. The calls
/// answer through a block; they are waited for here (never on the main
/// thread), with a time limit.
#[cfg(target_os = "macos")]
pub(crate) mod mac {
    use block2::RcBlock;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
    use objc2::{msg_send, sel};
    use std::sync::mpsc;
    use std::time::Duration;
    use tauri::AppHandle;

    #[link(name = "AuthenticationServices", kind = "framework")]
    unsafe extern "C" {}

    fn class(name: &std::ffi::CStr) -> Option<&'static AnyClass> {
        AnyClass::get(name)
    }

    fn responds(class: &AnyClass, selector: Sel) -> bool {
        // SAFETY: asking a class whether it answers a selector has no side effects.
        let answers: Bool = unsafe { msg_send![class, respondsToSelector: selector] };
        answers.as_bool()
    }

    /// Whether UwULock is switched on in System Settings; `None` when the
    /// system doesn't answer in time.
    pub(super) fn enabled() -> Option<bool> {
        let store_class = class(c"ASCredentialIdentityStore")?;
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |state: *mut AnyObject| {
            let enabled = if state.is_null() {
                false
            } else {
                // SAFETY: `state` is the ASCredentialIdentityStoreState the system hands the block.
                let on: Bool = unsafe { msg_send![state, isEnabled] };
                on.as_bool()
            };
            let _ = tx.send(enabled);
        });
        // SAFETY: `sharedStore` returns the shared instance; the Objective-C name of Swift's
        // `getState(_:)` is `getCredentialIdentityStoreStateWithCompletion:`, taking a block of
        // `void (^)(ASCredentialIdentityStoreState *)`, as `block` is. Checked before it is sent:
        // an unknown selector would raise and abort the app.
        unsafe {
            let store = shared_store(store_class)?;
            let selector = sel!(getCredentialIdentityStoreStateWithCompletion:);
            let answers: Bool = msg_send![store, respondsToSelector: selector];
            if !answers.as_bool() {
                return None;
            }
            let _: () = msg_send![store, getCredentialIdentityStoreStateWithCompletion: &*block];
        }
        rx.recv_timeout(Duration::from_secs(5)).ok()
    }

    /// `ASCredentialIdentityStore.sharedStore`; `store_class` is that class.
    fn shared_store(store_class: &AnyClass) -> Option<&'static AnyObject> {
        // SAFETY: `sharedStore` returns the shared instance (or nil), which lives as long as the
        // process.
        unsafe {
            let store: *mut AnyObject = msg_send![store_class, sharedStore];
            store.as_ref()
        }
    }

    /// Empties the system's list of UwULock's passwords and passkeys (logout, switched off): on
    /// macOS only the extension fills it, so nothing else would. Doesn't wait.
    pub(crate) fn remove_identities() {
        let Some(store_class) = class(c"ASCredentialIdentityStore") else {
            return;
        };
        let block = RcBlock::new(|_done: Bool, _error: *mut AnyObject| {});
        // SAFETY: `removeAllCredentialIdentitiesWithCompletion:` takes a block of
        // `void (^)(BOOL, NSError *)`, as `block` is; checked before it is sent.
        unsafe {
            let Some(store) = shared_store(store_class) else {
                return;
            };
            let selector = sel!(removeAllCredentialIdentitiesWithCompletion:);
            let answers: Bool = msg_send![store, respondsToSelector: selector];
            if answers.as_bool() {
                let _: () = msg_send![store, removeAllCredentialIdentitiesWithCompletion: &*block];
            }
        }
    }

    /// macOS 15+: the system asks the person itself.
    pub(super) fn can_ask() -> bool {
        class(c"ASSettingsHelper").is_some_and(|c| {
            responds(
                c,
                sel!(requestToTurnOnCredentialProviderExtensionWithCompletionHandler:),
            )
        })
    }

    /// Asks the system to switch UwULock on (macOS 15+), or opens the
    /// AutoFill settings (macOS 14). Waits for the person, at most two
    /// minutes.
    pub(super) fn ask(app: &AppHandle) {
        let Some(helper) = class(c"ASSettingsHelper") else {
            return;
        };
        let (tx, rx) = mpsc::channel::<()>();
        let direct = can_ask();
        let opens = responds(
            helper,
            sel!(openCredentialProviderAppSettingsWithCompletionHandler:),
        );
        if !direct && !opens {
            return;
        }
        let started = app.run_on_main_thread(move || {
            if direct {
                let tx = tx.clone();
                let block = RcBlock::new(move |_on: Bool| {
                    let _ = tx.send(());
                });
                // SAFETY: the class method takes a block of `void (^)(BOOL)`, as `block` is.
                unsafe {
                    let _: () = msg_send![
                        helper,
                        requestToTurnOnCredentialProviderExtensionWithCompletionHandler: &*block
                    ];
                }
            } else {
                let tx = tx.clone();
                let block = RcBlock::new(move |_error: *mut AnyObject| {
                    let _ = tx.send(());
                });
                // SAFETY: the class method takes a block of `void (^)(NSError *)`, as `block` is.
                unsafe {
                    let _: () = msg_send![
                        helper,
                        openCredentialProviderAppSettingsWithCompletionHandler: &*block
                    ];
                }
            }
        });
        if started.is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(120));
        }
    }
}

// ── iOS 26: credentials another app hands over ─────────────

/// Whether another app (Apple Passwords) handed over credentials that wait.
#[tauri::command]
pub(crate) async fn credential_exchange_pending() -> bool {
    #[cfg(target_os = "ios")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            crate::phone::plugin()
                .and_then(|plugin| plugin.credential_exchange_pending().ok())
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }
    #[cfg(not(target_os = "ios"))]
    false
}

/// The handed-over credentials as CXF JSON, for the page's import (which
/// shows them before anything is saved); `discard` drops them. Only once.
#[tauri::command]
pub(crate) async fn credential_exchange_import(discard: bool) -> Result<String> {
    #[cfg(target_os = "ios")]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let plugin = crate::phone::plugin()
                .ok_or_else(|| Failure::new("unsupported", "the phone's plugin isn't there"))?;
            plugin.credential_exchange_import(discard).map_err(|e| {
                Failure::new(
                    if e.is("missing") { "missing" } else { "failed" },
                    e.message,
                )
            })
        })
        .await
        .map_err(|e| Failure::new("failed", e.to_string()))?
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = discard;
        Err(Failure::new("unsupported", "only on iOS 26"))
    }
}

// ── The AutoFill protocol ───────────────────────────────────

/// How much of the protocol is read: the extension keeps it under this
/// anyway (AutoFillLog.swift).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const LOG_BYTES: u64 = 64 * 1024;
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

#[tauri::command]
pub(crate) async fn autofill_log() -> AutofillLog {
    tauri::async_runtime::spawn_blocking(read_log)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn autofill_log_clear() -> Result<()> {
    tauri::async_runtime::spawn_blocking(clear_log)
        .await
        .map_err(|e| Failure::new("failed", e.to_string()))?
}

#[cfg(target_os = "ios")]
fn read_log() -> AutofillLog {
    match crate::phone::plugin().map(|plugin| plugin.autofill_log()) {
        Some(Ok(Some(text))) => AutofillLog {
            supported: true,
            lines: log_lines(&text),
            problem: None,
        },
        Some(Ok(None)) => AutofillLog {
            supported: true,
            lines: Vec::new(),
            problem: Some("no App Group in this build".into()),
        },
        Some(Err(error)) => {
            tracing::debug!(%error, "the AutoFill protocol didn't read");
            AutofillLog {
                supported: true,
                lines: Vec::new(),
                problem: Some(error.to_string()),
            }
        }
        None => AutofillLog::default(),
    }
}

#[cfg(target_os = "ios")]
fn clear_log() -> Result<()> {
    let plugin = crate::phone::plugin()
        .ok_or_else(|| Failure::new("unsupported", "the phone's plugin isn't there"))?;
    plugin
        .autofill_log_clear()
        .map_err(|e| Failure::new("failed", e.message))
}

#[cfg(target_os = "macos")]
fn read_log() -> AutofillLog {
    let Ok(path) = crate::passkeys::apple::autofill_log_path() else {
        return AutofillLog::default();
    };
    let (lines, problem) = match read_tail(&path, LOG_BYTES) {
        Ok(text) => (log_lines(&text), None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Vec::new(), None),
        Err(error) => {
            tracing::debug!(%error, "the AutoFill protocol didn't read");
            (Vec::new(), Some(error.to_string()))
        }
    };
    AutofillLog {
        supported: true,
        lines,
        problem,
    }
}

#[cfg(target_os = "macos")]
fn clear_log() -> Result<()> {
    let path =
        crate::passkeys::apple::autofill_log_path().map_err(|e| Failure::new("unsupported", e))?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Failure::new("io", e.to_string())),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn read_log() -> AutofillLog {
    AutofillLog::default()
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn clear_log() -> Result<()> {
    Err(Failure::new(
        "unsupported",
        "no AutoFill extension on this system",
    ))
}

/// The last `max` bytes of a file, as text (a character cut in two at the
/// front becomes U+FFFD; [`log_lines`] drops that line anyway).
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn read_tail(path: &std::path::Path, max: u64) -> std::io::Result<String> {
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
#[cfg_attr(
    not(any(target_os = "ios", target_os = "macos", test)),
    allow(dead_code)
)]
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
