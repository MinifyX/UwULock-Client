//! The Safari extension the Apple builds carry (docs/extension.md): the same
//! extension as in Chrome and Firefox, with its own login and sync, in an app
//! extension Safari loads from UwULock. The app only says whether it is
//! switched on and opens Safari's settings for it.
//!
//! - **macOS**: `SFSafariExtensionManager` answers whether it is on,
//!   `SFSafariApplication` opens Safari's Extensions settings on it. Only in
//!   builds that carry it (`PlugIns/UwULockSafari.appex`): the disk image's
//!   and the Mac App Store's. Its id is the app's with `.safari`.
//! - **iOS**: no way to ask; the page says where to switch it on.
//! - Elsewhere: nothing.

use serde::Serialize;
use tauri::AppHandle;

use crate::vault::{Failure, Result};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafariView {
    /// `macos`, `ios` or `none`.
    platform: &'static str,
    /// This build carries the extension.
    available: bool,
    /// Switched on in Safari; `None` when Safari doesn't say (iOS, or no answer in time).
    enabled: Option<bool>,
}

#[tauri::command]
pub(crate) async fn safari_extension_status(app: AppHandle) -> SafariView {
    tauri::async_runtime::spawn_blocking(move || status(&app))
        .await
        .unwrap_or_default()
}

/// Opens Safari's settings on the extension (macOS).
#[tauri::command]
pub(crate) async fn safari_extension_open(app: AppHandle) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        if !mac::bundled() {
            return Err(Failure::new(
                "unsupported",
                "this build has no Safari extension",
            ));
        }
        let id = mac::identifier(&app);
        tauri::async_runtime::spawn_blocking(move || mac::open(&id))
            .await
            .map_err(|e| Failure::new("failed", e.to_string()))?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err(Failure::new("unsupported", "only on macOS"))
    }
}

#[cfg(target_os = "macos")]
fn status(app: &AppHandle) -> SafariView {
    let available = mac::bundled();
    SafariView {
        platform: "macos",
        available,
        enabled: available
            .then(|| mac::enabled(&mac::identifier(app)))
            .flatten(),
    }
}

#[cfg(target_os = "ios")]
fn status(_app: &AppHandle) -> SafariView {
    SafariView {
        platform: "ios",
        available: true,
        enabled: None,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn status(_app: &AppHandle) -> SafariView {
    SafariView {
        platform: "none",
        ..SafariView::default()
    }
}

/// SafariServices through the Objective-C runtime. The calls answer through a
/// block; they are waited for here (never on the main thread), with a time limit.
#[cfg(target_os = "macos")]
mod mac {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::{msg_send, sel};
    use std::sync::mpsc;
    use std::time::Duration;
    use tauri::AppHandle;

    use crate::vault::{Failure, Result};

    #[link(name = "SafariServices", kind = "framework")]
    unsafe extern "C" {}

    /// The extension's bundle id: the app's with `.safari` (app.uwulock.desktop.safari in the
    /// disk image, app.uwulock.safari in the Mac App Store's).
    pub(super) fn identifier(app: &AppHandle) -> String {
        format!("{}.safari", app.config().identifier)
    }

    /// Whether this build carries the extension: UwULock.app/Contents/PlugIns/UwULockSafari.appex.
    pub(super) fn bundled() -> bool {
        std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent()?
                    .parent()
                    .map(|contents| contents.to_path_buf())
            })
            .is_some_and(|contents| contents.join("PlugIns/UwULockSafari.appex").is_dir())
    }

    fn ns_string(text: &str) -> Option<Retained<AnyObject>> {
        let class = AnyClass::get(c"NSString")?;
        let c = std::ffi::CString::new(text).ok()?;
        // SAFETY: `stringWithUTF8String:` copies the NUL-terminated UTF-8 it is given and returns
        // an autoreleased NSString (or nil), which `Retained` keeps alive.
        unsafe {
            let string: *mut AnyObject = msg_send![class, stringWithUTF8String: c.as_ptr()];
            Retained::retain(string)
        }
    }

    /// Whether the extension is switched on in Safari; `None` when Safari doesn't answer.
    pub(super) fn enabled(id: &str) -> Option<bool> {
        let manager = AnyClass::get(c"SFSafariExtensionManager")?;
        let selector = sel!(getStateOfSafariExtensionWithIdentifier:completionHandler:);
        // SAFETY: asking a class whether it answers a selector has no side effects.
        let answers: Bool = unsafe { msg_send![manager, respondsToSelector: selector] };
        if !answers.as_bool() {
            return None;
        }
        let id = ns_string(id)?;
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |state: *mut AnyObject, _error: *mut AnyObject| {
            let enabled = (!state.is_null()).then(|| {
                // SAFETY: `state` is the SFSafariExtensionState Safari hands the block.
                let on: Bool = unsafe { msg_send![state, isEnabled] };
                on.as_bool()
            });
            let _ = tx.send(enabled);
        });
        // SAFETY: the class method takes an NSString and a block of
        // `void (^)(SFSafariExtensionState *, NSError *)`, as `block` is; checked above.
        unsafe {
            let _: () = msg_send![
                manager,
                getStateOfSafariExtensionWithIdentifier: &*id,
                completionHandler: &*block
            ];
        }
        rx.recv_timeout(Duration::from_secs(5)).ok().flatten()
    }

    /// Safari's settings, on the Extensions pane with UwULock's selected.
    pub(super) fn open(id: &str) -> Result<()> {
        let application = AnyClass::get(c"SFSafariApplication")
            .ok_or_else(|| Failure::new("unsupported", "SafariServices isn't there"))?;
        let selector = sel!(showPreferencesForExtensionWithIdentifier:completionHandler:);
        // SAFETY: asking a class whether it answers a selector has no side effects.
        let answers: Bool = unsafe { msg_send![application, respondsToSelector: selector] };
        if !answers.as_bool() {
            return Err(Failure::new("unsupported", "Safari can't be asked"));
        }
        let id = ns_string(id).ok_or_else(|| Failure::new("failed", "no NSString"))?;
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |error: *mut AnyObject| {
            let _ = tx.send(error.is_null());
        });
        // SAFETY: the class method takes an NSString and a block of `void (^)(NSError *)`, as
        // `block` is; checked above.
        unsafe {
            let _: () = msg_send![
                application,
                showPreferencesForExtensionWithIdentifier: &*id,
                completionHandler: &*block
            ];
        }
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(true) | Err(_) => Ok(()),
            Ok(false) => Err(Failure::new(
                "failed",
                "Safari didn't open its settings for UwULock",
            )),
        }
    }
}
