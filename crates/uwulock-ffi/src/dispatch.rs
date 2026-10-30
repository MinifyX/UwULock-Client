//! The command table: every IPC command of the Tauri app (apps/desktop/src-tauri/src/commands.rs)
//! with the same name and the same camelCase arguments. Written by scripts/ffi-dispatch.py from
//! that file: run it again when a command there changes.
//!
//! [`parse`] reads the arguments without running anything; [`run_now`] runs what Tauri runs on
//! its own thread (quick, synchronous), [`run_later`] what it awaits.

use serde::Serialize;
use uwulock_app::{App, Failure};

/// A command with its arguments read.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Call {
    Now(Now),
    Later(Later),
}

/// The synchronous commands. Each lives for one call: no boxing.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Now {
    VaultStatus,
    LoginCancel,
    Lock,
    Logout(vault::Logout),
    SwitchAccount(vault::SwitchAccount),
    RenameAccount(vault::RenameAccount),
    Touch,
    SetSecurity(vault::SetSecurity),
    VaultOverview,
    VaultItems,
    VaultItem(vault::VaultItem),
    RevealField(vault::RevealField),
    CopyField(vault::CopyField),
    CopyGenerated(vault::CopyGenerated),
    CopyText(vault::CopyText),
    TotpCode(vault::TotpCode),
    GeneratePassword(vault::GeneratePassword),
    ItemPasskeys(vault::ItemPasskeys),
    UwuExtrasKeySeen,
    UwuStatus,
    OpenWebVaultAt(extras::OpenWebVaultAt),
    RevealVersionField(extras::RevealVersionField),
    MoveCancel,
    Sends,
    StageSendFile,
    PasskeyRequest,
    PasskeyProviderStatus,
    SetPasskeyProvider(passkeys::SetPasskeyProvider),
    OpenProjectPage(system::OpenProjectPage),
    OpenItemUri(system::OpenItemUri),
    OpenWebVault,
}

/// The asynchronous commands.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Later {
    Login(vault::Login),
    LoginTwoFactor(vault::LoginTwoFactor),
    LoginNewDevice(vault::LoginNewDevice),
    LoginSendEmail,
    LoginSso(vault::LoginSso),
    LoginSsoPassword(vault::LoginSsoPassword),
    Unlock(vault::Unlock),
    UnlockWithHello(vault::UnlockWithHello),
    SetHello(vault::SetHello),
    SyncNow,
    VerifyReprompt(vault::VerifyReprompt),
    DeletePasskey(vault::DeletePasskey),
    SaveItem(vault::SaveItem),
    SetFavorite(vault::SetFavorite),
    SetItemFolder(vault::SetItemFolder),
    DeleteItem(vault::DeleteItem),
    RestoreItem(vault::RestoreItem),
    SaveFolder(vault::SaveFolder),
    DeleteFolder(vault::DeleteFolder),
    UwuTravel,
    ItemIcons(extras::ItemIcons),
    SetOwnIcon(extras::SetOwnIcon),
    FetchDeviceIcon(extras::FetchDeviceIcon),
    DeleteOwnIcon(extras::DeleteOwnIcon),
    IconLibrary,
    LibraryIcon(extras::LibraryIcon),
    DeviceIcon(extras::DeviceIcon),
    ItemVersions(extras::ItemVersions),
    RestoreVersion(extras::RestoreVersion),
    DeleteVersions(extras::DeleteVersions),
    SetReminder(extras::SetReminder),
    DeleteReminder(extras::DeleteReminder),
    FileRequests,
    CreateFileRequest(extras::CreateFileRequest),
    UpdateFileRequest(extras::UpdateFileRequest),
    DeleteFileRequest(extras::DeleteFileRequest),
    FileRequestSubmissions(extras::FileRequestSubmissions),
    SaveSubmissionFile(extras::SaveSubmissionFile),
    MarkSubmissionSeen(extras::MarkSubmissionSeen),
    DeleteSubmission(extras::DeleteSubmission),
    TakeOverSubmission(extras::TakeOverSubmission),
    MaskedConnection,
    MaskedAddresses,
    CreateMaskedAddress(extras::CreateMaskedAddress),
    UpdateMaskedAddress(extras::UpdateMaskedAddress),
    DeleteMaskedAddress(extras::DeleteMaskedAddress),
    SendOptions,
    ShareAsSend(extras::ShareAsSend),
    HealthReport(health::HealthReport),
    HealthIgnore(health::HealthIgnore),
    HealthOpenPage(health::HealthOpenPage),
    HealthSavePassword(health::HealthSavePassword),
    HealthEmailOptIn,
    SetHealthEmailOptIn(health::SetHealthEmailOptIn),
    HealthCheckEmails,
    ImportVault(importing::ImportVault),
    ImportOpenBitwarden(importing::ImportOpenBitwarden),
    ImportKdbxArgon2(importing::ImportKdbxArgon2),
    ImportKdbxAesKdf(importing::ImportKdbxAesKdf),
    MoveTarget,
    MoveLogin(moving::MoveLogin),
    MoveLoginTwoFactor(moving::MoveLoginTwoFactor),
    MoveLoginNewDevice(moving::MoveLoginNewDevice),
    MoveLoginSendEmail,
    MoveStart,
    MoveClose,
    SaveSend(sends::SaveSend),
    RemoveSendAuth(sends::RemoveSendAuth),
    DeleteSend(sends::DeleteSend),
    SuiteView(suite::SuiteView),
    SuiteCreate(suite::SuiteCreate),
    SuiteSave(suite::SuiteSave),
    SuiteReveal(suite::SuiteReveal),
    SuiteCopy(suite::SuiteCopy),
    SuiteGenerateKey(suite::SuiteGenerateKey),
    SuiteImportKey(suite::SuiteImportKey),
    SuiteSaveKey(suite::SuiteSaveKey),
    SuiteSaveRdp(suite::SuiteSaveRdp),
    SuiteOpenInApp(suite::SuiteOpenInApp),
    WifiConnect(wifi::WifiConnect),
    WifiSettings,
    AutofillProviderStatus,
    AutofillProviderRequest(autofill::AutofillProviderRequest),
    CredentialExchangePending,
    CredentialExchangeImport(autofill::CredentialExchangeImport),
    AutofillLog,
    AutofillLogClear,
    PasskeyAnswer(passkeys::PasskeyAnswer),
    SafariExtensionStatus,
    SafariExtensionOpen,
}

