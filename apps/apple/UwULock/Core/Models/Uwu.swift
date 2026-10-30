import Foundation

/// A domain a Send's link can use.
struct SendDomain: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var url: String
}

/// Travel mode: items marked for it are hidden while it is on.
struct UwuTravel: Codable, Sendable, Equatable, Hashable {
    var enabled: Bool
    var hiddenCount: Int?
}

/// Things the person has not looked at yet.
struct UwuUnseen: Codable, Sendable, Equatable, Hashable {
    var securityNotices: Int
    var fileRequestSubmissions: Int
}

/// The reminder on an item.
struct ReminderMark: Codable, Sendable, Equatable, Hashable {
    var due: String?
    var everyMonths: Int?
    var isDue: Bool
}

/// A masked address linked to an item.
struct MaskedLink: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var email: String
}

/// What the server limits.
struct UwuLimits: Codable, Sendable, Equatable, Hashable {
    var maxFileBytes: Int?
    var versionsPerItem: Int?
    var versionDays: Int?
    var fileRequestMaxFiles: Int?
    var fileRequestMaxDays: Int?
}

/// UwULock Server's extras for the account on screen: which the server offers and the marks on
/// items. An account on Bitwarden or Vaultwarden has an empty one (`uwu` false).
struct UwuStatus: Codable, Sendable, Equatable, Hashable {
    var uwu: Bool
    var features: [String]
    var travel: UwuTravel
    var unseen: UwuUnseen
    var organizations: [Organization]
    var sendDomains: [SendDomain]
    /// Reminders by item id.
    var reminders: [String: ReminderMark]
    /// Masked addresses by item id.
    var masked: [String: MaskedLink]
    /// Items with an own icon, by item id, with its revision.
    var ownIcons: [String: String?]
    var automaticIcons: Bool
    var limits: UwuLimits?
    /// The extras key is not the one this device took before: warn until seen.
    var extrasKeyChanged: Bool

    /// The status of an account without extras.
    static let empty = UwuStatus(
        uwu: false, features: [], travel: UwuTravel(enabled: false, hiddenCount: nil),
        unseen: UwuUnseen(securityNotices: 0, fileRequestSubmissions: 0), organizations: [],
        sendDomains: [], reminders: [:], masked: [:], ownIcons: [:], automaticIcons: false,
        limits: nil, extrasKeyChanged: false)

    /// Whether the server offers `feature`.
    func has(_ feature: UwuFeature) -> Bool {
        features.contains(feature.rawValue)
    }
}

/// The features a UwULock Server can offer (the ids in `UwuStatus.features`).
enum UwuFeature: String, Codable, Sendable, CaseIterable {
    case icons
    case ownIcons = "own-icons"
    case iconLibrary = "icon-library"
    case versions
    case travelMode = "travel-mode"
    case reminders
    case fileRequests = "file-requests"
    case maskedAddresses = "masked-addresses"
    case sendDomains = "send-domains"
    case sendEmails = "send-emails"
    case sends
    case suite
}

/// Where `open_web_vault_at` goes in the web vault.
enum WebVaultPlace: String, Codable, Sendable, CaseIterable {
    case organization
    case masked
    case travel
    case keys
    case fileRequests = "file-requests"
}

/// One change between two versions of an item (`kind`: `changed`, `added` or `removed`).
struct VersionChange: Codable, Sendable, Equatable, Hashable {
    var field: String
    var label: String?
    var kind: String
    var secret: Bool
    var before: String?
    var after: String?
}

/// An earlier version of an item.
struct UwuVersion: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var revisionDate: String?
    var replacedDate: String?
    var size: Int
    var broken: Bool
    var changes: [VersionChange]
}

/// A link where others can send the account files.
struct FileRequest: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var label: String?
    var title: String?
    var note: String?
    var owner: String?
    var link: String?
    var passwordSet: Bool
    var expirationDate: String?
    var deletionDate: String?
    var maxSubmissions: Int?
    var submissionCount: Int
    var maxFiles: Int
    var maxFileBytes: Int?
    var textAllowed: Bool
    var sendDomainId: String?
    var disabled: Bool
    var unseen: Int
    var bytes: Int
    /// Its details encrypt for a key that is not the account's own: no link.
    var foreignKey: Bool
}

