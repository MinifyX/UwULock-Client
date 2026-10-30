//! The IPC commands: one each for the page's calls, with the names and
//! arguments it uses, each the method of the same name on [`App`]
//! (uwulock-app). Generated once from the commands as they were here; keep
//! the names and arguments as they are, the page depends on them.

use tauri::{AppHandle, Manager, State};
use uwulock_app::App;

/// The app, for an async command: a clone, as the command outlives the
/// call's borrow.
fn app(handle: &AppHandle) -> App {
    handle.state::<App>().inner().clone()
}

pub(crate) mod vault {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    use serde_json::Value;
    #[allow(unused_imports)]
    use uwulock_app::vault::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};
    use uwulock_bitwarden::{generator, passkey};

    #[tauri::command]
    pub(crate) fn vault_status(app: State<'_, App>) -> Status {
        app.vault_status()
    }

    #[tauri::command]
    pub(crate) async fn login(
        handle: AppHandle,
        server: ServerInput,
        email: String,
        password: String,
    ) -> Result<LoginStep> {
        app(&handle).login(server, email, password).await
    }

    #[tauri::command]
    pub(crate) async fn login_two_factor(
        handle: AppHandle,
        provider: u8,
        code: String,
        remember: bool,
    ) -> Result<LoginStep> {
        app(&handle)
            .login_two_factor(provider, code, remember)
            .await
    }

    #[tauri::command]
    pub(crate) async fn login_new_device(handle: AppHandle, code: String) -> Result<LoginStep> {
        app(&handle).login_new_device(code).await
    }

    #[tauri::command]
    pub(crate) async fn login_send_email(handle: AppHandle) -> Result<()> {
        app(&handle).login_send_email().await
    }

    #[tauri::command]
    pub(crate) fn login_cancel(app: State<'_, App>) {
        app.login_cancel()
    }

    #[tauri::command]
    pub(crate) async fn login_sso(
        handle: AppHandle,
        server: ServerInput,
        email: Option<String>,
    ) -> Result<LoginStep> {
        app(&handle).login_sso(server, email).await
    }

    #[tauri::command]
    pub(crate) async fn login_sso_password(
        handle: AppHandle,
        password: String,
    ) -> Result<LoginStep> {
        app(&handle).login_sso_password(password).await
    }

    #[tauri::command]
    pub(crate) async fn unlock(handle: AppHandle, password: String) -> Result<Status> {
        app(&handle).unlock(password).await
    }

    #[tauri::command]
    pub(crate) async fn unlock_with_hello(
        handle: AppHandle,
        prompt: Option<uwulock_app::hello::Prompt>,
    ) -> Result<Status> {
        app(&handle).unlock_with_hello(prompt).await
    }

    #[tauri::command]
    pub(crate) async fn set_hello(
        handle: AppHandle,
        enabled: bool,
        prompt: Option<uwulock_app::hello::Prompt>,
    ) -> Result<Status> {
        app(&handle).set_hello(enabled, prompt).await
    }

    #[tauri::command]
    pub(crate) fn lock(app: State<'_, App>) {
        app.lock()
    }

    #[tauri::command]
    pub(crate) fn logout(app: State<'_, App>, id: Option<String>) -> Result<Status> {
        app.logout(id)
    }

    #[tauri::command]
    pub(crate) fn switch_account(app: State<'_, App>, id: String) -> Result<Status> {
        app.switch_account(id)
    }

    #[tauri::command]
    pub(crate) fn rename_account(app: State<'_, App>, id: String, label: String) -> Result<Status> {
        app.rename_account(id, label)
    }

    #[tauri::command]
    pub(crate) fn touch(app: State<'_, App>) {
        app.touch()
    }

    #[tauri::command]
    pub(crate) fn set_security(
        app: State<'_, App>,
        auto_lock_minutes: Option<u32>,
        clipboard_seconds: Option<u32>,
        lock_with_system: Option<bool>,
    ) {
        app.set_security(auto_lock_minutes, clipboard_seconds, lock_with_system)
    }

    #[tauri::command]
    pub(crate) async fn sync_now(handle: AppHandle) -> Result<Status> {
        app(&handle).sync_now().await
    }

    #[tauri::command]
    pub(crate) fn vault_overview(app: State<'_, App>) -> Result<Overview> {
        app.vault_overview()
    }

    #[tauri::command]
    pub(crate) fn vault_items(app: State<'_, App>) -> Result<Vec<ItemSummary>> {
        app.vault_items()
    }

    #[tauri::command]
    pub(crate) fn vault_item(app: State<'_, App>, id: String) -> Result<Value> {
        app.vault_item(id)
    }

    #[tauri::command]
    pub(crate) async fn verify_reprompt(
        handle: AppHandle,
        id: String,
        password: String,
    ) -> Result<()> {
        app(&handle).verify_reprompt(id, password).await
    }

    #[tauri::command]
    pub(crate) fn reveal_field(app: State<'_, App>, id: String, field: String) -> Result<String> {
        app.reveal_field(id, field)
    }

    #[tauri::command]
    pub(crate) fn copy_field(app: State<'_, App>, id: String, field: String) -> Result<()> {
        app.copy_field(id, field)
    }

    #[tauri::command]
    pub(crate) fn copy_generated(app: State<'_, App>, text: String) -> Result<()> {
        app.copy_generated(text)
    }

    #[tauri::command]
    pub(crate) fn copy_text(app: State<'_, App>, text: String) -> Result<()> {
        app.copy_text(text)
    }

    #[tauri::command]
    pub(crate) fn totp_code(app: State<'_, App>, id: String) -> Result<TotpCode> {
        app.totp_code(id)
    }

    #[tauri::command]
    pub(crate) fn generate_password(
        app: State<'_, App>,
        options: generator::Options,
    ) -> Result<Generated> {
        app.generate_password(options)
    }

    #[tauri::command]
    pub(crate) fn item_passkeys(
        app: State<'_, App>,
        id: String,
    ) -> Result<Vec<passkey::PasskeyInfo>> {
        app.item_passkeys(id)
    }

    #[tauri::command]
    pub(crate) async fn delete_passkey(
        handle: AppHandle,
        id: String,
        index: usize,
        credential_id: Option<String>,
    ) -> Result<()> {
        app(&handle).delete_passkey(id, index, credential_id).await
    }

    #[tauri::command]
    pub(crate) async fn save_item(
        handle: AppHandle,
        id: Option<String>,
        draft: Draft,
    ) -> Result<String> {
        app(&handle).save_item(id, draft).await
    }

    #[tauri::command]
    pub(crate) async fn set_favorite(handle: AppHandle, id: String, favorite: bool) -> Result<()> {
        app(&handle).set_favorite(id, favorite).await
    }

    #[tauri::command]
    pub(crate) async fn set_item_folder(
        handle: AppHandle,
        id: String,
        folder_id: Option<String>,
    ) -> Result<()> {
        app(&handle).set_item_folder(id, folder_id).await
    }

    #[tauri::command]
    pub(crate) async fn delete_item(handle: AppHandle, id: String, permanent: bool) -> Result<()> {
        app(&handle).delete_item(id, permanent).await
    }

    #[tauri::command]
    pub(crate) async fn restore_item(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).restore_item(id).await
    }

    #[tauri::command]
    pub(crate) async fn save_folder(
        handle: AppHandle,
        id: Option<String>,
        name: String,
    ) -> Result<String> {
        app(&handle).save_folder(id, name).await
    }

    #[tauri::command]
    pub(crate) async fn delete_folder(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).delete_folder(id).await
    }
}

