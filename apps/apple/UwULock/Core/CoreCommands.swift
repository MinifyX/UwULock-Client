// The core's commands, typed: one method per command of the Tauri app (the same names and
// camelCase arguments), grouped by area. The types they take and give: Models/. A command that
// answers a `Status` keeps it (`remember`), so `Core.status` is current without the event.
import Foundation

extension Core {

    // MARK: Login and accounts

    /// The status of the account on screen.
    func vaultStatus() async throws -> Status {
        let status: Status = try await call("vault_status")
        remember(status)
        return status
    }

    /// Logs in with the master password: done, or the next step.
    func login(server: ServerInput, email: String, password: String) async throws -> LoginStep {
        let step: LoginStep = try await call(
            "login", ["server": server, "email": email, "password": password])
        return remembered(step)
    }

    /// Answers the two-step login with a code from method `provider`.
    func loginTwoFactor(provider: Int, code: String, remember: Bool) async throws -> LoginStep {
        let step: LoginStep = try await call(
            "login_two_factor", ["provider": provider, "code": code, "remember": remember])
        return remembered(step)
    }

    /// Answers the new-device check with the code mailed to the account.
    func loginNewDevice(code: String) async throws -> LoginStep {
        let step: LoginStep = try await call("login_new_device", ["code": code])
        return remembered(step)
    }

    /// Mails the new-device code again.
    func loginSendEmail() async throws {
        try await run("login_send_email")
    }

    /// Drops a login that is half way.
    func loginCancel() async throws {
        try await run("login_cancel")
    }

    /// Logs in through the browser (SSO).
    func loginSso(server: ServerInput, email: String?) async throws -> LoginStep {
        let step: LoginStep = try await call("login_sso", ["server": server, "email": email])
        return remembered(step)
    }

    /// After SSO: opens the vault with the master password, or sets it for a new account.
    func loginSsoPassword(password: String) async throws -> LoginStep {
        let step: LoginStep = try await call("login_sso_password", ["password": password])
        return remembered(step)
    }

    // MARK: Lock and unlock

    /// Unlocks the vault with the master password.
    func unlock(password: String) async throws -> Status {
        let status: Status = try await call("unlock", ["password": password])
        remember(status)
        return status
    }

    /// Unlocks with Face ID / Touch ID / Optic ID.
    func unlockWithHello(prompt: UnlockPrompt? = nil) async throws -> Status {
        let status: Status = try await call("unlock_with_hello", ["prompt": prompt])
        remember(status)
        return status
    }

    /// Turns unlocking with biometrics on or off for the account.
    func setHello(enabled: Bool, prompt: UnlockPrompt? = nil) async throws -> Status {
        let status: Status = try await call("set_hello", ["enabled": enabled, "prompt": prompt])
        remember(status)
        return status
    }

    /// Locks the vault.
    func lock() async throws {
        try await run("lock")
    }

    /// Logs out `id`, or the account on screen without one. The others stay.
    func logout(id: String? = nil) async throws -> Status {
        let status: Status = try await call("logout", ["id": id])
        remember(status)
        return status
    }

    /// Makes `id` the account on screen.
    func switchAccount(id: String) async throws -> Status {
        let status: Status = try await call("switch_account", ["id": id])
        remember(status)
        return status
    }

    /// Gives the account a name of its own in the switcher.
    func renameAccount(id: String, label: String) async throws -> Status {
        let status: Status = try await call("rename_account", ["id": id, "label": label])
        remember(status)
        return status
    }

    /// Counts as activity: restarts the auto-lock's clock.
    func touch() async throws {
        try await run("touch")
    }

    /// Sets the auto-lock minutes, the clipboard seconds and locking with the system.
    func setSecurity(
        autoLockMinutes: Int?, clipboardSeconds: Int?, lockWithSystem: Bool
    ) async throws {
        try await run(
            "set_security",
            [
                "autoLockMinutes": autoLockMinutes, "clipboardSeconds": clipboardSeconds,
                "lockWithSystem": lockWithSystem,
            ])
    }

    /// Syncs the vault with the server now.
    func syncNow() async throws -> Status {
        let status: Status = try await call("sync_now")
        remember(status)
        return status
    }

    // MARK: Vault and items

