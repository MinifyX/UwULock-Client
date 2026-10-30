//! The Tauri host around uwulock-app.
//!
//! Thin on purpose: every IPC command is a wrapper around the method of the
//! same name on [`uwulock_app::App`] (`commands`), which holds the vault,
//! the accounts and the background work and is tested without a window.
//! What the app needs from the program around it is [`host::TauriHost`].
//!
//! - [`commands`] — the IPC commands, one per method of the app
//! - [`host`] — events to the page, the window, the clipboard, biometrics,
//!   the phone's plugin
//! - [`system`] — what only the Tauri app has: updates, the system bars
//! - [`hello`] — Windows Hello's signature, Touch ID's Keychain item
//! - [`session_lock`] — whether the screen is locked; Linux's logind and
//!   screen saver
//! - [`phone`] — Android and iOS: the plugin
//! - [`passkeys`] — the providers that live with the platform: a virtual
//!   security key (Linux), a plugin passkey manager (Windows), Credential
//!   Manager (Android)
//! - [`apple`] — the AutoFill extension's system half (iOS, macOS)
//! - [`safari`] — Safari, asked about the Mac builds' extension
//!
//! The same app runs on Android and iOS (docs/mobile.md); what only a
//! desktop has — the updater, Windows Hello, the screen lock — is left out
//! there by `#[cfg(desktop)]`/`#[cfg(mobile)]`. The updater is `#[cfg(self_update)]`
//! (build.rs): the Mac App Store build has none either (docs/app-store.md).

#[cfg(any(target_os = "ios", target_os = "macos"))]
mod apple;
#[cfg(target_os = "macos")]
mod blocks;
mod commands;
mod hello;
mod host;
mod passkeys;
#[cfg(mobile)]
mod phone;
#[cfg(target_os = "macos")]
mod safari;
mod session_lock;
mod system;
#[cfg(self_update)]
mod updates;

use std::sync::Arc;

