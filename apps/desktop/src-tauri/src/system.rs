//! The commands only the Tauri app has: updates, where this copy came from,
//! the system bars' colours. Links out of the app are uwulock-app's
//! (commands.rs).
//!
//! The updater is the desktop's own (`self_update`, build.rs): a phone gets new
//! versions as a new APK or IPA from the release page (docs/mobile.md), an App
//! Store build from the store. There the update commands answer that nothing
//! is waiting, so the page needs no second code path.

#[cfg(self_update)]
use crate::updates::{self, Channel, UpdateInfo};
use tauri::AppHandle;

#[cfg(not(self_update))]
type Channel = serde_json::Value;
#[cfg(not(self_update))]
type UpdateInfo = serde_json::Value;

/// Where this copy came from, for the page's words about updates: `"store"` for
/// an App Store build (feature `store`), `"direct"` for everything else.
#[tauri::command]
pub(crate) fn distribution() -> &'static str {
    if cfg!(feature = "store") {
        "store"
    } else {
        "direct"
    }
}

#[tauri::command]
pub(crate) fn set_update_channel(app: AppHandle, channel: Channel) {
    #[cfg(self_update)]
    updates::set_channel(&app, channel);
    #[cfg(not(self_update))]
    let _ = (app, channel);
}

/// A downloaded update waiting for a restart, if any.
#[tauri::command]
pub(crate) fn update_status(app: AppHandle) -> Option<UpdateInfo> {
    #[cfg(self_update)]
    return updates::ready(&app);
    #[cfg(not(self_update))]
    {
        let _ = app;
        None
    }
}

#[tauri::command]
pub(crate) async fn check_for_updates(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    #[cfg(self_update)]
    return updates::check(&app).await;
    #[cfg(not(self_update))]
    {
        let _ = app;
        Ok(None)
    }
}

/// Async: a Linux package waits for the password prompt and the package
/// manager, which must not hold up the main thread.
#[tauri::command]
pub(crate) async fn install_update(app: AppHandle) -> Result<(), String> {
    #[cfg(self_update)]
    return updates::install_now(&app).await;
    #[cfg(not(self_update))]
    {
        let _ = app;
        Err(if cfg!(feature = "store") {
            "This copy gets new versions from the App Store.".into()
        } else {
            "A phone gets new versions from the release page.".into()
        })
    }
}

/// The page's theme, for what the system draws around it: on a phone the
/// status and navigation bars. Nothing to do on a computer, whose title bar
/// is the page's own.
#[tauri::command]
pub(crate) fn set_appearance(dark: bool) {
    #[cfg(mobile)]
    std::thread::spawn(move || {
        // tokens.css: --uwu-canvas, dark and light.
        let background = if dark { "#141016" } else { "#F8F4F6" };
        if let Some(plugin) = crate::phone::plugin() {
            if let Err(error) = plugin.set_appearance(dark, background) {
                tracing::debug!(%error, "couldn't colour the system bars");
            }
        }
    });
    #[cfg(desktop)]
    let _ = dark;
}

/// On Windows, DLLs loaded by name at runtime resolve from System32 only, never
/// the install folder or PATH. The runtime half of `/DEPENDENTLOADFLAG` in
/// `build.rs`, which only covers statically imported DLLs. Must run before
/// anything else loads a DLL.
pub(crate) fn restrict_dll_search() {
    #[cfg(windows)]
    // SAFETY: a process-wide flag, set once before any other thread exists.
    unsafe {
        use windows_sys::Win32::System::LibraryLoader::{
            SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_SYSTEM32,
        };
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32);
    }
}
