import Foundation

/// The kind of a two-step login method (`other` for those this app does not know).
enum TwoFactorKind: String, Codable, Sendable, CaseIterable {
    case authenticator
    case email
    case yubikey
    case duo
    case webauthn
    case u2f
    case other
}

/// One way of two-step login the account has set up.
struct TwoFactorMethod: Codable, Sendable, Equatable, Hashable, Identifiable {
    /// Bitwarden's provider number.
    var provider: Int
    var kind: TwoFactorKind
    /// Whether UwULock can do this one.
    var supported: Bool
    /// For email codes: the masked address the code goes to.
    var hint: String?

    var id: Int { provider }
}

/// What a first master password has to be after an SSO login.
struct PasswordPolicy: Codable, Sendable, Equatable, Hashable {
    var minLength: Int
    var minComplexity: Int
}

/// Where a login stands after a step (tagged by `step`).
enum LoginStep: Codable, Sendable, Equatable, Hashable {
    /// Logged in: the new status.
    case done(Status)
    /// The account asks for a second step.
    case twoFactor(methods: [TwoFactorMethod], message: String?)
    /// A new device: the code mailed to the account is needed.
    case newDevice
    /// After SSO: the master password opens the vault, or (`newAccount`) is set now.
    case ssoPassword(email: String, newAccount: Bool, policy: PasswordPolicy)

    private enum CodingKeys: String, CodingKey {
        case step, status, methods, message, email, newAccount, policy
    }

    private enum Tag: String {
        case done
        case twoFactor = "two-factor"
        case newDevice = "new-device"
        case ssoPassword = "sso-password"
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let raw = try container.decode(String.self, forKey: .step)
        guard let tag = Tag(rawValue: raw) else {
            throw DecodingError.dataCorruptedError(
                forKey: .step, in: container, debugDescription: "Unknown login step \(raw)")
        }
        switch tag {
        case .done:
            self = .done(try container.decode(Status.self, forKey: .status))
        case .twoFactor:
            self = .twoFactor(
                methods: try container.decode([TwoFactorMethod].self, forKey: .methods),
                message: try container.decodeIfPresent(String.self, forKey: .message))
        case .newDevice:
            self = .newDevice
        case .ssoPassword:
            self = .ssoPassword(
                email: try container.decode(String.self, forKey: .email),
                newAccount: try container.decode(Bool.self, forKey: .newAccount),
                policy: try container.decode(PasswordPolicy.self, forKey: .policy))
        }
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .done(let status):
            try container.encode(Tag.done.rawValue, forKey: .step)
            try container.encode(status, forKey: .status)
        case .twoFactor(let methods, let message):
            try container.encode(Tag.twoFactor.rawValue, forKey: .step)
            try container.encode(methods, forKey: .methods)
            try container.encode(message, forKey: .message)
        case .newDevice:
            try container.encode(Tag.newDevice.rawValue, forKey: .step)
        case .ssoPassword(let email, let newAccount, let policy):
            try container.encode(Tag.ssoPassword.rawValue, forKey: .step)
            try container.encode(email, forKey: .email)
            try container.encode(newAccount, forKey: .newAccount)
            try container.encode(policy, forKey: .policy)
        }
    }
}
