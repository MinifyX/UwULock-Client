import Foundation

/// What an item is; `wifi` is a secure note with UwULock's marker field (saved as a `note`).
enum ItemKind: String, Codable, Sendable, CaseIterable {
    case login
    case note
    case card
    case identity
    case sshKey = "ssh-key"
    case wifi
}

/// The kinds an item can be saved as (a Wi-Fi network goes back as the note it is).
enum DraftKind: String, Codable, Sendable, CaseIterable {
    case login
    case note
    case card
    case identity
    case sshKey = "ssh-key"
}

/// How a custom field holds its value.
enum FieldKind: String, Codable, Sendable, CaseIterable {
    case text
    case hidden
    case boolean
    case linked
}

/// An item in the list: no secret in it.
struct ItemSummary: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var kind: ItemKind
    var name: String
    var subtitle: String?
    var host: String?
    var favorite: Bool
    var folderId: String?
    var organizationId: String?
    var collectionIds: [String]
    var deleted: Bool
    var reprompt: Bool
    /// False when the organization hides this item's passwords from this member.
    var viewPassword: Bool
    var hasTotp: Bool
    var hasPassword: Bool
    var hasUsername: Bool
    var broken: Bool
    var revisionDate: String?
}

/// An address of a login.
struct LoginUri: Codable, Sendable, Equatable, Hashable {
    var uri: String
    var match: Int?
    var host: String?
    var openable: Bool
}

/// The login part of an item's details.
struct LoginDetail: Codable, Sendable, Equatable, Hashable {
    var username: String?
    var hasPassword: Bool
    var hasTotp: Bool
    var passwordRevisionDate: String?
    var uris: [LoginUri]
    var passkeys: Int
}

/// The card part of an item's details (the number only as its ending).
struct CardDetail: Codable, Sendable, Equatable, Hashable {
    var cardholderName: String?
    var brand: String?
    var numberEnding: String?
    var expMonth: String?
    var expYear: String?
    var hasCode: Bool
}

/// One identity field; a sensitive one has no `value` until it is revealed.
struct IdentityField: Codable, Sendable, Equatable, Hashable, Identifiable {
    var name: String
    var sensitive: Bool
    var value: String?

    var id: String { name }
}

/// The SSH key part of an item's details.
struct SshKeyDetail: Codable, Sendable, Equatable, Hashable {
    var publicKey: String?
    var fingerprint: String?
    var hasPrivateKey: Bool
}

/// A custom field of an item, as shown.
struct FieldDetail: Codable, Sendable, Equatable, Hashable, Identifiable {
    var index: Int
    var name: String?
    var kind: FieldKind
    var value: String?
    var hasValue: Bool

    var id: Int { index }
}

/// An entry of an item's password history (the password itself: `revealField` `history:<n>`).
struct PasswordHistoryEntry: Codable, Sendable, Equatable, Hashable, Identifiable {
    var index: Int
    var lastUsed: String?

    var id: Int { index }
}

/// An item with its details, secrets left out. `locked`: the master password re-prompt is
/// pending, only `summary` is there.
struct ItemDetail: Codable, Sendable, Equatable, Hashable {
    var summary: ItemSummary
    var locked: Bool
    var notes: String?
    var login: LoginDetail?
    var card: CardDetail?
    var identity: [IdentityField]?
    var sshKey: SshKeyDetail?
    var fields: [FieldDetail]?
    var passwordHistory: [PasswordHistoryEntry]?
    var attachments: Int?
    var creationDate: String?
}

/// The current and the next one-time code of a login.
struct TotpCode: Codable, Sendable, Equatable, Hashable {
    var code: String
    var remaining: Int
    var period: Int
    /// The code of the following period; `showNext` is true in its last 10 seconds.
    var next: String
    var showNext: Bool
}

/// An address of a login as the editor sends it.
struct UriDraft: Codable, Sendable, Equatable, Hashable {
    var uri: String
    var match: Int?
}

/// The login part of a draft. A secret left `nil` keeps the stored one; `""` clears it.
struct LoginDraft: Codable, Sendable, Equatable, Hashable {
    var username: String?
    var password: String?
    var totp: String?
    var uris: [UriDraft]

    init(
        username: String? = nil, password: String? = nil, totp: String? = nil, uris: [UriDraft] = []
    ) {
        self.username = username
        self.password = password
        self.totp = totp
        self.uris = uris
    }
}

/// The card part of a draft. A secret left `nil` keeps the stored one; `""` clears it.
struct CardDraft: Codable, Sendable, Equatable, Hashable {
    var cardholderName: String?
    var brand: String?
    var number: String?
    var expMonth: String?
    var expYear: String?
    var code: String?

    init(
        cardholderName: String? = nil, brand: String? = nil, number: String? = nil,
        expMonth: String? = nil, expYear: String? = nil, code: String? = nil
    ) {
        self.cardholderName = cardholderName
        self.brand = brand
        self.number = number
        self.expMonth = expMonth
        self.expYear = expYear
        self.code = code
    }
}

/// The SSH key part of a draft. A part left `nil` keeps the stored one.
struct SshKeyDraft: Codable, Sendable, Equatable, Hashable {
    var privateKey: String?
    var publicKey: String?
    var fingerprint: String?

    init(privateKey: String? = nil, publicKey: String? = nil, fingerprint: String? = nil) {
        self.privateKey = privateKey
        self.publicKey = publicKey
        self.fingerprint = fingerprint
    }
}

/// A custom field in a draft; `from` is the index it had, for a value the editor never saw.
struct FieldDraft: Codable, Sendable, Equatable, Hashable {
    var name: String?
    var kind: FieldKind
    var value: String?
    var from: Int?

    init(name: String?, kind: FieldKind, value: String? = nil, from: Int? = nil) {
        self.name = name
        self.kind = kind
        self.value = value
        self.from = from
    }
}

/// What the editor sends back to save an item. A secret it never had stays `nil` and the core
/// keeps the stored value; `""` clears it.
struct Draft: Codable, Sendable, Equatable, Hashable {
    var kind: DraftKind
    var name: String
    var notes: String?
    var favorite: Bool
    var reprompt: Bool
    var folderId: String?
    var login: LoginDraft?
    var card: CardDraft?
    /// By field name; a name that is not in here keeps its value.
    var identity: [String: String]?
    var sshKey: SshKeyDraft?
    var fields: [FieldDraft]

    init(
        kind: DraftKind, name: String, notes: String? = nil, favorite: Bool = false,
        reprompt: Bool = false, folderId: String? = nil, login: LoginDraft? = nil,
        card: CardDraft? = nil, identity: [String: String]? = nil, sshKey: SshKeyDraft? = nil,
        fields: [FieldDraft] = []
    ) {
        self.kind = kind
        self.name = name
        self.notes = notes
        self.favorite = favorite
        self.reprompt = reprompt
        self.folderId = folderId
        self.login = login
        self.card = card
        self.identity = identity
        self.sshKey = sshKey
        self.fields = fields
    }
}