/// What the editor saves for a file request. On a change, `expiresInDays` nil keeps the date.
struct FileRequestInput: Codable, Sendable, Equatable, Hashable {
    var label: String
    var title: String
    var note: String?
    var owner: String?
    var expiresInDays: Int?
    var maxSubmissions: Int?
    var maxFiles: Int
    var maxFileMib: Int?
    var textAllowed: Bool
    var password: String?
    var sendDomainId: String?
    var disabled: Bool

    init(
        label: String, title: String, note: String? = nil, owner: String? = nil,
        expiresInDays: Int? = nil, maxSubmissions: Int? = nil, maxFiles: Int = 1,
        maxFileMib: Int? = nil, textAllowed: Bool = false, password: String? = nil,
        sendDomainId: String? = nil, disabled: Bool = false
    ) {
        self.label = label
        self.title = title
        self.note = note
        self.owner = owner
        self.expiresInDays = expiresInDays
        self.maxSubmissions = maxSubmissions
        self.maxFiles = maxFiles
        self.maxFileMib = maxFileMib
        self.textAllowed = textAllowed
        self.password = password
        self.sendDomainId = sendDomainId
        self.disabled = disabled
    }
}

/// A file of a submission; `risky`: a type that runs when opened (a program, a script, a macro).
struct SubmissionFile: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var name: String?
    var size: Int
    var risky: Bool
}

/// What someone sent through a file request.
struct Submission: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var creationDate: String?
    var seen: Bool
    var text: String?
    var senderName: String?
    var senderEmail: String?
    var files: [SubmissionFile]
    var broken: Bool
}

/// The account's connection to the mail service for masked addresses. `status` is the
/// server's word (`ok`, `revoked`, `unreachable`).
struct MaskedConnection: Codable, Sendable, Equatable, Hashable {
    var connected: Bool
    var server: String?
    var username: String?
    var domains: [String]?
    var defaultDomain: String?
    var status: String?
}

/// A masked address. `state` is the server's word (`enabled`, `disabled`, `pending`).
struct MaskedAddress: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var email: String
    var state: String
    var forDomain: String?
    var description: String?
    var createdAt: String?
    var lastMessageAt: String?
    var cipherId: String?
    var itemName: String?
}

/// A masked address' state the person can set.
enum MaskedState: String, Codable, Sendable, CaseIterable {
    case enabled
    case disabled
}

/// What sharing an item as a Send offers.
struct SendOptions: Codable, Sendable, Equatable, Hashable {
    var emails: Bool
    var domains: [SendDomain]
    var defaultDomainId: String?
}

/// One field shared with a Send: its name and its text. It travels as the two-element array
/// `[name, value]`.
struct ShareField: Codable, Sendable, Equatable, Hashable {
    var name: String
    var value: String

    init(name: String, value: String) {
        self.name = name
        self.value = value
    }

    init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        name = try container.decode(String.self)
        value = try container.decode(String.self)
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(name)
        try container.encode(value)
    }
}

/// What the person shares of an item as a Send.
struct ShareInput: Codable, Sendable, Equatable, Hashable {
    var fields: [ShareField]
    var deletionDays: Int
    var maxAccess: Int?
    var password: String?
    var emails: [String]
    var sendDomainId: String?
    var hideText: Bool
    /// An entry Send: UwULock's Send page shows it as an entry. Only then may `fields` name
    /// `totp` (live codes, never the key in the readable text).
    var entry: Bool?

    init(
        fields: [ShareField], deletionDays: Int, maxAccess: Int? = nil, password: String? = nil,
        emails: [String] = [], sendDomainId: String? = nil, hideText: Bool = false,
        entry: Bool? = nil
    ) {
        self.fields = fields
        self.deletionDays = deletionDays
        self.maxAccess = maxAccess
        self.password = password
        self.emails = emails
        self.sendDomainId = sendDomainId
        self.hideText = hideText
        self.entry = entry
    }
}

/// The Send an item was shared as.
struct SharedSend: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var link: String
    var deletionDate: String
}

/// A library of icons the server mirrors: where an icon set comes from.
struct LibrarySource: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var name: String
    var url: String
    var license: String
    var licenseUrl: String
    var attribution: String
}

/// An icon of the library; `variants` is `default`, and `light` / `dark` where it has them.
struct LibraryIcon: Codable, Sendable, Equatable, Hashable, Identifiable {
    var source: String
    var id: String
    var name: String
    var variants: [String]
    var aliases: [String]
}

/// The icon library's index.
struct IconLibrary: Codable, Sendable, Equatable, Hashable {
    var updated: String
    var sources: [LibrarySource]
    var icons: [LibraryIcon]
}