pub(crate) mod extras {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    use serde_json::Value;
    use std::collections::HashMap;
    #[allow(unused_imports)]
    use uwulock_app::extras::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};
    use uwulock_bitwarden::uwu::{MaskedAddress, MaskedConnection};

    #[tauri::command]
    pub(crate) fn uwu_extras_key_seen(app: State<'_, App>) -> Result<()> {
        app.uwu_extras_key_seen()
    }

    #[tauri::command]
    pub(crate) fn uwu_status(app: State<'_, App>) -> UwuStatus {
        app.uwu_status()
    }

    #[tauri::command]
    pub(crate) async fn uwu_travel(handle: AppHandle) -> Result<Travel> {
        app(&handle).uwu_travel().await
    }

    #[tauri::command]
    pub(crate) fn open_web_vault_at(
        app: State<'_, App>,
        place: String,
        id: Option<String>,
    ) -> Result<()> {
        app.open_web_vault_at(place, id)
    }

    #[tauri::command]
    pub(crate) async fn item_icons(
        handle: AppHandle,
        ids: Vec<String>,
        automatic: Option<bool>,
    ) -> Result<HashMap<String, String>> {
        app(&handle).item_icons(ids, automatic).await
    }

    #[tauri::command]
    pub(crate) async fn set_own_icon(handle: AppHandle, id: String, png: String) -> Result<()> {
        app(&handle).set_own_icon(id, png).await
    }

    #[tauri::command]
    pub(crate) async fn fetch_device_icon(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).fetch_device_icon(id).await
    }

    #[tauri::command]
    pub(crate) async fn delete_own_icon(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).delete_own_icon(id).await
    }

    #[tauri::command]
    pub(crate) async fn icon_library(handle: AppHandle) -> Result<Value> {
        app(&handle).icon_library().await
    }

    #[tauri::command]
    pub(crate) async fn library_icon(
        handle: AppHandle,
        source: String,
        icon_id: String,
        variant: Option<String>,
    ) -> Result<String> {
        app(&handle).library_icon(source, icon_id, variant).await
    }

    #[tauri::command]
    pub(crate) async fn device_icon(handle: AppHandle, uri: String) -> Result<String> {
        app(&handle).device_icon(uri).await
    }

    #[tauri::command]
    pub(crate) async fn item_versions(handle: AppHandle, id: String) -> Result<Vec<VersionView>> {
        app(&handle).item_versions(id).await
    }

    #[tauri::command]
    pub(crate) fn reveal_version_field(
        app: State<'_, App>,
        id: String,
        version_id: Option<String>,
        field: String,
    ) -> Result<String> {
        app.reveal_version_field(id, version_id, field)
    }

    #[tauri::command]
    pub(crate) async fn restore_version(
        handle: AppHandle,
        id: String,
        version_id: String,
    ) -> Result<()> {
        app(&handle).restore_version(id, version_id).await
    }

    #[tauri::command]
    pub(crate) async fn delete_versions(
        handle: AppHandle,
        id: String,
        version_id: Option<String>,
    ) -> Result<()> {
        app(&handle).delete_versions(id, version_id).await
    }

    #[tauri::command]
    pub(crate) async fn set_reminder(
        handle: AppHandle,
        id: String,
        due: Option<String>,
        every_months: Option<u32>,
    ) -> Result<()> {
        app(&handle).set_reminder(id, due, every_months).await
    }

    #[tauri::command]
    pub(crate) async fn delete_reminder(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).delete_reminder(id).await
    }

    #[tauri::command]
    pub(crate) async fn file_requests(handle: AppHandle) -> Result<Vec<FileRequestView>> {
        app(&handle).file_requests().await
    }

    #[tauri::command]
    pub(crate) async fn create_file_request(
        handle: AppHandle,
        input: FileRequestInput,
    ) -> Result<FileRequestView> {
        app(&handle).create_file_request(input).await
    }

    #[tauri::command]
    pub(crate) async fn update_file_request(
        handle: AppHandle,
        id: String,
        input: FileRequestInput,
        new_link: bool,
        remove_password: bool,
    ) -> Result<FileRequestView> {
        app(&handle)
            .update_file_request(id, input, new_link, remove_password)
            .await
    }

    #[tauri::command]
    pub(crate) async fn delete_file_request(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).delete_file_request(id).await
    }

    #[tauri::command]
    pub(crate) async fn file_request_submissions(
        handle: AppHandle,
        request_id: String,
    ) -> Result<Vec<SubmissionView>> {
        app(&handle).file_request_submissions(request_id).await
    }

    #[tauri::command]
    pub(crate) async fn save_submission_file(
        handle: AppHandle,
        request_id: String,
        submission_id: String,
        file_id: String,
        allow_risky: Option<bool>,
    ) -> Result<String> {
        app(&handle)
            .save_submission_file(request_id, submission_id, file_id, allow_risky)
            .await
    }

    #[tauri::command]
    pub(crate) async fn mark_submission_seen(
        handle: AppHandle,
        request_id: String,
        submission_id: String,
    ) -> Result<()> {
        app(&handle)
            .mark_submission_seen(request_id, submission_id)
            .await
    }

    #[tauri::command]
    pub(crate) async fn delete_submission(
        handle: AppHandle,
        request_id: String,
        submission_id: String,
    ) -> Result<()> {
        app(&handle)
            .delete_submission(request_id, submission_id)
            .await
    }

    #[tauri::command]
    pub(crate) async fn take_over_submission(
        handle: AppHandle,
        request_id: String,
        submission_id: String,
        name: String,
        sender_label: String,
    ) -> Result<String> {
        app(&handle)
            .take_over_submission(request_id, submission_id, name, sender_label)
            .await
    }

    #[tauri::command]
    pub(crate) async fn masked_connection(handle: AppHandle) -> Result<MaskedConnection> {
        app(&handle).masked_connection().await
    }

    #[tauri::command]
    pub(crate) async fn masked_addresses(handle: AppHandle) -> Result<Vec<MaskedView>> {
        app(&handle).masked_addresses().await
    }

    #[tauri::command]
    pub(crate) async fn create_masked_address(
        handle: AppHandle,
        for_domain: Option<String>,
        description: Option<String>,
        cipher_id: Option<String>,
    ) -> Result<MaskedAddress> {
        app(&handle)
            .create_masked_address(for_domain, description, cipher_id)
            .await
    }

    #[tauri::command]
    pub(crate) async fn update_masked_address(
        handle: AppHandle,
        id: String,
        address_state: Option<String>,
        link: Option<Option<String>>,
    ) -> Result<MaskedAddress> {
        app(&handle)
            .update_masked_address(id, address_state, link)
            .await
    }

    #[tauri::command]
    pub(crate) async fn delete_masked_address(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).delete_masked_address(id).await
    }

    #[tauri::command]
    pub(crate) async fn send_options(handle: AppHandle) -> Result<SendOptions> {
        app(&handle).send_options().await
    }

    #[tauri::command]
    pub(crate) async fn share_as_send(
        handle: AppHandle,
        id: String,
        input: ShareInput,
    ) -> Result<SharedSend> {
        app(&handle).share_as_send(id, input).await
    }
}

