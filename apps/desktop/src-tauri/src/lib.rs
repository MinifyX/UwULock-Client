//! The Tauri host.
//!
//! Thin on purpose: it turns IPC calls into calls on `uwulock-bitwarden`,
//! which does the protocol and the crypto and is tested without a window.
//!
//! - [`vault`] — logging in, unlocking, syncing, items, copying
//! - [`account`] — what is kept on disk, and where
//! - [`extras`] — UwULock Server's extras: icons, versions, reminders, file
//!   requests, masked addresses, travel mode; items shared as Sends
//! - [`health`] — the password check and its review one login at a time
//! - [`suite`] — UwUSSH's and UwURDP's hosts, logins and keys
//! - [`live`] — changes from other devices as they happen
//! - [`session_lock`] — locking when the screen locks or the computer sleeps
//! - [`hello`] — unlocking with Windows Hello, or a phone's fingerprint or face
//! - [`moving`] — moving a vault in from Bitwarden or Vaultwarden
//! - [`importing`] — moving in from another app's export file
//! - [`clipboard`] — copies that clear themselves
//! - [`system`] — updates and links out of the app
//! - [`phone`] — Android and iOS: the plugin, locking in the background
//! - [`passkeys`] — the vault's passkeys for the system: a virtual security
//!   key (Linux), a plugin passkey manager (Windows), Credential Manager
//!   (Android), the AutoFill extension (iOS, macOS)
//!
//! The same app runs on Android and iOS (docs/mobile.md); what only a
//! desktop has — the updater, Windows Hello, the screen lock — is left out
//! there by `#[cfg(desktop)]`/`#[cfg(mobile)]`. The updater is `#[cfg(self_update)]`
//! (build.rs): the Mac App Store build has none either (docs/app-store.md).

mod account;
mod clipboard;
mod extras;
mod health;
mod hello;
mod importing;
mod live;
mod moving;
mod passkeys;
#[cfg(mobile)]
mod phone;
mod sends;
mod session_lock;
mod suite;
mod system;
#[cfg(self_update)]
mod updates;
mod vault;
mod wifi;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    system::restrict_dll_search();

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
            let storage = account::Storage::new(dir)?;
            tracing::info!(path = %storage.dir().display(), "data folder");
            app.manage(passkeys::Provider::new(storage.dir()));
            let vault = vault::VaultState::new(storage);
            app.manage(vault.moves.clone());
            app.manage(vault);
            vault::start(app.handle());
            live::start(app.handle());
            extras::start(app.handle());
            session_lock::start(app.handle());
            passkeys::start(app.handle());
            // On a phone the plugin answers once the page is there (`vault_status`).
            #[cfg(desktop)]
            hello::probe(app.handle());
            #[cfg(self_update)]
            updates::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            finish_quit,
            vault::vault_status,
            vault::login,
            vault::login_two_factor,
            vault::login_new_device,
            vault::login_send_email,
            vault::login_cancel,
            vault::unlock,
            vault::unlock_with_hello,
            vault::set_hello,
            vault::lock,
            vault::logout,
            vault::switch_account,
            vault::rename_account,
            vault::touch,
            vault::set_security,
            vault::sync_now,
            vault::vault_overview,
            vault::vault_items,
            vault::vault_item,
            vault::verify_reprompt,
            vault::reveal_field,
            vault::copy_field,
            vault::copy_generated,
            vault::totp_code,
            vault::generate_password,
            vault::item_passkeys,
            vault::delete_passkey,
            vault::save_item,
            vault::set_favorite,
            vault::set_item_folder,
            vault::delete_item,
            vault::restore_item,
            vault::save_folder,
            vault::delete_folder,
            extras::uwu_status,
            extras::uwu_extras_key_seen,
            extras::uwu_travel,
            extras::open_web_vault_at,
            extras::item_icons,
            extras::set_own_icon,
            extras::fetch_device_icon,
            extras::delete_own_icon,
            extras::icon_library,
            extras::library_icon,
            extras::device_icon,
            extras::item_versions,
            extras::reveal_version_field,
            extras::restore_version,
            extras::delete_versions,
            extras::set_reminder,
            extras::delete_reminder,
            extras::file_requests,
            extras::create_file_request,
            extras::update_file_request,
            extras::delete_file_request,
            extras::file_request_submissions,
            extras::save_submission_file,
            extras::mark_submission_seen,
            extras::delete_submission,
            extras::take_over_submission,
            extras::masked_connection,
            extras::masked_addresses,
            extras::create_masked_address,
            extras::update_masked_address,
            extras::delete_masked_address,
            extras::send_options,
            extras::share_as_send,
            sends::sends,
            sends::stage_send_file,
            sends::save_send,
            sends::remove_send_auth,
            sends::delete_send,
            health::health_report,
            health::health_ignore,
            health::health_open_page,
            health::health_save_password,
            health::health_email_opt_in,
            health::set_health_email_opt_in,
            health::health_check_emails,
            importing::import_vault,
            importing::import_open_bitwarden,
            importing::import_kdbx_argon2,
            importing::import_kdbx_aes_kdf,
            moving::move_target,
            moving::move_login,
            moving::move_login_two_factor,
            moving::move_login_new_device,
            moving::move_login_send_email,
            moving::move_start,
            moving::move_cancel,
            moving::move_close,
            suite::suite_view,
            suite::suite_create,
            suite::suite_save,
            suite::suite_reveal,
            suite::suite_copy,
            suite::suite_generate_key,
            suite::suite_import_key,
            suite::suite_save_key,
            suite::suite_save_rdp,
            suite::suite_open_in_app,
            system::distribution,
            system::set_update_channel,
            system::update_status,
            system::check_for_updates,
            system::install_update,
            system::open_project_page,
            system::open_item_uri,
            system::open_web_vault,
            system::set_appearance,
            wifi::wifi_connect,
            wifi::wifi_settings,
            passkeys::passkey_request,
            passkeys::passkey_answer,
            passkeys::passkey_provider_status,
            passkeys::set_passkey_provider,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start UwULock")
        .run(|app, event| {
            #[cfg(mobile)]
            if let tauri::RunEvent::WindowEvent { event, .. } = &event {
                match event {
                    tauri::WindowEvent::Suspended => phone::suspended(app),
                    tauri::WindowEvent::Resumed => phone::resumed(app),
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
