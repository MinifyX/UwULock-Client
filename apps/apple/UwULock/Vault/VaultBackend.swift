import Foundation

/// The core's commands the vault's screens use. `Core` answers them; in Debug builds
/// `-UwULockVaultFixtures YES` puts made-up items behind them instead (screenshots, previews).
@MainActor
protocol VaultBackend: AnyObject {
    func events() -> AsyncStream<CoreEvent>
    func syncNow() async throws -> Status
    func vaultOverview() async throws -> Overview
    func vaultItems() async throws -> [ItemSummary]
    func vaultItem(id: String) async throws -> ItemDetail
    func uwuStatus() async throws -> UwuStatus
    func verifyReprompt(id: String, password: String) async throws
    func revealField(id: String, field: String) async throws -> String
    func copyField(id: String, field: String) async throws
    func copyGenerated(text: String) async throws
    func copyText(text: String) async throws
    func totpCode(id: String) async throws -> TotpCode
    func itemPasskeys(id: String) async throws -> [PasskeyInfo]
    func deletePasskey(id: String, index: Int, credentialId: String?) async throws
    func saveItem(id: String?, draft: Draft) async throws -> String
    func setFavorite(id: String, favorite: Bool) async throws
    func setItemFolder(id: String, folderId: String?) async throws
    func deleteItem(id: String, permanent: Bool) async throws
    func restoreItem(id: String) async throws
    func saveFolder(id: String?, name: String) async throws -> String
    func deleteFolder(id: String) async throws
    func generatePassword(options: GeneratorOptions) async throws -> Generated
    func itemIcons(ids: [String], automatic: Bool) async throws -> [String: String]
    func openItemUri(id: String, index: Int) async throws
    func openWebVault() async throws
    func openWebVaultAt(place: WebVaultPlace, id: String?) async throws
    func maskedConnection() async throws -> MaskedConnection
    func createMaskedAddress(forDomain: String?, description: String?, cipherId: String?)
        async throws -> MaskedAddress
    func linkMaskedAddress(id: String, cipherId: String) async throws -> MaskedAddress
    func setMaskedState(id: String, state: MaskedState) async throws -> MaskedAddress
}

extension Core: VaultBackend {}

#if DEBUG
/// Made-up items for screenshots and previews: nothing leaves the app, nothing is stored.
@MainActor
final class FixtureVaultBackend: VaultBackend {
    static var requested: Bool {
        UserDefaults.standard.bool(forKey: "UwULockVaultFixtures")
    }

    private var listeners: [AsyncStream<CoreEvent>.Continuation] = []
    private var folders = [
        Folder(id: "f-work", name: "Arbeit"), Folder(id: "f-home", name: "Zuhause"),
    ]
    private var items: [ItemSummary]
    private var secrets: [String: [String: String]]

