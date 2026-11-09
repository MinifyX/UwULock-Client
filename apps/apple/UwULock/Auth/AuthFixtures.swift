#if DEBUG
import Foundation

/// Made-up accounts for screenshots of the login, lock and settings screens:
/// `-UwULockPhase login|locked|unlocked -UwULockFixture auth [-UwULockStage twoFactor|newDevice|ssoPassword]`.
/// Without `-UwULockFixture auth` the core's status stays (the vault's screenshots use real data).
@MainActor
enum AuthFixtures {
    /// Puts the fixtures in; true when they replace the core's status.
    static func apply(to model: AppModel) -> Bool {
        let defaults = UserDefaults.standard
        guard defaults.string(forKey: "UwULockFixture") == "auth" else { return false }
        let state: StatusState = switch model.phase {
        case .login: .loggedOut
        case .locked: .locked
        case .unlocked: .unlocked
        }
        model.preview(status: status(state), stage: stage(defaults.string(forKey: "UwULockStage")))
        return true
    }

    private static func status(_ state: StatusState) -> Status {
        let now = Int(Date().timeIntervalSince1970)
        let accounts = state == .loggedOut ? [] : [
            AccountBrief(
                id: "a1", label: "Privat", email: "nyu@example.com", name: "Nyu", server: "bitwarden.eu",
                serverKind: .bitwardenEU, unlocked: state == .unlocked, active: true, lastSync: now - 300),
            AccountBrief(
                id: "a2", label: "Arbeit", email: "nyu@work.example", name: nil, server: "vault.example.com",
                serverKind: .selfHosted, unlocked: false, active: false, lastSync: now - 7200),
        ]
        return Status(
            state: state,
            accountId: state == .loggedOut ? nil : "a1",
            label: state == .loggedOut ? nil : "Privat",
            email: state == .loggedOut ? nil : "nyu@example.com",
            name: state == .loggedOut ? nil : "Nyu",
            server: state == .loggedOut ? nil : "bitwarden.eu",
            serverKind: state == .loggedOut ? nil : .bitwardenEU,
            serverUrl: nil,
            lastSync: state == .loggedOut ? nil : now - 300,
            syncing: false,
            syncError: nil,
            sessionExpired: false,
            live: .realtime,
            hello: state == .loggedOut ? nil : true,
            helloKind: {
                #if os(iOS)
                "faceId"
                #else
                "touchId"
                #endif
            }(),
            accounts: accounts)
    }

    private static func stage(_ name: String?) -> LoginStage {
        switch name {
        case "twoFactor":
            .twoFactor(
                methods: [
                    TwoFactorMethod(provider: 0, kind: .authenticator, supported: true, hint: nil),
                    TwoFactorMethod(provider: 1, kind: .email, supported: true, hint: "n***@example.com"),
                    TwoFactorMethod(provider: 7, kind: .webauthn, supported: false, hint: nil),
                ],
                message: nil)
        case "newDevice": .newDevice
        case "ssoPassword": .ssoPassword(email: "nyu@example.com", newAccount: true, policy: PasswordPolicy(minLength: 14, minComplexity: 3))
        default: .form
        }
    }
}
#endif