pub(crate) mod health {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::health::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};
    use uwulock_bitwarden::health::EmailOptIn;
    use uwulock_core::health::{Ignored, ProblemKind};

    #[tauri::command]
    pub(crate) async fn health_report(handle: AppHandle, fresh: bool) -> Result<HealthView> {
        app(&handle).health_report(fresh).await
    }

    #[tauri::command]
    pub(crate) async fn health_ignore(
        handle: AppHandle,
        item_id: String,
        kind: ProblemKind,
        ignored: bool,
    ) -> Result<Vec<Ignored>> {
        app(&handle).health_ignore(item_id, kind, ignored).await
    }

    #[tauri::command]
    pub(crate) async fn health_open_page(handle: AppHandle, item_id: String) -> Result<()> {
        app(&handle).health_open_page(item_id).await
    }

    #[tauri::command]
    pub(crate) async fn health_save_password(
        handle: AppHandle,
        item_id: String,
        password: String,
    ) -> Result<()> {
        app(&handle).health_save_password(item_id, password).await
    }

    #[tauri::command]
    pub(crate) async fn health_email_opt_in(handle: AppHandle) -> Result<Option<EmailOptIn>> {
        app(&handle).health_email_opt_in().await
    }

    #[tauri::command]
    pub(crate) async fn set_health_email_opt_in(
        handle: AppHandle,
        opted_in: bool,
    ) -> Result<EmailOptIn> {
        app(&handle).set_health_email_opt_in(opted_in).await
    }

    #[tauri::command]
    pub(crate) async fn health_check_emails(handle: AppHandle) -> Result<EmailsView> {
        app(&handle).health_check_emails().await
    }
}

