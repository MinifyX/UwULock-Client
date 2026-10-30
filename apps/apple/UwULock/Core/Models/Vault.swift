import Foundation

/// Which server an account lives on.
enum ServerKind: String, Codable, Sendable, CaseIterable {
    case bitwardenUS = "bitwarden-us"
    case bitwardenEU = "bitwarden-eu"
    case selfHosted = "self-hosted"
}

/// The server a login or move uses: `url` only for `selfHosted`.
struct ServerInput: Codable, Sendable, Equatable, Hashable {
    var kind: ServerKind
    var url: String?

    init(kind: ServerKind, url: String? = nil) {
        self.kind = kind
        self.url = url
    }
}

/// One account in the switcher.
struct AccountBrief: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var label: String
    var email: String
    var name: String?
    var server: String
    var serverKind: ServerKind
    var unlocked: Bool
    var active: Bool
    var lastSync: Int?
}

/// Where the account on screen stands.
enum StatusState: String, Codable, Sendable, CaseIterable {
    case loggedOut = "logged-out"
    case locked
    case unlocked
}

/// How changes from other devices arrive.
enum LiveChannel: String, Codable, Sendable, CaseIterable {
    case realtime
    case hub
}

/// The vault's status: which account, locked or not, syncing, and every account on the device.
struct Status: Codable, Sendable, Equatable, Hashable {
    var state: StatusState
    var accountId: String?
    var label: String?
    var email: String?
    var name: String?
    var server: String?
    var serverKind: ServerKind?
    var serverUrl: String?
    var lastSync: Int?
    var syncing: Bool
    var syncError: String?
    var sessionExpired: Bool
    /// Changes from other devices arrive as they happen, over this channel.
    var live: LiveChannel?
    /// Unlocking with biometrics: nil where there is none, else whether it is on.
    var hello: Bool?
    /// What it unlocks with: `faceId`, `touchId`, `opticId` (Apple), `windowsHello`, ...
    var helloKind: String?
    var accounts: [AccountBrief]
}

/// The words of the biometric dialog (`reason` completes macOS's "UwULock is trying to ...").
struct UnlockPrompt: Codable, Sendable, Equatable, Hashable {
    var title: String
    var subtitle: String
    var cancel: String
    var reason: String?

    init(title: String, subtitle: String, cancel: String, reason: String? = nil) {
        self.title = title
        self.subtitle = subtitle
        self.cancel = cancel
        self.reason = reason
    }
}

/// A folder of the vault.
struct Folder: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var name: String
}

/// A collection of an organization (`Collection` is Swift's protocol).
struct VaultCollection: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var organizationId: String
    var name: String
}

/// An organization the account belongs to.
struct Organization: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var name: String
}

/// The vault's folders, collections and organizations; `skipped` items could not be read.
struct Overview: Codable, Sendable, Equatable, Hashable {
    var folders: [Folder]
    var collections: [VaultCollection]
    var organizations: [Organization]
    var skipped: Int
}

/// The project pages `open_project_page` can open in the browser.
enum ProjectPage: String, Codable, Sendable, CaseIterable {
    case source
    case releases
    case issues
    case license
    case suite
}
