import Foundation

/// A passkey of a login, nothing secret. `readable: false`: it can only be deleted.
struct PasskeyInfo: Codable, Sendable, Equatable, Hashable, Identifiable {
    var index: Int
    var readable: Bool
    var credentialId: String
    /// Names this stored passkey for a delete, also an unreadable one.
    var fingerprint: String
    var rpId: String
    var rpName: String?
    var userName: String?
    var userDisplayName: String?
    var creationDate: String
    var discoverable: Bool

    var id: String { credentialId + "#\(index)" }
}

/// Which providers UwULock offers on this system.
struct PasskeyProviderSettings: Codable, Sendable, Equatable, Hashable {
    /// Linux: the virtual security key over /dev/uhid.
    var securityKey: Bool
    /// Windows 11: the plugin passkey manager.
    var windowsPlugin: Bool
    /// iOS and macOS: the AutoFill extension's sealed list.
    var appleExtension: Bool
}

/// How another program stands in for UwULock as the passkey provider.
enum PasskeyWarningKind: String, Codable, Sendable, CaseIterable {
    /// Linux: another program holds the one security key per user.
    case held
    /// Windows: the registry entry pointed elsewhere.
    case registry
}

/// Another program stands in for UwULock (`passkey-provider-warning` event, or in the status).
struct PasskeyProviderWarning: Codable, Sendable, Equatable, Hashable {
    var kind: PasskeyWarningKind
    /// The program, as far as UwULock could tell.
    var holder: String?
}

/// The platform a passkey provider runs on.
enum PasskeyPlatform: String, Codable, Sendable, CaseIterable {
    case linux
    case windows
    case android
    case apple
    case none
}

/// The passkey provider's settings and state.
struct PasskeyProviderStatus: Codable, Sendable, Equatable, Hashable {
    var settings: PasskeyProviderSettings
    var platform: PasskeyPlatform
    var active: Bool
    var problem: String?
    var warning: PasskeyProviderWarning?
}

/// What a browser or app asks of the passkey provider.
enum PasskeyRequestKind: String, Codable, Sendable, CaseIterable {
    case create
    case get
    case select
}

/// A login that could take a new passkey.
struct PasskeyLogin: Codable, Sendable, Equatable, Hashable, Identifiable {
    var itemId: String
    var name: String
    var userName: String?
    var hasPasskey: Bool

    var id: String { itemId }
}

/// A passkey that could answer a request.
struct PasskeyChoice: Codable, Sendable, Equatable, Hashable, Identifiable {
    var itemId: String
    var itemName: String
    var credentialId: String
    var userName: String?
    var userDisplayName: String?

    var id: String { credentialId }
}

/// A request from a browser or app for a passkey, as the dialog shows it.
struct PasskeyRequest: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: UInt64
    var kind: PasskeyRequestKind
    /// Who asks: "Firefox", "Windows", a program's name; empty when unknown.
    var client: String
    /// A browser UwULock knows, or a request Windows signed.
    var trusted: Bool
    var rpId: String?
    var rpName: String?
    var userName: String?
    var userDisplayName: String?
    var locked: Bool
    /// The site wants the master password typed again.
    var verify: Bool
    /// The vault already has a passkey the site knows.
    var excluded: Bool
    var logins: [PasskeyLogin]
    var passkeys: [PasskeyChoice]
}
