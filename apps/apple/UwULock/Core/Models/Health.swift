import Foundation

/// Which breach sources the password check may ask.
struct BreachSwitches: Codable, Sendable, Equatable, Hashable {
    var hibp: Bool
    var xonPasswords: Bool
    var siteBreaches: Bool
    var emailCheck: Bool
    var changePassword: Bool
}

/// One checked login.
struct Finding: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var name: String
    var subtitle: String?
    var bits: Int
    var weak: Bool
    /// How many other logins have the same password.
    var reused: Int
    var unsecured: Bool
    /// Times seen in a breach; nil when that was not checked.
    var breached: Int?
    var breachSources: [String]
    var host: String?
    var uri: String?
    var passwordChanged: String?
}

/// The check's result: the findings and how complete the breach check was.
struct HealthReport: Codable, Sendable, Equatable, Hashable {
    var findings: [Finding]
    var checked: Int
    var breachesChecked: Bool
    var breachesIncomplete: Bool
}

/// A breach of a website, after the last password change.
struct SiteBreach: Codable, Sendable, Equatable, Hashable {
    var domain: String
    var title: String
    var date: String?
    var added: String?
    var records: Int
    var passwords: Bool
    var dataClasses: [String]
    var sources: [String: String]
}

/// Where a list of breaches or of 2FA sites comes from.
struct HealthSource: Codable, Sendable, Equatable, Hashable {
    var id: String?
    var name: String
    var url: String?
    var license: String?
}

/// A website's two-step login options.
struct TwofaEntry: Codable, Sendable, Equatable, Hashable {
    var domain: String
    var name: String
    var methods: [String]
    var documentation: String?
}

/// A login whose website offers 2FA that is not set up.
struct MissingTwofa: Codable, Sendable, Equatable, Hashable, Identifiable {
    var itemId: String
    var name: String
    var host: String
    var entry: TwofaEntry

    var id: String { itemId }
}

/// The kinds of problem the check finds.
enum ProblemKind: String, Codable, Sendable, CaseIterable {
    case breached
    case siteBreach
    case reused
    case weak
    case unsecured
    case twofa
}

/// A problem of a login (tagged by `kind`).
enum HealthProblem: Codable, Sendable, Equatable, Hashable {
    case breached(count: Int, sources: [String])
    case siteBreach(SiteBreach)
    case reused(others: Int)
    case weak(bits: Int)
    case unsecured
    case twofa(documentation: String?)

    /// The kind of problem this is.
    var kind: ProblemKind {
        switch self {
        case .breached: .breached
        case .siteBreach: .siteBreach
        case .reused: .reused
        case .weak: .weak
        case .unsecured: .unsecured
        case .twofa: .twofa
        }
    }

    private enum CodingKeys: String, CodingKey {
        case kind, count, sources, breach, others, bits, documentation
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let raw = try container.decode(String.self, forKey: .kind)
        guard let kind = ProblemKind(rawValue: raw) else {
            throw DecodingError.dataCorruptedError(
                forKey: .kind, in: container, debugDescription: "Unknown problem kind \(raw)")
        }
        switch kind {
        case .breached:
            self = .breached(
                count: try container.decode(Int.self, forKey: .count),
                sources: try container.decode([String].self, forKey: .sources))
        case .siteBreach:
            self = .siteBreach(try container.decode(SiteBreach.self, forKey: .breach))
        case .reused:
            self = .reused(others: try container.decode(Int.self, forKey: .others))
        case .weak:
            self = .weak(bits: try container.decode(Int.self, forKey: .bits))
        case .unsecured:
            self = .unsecured
        case .twofa:
            self = .twofa(
                documentation: try container.decodeIfPresent(String.self, forKey: .documentation))
        }
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(kind, forKey: .kind)
        switch self {
        case .breached(let count, let sources):
            try container.encode(count, forKey: .count)
            try container.encode(sources, forKey: .sources)
        case .siteBreach(let breach):
            try container.encode(breach, forKey: .breach)
        case .reused(let others):
            try container.encode(others, forKey: .others)
        case .weak(let bits):
            try container.encode(bits, forKey: .bits)
        case .unsecured:
            break
        case .twofa(let documentation):
            try container.encode(documentation, forKey: .documentation)
        }
    }
}

/// A login with its problems: one card of the review.
struct HealthCard: Codable, Sendable, Equatable, Hashable, Identifiable {
    var finding: Finding
    var problems: [HealthProblem]

    var id: String { finding.id }
}

/// A problem the person chose not to see again.
struct IgnoredEntry: Codable, Sendable, Equatable, Hashable {
    var itemId: String
    var kind: ProblemKind
    var since: String
}

/// The account's consent to the check of its addresses.
struct EmailOptIn: Codable, Sendable, Equatable, Hashable {
    var optedIn: Bool
    var since: String?
}

/// The whole check as the screen shows it: the report, the review's cards and the extras.
struct HealthView: Codable, Sendable, Equatable, Hashable {
    /// A UwULock Server: there may be breach sources and the shared ignore list.
    var uwu: Bool
    var switches: BreachSwitches
    var report: HealthReport
    var checkedAt: String?
    var siteBreaches: [String: SiteBreach]
    var siteSources: [HealthSource]
    var sitesFailed: Bool
    var twofa: [MissingTwofa]
    var twofaSource: HealthSource?
    var twofaFailed: Bool
    /// Nil where the server keeps no ignore list.
    var ignored: [IgnoredEntry]?
    var cards: [HealthCard]
    var emailOptIn: EmailOptIn?
}

/// What the check of one address found; `status` is the server's word (`found`, `clean`,
/// `later`, `failed`).
struct EmailResult: Codable, Sendable, Equatable, Hashable, Identifiable {
    var email: String
    var status: String
    var breaches: [String]

    var id: String { email }
}

/// The answers for the addresses; `retryAfter` seconds before the ones that said `later`.
struct EmailsResult: Codable, Sendable, Equatable, Hashable {
    var results: [EmailResult]
    var retryAfter: Int?
}

/// How far one breach source is; `waiting`: questions waiting for a busy server.
struct SourceProgress: Codable, Sendable, Equatable, Hashable {
    var done: Int
    var total: Int
    var waiting: Int
}

/// The `health-progress` event: in all, and per source for breached passwords.
struct CheckProgress: Codable, Sendable, Equatable, Hashable {
    var done: Int
    var total: Int
    var hibp: SourceProgress?
    var xon: SourceProgress?
}
