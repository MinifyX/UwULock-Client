// The system around the core: what crates/uwulock-ffi's `HostDelegate` asks of iOS and macOS.
// It does what the Tauri app's host did (apps/desktop/src-tauri/src/host.rs, hello.rs, apple.rs,
// safari.rs and the phone plugin's UwuLockMobilePlugin.swift, Passkeys.swift and
// CredentialExchange.swift), with the same Keychain items, groups and folders, so a vault set up
// by the Tauri app opens here and its Face ID / Touch ID unlock keeps working.
//
// The core calls in on its own threads and waits for the answer, never on the main thread: a
// method may block for a Face ID sheet or a system answer. What has to happen on the main thread
// hops there.
#if RUST_CORE && canImport(UwuLockFFI)
import AuthenticationServices
import CryptoKit
import Foundation
import LocalAuthentication
import Security
#if os(iOS)
import NetworkExtension
import UIKit
import UniformTypeIdentifiers
#else
import AppKit
import CoreGraphics
import SafariServices
#endif

final class AppleHost: HostDelegate, @unchecked Sendable {
    /// Where the events go.
    weak var core: Core?

    private let lock = NSLock()
    /// iOS: the pasteboard's change count right after UwULock's last copy of a secret.
    private var lastCopy: Int?
    /// The last line noted with `once` in the AutoFill log.
    private var lastNote = ""

    // MARK: Events and the window