    /// The folders, collections and organizations.
    func vaultOverview() async throws -> Overview {
        try await call("vault_overview")
    }

    /// Every item as a summary.
    func vaultItems() async throws -> [ItemSummary] {
        try await call("vault_items")
    }

    /// One item with its details (secrets left out).
    func vaultItem(id: String) async throws -> ItemDetail {
        try await call("vault_item", ["id": id])
    }

    /// Answers an item's master password re-prompt.
    func verifyReprompt(id: String, password: String) async throws {
        try await run("verify_reprompt", ["id": id, "password": password])
    }

    /// One secret value of an item (`password`, `card-number`, `field:<n>`, ...).
    func revealField(id: String, field: String) async throws -> String {
        try await call("reveal_field", ["id": id, "field": field])
    }

    /// Copies one value of an item to the clipboard, which clears itself later.
    func copyField(id: String, field: String) async throws {
        try await run("copy_field", ["id": id, "field": field])
    }

    /// Copies a generated password to the clipboard, which clears itself later.
    func copyGenerated(text: String) async throws {
        try await run("copy_generated", ["text": text])
    }

    /// Copies a plain text that is not secret and is not cleared.
    func copyText(text: String) async throws {
        try await run("copy_text", ["text": text])
    }

    /// The current and the next one-time code of a login.
    func totpCode(id: String) async throws -> TotpCode {
        try await call("totp_code", ["id": id])
    }

    /// The passkeys of a login, nothing secret.
    func itemPasskeys(id: String) async throws -> [PasskeyInfo] {
        try await call("item_passkeys", ["id": id])
    }

    /// Deletes a passkey; with `credentialId`, only if the one at `index` still is that one.
    func deletePasskey(id: String, index: Int, credentialId: String?) async throws {
        try await run(
            "delete_passkey", ["id": id, "index": index, "credentialId": credentialId])
    }

    /// Saves an item (a new one without `id`); answers its id.
    func saveItem(id: String?, draft: Draft) async throws -> String {
        try await call("save_item", ["id": id, "draft": draft])
    }

    /// Marks an item as a favorite or not.
    func setFavorite(id: String, favorite: Bool) async throws {
        try await run("set_favorite", ["id": id, "favorite": favorite])
    }

    /// Moves an item into a folder (`nil`: out of any folder).
    func setItemFolder(id: String, folderId: String?) async throws {
        try await run("set_item_folder", ["id": id, "folderId": folderId])
    }

    /// Moves an item to the trash, or deletes it for good with `permanent`.
    func deleteItem(id: String, permanent: Bool) async throws {
        try await run("delete_item", ["id": id, "permanent": permanent])
    }

    /// Takes an item back out of the trash.
    func restoreItem(id: String) async throws {
        try await run("restore_item", ["id": id])
    }

    /// Saves a folder (a new one without `id`); answers its id.
    func saveFolder(id: String?, name: String) async throws -> String {
        try await call("save_folder", ["id": id, "name": name])
    }

    /// Deletes a folder; its items stay.
    func deleteFolder(id: String) async throws {
        try await run("delete_folder", ["id": id])
    }

    // MARK: Generator

    /// Makes a password from the options.
    func generatePassword(options: GeneratorOptions) async throws -> Generated {
        try await call("generate_password", ["options": options])
    }

    // MARK: Icons, versions and reminders

    /// The icons of these items as `data:` URLs, by item id.
    func itemIcons(ids: [String], automatic: Bool) async throws -> [String: String] {
        try await call("item_icons", ["ids": ids, "automatic": automatic])
    }

    /// Seals a PNG `data:` URL as the item's own icon.
    func setOwnIcon(id: String, png: String) async throws {
        try await run("set_own_icon", ["id": id, "png": png])
    }

    /// Fetches the icon of the device at the item's local address and keeps it as its own.
    func fetchDeviceIcon(id: String) async throws {
        try await run("fetch_device_icon", ["id": id])
    }

    /// Removes the item's own icon.
    func deleteOwnIcon(id: String) async throws {
        try await run("delete_own_icon", ["id": id])
    }

    /// The icon library's index as the server mirrors it.
    func iconLibrary() async throws -> IconLibrary {
        try await call("icon_library")
    }

