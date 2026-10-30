import Foundation

/// What the system said to "Connect" for a Wi-Fi item.
enum WifiOutcome: String, Codable, Sendable, CaseIterable {
    case saved
    case alreadySaved = "already-saved"
    case suggested
    case declined
    case disallowed
    case unsupported
    case failed
}

/// The answer of `wifi_connect`; `reason` comes with `unsupported`.
struct WifiJoined: Codable, Sendable, Equatable, Hashable {
    var outcome: WifiOutcome
    var reason: String?
    var message: String?
}
