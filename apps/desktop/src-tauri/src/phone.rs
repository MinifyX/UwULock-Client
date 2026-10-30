//! Android: the plugin with UwULock's own phone code
//! (`tauri-plugin-uwulock-mobile`). Locking after a while in the background
//! is the app's (uwulock-app's phone.rs); lib.rs tells it when UwULock leaves
//! the screen and comes back.

use std::sync::OnceLock;
use tauri::{AppHandle, Manager, State, Wry};
use tauri_plugin_uwulock_mobile::Mobile;

static APP: OnceLock<AppHandle> = OnceLock::new();

pub(crate) fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// The plugin, once the app is set up. Its calls block until the phone
/// answers: only ever from a thread of their own, never a command's.
pub(crate) fn plugin() -> Option<State<'static, Mobile<Wry>>> {
    APP.get()?.try_state::<Mobile<Wry>>()
}