    init() {
        func item(
            _ id: String, _ kind: ItemKind, _ name: String, subtitle: String? = nil,
            host: String? = nil, favorite: Bool = false, folder: String? = nil,
            totp: Bool = false, password: Bool = false, username: Bool = false,
            reprompt: Bool = false, org: String? = nil, deleted: Bool = false
        ) -> ItemSummary {
            ItemSummary(
                id: id, kind: kind, name: name, subtitle: subtitle, host: host, favorite: favorite,
                folderId: folder, organizationId: org, collectionIds: org == nil ? [] : ["c-team"],
                deleted: deleted, reprompt: reprompt, viewPassword: true, hasTotp: totp,
                hasPassword: password, hasUsername: username, broken: false,
                revisionDate: "2026-10-28T09:41:00Z")
        }
        items = [
            item(
                "i-mail", .login, "Mailbox", subtitle: "nyu@example.com", host: "mail.example.com",
                favorite: true, folder: "f-home", totp: true, password: true, username: true),
            item(
                "i-shop", .login, "Shop", subtitle: "nyu", host: "shop.example.net", password: true,
                username: true),
            item(
                "i-git", .login, "Code-Hosting", subtitle: "nyu-dev", host: "git.example.org",
                folder: "f-work", totp: true, password: true, username: true),
            item(
                "i-bank", .login, "Bank", subtitle: "123456", host: "bank.example.com",
                password: true, username: true, reprompt: true),
            item("i-card", .card, "Reisekarte", subtitle: "Visa, *4242", favorite: true),
            item("i-id", .identity, "Nyu Example", subtitle: "Nyu Example", folder: "f-home"),
            item("i-note", .note, "WLAN-Router", subtitle: nil, folder: "f-home"),
            item("i-ssh", .sshKey, "Server-Schlüssel", subtitle: "SHA256:uwu…", folder: "f-work"),
            item("i-wifi", .wifi, "Wohnzimmer", subtitle: "UwU-Net"),
            item(
                "i-team", .login, "Team-Wiki", subtitle: "nyu@example.com",
                host: "wiki.example.org", password: true, username: true, org: "o-team"),
            item("i-old", .login, "Altes Forum", host: "forum.example.net", deleted: true),
        ]
        secrets = [
            "i-mail": [
                "password": "Nyu-7#paws-Sunny42", "username": "nyu@example.com",
                "history:0": "old-Paws-2024!",
            ],
            "i-shop": ["password": "kitten&Yarn-1999"],
            "i-git": ["password": "tr0ub4dor&3-uwu"],
            "i-bank": ["password": "Sehr-Geheim-1234"],
            "i-card": ["card-number": "4242 4242 4242 4242", "card-code": "123"],
            "i-ssh": ["ssh-private": "-----BEGIN OPENSSH PRIVATE KEY-----\nuwu\n-----END OPENSSH PRIVATE KEY-----"],
            "i-wifi": ["field:2": "Katzen-Netz-42"],
            "i-team": ["password": "Team-Pfote-77"],
        ]
    }

    func events() -> AsyncStream<CoreEvent> {
        let (stream, continuation) = AsyncStream<CoreEvent>.makeStream()
        listeners.append(continuation)
        return stream
    }

    private func changed() {
        for listener in listeners { listener.yield(.vaultChanged) }
    }

    func syncNow() async throws -> Status {
        try await Task.sleep(for: .milliseconds(600))
        changed()
        return Status(
            state: .unlocked, accountId: "a-1", label: "Nyu", email: "nyu@example.com",
            name: "Nyu", server: "vault.example.com", serverKind: .selfHosted, serverUrl: nil,
            lastSync: Int(Date().timeIntervalSince1970), syncing: false, syncError: nil,
            sessionExpired: false, live: nil, hello: nil, helloKind: nil, accounts: [])
    }

    func vaultOverview() async throws -> Overview {
        Overview(
            folders: folders, collections: [VaultCollection(id: "c-team", organizationId: "o-team", name: "Gemeinsam")],
            organizations: [Organization(id: "o-team", name: "Familie Example")], skipped: 0)
    }

    func vaultItems() async throws -> [ItemSummary] { items }

    func vaultItem(id: String) async throws -> ItemDetail {
        guard let summary = items.first(where: { $0.id == id }) else {
            throw CoreFailure(kind: "not-found", message: "Den Eintrag gibt es nicht mehr.")
        }
        var detail = ItemDetail(
            summary: summary, locked: false, notes: nil, login: nil, card: nil, identity: nil,
            sshKey: nil, fields: [], passwordHistory: [], attachments: 0,
            creationDate: "2025-03-14T15:09:00Z")
        switch summary.kind {
        case .login:
            detail.login = LoginDetail(
                username: summary.subtitle, hasPassword: summary.hasPassword,
                hasTotp: summary.hasTotp, passwordRevisionDate: "2026-09-01T08:00:00Z",
                uris: [
                    LoginUri(uri: "https://\(summary.host ?? "example.com")/login", match: nil, host: summary.host, openable: true),
                    LoginUri(uri: "https://app.example.com", match: 1, host: "app.example.com", openable: true),
                ],
                passkeys: id == "i-mail" ? 1 : 0)
            if id == "i-mail" {
                detail.notes = "Wiederherstellungscodes liegen im Tresor-Ordner „Zuhause“."
                detail.fields = [
                    FieldDetail(index: 0, name: "Kundennummer", kind: .text, value: "UWU-0042", hasValue: true),
                    FieldDetail(index: 1, name: "PIN", kind: .hidden, value: nil, hasValue: true),
                    FieldDetail(index: 2, name: "Newsletter", kind: .boolean, value: "true", hasValue: true),
                ]
                detail.passwordHistory = [PasswordHistoryEntry(index: 0, lastUsed: "2026-02-11T10:00:00Z")]
            }
            detail.locked = summary.reprompt && !unlocked.contains(id)
        case .card:
            detail.card = CardDetail(
                cardholderName: "Nyu Example", brand: "Visa", numberEnding: "4242", expMonth: "7",
                expYear: "2029", hasCode: true)
        case .identity:
            detail.identity = IdentityFields.all.map {
                IdentityField(name: $0.name, sensitive: $0.sensitive, value: fixtureIdentity[$0.name])
            }.filter { $0.value != nil }
        case .note:
            detail.notes = "Admin-Oberfläche: http://192.0.2.1\nBenutzer: admin"
        case .sshKey:
            detail.sshKey = SshKeyDetail(
                publicKey: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIUwU nyu@example.com",
                fingerprint: "SHA256:uwuNyuPawsAndWhiskers0123456789", hasPrivateKey: true)
        case .wifi:
            detail.fields = [
                FieldDetail(index: 0, name: "uwulock:type", kind: .text, value: "wifi", hasValue: true),
                FieldDetail(index: 1, name: "SSID", kind: .text, value: "UwU-Net", hasValue: true),
                FieldDetail(index: 2, name: "Password", kind: .hidden, value: nil, hasValue: true),
                FieldDetail(index: 3, name: "Security", kind: .text, value: "WPA2/WPA3", hasValue: true),
                FieldDetail(index: 4, name: "Hidden network", kind: .boolean, value: "false", hasValue: true),
            ]
        }
        return detail
    }

