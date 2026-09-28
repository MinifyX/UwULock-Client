//! The Tauri host.
//!
//! Thin on purpose: it turns IPC calls into calls on `uwulock-bitwarden`,
//! which does the protocol and the crypto and is tested without a window.
//!
//! - [`vault`] — logging in, unlocking, syncing, items, copying
//! - [`account`] — what is kept on disk, and where
//! - [`extras`] — UwULock Server's extras: icons, versions, reminders, file
//!   requests, masked addresses, travel mode; items shared as Sends
//! - [`live`] — changes from other devices as they happen
//! - [`session_lock`] — locking when the screen locks or the computer sleeps
//! - [`hello`] — unlocking with Windows Hello
//! - [`moving`] — moving a vault in from Bitwarden or Vaultwarden
//! - [`clipboard`] — copies that clear themselves
//! - [`system`] — updates and links out of the app

mod account;
mod clipboard;
mod extras;
mod hello;
mod live;
mod moving;
mod session_lock;
mod system;
mod updates;
mod vault;

use tauri::Manager;

pub fn run() {
    system::restrict_dll_search();

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("UWULOCK_LOG").unwrap_or_else(|_| {
            "uwulock=debug,uwulock_bitwarden=debug,uwulock_core=debug,warn".to_string()
        }))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            if updates::apply_pending_on_start(app.handle()) {
                // The downloaded setup replaces this version and starts UwULock again.
                std::process::exit(0);
            }

            // %APPDATA%\app.uwulock.desktop on Windows. UWULOCK_DATA_DIR points
            // elsewhere, so trying things out never touches the real account.
            let dir = match std::env::var_os("UWULOCK_DATA_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            let storage = account::Storage::new(dir)?;
            tracing::info!(path = %storage.dir().display(), "data folder");
            app.manage(vault::VaultState::new(storage));
            app.manage(moving::MoveState::default());
            vault::start(app.handle());
            live::start(app.handle());
            extras::start(app.handle());
            session_lock::start(app.handle());
            hello::probe();
            updates::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            vault::save_item,
            vault::set_favorite,
            vault::set_item_folder,
            vault::delete_item,
            vault::restore_item,
            vault::save_folder,
            vault::delete_folder,
            extras::uwu_status,
            extras::uwu_travel,
            extras::open_web_vault_at,
            extras::item_icons,
            extras::set_own_icon,
            extras::fetch_device_icon,
            extras::delete_own_icon,
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
            moving::move_target,
            moving::move_login,
            moving::move_login_two_factor,
            moving::move_login_new_device,
            moving::move_login_send_email,
            moving::move_start,
            moving::move_cancel,
            moving::move_close,
            system::set_update_channel,
            system::update_status,
            system::check_for_updates,
            system::install_update,
            system::open_project_page,
            system::open_item_uri,
            system::open_web_vault,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start UwULock");
}