/// Every command's name, in the order of commands.rs.
pub(crate) const COMMANDS: &[&str] = &[
    "vault_status",
    "login",
    "login_two_factor",
    "login_new_device",
    "login_send_email",
    "login_cancel",
    "login_sso",
    "login_sso_password",
    "unlock",
    "unlock_with_hello",
    "set_hello",
    "lock",
    "logout",
    "switch_account",
    "rename_account",
    "touch",
    "set_security",
    "sync_now",
    "vault_overview",
    "vault_items",
    "vault_item",
    "verify_reprompt",
    "reveal_field",
    "copy_field",
    "copy_generated",
    "copy_text",
    "totp_code",
    "generate_password",
    "item_passkeys",
    "delete_passkey",
    "save_item",
    "set_favorite",
    "set_item_folder",
    "delete_item",
    "restore_item",
    "save_folder",
    "delete_folder",
    "uwu_extras_key_seen",
    "uwu_status",
    "uwu_travel",
    "open_web_vault_at",
    "item_icons",
    "set_own_icon",
    "fetch_device_icon",
    "delete_own_icon",
    "icon_library",
    "library_icon",
    "device_icon",
    "item_versions",
    "reveal_version_field",
    "restore_version",
    "delete_versions",
    "set_reminder",
    "delete_reminder",
    "file_requests",
    "create_file_request",
    "update_file_request",
    "delete_file_request",
    "file_request_submissions",
    "save_submission_file",
    "mark_submission_seen",
    "delete_submission",
    "take_over_submission",
    "masked_connection",
    "masked_addresses",
    "create_masked_address",
    "update_masked_address",
    "delete_masked_address",
    "send_options",
    "share_as_send",
    "health_report",
    "health_ignore",
    "health_open_page",
    "health_save_password",
    "health_email_opt_in",
    "set_health_email_opt_in",
    "health_check_emails",
    "import_vault",
    "import_open_bitwarden",
    "import_kdbx_argon2",
    "import_kdbx_aes_kdf",
    "move_target",
    "move_login",
    "move_login_two_factor",
    "move_login_new_device",
    "move_login_send_email",
    "move_start",
    "move_cancel",
    "move_close",
    "sends",
    "stage_send_file",
    "save_send",
    "remove_send_auth",
    "delete_send",
    "suite_view",
    "suite_create",
    "suite_save",
    "suite_reveal",
    "suite_copy",
    "suite_generate_key",
    "suite_import_key",
    "suite_save_key",
    "suite_save_rdp",
    "suite_open_in_app",
    "wifi_connect",
    "wifi_settings",
    "autofill_provider_status",
    "autofill_provider_request",
    "credential_exchange_pending",
    "credential_exchange_import",
    "autofill_log",
    "autofill_log_clear",
    "passkey_request",
    "passkey_answer",
    "passkey_provider_status",
    "set_passkey_provider",
    "safari_extension_status",
    "safari_extension_open",
    "open_project_page",
    "open_item_uri",
    "open_web_vault",
];

fn args<T: serde::de::DeserializeOwned>(command: &str, json: &str) -> Result<T, Failure> {
    let json = if json.trim().is_empty() { "{}" } else { json };
    serde_json::from_str(json).map_err(|error| Failure::new("args", format!("{command}: {error}")))
}

fn json<T: Serialize>(value: T) -> Result<String, Failure> {
    serde_json::to_string(&value).map_err(|error| Failure::new("failed", error.to_string()))
}