pub(crate) mod importing {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::importing::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) async fn import_vault(
        handle: AppHandle,
        format: String,
        text: String,
    ) -> Result<ImportOutcome> {
        app(&handle).import_vault(format, text).await
    }

    #[tauri::command]
    pub(crate) async fn import_open_bitwarden(
        handle: AppHandle,
        text: String,
        password: String,
    ) -> Result<String> {
        app(&handle).import_open_bitwarden(text, password).await
    }

    #[tauri::command]
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn import_kdbx_argon2(
        handle: AppHandle,
        id: bool,
        version: u32,
        key: Vec<u8>,
        salt: Vec<u8>,
        memory_kib: u32,
        iterations: u32,
        lanes: u32,
    ) -> Result<Vec<u8>> {
        app(&handle)
            .import_kdbx_argon2(id, version, key, salt, memory_kib, iterations, lanes)
            .await
    }

    #[tauri::command]
    pub(crate) async fn import_kdbx_aes_kdf(
        handle: AppHandle,
        key: Vec<u8>,
        seed: Vec<u8>,
        rounds: u64,
    ) -> Result<Vec<u8>> {
        app(&handle).import_kdbx_aes_kdf(key, seed, rounds).await
    }
}

pub(crate) mod moving {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::moving::*;
    use uwulock_app::vault::ServerInput;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) async fn move_target(handle: AppHandle) -> Result<MoveTarget> {
        app(&handle).move_target().await
    }

    #[tauri::command]
    pub(crate) async fn move_login(
        handle: AppHandle,
        server: ServerInput,
        email: String,
        password: String,
    ) -> Result<MoveStep> {
        app(&handle).move_login(server, email, password).await
    }

    #[tauri::command]
    pub(crate) async fn move_login_two_factor(
        handle: AppHandle,
        provider: u8,
        code: String,
    ) -> Result<MoveStep> {
        app(&handle).move_login_two_factor(provider, code).await
    }

    #[tauri::command]
    pub(crate) async fn move_login_new_device(handle: AppHandle, code: String) -> Result<MoveStep> {
        app(&handle).move_login_new_device(code).await
    }

    #[tauri::command]
    pub(crate) async fn move_login_send_email(handle: AppHandle) -> Result<()> {
        app(&handle).move_login_send_email().await
    }

    #[tauri::command]
    pub(crate) async fn move_start(handle: AppHandle) -> Result<()> {
        app(&handle).move_start().await
    }

    #[tauri::command]
    pub(crate) fn move_cancel(app: State<'_, App>) {
        app.move_cancel()
    }

    #[tauri::command]
    pub(crate) async fn move_close(handle: AppHandle) -> Result<()> {
        app(&handle).move_close().await
    }
}

