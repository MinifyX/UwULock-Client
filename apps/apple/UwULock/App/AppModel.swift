import Foundation
import Observation

/// Where the app is: logged out, locked, or open. The Rust core (milestone 4) drives it later;
/// for now it starts locked. `-UwULockPhase login|locked|unlocked` (launch argument) overrides
/// that, for screenshots.
enum AppPhase: String, Sendable {
    case login, locked, unlocked
}

@MainActor
@Observable
final class AppModel {
    var phase: AppPhase
    var selection: SidebarItem? = .all

    init() {
        let requested = UserDefaults.standard.string(forKey: "UwULockPhase").flatMap(AppPhase.init)
        phase = requested ?? .locked
        RustCore.prepare()
    }

    func lock() {
        guard phase == .unlocked else { return }
        phase = .locked
    }

    /// Placeholder: the real check runs in the Rust core.
    func unlock() {
        phase = .unlocked
    }

    /// Placeholder: login, 2FA and SSO come in milestone 4.
    func logIn() {
        phase = .unlocked
    }

    func logOut() {
        phase = .login
    }
}

/// UserDefaults keys of the appearance settings (this device only, as on the desktop).
enum AppSettingsKey {
    static let font = "appearance.font"
    static let textSize = "appearance.textSize"
    static let reduceMotion = "appearance.reduceMotion"
}