/// Reads `json` as the arguments of `command`.
pub(crate) fn parse(command: &str, json: &str) -> Result<Call, Failure> {
    Ok(match command {
        "vault_status" => Call::Now(Now::VaultStatus),
        "login" => Call::Later(Later::Login(args(command, json)?)),
        "login_two_factor" => Call::Later(Later::LoginTwoFactor(args(command, json)?)),
        "login_new_device" => Call::Later(Later::LoginNewDevice(args(command, json)?)),
        "login_send_email" => Call::Later(Later::LoginSendEmail),
        "login_cancel" => Call::Now(Now::LoginCancel),
        "login_sso" => Call::Later(Later::LoginSso(args(command, json)?)),
        "login_sso_password" => Call::Later(Later::LoginSsoPassword(args(command, json)?)),
        "unlock" => Call::Later(Later::Unlock(args(command, json)?)),
        "unlock_with_hello" => Call::Later(Later::UnlockWithHello(args(command, json)?)),
        "set_hello" => Call::Later(Later::SetHello(args(command, json)?)),
        "lock" => Call::Now(Now::Lock),
        "logout" => Call::Now(Now::Logout(args(command, json)?)),
        "switch_account" => Call::Now(Now::SwitchAccount(args(command, json)?)),
        "rename_account" => Call::Now(Now::RenameAccount(args(command, json)?)),
        "touch" => Call::Now(Now::Touch),
        "set_security" => Call::Now(Now::SetSecurity(args(command, json)?)),
        "sync_now" => Call::Later(Later::SyncNow),
        "vault_overview" => Call::Now(Now::VaultOverview),
        "vault_items" => Call::Now(Now::VaultItems),
        "vault_item" => Call::Now(Now::VaultItem(args(command, json)?)),
        "verify_reprompt" => Call::Later(Later::VerifyReprompt(args(command, json)?)),
        "reveal_field" => Call::Now(Now::RevealField(args(command, json)?)),
        "copy_field" => Call::Now(Now::CopyField(args(command, json)?)),
        "copy_generated" => Call::Now(Now::CopyGenerated(args(command, json)?)),
        "copy_text" => Call::Now(Now::CopyText(args(command, json)?)),
        "totp_code" => Call::Now(Now::TotpCode(args(command, json)?)),
        "generate_password" => Call::Now(Now::GeneratePassword(args(command, json)?)),
        "item_passkeys" => Call::Now(Now::ItemPasskeys(args(command, json)?)),
        "delete_passkey" => Call::Later(Later::DeletePasskey(args(command, json)?)),
        "save_item" => Call::Later(Later::SaveItem(args(command, json)?)),
        "set_favorite" => Call::Later(Later::SetFavorite(args(command, json)?)),
        "set_item_folder" => Call::Later(Later::SetItemFolder(args(command, json)?)),
        "delete_item" => Call::Later(Later::DeleteItem(args(command, json)?)),
        "restore_item" => Call::Later(Later::RestoreItem(args(command, json)?)),
        "save_folder" => Call::Later(Later::SaveFolder(args(command, json)?)),
        "delete_folder" => Call::Later(Later::DeleteFolder(args(command, json)?)),
        "uwu_extras_key_seen" => Call::Now(Now::UwuExtrasKeySeen),
        "uwu_status" => Call::Now(Now::UwuStatus),
        "uwu_travel" => Call::Later(Later::UwuTravel),
        "open_web_vault_at" => Call::Now(Now::OpenWebVaultAt(args(command, json)?)),
        "item_icons" => Call::Later(Later::ItemIcons(args(command, json)?)),
        "set_own_icon" => Call::Later(Later::SetOwnIcon(args(command, json)?)),
        "fetch_device_icon" => Call::Later(Later::FetchDeviceIcon(args(command, json)?)),
        "delete_own_icon" => Call::Later(Later::DeleteOwnIcon(args(command, json)?)),
        "icon_library" => Call::Later(Later::IconLibrary),
        "library_icon" => Call::Later(Later::LibraryIcon(args(command, json)?)),
        "device_icon" => Call::Later(Later::DeviceIcon(args(command, json)?)),
        "item_versions" => Call::Later(Later::ItemVersions(args(command, json)?)),
        "reveal_version_field" => Call::Now(Now::RevealVersionField(args(command, json)?)),
        "restore_version" => Call::Later(Later::RestoreVersion(args(command, json)?)),
        "delete_versions" => Call::Later(Later::DeleteVersions(args(command, json)?)),
        "set_reminder" => Call::Later(Later::SetReminder(args(command, json)?)),
        "delete_reminder" => Call::Later(Later::DeleteReminder(args(command, json)?)),
        "file_requests" => Call::Later(Later::FileRequests),
        "create_file_request" => Call::Later(Later::CreateFileRequest(args(command, json)?)),
        "update_file_request" => Call::Later(Later::UpdateFileRequest(args(command, json)?)),
        "delete_file_request" => Call::Later(Later::DeleteFileRequest(args(command, json)?)),
        "file_request_submissions" => {
            Call::Later(Later::FileRequestSubmissions(args(command, json)?))
        }
        "save_submission_file" => Call::Later(Later::SaveSubmissionFile(args(command, json)?)),
        "mark_submission_seen" => Call::Later(Later::MarkSubmissionSeen(args(command, json)?)),
        "delete_submission" => Call::Later(Later::DeleteSubmission(args(command, json)?)),
        "take_over_submission" => Call::Later(Later::TakeOverSubmission(args(command, json)?)),
        "masked_connection" => Call::Later(Later::MaskedConnection),
        "masked_addresses" => Call::Later(Later::MaskedAddresses),
        "create_masked_address" => Call::Later(Later::CreateMaskedAddress(args(command, json)?)),
        "update_masked_address" => Call::Later(Later::UpdateMaskedAddress(args(command, json)?)),
        "delete_masked_address" => Call::Later(Later::DeleteMaskedAddress(args(command, json)?)),
        "send_options" => Call::Later(Later::SendOptions),
        "share_as_send" => Call::Later(Later::ShareAsSend(args(command, json)?)),
        "health_report" => Call::Later(Later::HealthReport(args(command, json)?)),
        "health_ignore" => Call::Later(Later::HealthIgnore(args(command, json)?)),
        "health_open_page" => Call::Later(Later::HealthOpenPage(args(command, json)?)),
        "health_save_password" => Call::Later(Later::HealthSavePassword(args(command, json)?)),
        "health_email_opt_in" => Call::Later(Later::HealthEmailOptIn),
        "set_health_email_opt_in" => Call::Later(Later::SetHealthEmailOptIn(args(command, json)?)),
        "health_check_emails" => Call::Later(Later::HealthCheckEmails),
        "import_vault" => Call::Later(Later::ImportVault(args(command, json)?)),
        "import_open_bitwarden" => Call::Later(Later::ImportOpenBitwarden(args(command, json)?)),
        "import_kdbx_argon2" => Call::Later(Later::ImportKdbxArgon2(args(command, json)?)),
        "import_kdbx_aes_kdf" => Call::Later(Later::ImportKdbxAesKdf(args(command, json)?)),
        "move_target" => Call::Later(Later::MoveTarget),
        "move_login" => Call::Later(Later::MoveLogin(args(command, json)?)),
        "move_login_two_factor" => Call::Later(Later::MoveLoginTwoFactor(args(command, json)?)),
        "move_login_new_device" => Call::Later(Later::MoveLoginNewDevice(args(command, json)?)),
        "move_login_send_email" => Call::Later(Later::MoveLoginSendEmail),
        "move_start" => Call::Later(Later::MoveStart),
        "move_cancel" => Call::Now(Now::MoveCancel),
        "move_close" => Call::Later(Later::MoveClose),
        "sends" => Call::Now(Now::Sends),
        "stage_send_file" => Call::Now(Now::StageSendFile),
        "save_send" => Call::Later(Later::SaveSend(args(command, json)?)),
        "remove_send_auth" => Call::Later(Later::RemoveSendAuth(args(command, json)?)),
        "delete_send" => Call::Later(Later::DeleteSend(args(command, json)?)),
        "suite_view" => Call::Later(Later::SuiteView(args(command, json)?)),
        "suite_create" => Call::Later(Later::SuiteCreate(args(command, json)?)),
        "suite_save" => Call::Later(Later::SuiteSave(args(command, json)?)),
        "suite_reveal" => Call::Later(Later::SuiteReveal(args(command, json)?)),
        "suite_copy" => Call::Later(Later::SuiteCopy(args(command, json)?)),
        "suite_generate_key" => Call::Later(Later::SuiteGenerateKey(args(command, json)?)),
        "suite_import_key" => Call::Later(Later::SuiteImportKey(args(command, json)?)),
        "suite_save_key" => Call::Later(Later::SuiteSaveKey(args(command, json)?)),
        "suite_save_rdp" => Call::Later(Later::SuiteSaveRdp(args(command, json)?)),
        "suite_open_in_app" => Call::Later(Later::SuiteOpenInApp(args(command, json)?)),
        "wifi_connect" => Call::Later(Later::WifiConnect(args(command, json)?)),
        "wifi_settings" => Call::Later(Later::WifiSettings),
        "autofill_provider_status" => Call::Later(Later::AutofillProviderStatus),
        "autofill_provider_request" => {
            Call::Later(Later::AutofillProviderRequest(args(command, json)?))
        }
        "credential_exchange_pending" => Call::Later(Later::CredentialExchangePending),
        "credential_exchange_import" => {
            Call::Later(Later::CredentialExchangeImport(args(command, json)?))
        }
        "autofill_log" => Call::Later(Later::AutofillLog),
        "autofill_log_clear" => Call::Later(Later::AutofillLogClear),
        "passkey_request" => Call::Now(Now::PasskeyRequest),
        "passkey_answer" => Call::Later(Later::PasskeyAnswer(args(command, json)?)),
        "passkey_provider_status" => Call::Now(Now::PasskeyProviderStatus),
        "set_passkey_provider" => Call::Now(Now::SetPasskeyProvider(args(command, json)?)),
        "safari_extension_status" => Call::Later(Later::SafariExtensionStatus),
        "safari_extension_open" => Call::Later(Later::SafariExtensionOpen),
        "open_project_page" => Call::Now(Now::OpenProjectPage(args(command, json)?)),
        "open_item_uri" => Call::Now(Now::OpenItemUri(args(command, json)?)),
        "open_web_vault" => Call::Now(Now::OpenWebVault),
        _ => {
            return Err(Failure::new(
                "unknown-command",
                format!("No command {command}."),
            ))
        }
    })
}