pub(crate) mod sends {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::sends::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};
    use uwulock_bitwarden::send::SendDraft;

    #[tauri::command]
    pub(crate) fn sends(app: State<'_, App>) -> Result<Vec<SendView>> {
        app.sends()
    }

    #[tauri::command]
    pub(crate) fn stage_send_file(
        app: State<'_, App>,
        request: tauri::ipc::Request<'_>,
    ) -> Result<()> {
        let bytes = match request.body() {
            tauri::ipc::InvokeBody::Raw(bytes) => Some(bytes.as_slice()),
            _ => None,
        };
        app.stage_send_file(bytes)
    }

    #[tauri::command]
    pub(crate) async fn save_send(
        handle: AppHandle,
        id: Option<String>,
        draft: SendDraft,
        domain: Option<String>,
        choose_domain: bool,
    ) -> Result<String> {
        app(&handle)
            .save_send(id, draft, domain, choose_domain)
            .await
    }

    #[tauri::command]
    pub(crate) async fn remove_send_auth(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).remove_send_auth(id).await
    }

    #[tauri::command]
    pub(crate) async fn delete_send(handle: AppHandle, id: String) -> Result<()> {
        app(&handle).delete_send(id).await
    }
}

pub(crate) mod suite {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::suite::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) async fn suite_view(handle: AppHandle, space: String) -> Result<View> {
        app(&handle).suite_view(space).await
    }

    #[tauri::command]
    pub(crate) async fn suite_create(handle: AppHandle, space: String) -> Result<View> {
        app(&handle).suite_create(space).await
    }

    #[tauri::command]
    pub(crate) async fn suite_save(
        handle: AppHandle,
        space: String,
        ops: Vec<Op>,
    ) -> Result<Saved> {
        app(&handle).suite_save(space, ops).await
    }

    #[tauri::command]
    pub(crate) async fn suite_reveal(
        handle: AppHandle,
        space: String,
        id: String,
    ) -> Result<String> {
        app(&handle).suite_reveal(space, id).await
    }

    #[tauri::command]
    pub(crate) async fn suite_copy(handle: AppHandle, space: String, id: String) -> Result<()> {
        app(&handle).suite_copy(space, id).await
    }

    #[tauri::command]
    pub(crate) async fn suite_generate_key(
        handle: AppHandle,
        space: String,
        label: String,
        comment: String,
        passphrase: Option<String>,
    ) -> Result<Saved> {
        app(&handle)
            .suite_generate_key(space, label, comment, passphrase)
            .await
    }

    #[tauri::command]
    pub(crate) async fn suite_import_key(
        handle: AppHandle,
        space: String,
        label: String,
        private_key: String,
        passphrase: Option<String>,
    ) -> Result<Saved> {
        app(&handle)
            .suite_import_key(space, label, private_key, passphrase)
            .await
    }

    #[tauri::command]
    pub(crate) async fn suite_save_key(
        handle: AppHandle,
        space: String,
        id: String,
        half: String,
    ) -> Result<String> {
        app(&handle).suite_save_key(space, id, half).await
    }

    #[tauri::command]
    pub(crate) async fn suite_save_rdp(handle: AppHandle, id: String) -> Result<String> {
        app(&handle).suite_save_rdp(id).await
    }

    #[tauri::command]
    pub(crate) async fn suite_open_in_app(
        handle: AppHandle,
        space: String,
        id: String,
    ) -> Result<()> {
        app(&handle).suite_open_in_app(space, id).await
    }
}