    /// A library icon as a PNG `data:` URL.
    func libraryIcon(_ icon: LibraryIcon, variant: String = "default") async throws -> String {
        try await call(
            "library_icon", ["source": icon.source, "iconId": icon.id, "variant": variant])
    }

    /// The icon of a device on the local network at `uri`, as a PNG `data:` URL.
    func deviceIcon(uri: String) async throws -> String {
        try await call("device_icon", ["uri": uri])
    }

    /// The earlier versions of an item.
    func itemVersions(id: String) async throws -> [UwuVersion] {
        try await call("item_versions", ["id": id])
    }

    /// A secret value of an old version; without `versionId`, of the item as it is now.
    func revealVersionField(id: String, versionId: String?, field: String) async throws
        -> String
    {
        try await call(
            "reveal_version_field", ["id": id, "versionId": versionId, "field": field])
    }

    /// Makes an old version the item again.
    func restoreVersion(id: String, versionId: String) async throws {
        try await run("restore_version", ["id": id, "versionId": versionId])
    }

    /// Deletes one old version, or all of them without `versionId`.
    func deleteVersions(id: String, versionId: String? = nil) async throws {
        try await run("delete_versions", ["id": id, "versionId": versionId])
    }

    /// Sets a reminder to change an item's password (`everyMonths` repeats it).
    func setReminder(id: String, due: String?, everyMonths: Int?) async throws {
        try await run("set_reminder", ["id": id, "due": due, "everyMonths": everyMonths])
    }

    /// Removes an item's reminder.
    func deleteReminder(id: String) async throws {
        try await run("delete_reminder", ["id": id])
    }

    // MARK: Suite and UwULock Server extras

    /// The person saw that the extras key changed: it is the one from now on.
    func extrasKeySeen() async throws {
        try await run("uwu_extras_key_seen")
    }

    /// UwULock Server's extras for the account on screen.
    func uwuStatus() async throws -> UwuStatus {
        try await call("uwu_status")
    }

    /// Whether travel mode is on and how many items it hides.
    func uwuTravel() async throws -> UwuTravel {
        try await call("uwu_travel")
    }

    /// Opens a place of the web vault in the browser.
    func openWebVaultAt(place: WebVaultPlace, id: String? = nil) async throws {
        try await run("open_web_vault_at", ["place": place, "id": id])
    }

    /// The file requests of the account.
    func fileRequests() async throws -> [FileRequest] {
        try await call("file_requests")
    }

    /// Makes a file request.
    func createFileRequest(input: FileRequestInput) async throws -> FileRequest {
        try await call("create_file_request", ["input": input])
    }

    /// Changes a file request (`newLink`: a new link; `removePassword`: no password any more).
    func updateFileRequest(
        id: String, input: FileRequestInput, newLink: Bool, removePassword: Bool
    ) async throws -> FileRequest {
        try await call(
            "update_file_request",
            ["id": id, "input": input, "newLink": newLink, "removePassword": removePassword])
    }

    /// Deletes a file request with its submissions.
    func deleteFileRequest(id: String) async throws {
        try await run("delete_file_request", ["id": id])
    }

    /// What was sent through a file request.
    func fileRequestSubmissions(requestId: String) async throws -> [Submission] {
        try await call("file_request_submissions", ["requestId": requestId])
    }

    /// Saves a submitted file into the Downloads folder; answers the path.
    func saveSubmissionFile(
        requestId: String, submissionId: String, fileId: String, allowRisky: Bool
    ) async throws -> String {
        try await call(
            "save_submission_file",
            [
                "requestId": requestId, "submissionId": submissionId, "fileId": fileId,
                "allowRisky": allowRisky,
            ])
    }

    /// Marks a submission as seen.
    func markSubmissionSeen(requestId: String, submissionId: String) async throws {
        try await run(
            "mark_submission_seen", ["requestId": requestId, "submissionId": submissionId])
    }

    /// Deletes a submission.
    func deleteSubmission(requestId: String, submissionId: String) async throws {
        try await run("delete_submission", ["requestId": requestId, "submissionId": submissionId])
    }