    func emit(event: String, payloadJson: String) {
        let event = CoreEvent(name: event, json: payloadJson)
        // In order, after whatever the main thread does now (`start()` may emit from it).
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated { self?.core?.receive(event) }
        }
    }

    func showWindow() {
        #if os(macOS)
        DispatchQueue.main.async {
            NSApplication.shared.activate()
            NSApplication.shared.windows.first { $0.canBecomeMain }?.makeKeyAndOrderFront(nil)
        }
        #endif
    }

    func openUrl(url: String) throws {
        guard let target = URL(string: url), target.scheme != nil else {
            throw HostError.Failed(kind: "failed", message: "Not an address: \(url)")
        }
        let opened = onMain { () -> Bool in
            #if os(iOS)
            // An SSO login: the provider's page in the app (SsoBrowser), not in Safari.
            if SsoBrowser.shared.open(target) { return true }
            UIApplication.shared.open(target)
            return true
            #else
            return NSWorkspace.shared.open(target)
            #endif
        }
        if !opened {
            throw HostError.Failed(kind: "failed", message: "Nothing opens \(url).")
        }
    }

    // MARK: The clipboard

    /// A secret stays on this device (no Universal Clipboard), is hidden from clipboard
    /// managers (macOS: `org.nspasteboard.ConcealedType`) and, on iOS, expires by itself.
    func clipboardCopy(text: String, sensitive: Bool, expiresSeconds: UInt64?) throws {
        onMain {
            #if os(iOS)
            guard sensitive else {
                UIPasteboard.general.string = text
                return
            }
            var options: [UIPasteboard.OptionsKey: Any] = [.localOnly: true]
            if let expiresSeconds, expiresSeconds > 0 {
                options[.expirationDate] = Date().addingTimeInterval(Double(expiresSeconds))
            }
            UIPasteboard.general.setItems(
                [[UTType.utf8PlainText.identifier: text]], options: options)
            let count = UIPasteboard.general.changeCount
            self.lock.withLock { self.lastCopy = count }
            #else
            let board = NSPasteboard.general
            board.clearContents()
            board.setString(text, forType: .string)
            if sensitive {
                board.setString("", forType: NSPasteboard.PasteboardType("org.nspasteboard.ConcealedType"))
            }
            #endif
        }
    }

    /// iOS: only when nothing was copied since (the change count says so without reading the
    /// pasteboard, which would show the paste notice). macOS: only when it still holds the
    /// copied text.
    func clipboardClear(sha256: Data) {
        onMain {
            #if os(iOS)
            let ours = self.lock.withLock { () -> Int? in
                defer { self.lastCopy = nil }
                return self.lastCopy
            }
            if let ours, UIPasteboard.general.changeCount == ours {
                UIPasteboard.general.items = []
            }
            #else
            let board = NSPasteboard.general
            if let current = board.string(forType: .string),
                Data(SHA256.hash(data: Data(current.utf8))) == sha256
            {
                board.clearContents()
            }
            #endif
        }
    }

    // MARK: Face ID and Touch ID

    /// The unlock secrets: Keychain service `app.uwulock.unlock`, account = the account's name.
    /// iOS: the app's default Keychain group (the first in UwULock-iOS.entitlements, as the
    /// Tauri app's); macOS: `<team>.app.uwulock.unlock` in the data protection Keychain.
    private static let unlockService = "app.uwulock.unlock"

    func biometricKind() -> String? {
        #if os(macOS)
        guard Self.touchGroup() != nil else { return nil }
        #endif
        let context = LAContext()
        var error: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error)
        else { return nil }
        switch context.biometryType {
        case .faceID: return "faceId"
        case .touchID: return "touchId"
        case .opticID: return "opticId"
        default: return "biometric"
        }
    }

    func biometricSecret(
        name: String, create: Bool, title: String, subtitle: String, cancel: String,
        reason: String
    ) throws -> Data {
        #if os(iOS)
        let prompt = subtitle.isEmpty ? title : subtitle
        if create {
            return try createSecret(name: name, group: nil)
        }
        return try readSecret(name: name, group: nil, reason: prompt, cancel: cancel)
        #else
        guard let group = Self.touchGroup() else {
            throw HostError.Failed(
                kind: "unsupported", message: "Touch ID needs the Mac App Store build of UwULock.")
        }
        guard create else {
            return try readSecret(name: name, group: group, reason: reason, cancel: cancel)
        }
        // A new secret, and Touch ID once to confirm it opens.
        let secret = try createSecret(name: name, group: group)
        do {
            let back = try readSecret(name: name, group: group, reason: reason, cancel: cancel)
            guard back == secret else {
                throw HostError.Failed(
                    kind: "hello", message: "The Keychain gave back something else.")
            }
            return secret
        } catch {
            SecItemDelete(unlockQuery(name: name, group: group, quiet: true) as CFDictionary)
            throw error
        }
        #endif
    }

    func biometricForget(name: String) {
        #if os(iOS)
        SecItemDelete(unlockQuery(name: name, group: nil, quiet: true) as CFDictionary)
        #else
        guard let group = Self.touchGroup() else { return }
        SecItemDelete(unlockQuery(name: name, group: group, quiet: true) as CFDictionary)
        #endif
    }

    /// macOS: the Touch ID group, from the team in the Info.plist's Keychain group; `nil` in a
    /// build without a developer team.
    private static func touchGroup() -> String? {
        guard let team = teamPrefix() else { return nil }
        return team + "app.uwulock.unlock"
    }

    private func unlockQuery(name: String, group: String?, quiet: Bool) -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.unlockService,
            kSecAttrAccount as String: name,
            kSecAttrSynchronizable as String: false,
            kSecUseDataProtectionKeychain as String: true,
        ]
        if let group { query[kSecAttrAccessGroup as String] = group }
        if quiet {
            let context = LAContext()
            context.interactionNotAllowed = true
            query[kSecUseAuthenticationContext as String] = context
        }
        return query
    }

    /// 32 random bytes that only open after Face ID or Touch ID with the faces or fingers
    /// enrolled right now, on this device only, and only while it has a passcode.
    private func createSecret(name: String, group: String?) throws -> Data {
        var secret = Data(count: 32)
        let random = secret.withUnsafeMutableBytes {
            SecRandomCopyBytes(kSecRandomDefault, 32, $0.baseAddress!)
        }
        guard random == errSecSuccess else {
            throw HostError.Failed(kind: "hello", message: "No random bytes.")
        }
        var accessError: Unmanaged<CFError>?
        guard
            let access = SecAccessControlCreateWithFlags(
                nil, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, .biometryCurrentSet,
                &accessError)
        else {
            throw HostError.Failed(kind: "hello", message: "The Keychain couldn't protect the key.")
        }
        SecItemDelete(unlockQuery(name: name, group: group, quiet: true) as CFDictionary)
        var item = unlockQuery(name: name, group: group, quiet: false)
        item[kSecAttrAccessControl as String] = access
        item[kSecValueData as String] = secret
        #if os(macOS)
        item[kSecAttrLabel as String] = "UwULock unlock"
        #endif
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw HostError.Failed(kind: "hello", message: "The Keychain said no (\(status)).")
        }
        return secret
    }

    private func readSecret(name: String, group: String?, reason: String, cancel: String) throws
        -> Data
    {
        let context = LAContext()
        context.localizedCancelTitle = cancel
        context.localizedReason = reason
        // No "Enter password" after a face or finger that didn't fit: the master password is
        // the way back, behind the cancel button.
        context.localizedFallbackTitle = ""
        var query = unlockQuery(name: name, group: group, quiet: false)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        query[kSecUseAuthenticationContext as String] = context
        var found: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &found)
        switch status {
        case errSecSuccess:
            guard let data = found as? Data, data.count == 32 else {
                throw HostError.Failed(kind: "hello", message: "The Keychain gave back something else.")
            }
            return data
        case errSecUserCanceled:
            throw HostError.Failed(kind: "biometric-cancelled", message: "Cancelled.")
        case errSecItemNotFound:
            // With `biometryCurrentSet` the item goes once a face or finger is enrolled or removed.
            throw HostError.Failed(
                kind: "biometric-changed",
                message:
                    "Nothing is kept for unlocking on this device. Unlock with the master password and switch it on again."
            )
        case errSecAuthFailed:
            throw HostError.Failed(kind: "hello", message: "Face ID or Touch ID didn't recognise you.")
        default:
            throw HostError.Failed(kind: "hello", message: "The Keychain said no (\(status)).")
        }
    }

    // MARK: The system

    func screenLocked() -> Bool {
        #if os(macOS)
        guard let session = CGSessionCopyCurrentDictionary() as? [String: Any] else { return false }
        return (session["CGSSessionScreenIsLocked"] as? Bool) ?? false
        #else
        return false
        #endif
    }

    /// iOS: Documents (the Files app shows it: UIFileSharingEnabled); macOS: Downloads.
    func downloadDir() throws -> String {
        #if os(iOS)
        let place = FileManager.SearchPathDirectory.documentDirectory
        #else
        let place = FileManager.SearchPathDirectory.downloadsDirectory
        #endif
        guard let dir = FileManager.default.urls(for: place, in: .userDomainMask).first else {
            throw HostError.Failed(kind: "failed", message: "No folder for downloads.")
        }
        return dir.path
    }

    // MARK: Wi-Fi

    /// iOS: the network goes to the system (NEHotspotConfiguration), which asks the person.
    /// `nil` (no Connect button) on the Mac, as in the Tauri app, and for EAP-pwd, which iOS
    /// can't take from an app.
    func wifiJoin(networkJson: String) throws -> String? {
        #if os(iOS)
        // uwulock-app's wifi::Network.
        struct Network: Decodable {
            var ssid: String
            var security: String
            var password: String?
            var hidden: Bool?
            var eap: String?
            var phase2: String?
            var identity: String?
            var anonymousIdentity: String?
            var domain: String?
        }
        let network: Network
        do {
            network = try JSONDecoder().decode(Network.self, from: Data(networkJson.utf8))
        } catch {
            throw HostError.Failed(kind: "wifi", message: "The network didn't read: \(error)")
        }
        let configuration: NEHotspotConfiguration
        switch network.security {
        case "open":
            configuration = NEHotspotConfiguration(ssid: network.ssid)
        case "wpa2", "wpa3":
            configuration = NEHotspotConfiguration(
                ssid: network.ssid, passphrase: network.password ?? "", isWEP: false)
        case "wpa2-enterprise", "wpa3-enterprise":
            let eap = NEHotspotEAPSettings()
            switch network.eap {
            case "PEAP": eap.supportedEAPTypes = [NSNumber(value: NEHotspotEAPSettings.EAPType.EAPPEAP.rawValue)]
            case "TTLS": eap.supportedEAPTypes = [NSNumber(value: NEHotspotEAPSettings.EAPType.EAPTTLS.rawValue)]
            default: return nil
            }
            eap.username = network.identity ?? ""
            eap.password = network.password ?? ""
            if let outer = network.anonymousIdentity, !outer.isEmpty { eap.outerIdentity = outer }
            if network.eap == "TTLS" {
                switch network.phase2 {
                case "PAP": eap.ttlsInnerAuthenticationType = .eapttlsInnerAuthenticationPAP
                case "GTC", "NONE": eap.ttlsInnerAuthenticationType = .eapttlsInnerAuthenticationEAP
                default: eap.ttlsInnerAuthenticationType = .eapttlsInnerAuthenticationMSCHAPv2
                }
            }
            if let domain = network.domain, !domain.isEmpty { eap.trustedServerNames = [domain] }
            configuration = NEHotspotConfiguration(ssid: network.ssid, eapSettings: eap)
        default:
            return nil
        }
        configuration.hidden = network.hidden ?? false
        configuration.joinOnce = false
        let answer = wait(seconds: 120) { (done: @escaping @Sendable (NSError?) -> Void) in
            NEHotspotConfigurationManager.shared.apply(configuration) { error in
                done(error as NSError?)
            }
        }
        guard let answer else {
            throw HostError.Failed(kind: "wifi", message: "iOS didn't answer.")
        }
        guard let error = answer else { return "saved" }
        guard error.domain == NEHotspotConfigurationErrorDomain else {
            throw HostError.Failed(kind: "wifi", message: error.localizedDescription)
        }
        switch NEHotspotConfigurationError(rawValue: error.code) {
        case .alreadyAssociated: return "already-saved"
        case .userDenied: return "declined"
        default:
            throw HostError.Failed(kind: "wifi", message: "iOS didn't take the network (\(error.code)).")
        }
        #else
        return nil
        #endif
    }

    /// Neither has Wi-Fi settings an app may open (the Tauri app's answer on both).
    func wifiSettings() throws -> Bool {
        false
    }

    // MARK: UwULock as the AutoFill provider

    func autofillProviderStatus() throws -> ProviderState {
        providerState(enabled: Self.passkeyGroups() == nil ? nil : identityStoreEnabled())
    }

    /// The system asks the person in a sheet of its own; the state afterwards.
    func autofillProviderRequest(target: String) throws -> ProviderState {
        guard Self.passkeyGroups() != nil else { return providerState(enabled: false) }
        _ = wait(seconds: 300) { (done: @escaping @Sendable (Bool) -> Void) in
            DispatchQueue.main.async {
                ASSettingsHelper.requestToTurnOnCredentialProviderExtension { done($0) }
            }
        }
        return providerState(enabled: identityStoreEnabled())
    }

    private func providerState(enabled: Bool?) -> ProviderState {
        let supported = Self.passkeyGroups() != nil
        return ProviderState(supported: supported, enabled: supported ? enabled : nil, direct: supported)
    }

    private func identityStoreEnabled() -> Bool? {
        wait(seconds: 5) { (done: @escaping @Sendable (Bool) -> Void) in
            ASCredentialIdentityStore.shared.getState { done($0.isEnabled) }
        }
    }

    // MARK: Credentials another app hands over (iOS 26)

    func credentialExchangePending() -> Bool {
        #if os(iOS)
        CredentialExchange.pending
        #else
        false
        #endif
    }

    func credentialExchangeImport(discard: Bool) throws -> String {
        #if os(iOS)
        guard let token = CredentialExchange.take() else {
            throw HostError.Failed(kind: "missing", message: "Nothing was handed over.")
        }
        if discard { return "" }
        let answer = wait(seconds: 300) { (done: @escaping @Sendable (Result<String, any Error>) -> Void) in
            Task {
                do {
                    done(.success(try await CredentialExchange.load(token)))
                } catch {
                    done(.failure(error))
                }
            }
        }
        switch answer {
        case .success(let json)?: return json
        case .failure(let error)?: throw HostError.Failed(kind: "failed", message: "\(error)")
        case nil: throw HostError.Failed(kind: "failed", message: "The system didn't answer.")
        }
        #else
        throw HostError.Failed(kind: "unsupported", message: "Needs iOS 26.")
        #endif
    }

    // MARK: The AutoFill extension's protocol

    /// Passkeys/autofill.log in the App Group folder (AutoFillLog.swift): step names, hosts,
    /// counts and error codes, nothing secret. `nil` without an App Group (unsigned build).
    func autofillLog() throws -> String? {
        guard let folder = Self.passkeyGroups()?.folder else { return nil }
        let data: Data
        do {
            data = try Data(contentsOf: folder.appendingPathComponent("autofill.log"))
        } catch CocoaError.fileReadNoSuchFile {
            data = Data()
        } catch {
            let ns = error as NSError
            throw HostError.Failed(kind: "io", message: "the file didn't read (\(ns.domain) \(ns.code))")
        }
        // The extension keeps it under 64 KB; a cut-off first line is Rust's to drop.
        return String(decoding: data.suffix(64 * 1024), as: UTF8.self)
    }

    func autofillLogClear() throws {
        if let folder = Self.passkeyGroups()?.folder {
            try? FileManager.default.removeItem(at: folder.appendingPathComponent("autofill.log"))
        }
    }

    // MARK: Safari

    func safariExtensionStatus() -> SafariState {
        let bundled =
            Bundle.main.builtInPlugInsURL.map {
                FileManager.default.fileExists(
                    atPath: $0.appendingPathComponent("UwULockSafari.appex").path)
            } ?? false
        #if os(macOS)
        let enabled: Bool? =
            bundled
            ? wait(seconds: 5) { (done: @escaping @Sendable (Bool?) -> Void) in
                SFSafariExtensionManager.getStateOfSafariExtension(
                    withIdentifier: Self.safariIdentifier
                ) { state, _ in done(state?.isEnabled) }
            } ?? nil
            : nil
        return SafariState(platform: "macos", available: bundled, enabled: enabled)
        #else
        // iOS doesn't tell an app whether its Safari extension is on.
        return SafariState(platform: "ios", available: bundled, enabled: nil)
        #endif
    }

    func safariExtensionOpen() throws {
        #if os(macOS)
        let opened = wait(seconds: 20) { (done: @escaping @Sendable (Bool) -> Void) in
            SFSafariApplication.showPreferencesForExtension(withIdentifier: Self.safariIdentifier) {
                error in done(error == nil)
            }
        }
        if opened == false {
            throw HostError.Failed(kind: "failed", message: "Safari didn't open its settings for UwULock")
        }
        #else
        throw HostError.Failed(kind: "unsupported", message: "Safari can't be asked on iOS")
        #endif
    }

    private static var safariIdentifier: String {
        (Bundle.main.bundleIdentifier ?? "app.uwulock") + ".safari"
    }

    // MARK: The AutoFill extension's list and outbox

    /// The provider key: Keychain service `app.uwulock.passkeys`, account `provider-key`, in
    /// the Info.plist's UwULockKeychainGroup (`<team>.app.uwulock.passkeys`).
    private static let passkeyService = "app.uwulock.passkeys"
    private static let passkeyAccount = "provider-key"

    /// The App Group folder (Info.plist UwULockAppGroup: `group.app.uwulock` on iOS,
    /// `<team>.app.uwulock` on macOS) + `Passkeys/`, and the Keychain group. `nil` in a build
    /// without a developer team.
    private static func passkeyGroups() -> (folder: URL, keychain: String)? {
        guard
            let group = Bundle.main.object(forInfoDictionaryKey: "UwULockAppGroup") as? String,
            let keychain = Bundle.main.object(forInfoDictionaryKey: "UwULockKeychainGroup") as? String,
            !group.isEmpty, teamPrefix() != nil,
            let container = FileManager.default.containerURL(
                forSecurityApplicationGroupIdentifier: group)
        else { return nil }
        return (container.appendingPathComponent("Passkeys", isDirectory: true), keychain)
    }

    /// `<team>.` from the Info.plist's Keychain group; `nil` without a team.
    private static func teamPrefix() -> String? {
        guard let keychain = Bundle.main.object(forInfoDictionaryKey: "UwULockKeychainGroup") as? String,
            keychain.hasSuffix(".app.uwulock.passkeys")
        else { return nil }
        let team = String(keychain.dropLast("app.uwulock.passkeys".count))
        return team.count > 1 ? team : nil
    }

    func extensionProblem() -> String? {
        Self.passkeyGroups() == nil
            ? "This build isn't signed with an Apple developer team: the AutoFill extension can't reach the vault (docs/passkeys.md)."
            : nil
    }

    func extensionNote(text: String) {
        appendAutofillLog(text, once: false)
    }

    /// The sealed list for the extension and its key. The Keychain item is (re)written only
    /// when the one there isn't this key (told by its label, which takes no Face ID to read)
    /// or is gone. iOS also hands the identities to the system's list (QuickType bar).
    func extensionStore(list: Data, key: Data, identitiesJson: String) throws {
        guard let groups = Self.passkeyGroups() else {
            throw HostError.Failed(kind: "unsupported", message: "No App Group in this build.")
        }
        guard key.count == 32 else {
            throw HostError.Failed(kind: "failed", message: "The key isn't 32 bytes.")
        }
        do {
            try FileManager.default.createDirectory(
                at: groups.folder.appendingPathComponent("outbox/unreadable", isDirectory: true),
                withIntermediateDirectories: true)
        } catch {
            throw HostError.Failed(kind: "failed", message: "\(error)")
        }
        let label = Self.keyLabel(key)
        if !keyThere(label: label, keychain: groups.keychain) {
            var accessError: Unmanaged<CFError>?
            guard
                let access = SecAccessControlCreateWithFlags(
                    nil, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, .userPresence, &accessError)
            else {
                throw HostError.Failed(kind: "unavailable", message: "The Keychain couldn't protect the key.")
            }
            let query = passkeyQuery(keychain: groups.keychain)
            SecItemDelete(query as CFDictionary)
            var item = query
            item[kSecAttrAccessControl as String] = access
            item[kSecAttrLabel as String] = label
            item[kSecValueData as String] = key
            let status = SecItemAdd(item as CFDictionary, nil)
            guard status == errSecSuccess else {
                throw HostError.Failed(kind: "failed", message: "The Keychain said no (\(status)).")
            }
        }
        do {
            #if os(iOS)
            let options: Data.WritingOptions = [.atomic, .completeFileProtection]
            #else
            let options: Data.WritingOptions = [.atomic]
            #endif
            try list.write(to: groups.folder.appendingPathComponent("passkeys.sealed"), options: options)
        } catch {
            throw HostError.Failed(kind: "failed", message: "\(error)")
        }
        #if os(iOS)
        replaceIdentities(identitiesJson)
        #endif
    }

    func extensionOutbox() throws -> [OutboxFile] {
        guard let groups = Self.passkeyGroups() else { return [] }
        let outbox = groups.folder.appendingPathComponent("outbox", isDirectory: true)
        var files: [OutboxFile] = []
        for (folder, prefix) in [(outbox, ""), (outbox.appendingPathComponent("unreadable"), "unreadable/")] {
            let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
            for name in names.sorted() where Self.outboxFile(name) {
                if let data = try? Data(contentsOf: folder.appendingPathComponent(name)) {
                    files.append(OutboxFile(name: prefix + name, bytes: data))
                }
            }
        }
        return files
    }

    /// "<name>" from the outbox, "unreadable/<name>" from the set-aside ones.
    func extensionClearOutbox(names: [String]) throws {
        guard let groups = Self.passkeyGroups() else { return }
        let outbox = groups.folder.appendingPathComponent("outbox", isDirectory: true)
        for name in names {
            if name.hasPrefix("unreadable/") {
                let file = String(name.dropFirst("unreadable/".count))
                guard Self.outboxFile(file) else { continue }
                try? FileManager.default.removeItem(
                    at: outbox.appendingPathComponent("unreadable").appendingPathComponent(file))
            } else if Self.outboxFile(name) {
                try? FileManager.default.removeItem(at: outbox.appendingPathComponent(name))
            }
        }
    }

    /// Moves outbox files that didn't open into `outbox/unreadable/`: never deleted for that.
    func extensionSetAside(names: [String]) throws {
        guard let groups = Self.passkeyGroups() else { return }
        let outbox = groups.folder.appendingPathComponent("outbox", isDirectory: true)
        let unreadable = outbox.appendingPathComponent("unreadable", isDirectory: true)
        try? FileManager.default.createDirectory(at: unreadable, withIntermediateDirectories: true)
        for name in names where Self.outboxFile(name) {
            try? FileManager.default.moveItem(
                at: outbox.appendingPathComponent(name), to: unreadable.appendingPathComponent(name))
        }
    }

    func extensionClear() throws {
        if let groups = Self.passkeyGroups() {
            try? FileManager.default.removeItem(at: groups.folder.appendingPathComponent("passkeys.sealed"))
            SecItemDelete(passkeyQuery(keychain: groups.keychain) as CFDictionary)
        }
        _ = wait(seconds: 5) { (done: @escaping @Sendable (Bool) -> Void) in
            ASCredentialIdentityStore.shared.removeAllCredentialIdentities { ok, _ in done(ok) }
        }
    }

    private func passkeyQuery(keychain: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.passkeyService,
            kSecAttrAccount as String: Self.passkeyAccount,
            kSecAttrAccessGroup as String: keychain,
            kSecAttrSynchronizable as String: false,
            kSecUseDataProtectionKeychain as String: true,
        ]
    }

    /// Whether the Keychain has the provider key with this label: attributes only, never any UI.
    private func keyThere(label: String, keychain: String) -> Bool {
        let context = LAContext()
        context.interactionNotAllowed = true
        var query = passkeyQuery(keychain: keychain)
        query[kSecAttrLabel as String] = label
        query[kSecReturnAttributes as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        query[kSecUseAuthenticationContext as String] = context
        var found: AnyObject?
        return SecItemCopyMatching(query as CFDictionary, &found) == errSecSuccess
    }

    /// "UwULock passkeys " + hex of the first 8 bytes of SHA-256("uwulock-provider-key-id-v1" ‖
    /// key): `key_id` in crates/uwulock-authenticator/src/apple.rs.
    private static func keyLabel(_ key: Data) -> String {
        var input = Data("uwulock-provider-key-id-v1".utf8)
        input.append(key)
        defer { input.resetBytes(in: 0..<input.count) }
        let hex = SHA256.hash(data: input).prefix(8).map { String(format: "%02x", $0) }
        return "UwULock passkeys " + hex.joined()
    }

    /// A file name in the outbox: nothing that leaves it.
    private static func outboxFile(_ name: String) -> Bool {
        name.hasSuffix(".sealed") && name != ".sealed" && !name.hasPrefix(".")
            && !name.contains("/") && !name.contains("\\")
    }

    #if os(iOS)
    /// The system's list of passkeys and logins (QuickType bar, the sheet), from passkeys/apple.rs
    /// `Identity` as JSON.
    private func replaceIdentities(_ json: String) {
        struct Identity: Decodable {
            var kind: String?
            var rpId: String
            var userName: String
            var credentialId: String?
            var userHandle: String?
            var recordIdentifier: String?
            var serviceType: String?
        }
        let entries = (try? JSONDecoder().decode([Identity].self, from: Data(json.utf8))) ?? []
        let identities: [any ASCredentialIdentity] = entries.compactMap { identity in
            if identity.kind == "password" {
                return ASPasswordCredentialIdentity(
                    serviceIdentifier: ASCredentialServiceIdentifier(
                        identifier: identity.rpId, type: identity.serviceType == "url" ? .URL : .domain),
                    user: identity.userName, recordIdentifier: identity.recordIdentifier)
            }
            guard let id = identity.credentialId.flatMap(Self.fromBase64URL), !id.isEmpty else {
                return nil
            }
            return ASPasskeyCredentialIdentity(
                relyingPartyIdentifier: identity.rpId, userName: identity.userName, credentialID: id,
                userHandle: identity.userHandle.flatMap(Self.fromBase64URL) ?? Data(),
                recordIdentifier: identity.recordIdentifier)
        }
        let enabled = identityStoreEnabled() ?? false
        guard enabled else {
            appendAutofillLog("system list skipped: UwULock is not on in AutoFill & Passwords", once: true)
            return
        }
        let count = identities.count
        let line = wait(seconds: 10) { (done: @escaping @Sendable (String) -> Void) in
            ASCredentialIdentityStore.shared.replaceCredentialIdentities(identities) { ok, error in
                let ns = error.map { $0 as NSError }
                done(
                    "system list \(ok ? "replaced" : "not replaced"), \(count) entries"
                        + (ns.map { " (\($0.domain) \($0.code))" } ?? ""))
            }
        }
        if let line { appendAutofillLog(line, once: true) }
    }

    private static func fromBase64URL(_ text: String) -> Data? {
        var base = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while base.count % 4 != 0 { base += "=" }
        return Data(base64Encoded: base)
    }
    #endif

    /// One line into the protocol, as AutoFillLog.swift writes them (timestamp, a run tag, the
    /// text), cut back to its last 48 KB past 64 KB. `once`: not again while it is the same as
    /// the last such line.
    private func appendAutofillLog(_ text: String, once: Bool) {
        guard let folder = Self.passkeyGroups()?.folder else { return }
        if once {
            let fresh = lock.withLock { () -> Bool in
                defer { lastNote = text }
                return lastNote != text
            }
            guard fresh else { return }
        }
        let file = folder.appendingPathComponent("autofill.log")
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let line =
            formatter.string(from: Date()) + " [app] " + text.replacingOccurrences(of: "\n", with: " ") + "\n"
        Self.logQueue.async {
            // Only a missing file starts a new one: a file that didn't read now isn't replaced.
            var data: Data
            do {
                data = try Data(contentsOf: file)
            } catch CocoaError.fileReadNoSuchFile {
                data = Data()
            } catch {
                return
            }
            data.append(Data(line.utf8))
            if data.count > 64 * 1024 {
                let tail = data.suffix(48 * 1024)
                data = Data(tail.drop(while: { $0 != 0x0a }).dropFirst())
            }
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            #if os(iOS)
            try? data.write(to: file, options: [.atomic, .completeFileProtection])
            #else
            try? data.write(to: file, options: [.atomic])
            #endif
        }
    }

    private static let logQueue = DispatchQueue(label: "app.uwulock.autofill-log", qos: .utility)

    // MARK: The data folder

    /// iOS: the data folder stays out of iCloud and computer backups, as the Tauri app's: the
    /// vault comes back from the server, and the Face ID copy of the key only opens with a
    /// Keychain item that never leaves this iPhone anyway.
    static func keepOutOfBackups(_ folder: URL) {
        #if os(iOS)
        var folder = folder
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        do {
            try folder.setResourceValues(values)
        } catch {
            NSLog("UwULock: couldn't keep the data folder out of backups: \(error)")
        }
        #endif
    }

    // MARK: Waiting

    /// Runs `body` on the main thread and waits for it (at once when already there).
    private func onMain<T: Sendable>(_ body: @MainActor () -> T) -> T {
        if Thread.isMainThread {
            return MainActor.assumeIsolated(body)
        }
        return DispatchQueue.main.sync { MainActor.assumeIsolated(body) }
    }

    /// Starts something that answers through a completion handler and waits for the answer, at
    /// most `seconds`: `nil` when none came. Never on the main thread, where the answer may
    /// have to arrive.
    private func wait<T: Sendable>(
        seconds: Double, _ start: (@escaping @Sendable (T) -> Void) -> Void
    ) -> T? {
        let box = Answer<T>()
        start { box.set($0) }
        return box.get(within: seconds)
    }
}

