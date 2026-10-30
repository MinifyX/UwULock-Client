#if DEBUG
import Foundation

/// Sample move data for screenshots and previews (`FeatureFixtures.isOn`).
/// `-UwULockMoveFixture login|two-factor|new-device|preview|running|finished|cancelled|failed|no-target`
/// opens the flow on that step.
enum MoveFixtures {
    static let target = MoveTarget(
        uwulock: true, families: true, email: "nyu@example.com", server: "vault.example.com")

    static let preview = MovePreview(
        source: "bitwarden.com", sourceEmail: "nyu@example.org", target: "vault.example.com",
        targetEmail: "nyu@example.com", families: true,
        folders: MoveCount(total: 12, moved: 0, todo: 12),
        organizations: MoveCount(total: 2, moved: 1, todo: 1),
        collections: MoveCount(total: 5, moved: 2, todo: 3),
        items: MoveCount(total: 418, moved: 0, todo: 418),
        attachments: MoveCount(total: 9, moved: 0, todo: 9),
        sends: MoveCount(total: 3, moved: 0, todo: 3),
        fileBytes: 38_400_000,
        notices: [
            MoveNotice(code: .trash, count: 7),
            MoveNotice(code: .orgMembers, count: 1),
            MoveNotice(code: .sendPassword, count: 1),
            MoveNotice(code: .sendFileCounted, count: 2),
        ])

    static let methods = [
        TwoFactorMethod(provider: 0, kind: .authenticator, supported: true, hint: nil),
        TwoFactorMethod(provider: 1, kind: .email, supported: true, hint: "n***@example.org"),
        TwoFactorMethod(provider: 7, kind: .webauthn, supported: false, hint: nil),
    ]

    static let finished = MoveFinished(
        summary: MoveSummary(
            moved: MoveTally(
                folders: 12, organizations: 1, collections: 3, items: 416, attachments: 9, sends: 3),
            failed: [
                MoveFailure(kind: .item, message: "„Altes Konto“: Der Server lehnt den Eintrag ab."),
                MoveFailure(kind: .attachment, message: "scan.pdf: Die Datei ist zu groß."),
            ],
            orgsAsFolders: 0),
        cancelled: false)

    static let cancelled = MoveFinished(
        summary: MoveSummary(
            moved: MoveTally(
                folders: 12, organizations: 0, collections: 0, items: 120, attachments: 2, sends: 0),
            failed: [], orgsAsFolders: 1),
        cancelled: true)

    /// The step `-UwULockMoveFixture` asks for.
    static var requested: String? {
        UserDefaults.standard.string(forKey: "UwULockMoveFixture")
    }
}
#endif