    /// Takes a submission over as a new vault item; answers its id.
    func takeOverSubmission(
        requestId: String, submissionId: String, name: String, senderLabel: String
    ) async throws -> String {
        try await call(
            "take_over_submission",
            [
                "requestId": requestId, "submissionId": submissionId, "name": name,
                "senderLabel": senderLabel,
            ])
    }

    /// The account's connection to the mail service for masked addresses.
    func maskedConnection() async throws -> MaskedConnection {
        try await call("masked_connection")
    }

    /// The account's masked addresses.
    func maskedAddresses() async throws -> [MaskedAddress] {
        try await call("masked_addresses")
    }

    /// Makes a masked address, optionally for a site and linked to an item.
    func createMaskedAddress(forDomain: String?, description: String?, cipherId: String?)
        async throws -> MaskedAddress
    {
        try await call(
            "create_masked_address",
            ["forDomain": forDomain, "description": description, "cipherId": cipherId])
    }

    /// Turns a masked address on or off.
    func setMaskedState(id: String, state: MaskedState) async throws -> MaskedAddress {
        try await call(
            "update_masked_address", ["id": id, "addressState": state, "link": nil as String?])
    }

    /// Links a masked address to an item.
    func linkMaskedAddress(id: String, cipherId: String) async throws -> MaskedAddress {
        try await call(
            "update_masked_address",
            ["id": id, "addressState": nil as String?, "link": cipherId])
    }

    /// Deletes a masked address.
    func deleteMaskedAddress(id: String) async throws {
        try await run("delete_masked_address", ["id": id])
    }

    /// What sharing an item as a Send offers (emails, domains).
    func sendOptions() async throws -> SendOptions {
        try await call("send_options")
    }

    /// Shares fields of an item as a Send.
    func shareAsSend(id: String, input: ShareInput) async throws -> SharedSend {
        try await call("share_as_send", ["id": id, "input": input])
    }

    // MARK: Health

    /// The password check; `fresh` asks the breach sources again.
    func healthReport(fresh: Bool) async throws -> HealthView {
        try await call("health_report", ["fresh": fresh])
    }

    /// Ignores a problem of an item, or stops ignoring it; answers the ignore list.
    func setIgnored(itemId: String, kind: ProblemKind, ignored: Bool) async throws
        -> [IgnoredEntry]
    {
        try await call(
            "health_ignore", ["itemId": itemId, "kind": kind, "ignored": ignored])
    }

    /// Opens the change-password page (or the login) in the browser.
    func openChangePage(itemId: String) async throws {
        try await run("health_open_page", ["itemId": itemId])
    }

    /// Saves `password` to the login; the old one goes into its history.
    func saveNewPassword(itemId: String, password: String) async throws {
        try await run("health_save_password", ["itemId": itemId, "password": password])
    }

    /// The consent to the check of addresses; nil when the server does not offer it.
    func emailOptIn() async throws -> EmailOptIn? {
        try await call("health_email_opt_in")
    }

    /// Gives or withdraws the consent to the check of addresses.
    func setEmailOptIn(optedIn: Bool) async throws -> EmailOptIn {
        try await call("set_health_email_opt_in", ["optedIn": optedIn])
    }

    /// Checks the account's addresses for breaches.
    func checkEmails() async throws -> EmailsResult {
        try await call("health_check_emails")
    }

    // MARK: Import

    /// Imports Bitwarden-style JSON or CSV text into the vault on screen.
    func importVault(format: ImportFormat, text: String) async throws -> ImportOutcome {
        try await call("import_vault", ["format": format, "text": text])
    }

    /// Opens a password-protected Bitwarden export; answers the JSON text.
    func importOpenBitwarden(text: String, password: String) async throws -> String {
        try await call("import_open_bitwarden", ["text": text, "password": password])
    }

    /// KeePass's Argon2 key derivation (`id`: Argon2id, else Argon2d); answers the key bytes.
    func importKdbxArgon2(
        id: Bool, version: Int, key: [UInt8], salt: [UInt8], memoryKib: Int, iterations: Int,
        lanes: Int
    ) async throws -> [UInt8] {
        try await call(
            "import_kdbx_argon2",
            [
                "id": id, "version": version, "key": key, "salt": salt, "memoryKib": memoryKib,
                "iterations": iterations, "lanes": lanes,
            ])
    }

