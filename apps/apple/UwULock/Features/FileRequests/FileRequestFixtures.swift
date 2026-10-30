#if DEBUG
// Sample file requests for screenshots and previews (`-UwULockFixtures YES`).
import Foundation

enum FileRequestFixtures {
    private static func days(_ n: Double) -> String {
        FeatureDate.iso(Date.now.addingTimeInterval(n * 86_400))
    }

    static let requests: [FileRequest] = [
        FileRequest(
            id: "fr-tax", label: "Steuerunterlagen 2026", title: "Belege für die Steuererklärung",
            note: "Bitte alle Belege als PDF, gern auch gescannt.", owner: "Mika",
            link: "https://vault.example.com/#/file-request/fr-tax/k3y-s4mpl3", passwordSet: true,
            expirationDate: days(12), deletionDate: days(42), maxSubmissions: 5,
            submissionCount: 2, maxFiles: 10, maxFileBytes: 50 * 1024 * 1024, textAllowed: true,
            sendDomainId: nil, disabled: false, unseen: 1, bytes: 3_400_000, foreignKey: false),
        FileRequest(
            id: "fr-passport", label: nil, title: "Scan des Reisepasses", note: nil, owner: nil,
            link: "https://send.example.com/#/file-request/fr-passport/p4ss", passwordSet: false,
            expirationDate: days(3), deletionDate: days(33), maxSubmissions: 1,
            submissionCount: 1, maxFiles: 2, maxFileBytes: 10 * 1024 * 1024, textAllowed: false,
            sendDomainId: "d1", disabled: false, unseen: 1, bytes: 820_000, foreignKey: false),
        FileRequest(
            id: "fr-photos", label: "Fotos vom Sommerfest", title: "Eure Fotos", note: nil,
            owner: "Mika", link: "https://vault.example.com/#/file-request/fr-photos/ph0t0",
            passwordSet: false, expirationDate: days(-4), deletionDate: days(26),
            maxSubmissions: nil, submissionCount: 0, maxFiles: 20, maxFileBytes: nil,
            textAllowed: true, sendDomainId: nil, disabled: false, unseen: 0, bytes: 0,
            foreignKey: false),
        FileRequest(
            id: "fr-old", label: "Alter Link", title: "Vertrag", note: nil, owner: nil, link: nil,
            passwordSet: false, expirationDate: days(20), deletionDate: days(50),
            maxSubmissions: nil, submissionCount: 0, maxFiles: 1, maxFileBytes: nil,
            textAllowed: true, sendDomainId: nil, disabled: true, unseen: 0, bytes: 0,
            foreignKey: true),
    ]

    static let submissions: [String: [Submission]] = [
        "fr-tax": [
            Submission(
                id: "s1", creationDate: days(-0.1), seen: false,
                text: "Hallo Mika,\nanbei die Belege vom Handwerker und die Spendenquittung.",
                senderName: "Robin Beispiel", senderEmail: "robin@example.org",
                files: [
                    SubmissionFile(id: "f1", name: "Rechnung-Handwerker.pdf", size: 1_240_000, risky: false),
                    SubmissionFile(id: "f2", name: "Spendenquittung.pdf", size: 310_000, risky: false),
                    SubmissionFile(id: "f3", name: "Tabelle.xlsm", size: 88_000, risky: true),
                ],
                broken: false),
            Submission(
                id: "s2", creationDate: days(-2), seen: true, text: nil, senderName: nil,
                senderEmail: "kim@example.net",
                files: [SubmissionFile(id: "f4", name: nil, size: 1_700_000, risky: false)],
                broken: true),
        ],
        "fr-passport": [
            Submission(
                id: "s3", creationDate: days(-0.5), seen: false, text: nil,
                senderName: "Alex Muster", senderEmail: nil,
                files: [SubmissionFile(id: "f5", name: "Reisepass.jpg", size: 820_000, risky: false)],
                broken: false),
        ],
    ]

    /// What saving `input` would make of `request` (or a new one).
    static func apply(
        _ input: FileRequestInput, to request: FileRequest?, newLink: Bool, removePassword: Bool
    ) -> FileRequest {
        let id = request?.id ?? "fr-\(UUID().uuidString.prefix(8).lowercased())"
        let link = (request?.link == nil || newLink) ? "https://vault.example.com/#/file-request/\(id)/n3w" : request?.link
        return FileRequest(
            id: id, label: input.label.isEmpty ? nil : input.label,
            title: input.title, note: input.note, owner: input.owner, link: link,
            passwordSet: removePassword ? false : (input.password != nil || (request?.passwordSet ?? false)),
            expirationDate: input.expiresInDays.map { days(Double($0)) } ?? request?.expirationDate ?? days(7),
            deletionDate: request?.deletionDate ?? days(37), maxSubmissions: input.maxSubmissions,
            submissionCount: request?.submissionCount ?? 0, maxFiles: input.maxFiles,
            maxFileBytes: input.maxFileMib.map { $0 * 1024 * 1024 }, textAllowed: input.textAllowed,
            sendDomainId: input.sendDomainId, disabled: input.disabled, unseen: request?.unseen ?? 0,
            bytes: request?.bytes ?? 0, foreignKey: newLink ? false : (request?.foreignKey ?? false))
    }
}
#endif