    private let fixtureIdentity = [
        "title": "Dr.", "firstName": "Nyu", "lastName": "Example", "email": "nyu@example.com",
        "phone": "+49 30 1234567", "city": "Berlin", "country": "DE", "passportNumber": "C01X00T47",
    ]
    private var unlocked: Set<String> = []

    func uwuStatus() async throws -> UwuStatus {
        var status = UwuStatus.empty
        status.uwu = true
        status.features = [UwuFeature.maskedAddresses.rawValue, UwuFeature.reminders.rawValue, UwuFeature.sends.rawValue]
        status.reminders = ["i-shop": ReminderMark(due: "2026-10-01", everyMonths: 12, isDue: true)]
        status.masked = ["i-shop": MaskedLink(id: "m-1", email: "fuzzy.paws@masked.example")]
        return status
    }

    func verifyReprompt(id: String, password: String) async throws {
        guard !password.isEmpty else {
            throw CoreFailure(kind: "wrong-password", message: "Das Master-Passwort stimmt nicht.")
        }
        unlocked.insert(id)
    }

    func revealField(id: String, field: String) async throws -> String {
        if let value = secrets[id]?[field] { return value }
        if field == "totp" { return "JBSWY3DPEHPK3PXP" }
        if field.hasPrefix("field:") { return "4711" }
        if field.hasPrefix("identity:") { return "C01X00T47" }
        return "uwu-secret"
    }

    func copyField(id: String, field: String) async throws {}
    func copyGenerated(text: String) async throws {}
    func copyText(text: String) async throws {}

    func totpCode(id: String) async throws -> TotpCode {
        let now = Int(Date().timeIntervalSince1970)
        let remaining = 30 - now % 30
        let seed = (now / 30) &* 7919 % 1_000_000
        return TotpCode(
            code: String(format: "%06d", seed), remaining: remaining, period: 30,
            next: String(format: "%06d", (seed &+ 7919) % 1_000_000), showNext: remaining <= 5)
    }

    func itemPasskeys(id: String) async throws -> [PasskeyInfo] {
        guard id == "i-mail" else { return [] }
        return [
            PasskeyInfo(
                index: 0, readable: true, credentialId: "cred-1", fingerprint: "fp", rpId: "mail.example.com",
                rpName: "Example Mail", userName: "nyu@example.com", userDisplayName: "Nyu",
                creationDate: "2026-05-04T12:00:00Z", discoverable: true)
        ]
    }

    func deletePasskey(id: String, index: Int, credentialId: String?) async throws {}