    /// KeePass's AES-KDF key derivation; answers the key bytes.
    func importKdbxAesKdf(key: [UInt8], seed: [UInt8], rounds: UInt64) async throws -> [UInt8] {
        try await call("import_kdbx_aes_kdf", ["key": key, "seed": seed, "rounds": rounds])
    }

    // MARK: Moving in

    /// Whether the account on screen can take a move.
    func moveTarget() async throws -> MoveTarget {
        try await call("move_target")
    }

    /// Logs in to the source account of a move.
    func moveLogin(server: ServerInput, email: String, password: String) async throws
        -> MoveStep
    {
        try await call("move_login", ["server": server, "email": email, "password": password])
    }

    /// Answers the source's two-step login.
    func moveLoginTwoFactor(provider: Int, code: String) async throws -> MoveStep {
        try await call("move_login_two_factor", ["provider": provider, "code": code])
    }

    /// Answers the source's new-device check.
    func moveLoginNewDevice(code: String) async throws -> MoveStep {
        try await call("move_login_new_device", ["code": code])
    }

    /// Mails the source's new-device code again.
    func moveLoginSendEmail() async throws {
        try await run("move_login_send_email")
    }

    /// Starts the move; progress comes as events.
    func moveStart() async throws {
        try await run("move_start")
    }

    /// Cancels a running move.
    func moveCancel() async throws {
        try await run("move_cancel")
    }

    /// Closes the move dialog and forgets the source login.
    func moveClose() async throws {
        try await run("move_close")
    }

    // MARK: Sends

    /// The account's Sends, newest change first.
    func sends() async throws -> [Send] {
        try await call("sends")
    }

    // The file of a new file Send goes through `stageSendFile(_:)` (command stage_send_file),
    // which Core itself has: it takes raw bytes, not JSON.

    /// Saves a Send (a new one without `id`); answers its id.
    func saveSend(id: String?, draft: SendDraft, domain: SendLinkDomain = .unchanged)
        async throws -> String
    {
        let chosen: String?
        let choose: Bool
        switch domain {
        case .unchanged: (chosen, choose) = (nil, false)
        case .server: (chosen, choose) = (nil, true)
        case .domain(let domainId): (chosen, choose) = (domainId, true)
        }
        return try await call(
            "save_send",
            ["id": id, "draft": draft, "domain": chosen, "chooseDomain": choose])
    }

    /// Lets anybody with the link open the Send again: no password, no addresses.
    func removeSendAuth(id: String) async throws {
        try await run("remove_send_auth", ["id": id])
    }

    /// Deletes a Send.
    func deleteSend(id: String) async throws {
        try await run("delete_send", ["id": id])
    }

    // MARK: SSH and Remote Desktop (suite)

    /// The records of a suite space.
    func suiteView(space: SuiteSpace) async throws -> SuiteSpaceView {
        try await call("suite_view", ["space": space])
    }

    /// Makes a suite space that no app has made yet.
    func suiteCreate(space: SuiteSpace) async throws -> SuiteSpaceView {
        try await call("suite_create", ["space": space])
    }

    /// Applies edits to a suite space.
    func suiteSave(space: SuiteSpace, ops: [SuiteOp]) async throws -> SuiteSaved {
        try await call("suite_save", ["space": space, "ops": ops])
    }

    /// A secret of a suite record.
    func suiteReveal(space: SuiteSpace, id: String) async throws -> String {
        try await call("suite_reveal", ["space": space, "id": id])
    }

    /// Copies a secret of a suite record to the clipboard, which clears itself later.
    func suiteCopy(space: SuiteSpace, id: String) async throws {
        try await run("suite_copy", ["space": space, "id": id])
    }

    /// Generates an SSH key pair.
    func suiteGenerateKey(
        space: SuiteSpace, label: String, comment: String, passphrase: String?
    ) async throws -> SuiteSaved {
        try await call(
            "suite_generate_key",
            ["space": space, "label": label, "comment": comment, "passphrase": passphrase])
    }

    /// Imports an SSH private key.
    func suiteImportKey(
        space: SuiteSpace, label: String, privateKey: String, passphrase: String?
    ) async throws -> SuiteSaved {
        try await call(
            "suite_import_key",
            [
                "space": space, "label": label, "privateKey": privateKey,
                "passphrase": passphrase,
            ])
    }

