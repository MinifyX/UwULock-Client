import Foundation
import Observation

/// Where the app is: logged out, locked, or open. The Rust core's status drives it
/// (`vault_status` at launch, then the `vault-status` event). `-UwULockPhase login|locked|unlocked`
/// (launch argument) pins it instead, for screenshots.
enum AppPhase: String, Sendable {
    case login, locked, unlocked

    init(_ state: StatusState) {
        switch state {
        case .loggedOut: self = .login
        case .locked: self = .locked
        case .unlocked: self = .unlocked
        }
    }
}

@MainActor
@Observable
final class AppModel {
    var phase: AppPhase
    var selection: SidebarItem? = .all
    /// What the last login or unlock attempt said (shown on the login and lock screens).
    var failure: CoreFailure?
    /// A login or unlock is under way.
    private(set) var busy = false

    /// The app's one core (crates/uwulock-ffi): accounts, vault, sync.
    let core: Core
    /// Set by `-UwULockPhase`: the core's status doesn't move the phase.
    private let pinned: Bool

    init() {
        let requested = UserDefaults.standard.string(forKey: "UwULockPhase").flatMap(AppPhase.init(rawValue:))
        pinned = requested != nil
        // Until the core answers: locked, never the vault.
        phase = requested ?? .locked
        core = Core()
        guard core.isAvailable else {
            NSLog("UwULock: no core: \(core.problem?.message ?? "")")
            return
        }
        core.start()
        Task { await self.follow() }
    }

    /// The status at launch, then every change the core tells.
    private func follow() async {
        let events = core.events()
        do {
            show(try await core.vaultStatus())
        } catch {
            failure = Core.failure(error)
        }
        for await event in events {
            if case .vaultStatus(let status) = event {
                show(status)
            }
        }
    }

    private func show(_ status: Status) {
        guard !pinned else { return }
        phase = AppPhase(status.state)
    }

    // MARK: Actions

    func lock() {
        guard phase == .unlocked else { return }
        guard core.isAvailable, !pinned else {
            phase = .locked
            return
        }
        perform { try await self.core.lock() }
    }

    func unlock(password: String) {
        guard core.isAvailable, !pinned else {
            phase = .unlocked
            return
        }
        perform { self.show(try await self.core.unlock(password: password)) }
    }

    func unlockWithBiometrics() {
        guard core.isAvailable, !pinned else {
            phase = .unlocked
            return
        }
        perform { self.show(try await self.core.unlockWithHello()) }
    }

    /// The first login step; two-step login, new devices and SSO are milestone 4's screens.
    func logIn(server: ServerInput, email: String, password: String) {
        guard core.isAvailable, !pinned else {
            phase = .unlocked
            return
        }
        perform {
            switch try await self.core.login(server: server, email: email, password: password) {
            case .done(let status):
                self.show(status)
            case .twoFactor, .newDevice, .ssoPassword:
                self.failure = .unsupported("This login needs another step, which this build can't do yet.")
            }
        }
    }

    func logOut() {
        guard core.isAvailable, !pinned else {
            phase = .login
            return
        }
        perform { self.show(try await self.core.logout()) }
    }

    private func perform(_ action: @escaping @MainActor () async throws -> Void) {
        guard !busy else { return }
        busy = true
        failure = nil
        Task {
            defer { busy = false }
            do {
                try await action()
            } catch {
                failure = Core.failure(error)
            }
        }
    }
}

/// UserDefaults keys of the appearance settings (this device only, as on the desktop).
enum AppSettingsKey {
    static let font = "appearance.font"
    static let textSize = "appearance.textSize"
    static let reduceMotion = "appearance.reduceMotion"
}
