//! The Tauri host.
//!
//! Thin on purpose: it turns IPC calls into calls on `uwulock-bitwarden`,
//! which does the protocol and the crypto and is tested without a window.
//!
//! - [`vault`] — logging in, unlocking, syncing, items, copying
//! - [`account`] — what is kept on disk, and where
//! - [`clipboard`] — copies that clear themselves
//! - [`system`] — updates and links out of the app

mod account;
mod clipboard;
mod system;
mod updates;
mod vault;

use tauri::Manager;

pub fn run() {
    system::restrict_dll_search();

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("UWULOCK_LOG")
                .unwrap_or_else(|_| "uwulock=debug,uwulock_bitwarden=debug,warn".to_string()),
        )
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
            vault::start(app.handle());
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