/// One answer from another thread.
private final class Answer<T: Sendable>: @unchecked Sendable {
    private let done = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var value: T?

    func set(_ answer: T) {
        let first = lock.withLock { () -> Bool in
            guard value == nil else { return false }
            value = answer
            return true
        }
        if first { done.signal() }
    }

    func get(within seconds: Double) -> T? {
        guard done.wait(timeout: .now() + seconds) == .success else { return nil }
        return lock.withLock { value }
    }
}

#if os(iOS)
/// Credentials another app hands over (iOS 26): Apple Passwords' "Export data to another app" →
/// UwULock. The system opens UwULock with an `NSUserActivity` of type
/// `ASCredentialExchangeActivity` (UwULockApp's `onContinueUserActivity`); its token is kept
/// here until the person takes the data in (`credential_exchange_import`). The data comes from
/// the system only then, and only once.
enum CredentialExchange {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var token: UUID?

    static var pending: Bool {
        lock.withLock { token != nil }
    }

    static func take() -> UUID? {
        lock.withLock {
            defer { token = nil }
            return token
        }
    }

    /// Keeps the token of an exchange activity.
    static func receive(_ activity: NSUserActivity) {
        guard activity.activityType == ASCredentialExchangeActivity else { return }
        guard let received = activity.userInfo?[ASCredentialImportToken] as? UUID else {
            NSLog("UwULock: a credential exchange without a token")
            return
        }
        lock.withLock { token = received }
        NSLog("UwULock: credentials handed over by another app wait to be taken in")
    }

    /// The handed-over data as CXF JSON (Apple's own Codable form of it, dates in seconds).
    static func load(_ token: UUID) async throws -> String {
        let data = try await ASCredentialImportManager().importCredentials(token: token)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .secondsSince1970
        return String(decoding: try encoder.encode(data), as: UTF8.self)
    }
}
#endif
#endif
