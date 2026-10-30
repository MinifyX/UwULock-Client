import Foundation

/// The system the AutoFill provider status is about.
enum ProviderPlatform: String, Codable, Sendable, CaseIterable {
    case ios
    case macos
    case android
    case none
}

/// Whether UwULock is the system's AutoFill provider for passwords and passkeys.
struct ProviderView: Codable, Sendable, Equatable, Hashable {
    var platform: ProviderPlatform
    /// This system can have UwULock as its provider.
    var supported: Bool
    /// Switched on in the system; nil when it does not say.
    var enabled: Bool?
    /// The system asks itself (iOS 18+, macOS 15+, Android); otherwise the settings open.
    var direct: Bool
    /// Android: the autofill service is UwULock's too.
    var autofill: Bool?
    /// iOS and macOS: UwULock's own setting that leaves the extension its list.
    var list: Bool
}

/// What `autofill_provider_request` asks the system for.
enum AutofillTarget: String, Codable, Sendable, CaseIterable {
    /// Passwords and passkeys.
    case credentials
    /// Android's autofill service.
    case autofill
}

/// What the AutoFill extension logged on this device: hosts, counts and short id prefixes,
/// never a password, a user name or a full address.
struct AutofillLog: Codable, Sendable, Equatable, Hashable {
    var supported: Bool
    var lines: [String]
    var problem: String?
}

/// The system the Safari extension is about.
enum SafariPlatform: String, Codable, Sendable, CaseIterable {
    case macos
    case ios
    case none
}

/// The Safari extension the Apple builds carry.
struct SafariView: Codable, Sendable, Equatable, Hashable {
    var platform: SafariPlatform
    /// This build carries the extension.
    var available: Bool
    /// Switched on in Safari; nil when Safari does not say (iOS).
    var enabled: Bool?
}
