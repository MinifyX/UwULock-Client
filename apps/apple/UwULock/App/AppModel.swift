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

/// Why the login screen shows (App.tsx): the first account, another one, or the server forgot
/// this device's session (`again`: server and email stay as they were).
enum LoginMode: Equatable, Sendable {
    case first, adding, again
}

/// Where a login stands between its steps (LoginScreen.tsx).
enum LoginStage: Equatable, Sendable {
    /// Server, email and master password.
    case form
    /// SSO: waiting for the browser to come back.
    case browser
    case twoFactor(methods: [TwoFactorMethod], message: String?)
    case newDevice
    case ssoPassword(email: String, newAccount: Bool, policy: PasswordPolicy)
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

    /// The core's last status: accounts, sync, biometrics.
    private(set) var status: Status?
    /// The login's step; `.form` when none is under way.
    private(set) var loginStage: LoginStage = .form
    /// "Konto hinzufügen": the login screen over an open or locked vault.
    private(set) var adding = false
    /// A message the login form shows when a later step sent it back (`sso-restart`).
    var loginNotice: String?
    /// Another device asks to log in with this account (the `auth-request` event).
    var authRequest: String?
    /// Counts login flows: an answer for an older one (cancelled SSO, "Zurück") is dropped.
    @ObservationIgnored private var flow = 0
    @ObservationIgnored private var lastTouch = Date.distantPast
    #if DEBUG
    /// Screenshot fixtures stand in for the core's status (`-UwULockPhase` with `-UwULockFixture`).
    @ObservationIgnored private var fixture = false
    #endif

    /// The app's one core (crates/uwulock-ffi): accounts, vault, sync.
    let core: Core
    /// Set by `-UwULockPhase`: the core's status doesn't move the phase.
    let pinned: Bool

    init() {
        let requested = UserDefaults.standard.string(forKey: "UwULockPhase").flatMap(AppPhase.init(rawValue:))
        pinned = requested != nil
        // Until the core answers: locked, never the vault.
        phase = requested ?? .locked
        core = Core()
        #if DEBUG
        if pinned { fixture = AuthFixtures.apply(to: self) }
        #endif
        guard core.isAvailable else {
            NSLog("UwULock: no core: \(core.problem?.message ?? "")")
            return
        }
        core.start()
        applySecurity()
        Task { await self.follow() }
    }