    /// Saves a key's public line or private half as a file; answers where.
    func suiteSaveKey(space: SuiteSpace, id: String, half: SuiteKeyHalf) async throws -> String {
        try await call("suite_save_key", ["space": space, "id": id, "half": half])
    }

    /// Saves a `.rdp` file of a UwURDP host, without password and drives; answers where.
    func suiteSaveRdp(id: String) async throws -> String {
        try await call("suite_save_rdp", ["id": id])
    }

    /// Opens a host in the UwUSSH / UwURDP app (computers only).
    func suiteOpenInApp(space: SuiteSpace, id: String) async throws {
        try await run("suite_open_in_app", ["space": space, "id": id])
    }

    // MARK: Wi-Fi

    /// Android: hands a Wi-Fi item to the system.
    func wifiConnect(id: String) async throws -> WifiJoined {
        try await call("wifi_connect", ["id": id])
    }

    /// Android: opens the system's Wi-Fi settings; false where there are none.
    func wifiSettings() async throws -> Bool {
        try await call("wifi_settings")
    }

    // MARK: AutoFill

    /// Whether UwULock is the system's AutoFill provider.
    func autofillProviderStatus() async throws -> ProviderView {
        try await call("autofill_provider_status")
    }

    /// Asks the system to make UwULock the AutoFill provider (or opens its settings).
    func autofillProviderRequest(target: AutofillTarget) async throws -> ProviderView {
        try await call("autofill_provider_request", ["target": target])
    }

    /// Whether Apple Passwords handed over an import (Credential Exchange) that waits.
    func credentialExchangePending() async throws -> Bool {
        try await call("credential_exchange_pending")
    }

    /// Takes (or with `discard` drops) the handed-over import; answers its JSON.
    func credentialExchangeImport(discard: Bool) async throws -> String {
        try await call("credential_exchange_import", ["discard": discard])
    }

    /// What the AutoFill extension logged on this device.
    func autofillLog() async throws -> AutofillLog {
        try await call("autofill_log")
    }

    /// Clears the AutoFill extension's log.
    func autofillLogClear() async throws {
        try await run("autofill_log_clear")
    }

    // MARK: Passkeys

    /// The passkey request that waits for an answer, if any.
    func passkeyRequest() async throws -> PasskeyRequest? {
        try await call("passkey_request")
    }

    /// Answers a passkey request: allow it with the chosen item or passkey, or refuse.
    func passkeyAnswer(
        id: UInt64, allow: Bool, itemId: String? = nil, credentialId: String? = nil,
        password: String? = nil
    ) async throws {
        try await run(
            "passkey_answer",
            [
                "id": id, "allow": allow, "itemId": itemId, "credentialId": credentialId,
                "password": password,
            ])
    }

    /// The passkey provider's settings and state.
    func passkeyProviderStatus() async throws -> PasskeyProviderStatus {
        try await call("passkey_provider_status")
    }

    /// Changes the passkey provider's settings.
    func setPasskeyProvider(settings: PasskeyProviderSettings) async throws
        -> PasskeyProviderStatus
    {
        try await call("set_passkey_provider", ["settings": settings])
    }

    // MARK: Safari

    /// Whether the Safari extension is switched on.
    func safariExtensionStatus() async throws -> SafariView {
        try await call("safari_extension_status")
    }

    /// Opens Safari's settings on the extension (macOS).
    func safariExtensionOpen() async throws {
        try await run("safari_extension_open")
    }

    // MARK: System

    /// Opens a project page in the browser.
    func openProjectPage(page: ProjectPage) async throws {
        try await run("open_project_page", ["page": page])
    }

    /// Opens the address at `index` of a login in the browser.
    func openItemUri(id: String, index: Int) async throws {
        try await run("open_item_uri", ["id": id, "index": index])
    }

    /// Opens the web vault in the browser.
    func openWebVault() async throws {
        try await run("open_web_vault")
    }

    // MARK: Helpers

    /// Keeps the status of a finished login step.
    private func remembered(_ step: LoginStep) -> LoginStep {
        if case .done(let status) = step {
            remember(status)
        }
        return step
    }
}