pub(crate) mod wifi {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::wifi::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) async fn wifi_connect(handle: AppHandle, id: String) -> Result<Joined> {
        app(&handle).wifi_connect(id).await
    }

    #[tauri::command]
    pub(crate) async fn wifi_settings(handle: AppHandle) -> Result<bool> {
        app(&handle).wifi_settings().await
    }
}

pub(crate) mod autofill {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::autofill::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) async fn autofill_provider_status(handle: AppHandle) -> ProviderView {
        app(&handle).autofill_provider_status().await
    }

    #[tauri::command]
    pub(crate) async fn autofill_provider_request(
        handle: AppHandle,
        target: String,
    ) -> Result<ProviderView> {
        app(&handle).autofill_provider_request(target).await
    }

    #[tauri::command]
    pub(crate) async fn credential_exchange_pending(handle: AppHandle) -> bool {
        app(&handle).credential_exchange_pending().await
    }

    #[tauri::command]
    pub(crate) async fn credential_exchange_import(
        handle: AppHandle,
        discard: bool,
    ) -> Result<String> {
        app(&handle).credential_exchange_import(discard).await
    }

    #[tauri::command]
    pub(crate) async fn autofill_log(handle: AppHandle) -> AutofillLog {
        app(&handle).autofill_log().await
    }

    #[tauri::command]
    pub(crate) async fn autofill_log_clear(handle: AppHandle) -> Result<()> {
        app(&handle).autofill_log_clear().await
    }
}

pub(crate) mod passkeys {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::passkeys::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) fn passkey_request(app: State<'_, App>) -> Option<RequestView> {
        app.passkey_request()
    }

    #[tauri::command]
    pub(crate) async fn passkey_answer(
        handle: AppHandle,
        id: u64,
        allow: bool,
        item_id: Option<String>,
        credential_id: Option<String>,
        password: Option<String>,
    ) -> Result<()> {
        app(&handle)
            .passkey_answer(id, allow, item_id, credential_id, password)
            .await
    }

    #[tauri::command]
    pub(crate) fn passkey_provider_status(app: State<'_, App>) -> ProviderStatus {
        app.passkey_provider_status()
    }

    #[tauri::command]
    pub(crate) fn set_passkey_provider(
        app: State<'_, App>,
        settings: Settings,
    ) -> Result<ProviderStatus> {
        app.set_passkey_provider(settings)
    }
}

pub(crate) mod mello {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::mello::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) fn mello_link(app: State<'_, App>) -> LinkView {
        app.mello_link()
    }

    #[tauri::command]
    pub(crate) fn set_mello_link(app: State<'_, App>, enabled: bool) -> Result<LinkView> {
        app.set_mello_link(enabled)
    }

    #[tauri::command]
    pub(crate) fn mello_link_renew(app: State<'_, App>) -> Result<LinkView> {
        app.mello_link_renew()
    }
}

pub(crate) mod safari {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::safari::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) async fn safari_extension_status(handle: AppHandle) -> SafariView {
        app(&handle).safari_extension_status().await
    }

    #[tauri::command]
    pub(crate) async fn safari_extension_open(handle: AppHandle) -> Result<()> {
        app(&handle).safari_extension_open().await
    }
}

pub(crate) mod system {
    #[allow(unused_imports)]
    use super::{app, App, AppHandle, State};
    #[allow(unused_imports)]
    use uwulock_app::system::*;
    #[allow(unused_imports)]
    use uwulock_app::{Failure, Result};

    #[tauri::command]
    pub(crate) fn open_project_page(
        app: State<'_, App>,
        page: String,
    ) -> std::result::Result<(), String> {
        app.open_project_page(page)
    }

    #[tauri::command]
    pub(crate) fn open_item_uri(
        app: State<'_, App>,
        id: String,
        index: usize,
    ) -> std::result::Result<(), String> {
        app.open_item_uri(id, index)
    }

    #[tauri::command]
    pub(crate) fn open_web_vault(app: State<'_, App>) -> std::result::Result<(), String> {
        app.open_web_vault()
    }
}