    /// Which login screen shows.
    var loginMode: LoginMode {
        if adding { return .adding }
        if let status, status.state != .loggedOut, status.sessionExpired { return .again }
        return .first
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
            switch event {
            case .vaultStatus(let status): show(status)
            case .authRequest(let id): authRequest = id
            default: break
            }
        }
    }

    func show(_ status: Status) {
        #if DEBUG
        if fixture { return }
        #endif
        self.status = status
        core.remember(status)
        updatePhase()
    }

    /// App.tsx's order: logged out → login; adding → login; locked → lock; session expired →
    /// login again; otherwise the vault.
    private func updatePhase() {
        guard !pinned, let status else { return }
        let next: AppPhase
        switch status.state {
        case .loggedOut: next = .login
        case .locked: next = adding ? .login : .locked
        case .unlocked: next = adding || status.sessionExpired ? .login : .unlocked
        }
        if next != phase {
            phase = next
            if next != .login { resetLogin() }
        }
    }

    // MARK: Lock and unlock

    func lock() {
        guard phase == .unlocked || adding else { return }
        guard core.isAvailable, !pinned else {
            phase = .locked
            return
        }
        adding = false
        perform { try await self.core.lock() }
    }

    func unlock(password: String) {
        guard core.isAvailable, !pinned else {
            phase = .unlocked
            return
        }
        perform { self.show(try await self.core.unlock(password: password)) }
    }

    /// Face ID / Touch ID; a cancel is no error worth showing (LockScreen.tsx).
    func unlockWithBiometrics() {
        guard core.isAvailable, !pinned else {
            phase = .unlocked
            return
        }
        perform(quietly: ["biometric-cancelled"]) {
            self.show(try await self.core.unlockWithHello(prompt: .standard))
        }
    }

    /// Switches unlocking with Face ID / Touch ID on or off (the master password stays).
    func setBiometrics(_ enabled: Bool) async throws {
        guard core.isAvailable, !pinned else { return }
        show(try await core.setHello(enabled: enabled, prompt: enabled ? .standard : nil))
    }

    // MARK: Login

    /// The first login step. The core turns the master password into its key and hash.
    func logIn(server: ServerInput, email: String, password: String) {
        guard core.isAvailable, !pinned else {
            phase = .unlocked
            return
        }
        loginNotice = nil
        let id = flow
        perform {
            let step = try await self.core.login(server: server, email: email, password: password)
            self.advance(step, flow: id, remember: (server, email))
        }
    }

    /// SSO for self-hosted servers: the system browser on the Mac (as on the desktop), the page
    /// in the app on iOS (SsoBrowser).
    func logInWithSso(server: ServerInput, email: String?) {
        guard core.isAvailable, !pinned else { return }
        loginNotice = nil
        failure = nil
        flow += 1
        let id = flow
        loginStage = .browser
        #if os(iOS)
        SsoBrowser.shared.begin { [weak self] in
            guard let self, self.flow == id else { return }
            self.backToForm()
        }
        #endif
        Task {
            do {
                let step = try await core.loginSso(server: server, email: email)
                #if os(iOS)
                SsoBrowser.shared.finish()
                #endif
                advance(step, flow: id, remember: (server, email ?? ""))
            } catch {
                #if os(iOS)
                SsoBrowser.shared.finish()
                #endif
                guard flow == id else { return }
                loginStage = .form
                let failure = Core.failure(error)
                if failure.kind != "cancelled" { self.failure = failure }
            }
        }
    }

    /// Two-step login. A code the server didn't take keeps the step and says so.
    func submitTwoFactor(provider: Int, code: String, remember: Bool) {
        let id = flow
        perform {
            let step = try await self.core.loginTwoFactor(provider: provider, code: code, remember: remember)
            if case .twoFactor = step, self.flow == id {
                self.failure = CoreFailure(kind: "code-rejected", message: authStr("Der Code wurde nicht angenommen."))
                return
            }
            self.advance(step, flow: id, remember: nil)
        }
    }

    func submitNewDevice(code: String) {
        let id = flow
        perform {
            let step = try await self.core.loginNewDevice(code: code)
            if case .newDevice = step, self.flow == id {
                self.failure = CoreFailure(kind: "code-rejected", message: authStr("Der Code wurde nicht angenommen."))
                return
            }
            self.advance(step, flow: id, remember: nil)
        }
    }

    /// Two-step login by email: sends (again) the code.
    func sendEmailCode() async throws {
        try await core.loginSendEmail()
    }

    /// After SSO: the master password opens (or, for a new account, sets up) the vault.
    func submitSsoPassword(_ password: String) {
        let id = flow
        perform(quietly: ["sso-restart"]) {
            do {
                let step = try await self.core.loginSsoPassword(password: password)
                self.advance(step, flow: id, remember: nil)
            } catch {
                let failure = Core.failure(error)
                if failure.kind == "sso-restart", self.flow == id {
                    self.backToForm()
                    self.loginNotice = failure.authMessage
                }
                throw error
            }
        }
    }

    /// "Zurück" from a step, or "Abbrechen" while the browser is open: the core forgets the login.
    func backToForm() {
        flow += 1
        failure = nil
        loginStage = .form
        #if os(iOS)
        SsoBrowser.shared.finish()
        #endif
        if core.isAvailable, !pinned {
            Task { try? await core.loginCancel() }
        }
    }

    /// "Konto hinzufügen" from the lock screen, the account menu or Settings.
    func startAdding() {
        resetLogin()
        adding = true
        if pinned { phase = .login } else { updatePhase() }
    }

    /// "Abbrechen" on the login screen: back to the vault (adding) or locked (session expired).
    func cancelLogin() {
        switch loginMode {
        case .first: return
        case .adding:
            backToForm()
            adding = false
            if pinned { phase = .unlocked } else { updatePhase() }
        case .again:
            backToForm()
            guard core.isAvailable, !pinned else { return }
            perform { try await self.core.lock() }
        }
    }

    private func advance(_ step: LoginStep, flow id: Int, remember: (ServerInput, String)?) {
        guard flow == id else { return }
        switch step {
        case .done(let status):
            if let (server, email) = remember {
                LastLogin.save(server: server, email: email)
            } else if case .ssoPassword(let email, _, _) = loginStage {
                LastLogin.save(email: email)
            }
            adding = false
            loginStage = .form
            show(status)
        case .twoFactor(let methods, let message):
            if let (server, email) = remember { LastLogin.save(server: server, email: email) }
            loginStage = .twoFactor(methods: methods, message: message)
        case .newDevice:
            if let (server, email) = remember { LastLogin.save(server: server, email: email) }
            loginStage = .newDevice
        case .ssoPassword(let email, let newAccount, let policy):
            if let (server, _) = remember { LastLogin.save(server: server, email: email) }
            loginStage = .ssoPassword(email: email, newAccount: newAccount, policy: policy)
        }
    }

    private func resetLogin() {
        flow += 1
        loginStage = .form
        loginNotice = nil
        failure = nil
    }

    // MARK: Accounts

    /// Logs the active account (or `id`) out of this device.
    func logOut(id: String? = nil) {
        guard core.isAvailable, !pinned else {
            phase = .login
            return
        }
        perform { self.show(try await self.core.logout(id: id)) }
    }

    func switchAccount(_ id: String) {
        guard core.isAvailable, !pinned else { return }
        adding = false
        perform { self.show(try await self.core.switchAccount(id: id)) }
    }

    func renameAccount(_ id: String, label: String) async throws {
        guard core.isAvailable, !pinned else { return }
        show(try await core.renameAccount(id: id, label: label))
    }

    func syncNow() async throws {
        guard core.isAvailable, !pinned else { return }
        show(try await core.syncNow())
    }

    // MARK: Security settings

    /// Hands the security settings (this device only) to the core: at launch and on every change.
    func applySecurity() {
        guard core.isAvailable, !pinned else { return }
        let settings = SecurityPrefs.current
        Task {
            try? await core.setSecurity(
                autoLockMinutes: settings.autoLock > 0 ? settings.autoLock : nil,
                clipboardSeconds: settings.clipboardClear > 0 ? settings.clipboardClear : nil,
                lockWithSystem: settings.lockWithSystem)
        }
    }

    /// Input in the app: the auto-lock's clock starts again (at most every 20 s, as App.tsx).
    func noteActivity() {
        guard phase == .unlocked, core.isAvailable, !pinned else { return }
        let now = Date()
        guard now.timeIntervalSince(lastTouch) > 20 else { return }
        lastTouch = now
        Task { try? await core.touch() }
    }

    private func perform(
        quietly quiet: Set<String> = [],
        _ action: @escaping @MainActor () async throws -> Void
    ) {
        guard !busy else { return }
        busy = true
        failure = nil
        Task {
            defer { busy = false }
            do {
                try await action()
            } catch {
                let failure = Core.failure(error)
                if !quiet.contains(failure.kind) { self.failure = failure }
            }
        }
    }

    #if DEBUG
    /// Screenshot fixtures only (`-UwULockPhase`).
    func preview(status: Status, stage: LoginStage = .form) {
        self.status = status
        self.loginStage = stage
    }
    #endif
}