/// Runs a synchronous command; the answer as JSON.
pub(crate) fn run_now(app: &App, call: Now) -> Result<String, Failure> {
    match call {
        Now::VaultStatus => json(app.vault_status()),
        Now::LoginCancel => {
            app.login_cancel();
            json(())
        }
        Now::Lock => {
            app.lock();
            json(())
        }
        Now::Logout(a) => json(app.logout(a.id)?),
        Now::SwitchAccount(a) => json(app.switch_account(a.id)?),
        Now::RenameAccount(a) => json(app.rename_account(a.id, a.label)?),
        Now::Touch => {
            app.touch();
            json(())
        }
        Now::SetSecurity(a) => {
            app.set_security(a.auto_lock_minutes, a.clipboard_seconds, a.lock_with_system);
            json(())
        }
        Now::VaultOverview => json(app.vault_overview()?),
        Now::VaultItems => json(app.vault_items()?),
        Now::VaultItem(a) => json(app.vault_item(a.id)?),
        Now::RevealField(a) => json(app.reveal_field(a.id, a.field)?),
        Now::CopyField(a) => json(app.copy_field(a.id, a.field)?),
        Now::CopyGenerated(a) => json(app.copy_generated(a.text)?),
        Now::CopyText(a) => json(app.copy_text(a.text)?),
        Now::TotpCode(a) => json(app.totp_code(a.id)?),
        Now::GeneratePassword(a) => json(app.generate_password(a.options)?),
        Now::ItemPasskeys(a) => json(app.item_passkeys(a.id)?),
        Now::UwuExtrasKeySeen => json(app.uwu_extras_key_seen()?),
        Now::UwuStatus => json(app.uwu_status()),
        Now::OpenWebVaultAt(a) => json(app.open_web_vault_at(a.place, a.id)?),
        Now::RevealVersionField(a) => {
            json(app.reveal_version_field(a.id, a.version_id, a.field)?)
        }
        Now::MoveCancel => {
            app.move_cancel();
            json(())
        }
        Now::Sends => json(app.sends()?),
        Now::StageSendFile => json(app.stage_send_file(None)?),
        Now::PasskeyRequest => json(app.passkey_request()),
        Now::PasskeyProviderStatus => json(app.passkey_provider_status()),
        Now::SetPasskeyProvider(a) => json(app.set_passkey_provider(a.settings)?),
        Now::OpenProjectPage(a) => json(
            app.open_project_page(a.page)
                .map_err(|message| Failure::new("unknown", message))?,
        ),
        Now::OpenItemUri(a) => json(
            app.open_item_uri(a.id, a.index)
                .map_err(|message| Failure::new("unknown", message))?,
        ),
        Now::OpenWebVault => json(
            app.open_web_vault()
                .map_err(|message| Failure::new("unknown", message))?,
        ),
    }
}

