import Foundation

/// What a Send carries.
enum SendKind: Int, Codable, Sendable, CaseIterable {
    case text = 0
    case file = 1
}

/// Who may open a Send.
enum SendAccess: Int, Codable, Sendable, CaseIterable {
    /// Only the listed `emails`, with a code by mail.
    case emails = 0
    /// Anybody with the password.
    case password = 1
    /// Anybody with the link.
    case anyone = 2
}

/// One of the account's Sends.
struct Send: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var kind: SendKind
    var name: String
    /// Only for the owner.
    var notes: String?
    /// An entry Send's text without its marker line.
    var text: String?
    var hidden: Bool
    var fileName: String?
    var size: Int?
    var maxAccessCount: Int?
    var accessCount: Int
    var hasPassword: Bool
    var authType: SendAccess
    var emails: [String]
    var disabled: Bool
    var hideEmail: Bool
    var revisionDate: String?
    var expirationDate: String?
    var deletionDate: String?
    /// Shared from an item as an entry: its text is not edited.
    var entry: Bool
    var link: String
    /// The send domain its link uses; nil is the server's own address.
    var sendDomainId: String?
}

/// What the editor saves for a Send.
struct SendDraft: Codable, Sendable, Equatable, Hashable {
    var kind: SendKind
    var name: String
    var notes: String?
    var text: String?
    var hidden: Bool
    var fileName: String?
    /// A new password; nil keeps the one there is.
    var password: String?
    var maxAccessCount: Int?
    var expirationDate: String?
    var deletionDate: String
    var disabled: Bool
    var hideEmail: Bool
    var authType: SendAccess
    var emails: [String]

    init(
        kind: SendKind, name: String, notes: String? = nil, text: String? = nil,
        hidden: Bool = false, fileName: String? = nil, password: String? = nil,
        maxAccessCount: Int? = nil, expirationDate: String? = nil, deletionDate: String,
        disabled: Bool = false, hideEmail: Bool = false, authType: SendAccess = .anyone,
        emails: [String] = []
    ) {
        self.kind = kind
        self.name = name
        self.notes = notes
        self.text = text
        self.hidden = hidden
        self.fileName = fileName
        self.password = password
        self.maxAccessCount = maxAccessCount
        self.expirationDate = expirationDate
        self.deletionDate = deletionDate
        self.disabled = disabled
        self.hideEmail = hideEmail
        self.authType = authType
        self.emails = emails
    }
}

/// Where a saved Send's link points (`save_send`'s `domain` / `chooseDomain`).
enum SendLinkDomain: Sendable, Equatable, Hashable {
    /// Leave the link as it is.
    case unchanged
    /// The server's own address.
    case server
    /// A send domain, by id.
    case domain(id: String)
}