/// UserDefaults keys of the settings (this device only, as on the desktop).
enum AppSettingsKey {
    static let font = "appearance.font"
    static let textSize = "appearance.textSize"
    /// Kept for older builds; `motion` replaces it.
    static let reduceMotion = "appearance.reduceMotion"
    /// `AppTheme`: system, light, dark.
    static let theme = "appearance.theme"
    /// `AppContrast`: system, normal, high.
    static let contrast = "appearance.contrast"
    /// `AppMotion`: system, on, off.
    static let motion = "appearance.motion"
    /// iPhone: haptic feedback (default on).
    static let haptics = "appearance.haptics"
    /// The sidebar's trash (default on).
    static let showTrash = "vault.showTrash"
    /// Website icons through UwULock Server (default on).
    static let siteIcons = "vault.siteIcons"
    /// Minutes without input until the vault locks; 0: never (default 15).
    static let autoLock = "security.autoLock"
    /// Seconds until a copied value is cleared; 0: never (default 30).
    static let clipboardClear = "security.clipboardClear"
    /// Lock with the screen lock / sleep (Mac), after a minute in the background (iPhone).
    static let lockWithSystem = "security.lockWithSystem"
    /// `stable` or `beta` (the disk image's updates).
    static let updateChannel = "updates.channel"
    static let lastServerKind = "login.lastServerKind"
    static let lastServerUrl = "login.lastServerUrl"
    static let lastEmail = "login.lastEmail"
}

/// The security settings as the core takes them.
struct SecurityPrefs {
    var autoLock: Int
    var clipboardClear: Int
    var lockWithSystem: Bool

    static let autoLockChoices = [0, 1, 5, 15, 30, 60, 240]
    static let clipboardChoices = [0, 10, 30, 60, 120]

    static var current: SecurityPrefs {
        let defaults = UserDefaults.standard
        return SecurityPrefs(
            autoLock: defaults.object(forKey: AppSettingsKey.autoLock) as? Int ?? 15,
            clipboardClear: defaults.object(forKey: AppSettingsKey.clipboardClear) as? Int ?? 30,
            lockWithSystem: defaults.object(forKey: AppSettingsKey.lockWithSystem) as? Bool ?? true)
    }
}

/// The login form's defaults: the last server and email that worked (LoginScreen.tsx).
enum LastLogin {
    static var serverKind: ServerKind {
        UserDefaults.standard.string(forKey: AppSettingsKey.lastServerKind).flatMap(ServerKind.init(rawValue:))
            ?? .bitwardenEU
    }

    static var serverUrl: String { UserDefaults.standard.string(forKey: AppSettingsKey.lastServerUrl) ?? "" }
    static var email: String { UserDefaults.standard.string(forKey: AppSettingsKey.lastEmail) ?? "" }

    static func save(server: ServerInput, email: String) {
        let defaults = UserDefaults.standard
        defaults.set(server.kind.rawValue, forKey: AppSettingsKey.lastServerKind)
        defaults.set(server.url ?? "", forKey: AppSettingsKey.lastServerUrl)
        save(email: email)
    }

    static func save(email: String) {
        UserDefaults.standard.set(email.trimmingCharacters(in: .whitespaces), forKey: AppSettingsKey.lastEmail)
    }
}