    func saveItem(id: String?, draft: Draft) async throws -> String {
        let newId = id ?? "i-\(UUID().uuidString.prefix(6))"
        let kind: ItemKind = draft.fields.contains { $0.name == "uwulock:type" } ? .wifi : ItemKind(rawValue: draft.kind.rawValue) ?? .note
        let summary = ItemSummary(
            id: newId, kind: kind, name: draft.name, subtitle: draft.login?.username, host: draft.login?.uris.first.flatMap { URL(string: $0.uri)?.host() },
            favorite: draft.favorite, folderId: draft.folderId, organizationId: nil, collectionIds: [],
            deleted: false, reprompt: draft.reprompt, viewPassword: true, hasTotp: !(draft.login?.totp ?? "").isEmpty,
            hasPassword: true, hasUsername: draft.login?.username != nil, broken: false,
            revisionDate: ISO8601DateFormatter().string(from: Date()))
        if let index = items.firstIndex(where: { $0.id == newId }) { items[index] = summary } else { items.append(summary) }
        changed()
        return newId
    }

    private func update(_ id: String, _ change: (inout ItemSummary) -> Void) {
        guard let index = items.firstIndex(where: { $0.id == id }) else { return }
        change(&items[index])
        changed()
    }

    func setFavorite(id: String, favorite: Bool) async throws { update(id) { $0.favorite = favorite } }
    func setItemFolder(id: String, folderId: String?) async throws { update(id) { $0.folderId = folderId } }
    func deleteItem(id: String, permanent: Bool) async throws {
        if permanent {
            items.removeAll { $0.id == id }
            changed()
        } else {
            update(id) { $0.deleted = true }
        }
    }
    func restoreItem(id: String) async throws { update(id) { $0.deleted = false } }

    func saveFolder(id: String?, name: String) async throws -> String {
        if let id, let index = folders.firstIndex(where: { $0.id == id }) {
            folders[index].name = name
            changed()
            return id
        }
        let folder = Folder(id: "f-\(UUID().uuidString.prefix(6))", name: name)
        folders.append(folder)
        changed()
        return folder.id
    }

    func deleteFolder(id: String) async throws {
        folders.removeAll { $0.id == id }
        for index in items.indices where items[index].folderId == id { items[index].folderId = nil }
        changed()
    }

    func generatePassword(options: GeneratorOptions) async throws -> Generated {
        var sets: [String] = []
        if options.lowercase { sets.append("abcdefghijkmnopqrstuvwxyz") }
        if options.uppercase { sets.append("ABCDEFGHJKLMNPQRSTUVWXYZ") }
        if options.digits { sets.append("23456789") }
        if options.symbols { sets.append("!@#$%^&*") }
        if sets.isEmpty { sets = ["abcdefghijkmnopqrstuvwxyz"] }
        let all = sets.joined()
        let length = max(5, min(128, options.length))
        let password = String((0..<length).map { index in
            let set = index < sets.count ? sets[index] : all
            return set.randomElement()!
        }.shuffled())
        let bits = Int(Double(length) * log2(Double(all.count)))
        return Generated(password: password, bits: bits, length: length, required: sets.count)
    }

    func itemIcons(ids: [String], automatic: Bool) async throws -> [String: String] { [:] }
    func openItemUri(id: String, index: Int) async throws {}
    func openWebVault() async throws {}
    func openWebVaultAt(place: WebVaultPlace, id: String?) async throws {}

    func maskedConnection() async throws -> MaskedConnection {
        MaskedConnection(
            connected: true, server: "masked.example", username: "nyu", domains: ["masked.example"],
            defaultDomain: "masked.example", status: "ok")
    }

    func createMaskedAddress(forDomain: String?, description: String?, cipherId: String?)
        async throws -> MaskedAddress
    {
        MaskedAddress(
            id: "m-\(UUID().uuidString.prefix(4))", email: "sleepy.whiskers@masked.example",
            state: "enabled", forDomain: forDomain, description: description, createdAt: nil,
            lastMessageAt: nil, cipherId: cipherId, itemName: nil)
    }

    func linkMaskedAddress(id: String, cipherId: String) async throws -> MaskedAddress {
        try await createMaskedAddress(forDomain: nil, description: nil, cipherId: cipherId)
    }

    func setMaskedState(id: String, state: MaskedState) async throws -> MaskedAddress {
        try await createMaskedAddress(forDomain: nil, description: nil, cipherId: nil)
    }
}
#endif