/// Runs an asynchronous command; the answer as JSON.
pub(crate) async fn run_later(app: App, call: Later) -> Result<String, Failure> {
    match call {
        Later::Login(a) => json(app.login(a.server, a.email, a.password).await?),
        Later::LoginTwoFactor(a) => {
            json(app.login_two_factor(a.provider, a.code, a.remember).await?)
        }
        Later::LoginNewDevice(a) => json(app.login_new_device(a.code).await?),
        Later::LoginSendEmail => json(app.login_send_email().await?),
        Later::LoginSso(a) => json(app.login_sso(a.server, a.email).await?),
        Later::LoginSsoPassword(a) => json(app.login_sso_password(a.password).await?),
        Later::Unlock(a) => json(app.unlock(a.password).await?),
        Later::UnlockWithHello(a) => json(app.unlock_with_hello(a.prompt).await?),
        Later::SetHello(a) => json(app.set_hello(a.enabled, a.prompt).await?),
        Later::SyncNow => json(app.sync_now().await?),
        Later::VerifyReprompt(a) => json(app.verify_reprompt(a.id, a.password).await?),
        Later::DeletePasskey(a) => json(app.delete_passkey(a.id, a.index, a.credential_id).await?),
        Later::SaveItem(a) => json(app.save_item(a.id, a.draft).await?),
        Later::SetFavorite(a) => json(app.set_favorite(a.id, a.favorite).await?),
        Later::SetItemFolder(a) => json(app.set_item_folder(a.id, a.folder_id).await?),
        Later::DeleteItem(a) => json(app.delete_item(a.id, a.permanent).await?),
        Later::RestoreItem(a) => json(app.restore_item(a.id).await?),
        Later::SaveFolder(a) => json(app.save_folder(a.id, a.name).await?),
        Later::DeleteFolder(a) => json(app.delete_folder(a.id).await?),
        Later::UwuTravel => json(app.uwu_travel().await?),
        Later::ItemIcons(a) => json(app.item_icons(a.ids, a.automatic).await?),
        Later::SetOwnIcon(a) => json(app.set_own_icon(a.id, a.png).await?),
        Later::FetchDeviceIcon(a) => json(app.fetch_device_icon(a.id).await?),
        Later::DeleteOwnIcon(a) => json(app.delete_own_icon(a.id).await?),
        Later::IconLibrary => json(app.icon_library().await?),
        Later::LibraryIcon(a) => json(app.library_icon(a.source, a.icon_id, a.variant).await?),
        Later::DeviceIcon(a) => json(app.device_icon(a.uri).await?),
        Later::ItemVersions(a) => json(app.item_versions(a.id).await?),
        Later::RestoreVersion(a) => json(app.restore_version(a.id, a.version_id).await?),
        Later::DeleteVersions(a) => json(app.delete_versions(a.id, a.version_id).await?),
        Later::SetReminder(a) => json(app.set_reminder(a.id, a.due, a.every_months).await?),
        Later::DeleteReminder(a) => json(app.delete_reminder(a.id).await?),
        Later::FileRequests => json(app.file_requests().await?),
        Later::CreateFileRequest(a) => json(app.create_file_request(a.input).await?),
        Later::UpdateFileRequest(a) => json(
            app.update_file_request(a.id, a.input, a.new_link, a.remove_password)
                .await?,
        ),
        Later::DeleteFileRequest(a) => json(app.delete_file_request(a.id).await?),
        Later::FileRequestSubmissions(a) => json(app.file_request_submissions(a.request_id).await?),
        Later::SaveSubmissionFile(a) => json(
            app.save_submission_file(a.request_id, a.submission_id, a.file_id, a.allow_risky)
                .await?,
        ),
        Later::MarkSubmissionSeen(a) => json(
            app.mark_submission_seen(a.request_id, a.submission_id)
                .await?,
        ),
        Later::DeleteSubmission(a) => {
            json(app.delete_submission(a.request_id, a.submission_id).await?)
        }
        Later::TakeOverSubmission(a) => json(
            app.take_over_submission(a.request_id, a.submission_id, a.name, a.sender_label)
                .await?,
        ),
        Later::MaskedConnection => json(app.masked_connection().await?),
        Later::MaskedAddresses => json(app.masked_addresses().await?),
        Later::CreateMaskedAddress(a) => json(
            app.create_masked_address(a.for_domain, a.description, a.cipher_id)
                .await?,
        ),
        Later::UpdateMaskedAddress(a) => json(
            app.update_masked_address(a.id, a.address_state, a.link)
                .await?,
        ),
        Later::DeleteMaskedAddress(a) => json(app.delete_masked_address(a.id).await?),
        Later::SendOptions => json(app.send_options().await?),
        Later::ShareAsSend(a) => json(app.share_as_send(a.id, a.input).await?),
        Later::HealthReport(a) => json(app.health_report(a.fresh).await?),
        Later::HealthIgnore(a) => json(app.health_ignore(a.item_id, a.kind, a.ignored).await?),
        Later::HealthOpenPage(a) => json(app.health_open_page(a.item_id).await?),
        Later::HealthSavePassword(a) => {
            json(app.health_save_password(a.item_id, a.password).await?)
        }
        Later::HealthEmailOptIn => json(app.health_email_opt_in().await?),
        Later::SetHealthEmailOptIn(a) => json(app.set_health_email_opt_in(a.opted_in).await?),
        Later::HealthCheckEmails => json(app.health_check_emails().await?),
        Later::ImportVault(a) => json(app.import_vault(a.format, a.text).await?),
        Later::ImportOpenBitwarden(a) => json(app.import_open_bitwarden(a.text, a.password).await?),
        Later::ImportKdbxArgon2(a) => json(
            app.import_kdbx_argon2(
                a.id,
                a.version,
                a.key,
                a.salt,
                a.memory_kib,
                a.iterations,
                a.lanes,
            )
            .await?,
        ),
        Later::ImportKdbxAesKdf(a) => json(app.import_kdbx_aes_kdf(a.key, a.seed, a.rounds).await?),
        Later::MoveTarget => json(app.move_target().await?),
        Later::MoveLogin(a) => json(app.move_login(a.server, a.email, a.password).await?),
        Later::MoveLoginTwoFactor(a) => json(app.move_login_two_factor(a.provider, a.code).await?),
        Later::MoveLoginNewDevice(a) => json(app.move_login_new_device(a.code).await?),
        Later::MoveLoginSendEmail => json(app.move_login_send_email().await?),
        Later::MoveStart => json(app.move_start().await?),
        Later::MoveClose => json(app.move_close().await?),
        Later::SaveSend(a) => json(
            app.save_send(a.id, a.draft, a.domain, a.choose_domain)
                .await?,
        ),
        Later::RemoveSendAuth(a) => json(app.remove_send_auth(a.id).await?),
        Later::DeleteSend(a) => json(app.delete_send(a.id).await?),
        Later::SuiteView(a) => json(app.suite_view(a.space).await?),
        Later::SuiteCreate(a) => json(app.suite_create(a.space).await?),
        Later::SuiteSave(a) => json(app.suite_save(a.space, a.ops).await?),
        Later::SuiteReveal(a) => json(app.suite_reveal(a.space, a.id).await?),
        Later::SuiteCopy(a) => json(app.suite_copy(a.space, a.id).await?),
        Later::SuiteGenerateKey(a) => json(
            app.suite_generate_key(a.space, a.label, a.comment, a.passphrase)
                .await?,
        ),
        Later::SuiteImportKey(a) => json(
            app.suite_import_key(a.space, a.label, a.private_key, a.passphrase)
                .await?,
        ),
        Later::SuiteSaveKey(a) => json(app.suite_save_key(a.space, a.id, a.half).await?),
        Later::SuiteSaveRdp(a) => json(app.suite_save_rdp(a.id).await?),
        Later::SuiteOpenInApp(a) => json(app.suite_open_in_app(a.space, a.id).await?),
        Later::WifiConnect(a) => json(app.wifi_connect(a.id).await?),
        Later::WifiSettings => json(app.wifi_settings().await?),
        Later::AutofillProviderStatus => json(app.autofill_provider_status().await),
        Later::AutofillProviderRequest(a) => json(app.autofill_provider_request(a.target).await?),
        Later::CredentialExchangePending => json(app.credential_exchange_pending().await),
        Later::CredentialExchangeImport(a) => {
            json(app.credential_exchange_import(a.discard).await?)
        }
        Later::AutofillLog => json(app.autofill_log().await),
        Later::AutofillLogClear => json(app.autofill_log_clear().await?),
        Later::PasskeyAnswer(a) => json(
            app.passkey_answer(a.id, a.allow, a.item_id, a.credential_id, a.password)
                .await?,
        ),
        Later::SafariExtensionStatus => json(app.safari_extension_status().await),
        Later::SafariExtensionOpen => json(app.safari_extension_open().await?),
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod vault {
    use serde::Deserialize;
    use serde_json::Value;
    use uwulock_app::vault::*;
    use uwulock_app::{Failure, Result};
    use uwulock_bitwarden::{generator, passkey};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Login {
        pub(crate) server: ServerInput,
        pub(crate) email: String,
        pub(crate) password: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct LoginTwoFactor {
        pub(crate) provider: u8,
        pub(crate) code: String,
        pub(crate) remember: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct LoginNewDevice {
        pub(crate) code: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct LoginSso {
        pub(crate) server: ServerInput,
        pub(crate) email: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct LoginSsoPassword {
        pub(crate) password: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Unlock {
        pub(crate) password: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct UnlockWithHello {
        pub(crate) prompt: Option<uwulock_app::hello::Prompt>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SetHello {
        pub(crate) enabled: bool,
        pub(crate) prompt: Option<uwulock_app::hello::Prompt>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Logout {
        pub(crate) id: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SwitchAccount {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct RenameAccount {
        pub(crate) id: String,
        pub(crate) label: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SetSecurity {
        pub(crate) auto_lock_minutes: Option<u32>,
        pub(crate) clipboard_seconds: Option<u32>,
        pub(crate) lock_with_system: Option<bool>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct VaultItem {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct VerifyReprompt {
        pub(crate) id: String,
        pub(crate) password: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct RevealField {
        pub(crate) id: String,
        pub(crate) field: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct CopyField {
        pub(crate) id: String,
        pub(crate) field: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct CopyGenerated {
        pub(crate) text: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct CopyText {
        pub(crate) text: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct TotpCode {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct GeneratePassword {
        pub(crate) options: generator::Options,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ItemPasskeys {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeletePasskey {
        pub(crate) id: String,
        pub(crate) index: usize,
        pub(crate) credential_id: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SaveItem {
        pub(crate) id: Option<String>,
        pub(crate) draft: Draft,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SetFavorite {
        pub(crate) id: String,
        pub(crate) favorite: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SetItemFolder {
        pub(crate) id: String,
        pub(crate) folder_id: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteItem {
        pub(crate) id: String,
        pub(crate) permanent: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct RestoreItem {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SaveFolder {
        pub(crate) id: Option<String>,
        pub(crate) name: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteFolder {
        pub(crate) id: String,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod extras {
    use serde::Deserialize;
    use serde_json::Value;
    use std::collections::HashMap;
    use uwulock_app::extras::*;
    use uwulock_app::{Failure, Result};
    use uwulock_bitwarden::uwu::{MaskedAddress, MaskedConnection};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct OpenWebVaultAt {
        pub(crate) place: String,
        pub(crate) id: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ItemIcons {
        pub(crate) ids: Vec<String>,
        pub(crate) automatic: Option<bool>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SetOwnIcon {
        pub(crate) id: String,
        pub(crate) png: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct FetchDeviceIcon {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteOwnIcon {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct LibraryIcon {
        pub(crate) source: String,
        pub(crate) icon_id: String,
        pub(crate) variant: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeviceIcon {
        pub(crate) uri: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ItemVersions {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct RevealVersionField {
        pub(crate) id: String,
        pub(crate) version_id: Option<String>,
        pub(crate) field: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct RestoreVersion {
        pub(crate) id: String,
        pub(crate) version_id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteVersions {
        pub(crate) id: String,
        pub(crate) version_id: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SetReminder {
        pub(crate) id: String,
        pub(crate) due: Option<String>,
        pub(crate) every_months: Option<u32>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteReminder {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct CreateFileRequest {
        pub(crate) input: FileRequestInput,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct UpdateFileRequest {
        pub(crate) id: String,
        pub(crate) input: FileRequestInput,
        pub(crate) new_link: bool,
        pub(crate) remove_password: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteFileRequest {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct FileRequestSubmissions {
        pub(crate) request_id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SaveSubmissionFile {
        pub(crate) request_id: String,
        pub(crate) submission_id: String,
        pub(crate) file_id: String,
        pub(crate) allow_risky: Option<bool>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct MarkSubmissionSeen {
        pub(crate) request_id: String,
        pub(crate) submission_id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteSubmission {
        pub(crate) request_id: String,
        pub(crate) submission_id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct TakeOverSubmission {
        pub(crate) request_id: String,
        pub(crate) submission_id: String,
        pub(crate) name: String,
        pub(crate) sender_label: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct CreateMaskedAddress {
        pub(crate) for_domain: Option<String>,
        pub(crate) description: Option<String>,
        pub(crate) cipher_id: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct UpdateMaskedAddress {
        pub(crate) id: String,
        pub(crate) address_state: Option<String>,
        pub(crate) link: Option<Option<String>>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteMaskedAddress {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ShareAsSend {
        pub(crate) id: String,
        pub(crate) input: ShareInput,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod health {
    use serde::Deserialize;
    use uwulock_app::health::*;
    use uwulock_app::{Failure, Result};
    use uwulock_bitwarden::health::EmailOptIn;
    use uwulock_core::health::{Ignored, ProblemKind};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct HealthReport {
        pub(crate) fresh: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct HealthIgnore {
        pub(crate) item_id: String,
        pub(crate) kind: ProblemKind,
        pub(crate) ignored: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct HealthOpenPage {
        pub(crate) item_id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct HealthSavePassword {
        pub(crate) item_id: String,
        pub(crate) password: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SetHealthEmailOptIn {
        pub(crate) opted_in: bool,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod importing {
    use serde::Deserialize;
    use uwulock_app::importing::*;
    use uwulock_app::{Failure, Result};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ImportVault {
        pub(crate) format: String,
        pub(crate) text: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ImportOpenBitwarden {
        pub(crate) text: String,
        pub(crate) password: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ImportKdbxArgon2 {
        pub(crate) id: bool,
        pub(crate) version: u32,
        pub(crate) key: Vec<u8>,
        pub(crate) salt: Vec<u8>,
        pub(crate) memory_kib: u32,
        pub(crate) iterations: u32,
        pub(crate) lanes: u32,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ImportKdbxAesKdf {
        pub(crate) key: Vec<u8>,
        pub(crate) seed: Vec<u8>,
        pub(crate) rounds: u64,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod moving {
    use serde::Deserialize;
    use uwulock_app::moving::*;
    use uwulock_app::vault::ServerInput;
    use uwulock_app::{Failure, Result};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct MoveLogin {
        pub(crate) server: ServerInput,
        pub(crate) email: String,
        pub(crate) password: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct MoveLoginTwoFactor {
        pub(crate) provider: u8,
        pub(crate) code: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct MoveLoginNewDevice {
        pub(crate) code: String,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod sends {
    use serde::Deserialize;
    use uwulock_app::sends::*;
    use uwulock_app::{Failure, Result};
    use uwulock_bitwarden::send::SendDraft;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SaveSend {
        pub(crate) id: Option<String>,
        pub(crate) draft: SendDraft,
        pub(crate) domain: Option<String>,
        pub(crate) choose_domain: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct RemoveSendAuth {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct DeleteSend {
        pub(crate) id: String,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod suite {
    use serde::Deserialize;
    use uwulock_app::suite::*;
    use uwulock_app::{Failure, Result};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteView {
        pub(crate) space: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteCreate {
        pub(crate) space: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteSave {
        pub(crate) space: String,
        pub(crate) ops: Vec<Op>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteReveal {
        pub(crate) space: String,
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteCopy {
        pub(crate) space: String,
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteGenerateKey {
        pub(crate) space: String,
        pub(crate) label: String,
        pub(crate) comment: String,
        pub(crate) passphrase: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteImportKey {
        pub(crate) space: String,
        pub(crate) label: String,
        pub(crate) private_key: String,
        pub(crate) passphrase: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteSaveKey {
        pub(crate) space: String,
        pub(crate) id: String,
        pub(crate) half: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteSaveRdp {
        pub(crate) id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SuiteOpenInApp {
        pub(crate) space: String,
        pub(crate) id: String,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod wifi {
    use serde::Deserialize;
    use uwulock_app::wifi::*;
    use uwulock_app::{Failure, Result};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct WifiConnect {
        pub(crate) id: String,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod autofill {
    use serde::Deserialize;
    use uwulock_app::autofill::*;
    use uwulock_app::{Failure, Result};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct AutofillProviderRequest {
        pub(crate) target: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct CredentialExchangeImport {
        pub(crate) discard: bool,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod passkeys {
    use serde::Deserialize;
    use uwulock_app::passkeys::*;
    use uwulock_app::{Failure, Result};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct PasskeyAnswer {
        pub(crate) id: u64,
        pub(crate) allow: bool,
        pub(crate) item_id: Option<String>,
        pub(crate) credential_id: Option<String>,
        pub(crate) password: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct SetPasskeyProvider {
        pub(crate) settings: Settings,
    }
}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod safari {}

/// The arguments, with the types the Tauri command's module uses.
#[allow(unused_imports)]
pub(crate) mod system {
    use serde::Deserialize;
    use uwulock_app::system::*;
    use uwulock_app::{Failure, Result};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct OpenProjectPage {
        pub(crate) page: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct OpenItemUri {
        pub(crate) id: String,
        pub(crate) index: usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command with its required arguments, as the page sends them.
    const SAMPLES: &[(&str, &str)] = &[
        ("vault_status", r#"{}"#),
        (
            "login",
            r#"{"server": {"kind": "bitwarden"}, "email": "x", "password": "x"}"#,
        ),
        (
            "login_two_factor",
            r#"{"provider": 1, "code": "x", "remember": true}"#,
        ),
        ("login_new_device", r#"{"code": "x"}"#),
        ("login_send_email", r#"{}"#),
        ("login_cancel", r#"{}"#),
        ("login_sso", r#"{"server": {"kind": "bitwarden"}}"#),
        ("login_sso_password", r#"{"password": "x"}"#),
        ("unlock", r#"{"password": "x"}"#),
        ("unlock_with_hello", r#"{}"#),
        ("set_hello", r#"{"enabled": true}"#),
        ("lock", r#"{}"#),
        ("logout", r#"{}"#),
        ("switch_account", r#"{"id": "x"}"#),
        ("rename_account", r#"{"id": "x", "label": "x"}"#),
        ("touch", r#"{}"#),
        ("set_security", r#"{}"#),
        ("sync_now", r#"{}"#),
        ("vault_overview", r#"{}"#),
        ("vault_items", r#"{}"#),
        ("vault_item", r#"{"id": "x"}"#),
        ("verify_reprompt", r#"{"id": "x", "password": "x"}"#),
        ("reveal_field", r#"{"id": "x", "field": "x"}"#),
        ("copy_field", r#"{"id": "x", "field": "x"}"#),
        ("copy_generated", r#"{"text": "x"}"#),
        ("copy_text", r#"{"text": "x"}"#),
        ("totp_code", r#"{"id": "x"}"#),
        (
            "generate_password",
            r#"{"options": {"length": 20, "lowercase": true, "uppercase": true, "digits": true, "symbols": false, "avoidAmbiguous": false}}"#,
        ),
        ("item_passkeys", r#"{"id": "x"}"#),
        ("delete_passkey", r#"{"id": "x", "index": 0}"#),
        ("save_item", r#"{"draft": {"kind": "login", "name": "x"}}"#),
        ("set_favorite", r#"{"id": "x", "favorite": true}"#),
        ("set_item_folder", r#"{"id": "x"}"#),
        ("delete_item", r#"{"id": "x", "permanent": true}"#),
        ("restore_item", r#"{"id": "x"}"#),
        ("save_folder", r#"{"name": "x"}"#),
        ("delete_folder", r#"{"id": "x"}"#),
        ("uwu_extras_key_seen", r#"{}"#),
        ("uwu_status", r#"{}"#),
        ("uwu_travel", r#"{}"#),
        ("open_web_vault_at", r#"{"place": "x"}"#),
        ("item_icons", r#"{"ids": ["x"]}"#),
        ("set_own_icon", r#"{"id": "x", "png": "x"}"#),
        ("fetch_device_icon", r#"{"id": "x"}"#),
        ("delete_own_icon", r#"{"id": "x"}"#),
        ("icon_library", r#"{}"#),
        ("library_icon", r#"{"source": "x", "iconId": "x"}"#),
        ("device_icon", r#"{"uri": "x"}"#),
        ("item_versions", r#"{"id": "x"}"#),
        ("reveal_version_field", r#"{"id": "x", "field": "x"}"#),
        ("restore_version", r#"{"id": "x", "versionId": "x"}"#),
        ("delete_versions", r#"{"id": "x"}"#),
        ("set_reminder", r#"{"id": "x"}"#),
        ("delete_reminder", r#"{"id": "x"}"#),
        ("file_requests", r#"{}"#),
        (
            "create_file_request",
            r#"{"input": {"label": "x", "title": "x", "maxFiles": 1, "textAllowed": true}}"#,
        ),
        (
            "update_file_request",
            r#"{"id": "x", "input": {"label": "x", "title": "x", "maxFiles": 1, "textAllowed": true}, "newLink": true, "removePassword": true}"#,
        ),
        ("delete_file_request", r#"{"id": "x"}"#),
        ("file_request_submissions", r#"{"requestId": "x"}"#),
        (
            "save_submission_file",
            r#"{"requestId": "x", "submissionId": "x", "fileId": "x"}"#,
        ),
        (
            "mark_submission_seen",
            r#"{"requestId": "x", "submissionId": "x"}"#,
        ),
        (
            "delete_submission",
            r#"{"requestId": "x", "submissionId": "x"}"#,
        ),
        (
            "take_over_submission",
            r#"{"requestId": "x", "submissionId": "x", "name": "x", "senderLabel": "x"}"#,
        ),
        ("masked_connection", r#"{}"#),
        ("masked_addresses", r#"{}"#),
        ("create_masked_address", r#"{}"#),
        ("update_masked_address", r#"{"id": "x"}"#),
        ("delete_masked_address", r#"{"id": "x"}"#),
        ("send_options", r#"{}"#),
        (
            "share_as_send",
            r#"{"id": "x", "input": {"fields": [["password", "Password"]], "deletionDays": 7}}"#,
        ),
        ("health_report", r#"{"fresh": true}"#),
        (
            "health_ignore",
            r#"{"itemId": "x", "kind": "weak", "ignored": true}"#,
        ),
        ("health_open_page", r#"{"itemId": "x"}"#),
        (
            "health_save_password",
            r#"{"itemId": "x", "password": "x"}"#,
        ),
        ("health_email_opt_in", r#"{}"#),
        ("set_health_email_opt_in", r#"{"optedIn": true}"#),
        ("health_check_emails", r#"{}"#),
        ("import_vault", r#"{"format": "x", "text": "x"}"#),
        ("import_open_bitwarden", r#"{"text": "x", "password": "x"}"#),
        (
            "import_kdbx_argon2",
            r#"{"id": true, "version": 1, "key": [1, 2], "salt": [1, 2], "memoryKib": 1, "iterations": 1, "lanes": 1}"#,
        ),
        (
            "import_kdbx_aes_kdf",
            r#"{"key": [1, 2], "seed": [1, 2], "rounds": 1}"#,
        ),
        ("move_target", r#"{}"#),
        (
            "move_login",
            r#"{"server": {"kind": "bitwarden"}, "email": "x", "password": "x"}"#,
        ),
        ("move_login_two_factor", r#"{"provider": 1, "code": "x"}"#),
        ("move_login_new_device", r#"{"code": "x"}"#),
        ("move_login_send_email", r#"{}"#),
        ("move_start", r#"{}"#),
        ("move_cancel", r#"{}"#),
        ("move_close", r#"{}"#),
        ("sends", r#"{}"#),
        ("stage_send_file", r#"{}"#),
        (
            "save_send",
            r#"{"draft": {"kind": 0, "name": "x", "deletionDate": "2030-01-01T00:00:00Z"}, "chooseDomain": true}"#,
        ),
        ("remove_send_auth", r#"{"id": "x"}"#),
        ("delete_send", r#"{"id": "x"}"#),
        ("suite_view", r#"{"space": "x"}"#),
        ("suite_create", r#"{"space": "x"}"#),
        ("suite_save", r#"{"space": "x", "ops": []}"#),
        ("suite_reveal", r#"{"space": "x", "id": "x"}"#),
        ("suite_copy", r#"{"space": "x", "id": "x"}"#),
        (
            "suite_generate_key",
            r#"{"space": "x", "label": "x", "comment": "x"}"#,
        ),
        (
            "suite_import_key",
            r#"{"space": "x", "label": "x", "privateKey": "x"}"#,
        ),
        (
            "suite_save_key",
            r#"{"space": "x", "id": "x", "half": "x"}"#,
        ),
        ("suite_save_rdp", r#"{"id": "x"}"#),
        ("suite_open_in_app", r#"{"space": "x", "id": "x"}"#),
        ("wifi_connect", r#"{"id": "x"}"#),
        ("wifi_settings", r#"{}"#),
        ("autofill_provider_status", r#"{}"#),
        ("autofill_provider_request", r#"{"target": "x"}"#),
        ("credential_exchange_pending", r#"{}"#),
        ("credential_exchange_import", r#"{"discard": true}"#),
        ("autofill_log", r#"{}"#),
        ("autofill_log_clear", r#"{}"#),
        ("passkey_request", r#"{}"#),
        ("passkey_answer", r#"{"id": 1, "allow": true}"#),
        ("passkey_provider_status", r#"{}"#),
        ("set_passkey_provider", r#"{"settings": {}}"#),
        ("safari_extension_status", r#"{}"#),
        ("safari_extension_open", r#"{}"#),
        ("open_project_page", r#"{"page": "x"}"#),
        ("open_item_uri", r#"{"id": "x", "index": 0}"#),
        ("open_web_vault", r#"{}"#),
    ];

    #[test]
    fn every_command_reads_its_arguments() {
        assert_eq!(SAMPLES.len(), COMMANDS.len());
        for ((name, json), listed) in SAMPLES.iter().zip(COMMANDS) {
            assert_eq!(name, listed);
            if let Err(failure) = parse(name, json) {
                panic!("{name}: {}", failure.message());
            }
        }
    }

    #[test]
    fn a_missing_argument_says_which() {
        let Err(failure) = parse("login", r#"{"server": {"kind": "bitwarden"}}"#) else {
            panic!("login without an email");
        };
        assert_eq!(failure.kind(), "args");
        assert!(failure.message().contains("email"), "{}", failure.message());
    }

    #[test]
    fn an_unknown_command_is_an_error() {
        let Err(failure) = parse("format_disk", "{}") else {
            panic!("an unknown command ran");
        };
        assert_eq!(failure.kind(), "unknown-command");
    }
}
