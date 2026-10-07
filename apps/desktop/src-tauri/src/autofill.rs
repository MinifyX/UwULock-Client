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
        Ok(state) => from_plugin(platform(), state, list_flag(app)),
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
    ProviderView {
        platform: "macos",
        supported,
        enabled: supported.then(mac::enabled).flatten(),
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
mod mac {
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
        // SAFETY: `sharedStore` returns the shared instance; `getState:` takes a block of
        // `void (^)(ASCredentialIdentityStoreState *)`, as `block` is.
        unsafe {
            let store: *mut AnyObject = msg_send![store_class, sharedStore];
            if store.is_null() {
                return None;
            }
            let _: () = msg_send![store, getState: &*block];
        }
        rx.recv_timeout(Duration::from_secs(5)).ok()
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
