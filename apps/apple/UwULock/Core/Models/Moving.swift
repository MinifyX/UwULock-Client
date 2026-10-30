import Foundation

/// The account on screen as the target of a move.
struct MoveTarget: Codable, Sendable, Equatable, Hashable {
    /// It is on a UwULock Server.
    var uwulock: Bool
    /// Its server makes families.
    var families: Bool
    var email: String
    var server: String
}

/// How much of one kind there is to move.
struct MoveCount: Codable, Sendable, Equatable, Hashable {
    var total: Int
    var moved: Int
    var todo: Int
}

/// What the preview warns about.
enum MoveNoticeCode: String, Codable, Sendable, CaseIterable {
    case trash
    case broken
    case readOnly = "read-only"
    case orgMembers = "org-members"
    case orgsAsFolders = "orgs-as-folders"
    case sendPassword = "send-password"
    case sendEmails = "send-emails"
    case sendFileLocked = "send-file-locked"
    case sendExpired = "send-expired"
    case sendFileCounted = "send-file-counted"
    case tooLarge = "too-large"
}

/// A warning of the preview with the number of objects it concerns.
struct MoveNotice: Codable, Sendable, Equatable, Hashable {
    var code: MoveNoticeCode
    var count: Int
}

/// What a move would do, from the source login.
struct MovePreview: Codable, Sendable, Equatable, Hashable {
    var source: String
    var sourceEmail: String
    var target: String
    var targetEmail: String
    var families: Bool
    var folders: MoveCount
    var organizations: MoveCount
    var collections: MoveCount
    var items: MoveCount
    var attachments: MoveCount
    var sends: MoveCount
    var fileBytes: Int
    var notices: [MoveNotice]
}

/// Where the source login of a move stands (tagged by `step`).
enum MoveStep: Codable, Sendable, Equatable, Hashable {
    /// Logged in: the preview.
    case done(MovePreview)
    /// The source asks for a second step.
    case twoFactor(methods: [TwoFactorMethod], message: String?)
    /// A new device: the mailed code is needed.
    case newDevice

    private enum CodingKeys: String, CodingKey {
        case step, preview, methods, message
    }

    private enum Tag: String {
        case done
        case twoFactor = "two-factor"
        case newDevice = "new-device"
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let raw = try container.decode(String.self, forKey: .step)
        guard let tag = Tag(rawValue: raw) else {
            throw DecodingError.dataCorruptedError(
                forKey: .step, in: container, debugDescription: "Unknown move step \(raw)")
        }
        switch tag {
        case .done:
            self = .done(try container.decode(MovePreview.self, forKey: .preview))
        case .twoFactor:
            self = .twoFactor(
                methods: try container.decode([TwoFactorMethod].self, forKey: .methods),
                message: try container.decodeIfPresent(String.self, forKey: .message))
        case .newDevice:
            self = .newDevice
        }
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .done(let preview):
            try container.encode(Tag.done.rawValue, forKey: .step)
            try container.encode(preview, forKey: .preview)
        case .twoFactor(let methods, let message):
            try container.encode(Tag.twoFactor.rawValue, forKey: .step)
            try container.encode(methods, forKey: .methods)
            try container.encode(message, forKey: .message)
        case .newDevice:
            try container.encode(Tag.newDevice.rawValue, forKey: .step)
        }
    }
}

/// The kinds of object a move carries over.
enum MoveKind: String, Codable, Sendable, CaseIterable {
    case folder
    case organization
    case collection
    case item
    case attachment
    case send
}

/// The `move-progress` event: how far the move is with one kind.
struct MoveProgress: Codable, Sendable, Equatable, Hashable {
    var done: Int
    var total: Int
    var kind: MoveKind
}

/// How many objects of each kind were moved.
struct MoveTally: Codable, Sendable, Equatable, Hashable {
    var folders: Int
    var organizations: Int
    var collections: Int
    var items: Int
    var attachments: Int
    var sends: Int
}

/// An object that could not be moved.
struct MoveFailure: Codable, Sendable, Equatable, Hashable {
    var kind: MoveKind
    var message: String
}

/// What a finished move did.
struct MoveSummary: Codable, Sendable, Equatable, Hashable {
    var moved: MoveTally
    var failed: [MoveFailure]
    var orgsAsFolders: Int
}

/// The `move-finished` event: the summary, and whether the move was cancelled.
struct MoveFinished: Codable, Sendable, Equatable, Hashable {
    var summary: MoveSummary
    var cancelled: Bool
}
