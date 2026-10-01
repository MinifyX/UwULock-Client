//! App-level commands: updates and links out of the app.
//!
//! The updater is the desktop's: a phone gets new versions as a new APK or
//! IPA from the release page (docs/mobile.md). There the update commands
//! answer that nothing is waiting, so the page needs no second code path.

#[cfg(desktop)]
use crate::updates::{self, Channel, UpdateInfo};
use crate::vault::VaultState;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

#[cfg(mobile)]
type Channel = serde_json::Value;
#[cfg(mobile)]
type UpdateInfo = serde_json::Value;

#[tauri::command]
pub(crate) fn set_update_channel(app: AppHandle, channel: Channel) {
    #[cfg(desktop)]
    updates::set_channel(&app, channel);
    #[cfg(mobile)]
    let _ = (app, channel);
}

/// A downloaded update waiting for a restart, if any.
#[tauri::command]
pub(crate) fn update_status(app: AppHandle) -> Option<UpdateInfo> {
    #[cfg(desktop)]
    return updates::ready(&app);
    #[cfg(mobile)]
    {
        let _ = app;
        None
    }
}

#[tauri::command]
pub(crate) async fn check_for_updates(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    #[cfg(desktop)]
    return updates::check(&app).await;
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(None)
    }
}

/// Async: a Linux package waits for the password prompt and the package
/// manager, which must not hold up the main thread.
#[tauri::command]
pub(crate) async fn install_update(app: AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    return updates::install_now(&app).await;
    #[cfg(mobile)]
    {
        let _ = app;
        Err("A phone gets new versions from the release page.".into())
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

/// The project pages the app links to. The page names one; it never hands in
/// an address of its own.
#[tauri::command]
pub(crate) fn open_project_page(app: AppHandle, page: String) -> Result<(), String> {
    let url = match page.as_str() {
        "source" => "https://github.com/MinifyX/UwULock-Client",
        "releases" => "https://github.com/MinifyX/UwULock-Client/releases",
        "issues" => "https://github.com/MinifyX/UwULock-Client/issues",
        "license" => "https://www.gnu.org/licenses/gpl-3.0.html",
        "suite" => "https://uwu.minifyx.de",
        _ => return Err(format!("unknown page: {page}")),
    };
    open(&app, url)
}

/// Opens a login's address in the browser. The page names the item and the
/// address by position; only http and https leave the app.
#[tauri::command]
pub(crate) fn open_item_uri(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    index: usize,
) -> Result<(), String> {
    let uri = state
        .item_uri(&id, index)
        .ok_or("This address can't be opened.")?;
    let parsed = url::Url::parse(&uri).map_err(|_| "This address can't be opened.")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only web addresses open in the browser.".into());
    }
    open(&app, parsed.as_str())
}

/// The account's web vault, for everything UwULock can't do yet.
#[tauri::command]
pub(crate) fn open_web_vault(app: AppHandle, state: State<'_, VaultState>) -> Result<(), String> {
    let url = state.web_vault().ok_or("No account on this device.")?;
    open(&app, &url)
}

fn open(app: &AppHandle, url: &str) -> Result<(), String> {
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Couldn't open the browser: {e}"))
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
