import Foundation

/// How the text of an import is written.
enum ImportFormat: String, Codable, Sendable, CaseIterable {
    case json
    case csv
}

/// The `import-progress` event: items done of all.
struct ImportProgress: Codable, Sendable, Equatable, Hashable {
    var done: Int
    var total: Int
}

/// Why an item stayed out of an import.
enum SkippedReason: String, Codable, Sendable, CaseIterable {
    /// The core cannot save it (an SSH key without all its parts, ...).
    case invalid
    /// A value (mostly the notes) is longer than the server takes.
    case tooLong = "too-long"
}

/// An item an import left out.
struct SkippedItem: Codable, Sendable, Equatable, Hashable {
    var name: String
    var reason: SkippedReason
}

/// What an import did.
struct ImportOutcome: Codable, Sendable, Equatable {
    var imported: Int
    var foldersCreated: Int
    var skipped: [SkippedItem]
    /// Set when the import stopped part way; what came in before stays.
    var error: CoreFailure?
}
