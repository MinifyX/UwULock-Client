#if DEBUG
import Foundation

/// Sample imports for screenshots and previews (`FeatureFixtures.isOn`).
/// `-UwULockImportFixture password|preview|running|done|partial` opens the flow on that step.
enum ImportFixtures {
    static let file = ImportFile(name: "Tresor.kdbx", bytes: [])

    static var parsed: ImportParsed {
        let folders = [
            ImportExportFolder(id: "f-work", name: "Arbeit"),
            ImportExportFolder(id: "f-home", name: "Zuhause"),
        ]
        func login(_ name: String, _ user: String, _ site: String, folder: String?, totp: Bool = false)
            -> ImportExportItem {
            let item = ImportExportItem(type: .login, name: name)
            item.folderId = folder
            item.login = ImportExportLogin(
                username: user, password: "correct-horse-battery", totp: totp ? "otpauth://totp/Nyu?secret=JBSWY3DPEHPK3PXP" : nil,
                uris: [ImportExportUri(uri: "https://\(site)", match: nil)])
            return item
        }
        let note = ImportExportItem(type: .note, name: "Router-Notizen")
        note.notes = "Admin-Oberfläche unter 192.0.2.1"
        note.secureNote = true
        note.folderId = "f-home"
        let card = ImportExportItem(type: .card, name: "Reisekarte")
        card.card = ImportExportCard(cardholderName: "Nyu Neko", brand: "Visa", number: "4111111111111111",
                                     expMonth: "3", expYear: "2029", code: "123")
        let person = ImportExportItem(type: .identity, name: "Ich")
        var identity = ImportExportIdentity()
        identity.firstName = "Nyu"
        identity.lastName = "Neko"
        identity.email = "nyu@example.com"
        person.identity = identity
        let items = [
            login("Mail", "nyu@example.com", "mail.example.com", folder: "f-work", totp: true),
            login("Wiki", "nyu", "wiki.example.org", folder: "f-work"),
            login("Forum", "nyu-neko", "forum.example.net", folder: nil),
            login("Router", "admin", "router.home.test", folder: "f-home"),
            login("Shop", "nyu@example.org", "shop.example.com", folder: nil, totp: true),
            login("Cloud", "nyu@example.net", "cloud.example.net", folder: "f-work"),
            login("Kalender", "nyu", "calendar.example.org", folder: "f-work"),
            login("Spiele", "nyu", "games.example", folder: nil),
            note, card, person,
            login("Bank", "nyu-neko", "bank.example.com", folder: "f-home", totp: true),
        ]
        let data = ImportExportData(folders: folders, items: items)
        return ImportParsed(
            source: .keepass, format: "KDBX 4.1", data: data,
            warnings: [featureString("2 Einträge aus dem Papierkorb oder den Vorlagen bleiben weg.")],
            submitFormat: .json, submitText: data.jsonText())
    }

    static let outcome = ImportOutcome(
        imported: 11, foldersCreated: 2,
        skipped: [SkippedItem(name: "Alter Schlüssel", reason: .invalid)], error: nil)

    static let partial = ImportOutcome(
        imported: 7, foldersCreated: 1,
        skipped: [SkippedItem(name: "Lange Notiz", reason: .tooLong)],
        error: CoreFailure(kind: "network", message: "Der Server antwortet nicht."))

    /// The step `-UwULockImportFixture` asks for.
    @MainActor static var requested: ImportStep? {
        guard FeatureFixtures.isOn else { return nil }
        switch UserDefaults.standard.string(forKey: "UwULockImportFixture") {
        case "password": return .password(file: file, kind: .keepass)
        case "preview": return .preview(parsed)
        case "running": return .running(parsed: parsed, progress: ImportProgress(done: 7, total: 12))
        case "done": return .done(outcome)
        case "partial": return .done(partial)
        default: return nil
        }
    }
}
#endif
