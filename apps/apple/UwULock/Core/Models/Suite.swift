import Foundation

/// The two sections of the UwULock suite kept in the vault: SSH (UwUSSH) and Remote Desktop (UwURDP).
enum SuiteSpace: String, Codable, Sendable, CaseIterable {
    case ssh
    case rdp
}

/// The kinds of record a suite space holds. A record's own `kind` is a String, since a newer
/// app may add kinds; `SuiteRecord.knownKind` is this when it is one of these.
enum SuiteKind: String, Codable, Sendable, CaseIterable {
    case host
    case group
    case identity
    case key
    case snippet
    case portForward = "port_forward"
    case knownHost = "known_host"
    case secret
}

/// A record of a suite space. The payload is UwUSSH's and UwURDP's JSON.
struct SuiteRecord: Codable, Sendable, Equatable, Hashable, Identifiable {
    var id: String
    var kind: String
    var seq: Int
    var updatedMs: Int
    /// The payload; nil for a secret, or a record that did not open.
    var data: [String: JSONValue]?
    var broken: Bool
    /// Identities and keys: who points at them.
    var usedBy: [String]?

    /// The kind, when this app knows it.
    var knownKind: SuiteKind? { SuiteKind(rawValue: kind) }
}

/// A suite space with its records; `exists` is false until an app has made it.
struct SuiteSpaceView: Codable, Sendable, Equatable, Hashable {
    var space: SuiteSpace
    var exists: Bool
    var records: [SuiteRecord]
}

/// What a save answers: the space now, the records the server had newer copies of (taken over,
/// nothing overwritten) and the key a generation or import made.
struct SuiteSaved: Codable, Sendable, Equatable, Hashable {
    var view: SuiteSpaceView
    var conflicts: [String]
    var id: String?
}

/// One edit of a save (tagged by `op`): the core lays a patch over the record, so fields of a
/// newer app stay. `seq` is the version the page edited (nil for a new record).
enum SuiteOp: Codable, Sendable, Equatable, Hashable {
    case put(id: String, kind: String, seq: Int?, patch: [String: JSONValue])
    case secret(id: String, text: String, seq: Int?)
    case delete(id: String, seq: Int?)

    private enum CodingKeys: String, CodingKey {
        case op, id, kind, seq, patch, text
    }

    private enum Tag: String {
        case put
        case secret
        case delete
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let raw = try container.decode(String.self, forKey: .op)
        guard let tag = Tag(rawValue: raw) else {
            throw DecodingError.dataCorruptedError(
                forKey: .op, in: container, debugDescription: "Unknown suite op \(raw)")
        }
        let id = try container.decode(String.self, forKey: .id)
        let seq = try container.decodeIfPresent(Int.self, forKey: .seq)
        switch tag {
        case .put:
            self = .put(
                id: id, kind: try container.decode(String.self, forKey: .kind), seq: seq,
                patch: try container.decode([String: JSONValue].self, forKey: .patch))
        case .secret:
            self = .secret(
                id: id, text: try container.decode(String.self, forKey: .text), seq: seq)
        case .delete:
            self = .delete(id: id, seq: seq)
        }
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .put(let id, let kind, let seq, let patch):
            try container.encode(Tag.put.rawValue, forKey: .op)
            try container.encode(id, forKey: .id)
            try container.encode(kind, forKey: .kind)
            try container.encodeIfPresent(seq, forKey: .seq)
            try container.encode(patch, forKey: .patch)
        case .secret(let id, let text, let seq):
            try container.encode(Tag.secret.rawValue, forKey: .op)
            try container.encode(id, forKey: .id)
            try container.encode(text, forKey: .text)
            try container.encodeIfPresent(seq, forKey: .seq)
        case .delete(let id, let seq):
            try container.encode(Tag.delete.rawValue, forKey: .op)
            try container.encode(id, forKey: .id)
            try container.encodeIfPresent(seq, forKey: .seq)
        }
    }
}

/// Which half of a key `suite_save_key` saves.
enum SuiteKeyHalf: String, Codable, Sendable, CaseIterable {
    case `public`
    case `private`
}