use tauri::Manager;
use uwulock_app::{App, Storage};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    system::restrict_dll_search();
    // One runtime for the app's background work and Tauri's async commands.
    tauri::async_runtime::set(uwulock_app::runtime::shared());

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("UWULOCK_LOG").unwrap_or_else(|_| {
            "uwulock=debug,uwulock_bitwarden=debug,uwulock_core=debug,warn".to_string()
        }))
        // Android's log and Xcode's console show no colours, only their codes.
        .with_ansi(cfg!(desktop))
        .init();

    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    #[cfg(self_update)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(mobile)]
    let builder = builder
        .plugin(tauri_plugin_uwulock_mobile::init())
        .plugin(tauri_plugin_haptics::init());

    builder
        .setup(|app| {
            // macOS ends an app without asking the window; this asks the page
            // first (`onMacQuit` in App.tsx, answered through `finish_quit`).
            #[cfg(target_os = "macos")]
            {
                use tauri::Emitter;
                let handle = app.handle().clone();
                if let Err(error) = uwu_macos::install_quit_guard(move || {
                    handle.emit(uwu_macos::QUIT_EVENT, ()).is_ok()
                }) {
                    tracing::warn!(%error, "quit guard");
                }
            }
            #[cfg(self_update)]
            if updates::apply_pending_on_start(app.handle()) {
                // The downloaded setup replaces this version and starts UwULock again.
                std::process::exit(0);
            }
            #[cfg(mobile)]
            phone::init(app.handle());

            // %APPDATA%\app.uwulock.desktop on Windows. UWULOCK_DATA_DIR points
            // elsewhere, so trying things out never touches the real account.
            let dir = match std::env::var_os("UWULOCK_DATA_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            let storage = Storage::new(dir)?;
            tracing::info!(path = %storage.dir().display(), "data folder");
            let uwulock = App::new(
                storage,
                Arc::new(host::TauriHost::new(app.handle().clone())),
            );
            app.manage(uwulock.clone());
            #[cfg(target_os = "android")]
            passkeys::android::init(&uwulock);
            uwulock.start();
            #[cfg(target_os = "linux")]
            session_lock::linux::watch(&uwulock);
            #[cfg(self_update)]
            updates::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            finish_quit,
            commands::vault::vault_status,
            commands::vault::login,
            commands::vault::login_two_factor,
            commands::vault::login_new_device,
            commands::vault::login_send_email,
            commands::vault::login_cancel,
            commands::vault::login_sso,
            commands::vault::login_sso_password,
            commands::vault::unlock,
            commands::vault::unlock_with_hello,
            commands::vault::set_hello,
            commands::vault::lock,
            commands::vault::logout,
            commands::vault::switch_account,
            commands::vault::rename_account,
            commands::vault::touch,
            commands::vault::set_security,
            commands::vault::sync_now,
            commands::vault::vault_overview,
            commands::vault::vault_items,
            commands::vault::vault_item,
            commands::vault::verify_reprompt,
            commands::vault::reveal_field,
            commands::vault::copy_field,
            commands::vault::copy_generated,
            commands::vault::copy_text,
            commands::vault::totp_code,
            commands::vault::generate_password,
            commands::vault::item_passkeys,
            commands::vault::delete_passkey,
            commands::vault::save_item,
            commands::vault::set_favorite,
            commands::vault::set_item_folder,
            commands::vault::delete_item,
            commands::vault::restore_item,
            commands::vault::save_folder,
            commands::vault::delete_folder,
            commands::extras::uwu_status,
            commands::extras::uwu_extras_key_seen,
            commands::extras::uwu_travel,
            commands::extras::open_web_vault_at,
            commands::extras::item_icons,
            commands::extras::set_own_icon,
            commands::extras::fetch_device_icon,
            commands::extras::delete_own_icon,
            commands::extras::icon_library,
            commands::extras::library_icon,
            commands::extras::device_icon,
            commands::extras::item_versions,
            commands::extras::reveal_version_field,
            commands::extras::restore_version,
            commands::extras::delete_versions,
            commands::extras::set_reminder,
            commands::extras::delete_reminder,
            commands::extras::file_requests,
            commands::extras::create_file_request,
            commands::extras::update_file_request,
            commands::extras::delete_file_request,
            commands::extras::file_request_submissions,
            commands::extras::save_submission_file,
            commands::extras::mark_submission_seen,
            commands::extras::delete_submission,
            commands::extras::take_over_submission,
            commands::extras::masked_connection,
            commands::extras::masked_addresses,
            commands::extras::create_masked_address,
            commands::extras::update_masked_address,
            commands::extras::delete_masked_address,
            commands::extras::send_options,
            commands::extras::share_as_send,
            commands::sends::sends,
            commands::sends::stage_send_file,
            commands::sends::save_send,
            commands::sends::remove_send_auth,
            commands::sends::delete_send,
            commands::health::health_report,
            commands::health::health_ignore,
            commands::health::health_open_page,
            commands::health::health_save_password,
            commands::health::health_email_opt_in,
            commands::health::set_health_email_opt_in,
            commands::health::health_check_emails,
            commands::importing::import_vault,
            commands::importing::import_open_bitwarden,
            commands::importing::import_kdbx_argon2,
            commands::importing::import_kdbx_aes_kdf,
            commands::moving::move_target,
            commands::moving::move_login,
            commands::moving::move_login_two_factor,
            commands::moving::move_login_new_device,
            commands::moving::move_login_send_email,
            commands::moving::move_start,
            commands::moving::move_cancel,
            commands::moving::move_close,
            commands::suite::suite_view,
            commands::suite::suite_create,
            commands::suite::suite_save,
            commands::suite::suite_reveal,
            commands::suite::suite_copy,
            commands::suite::suite_generate_key,
            commands::suite::suite_import_key,
            commands::suite::suite_save_key,
            commands::suite::suite_save_rdp,
            commands::suite::suite_open_in_app,
            system::distribution,
            system::set_update_channel,
            system::update_status,
            system::check_for_updates,
            system::install_update,
            commands::system::open_project_page,
            commands::system::open_item_uri,
            commands::system::open_web_vault,
            system::set_appearance,
            commands::wifi::wifi_connect,
            commands::wifi::wifi_settings,
            commands::passkeys::passkey_request,
            commands::passkeys::passkey_answer,
            commands::autofill::autofill_provider_status,
            commands::autofill::autofill_provider_request,
            commands::autofill::credential_exchange_pending,
            commands::autofill::credential_exchange_import,
            commands::autofill::autofill_log,
            commands::autofill::autofill_log_clear,
            commands::passkeys::passkey_provider_status,
            commands::passkeys::set_passkey_provider,
            commands::safari::safari_extension_status,
            commands::safari::safari_extension_open,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start UwULock")
        .run(|app, event| {
            #[cfg(mobile)]
            if let tauri::RunEvent::WindowEvent { event, .. } = &event {
                match event {
                    tauri::WindowEvent::Suspended => app.state::<App>().suspended(),
                    tauri::WindowEvent::Resumed => app.state::<App>().resumed(),
                    _ => {}
                }
            }
            // macOS: ⌘W only hid the window (App.tsx); a click on the Dock
            // icon brings it back.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } = event
            {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            #[cfg(all(desktop, not(target_os = "macos")))]
            let _ = (app, event);
        });
}

/// The page's answer to a quit from the Dock, ⌘Q or a logout (macOS): go ahead
/// or stay. Does nothing elsewhere.
#[tauri::command]
fn finish_quit(proceed: bool) {
    uwu_macos::reply_quit(proceed);
}
