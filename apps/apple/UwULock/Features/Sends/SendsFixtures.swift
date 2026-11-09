#if DEBUG
// Sample Sends and an item to share, for screenshots and previews (`-UwULockFixtures YES`).
import Foundation

enum SendsFixtures {
    private static func date(_ days: Double) -> String {
        FeatureDate.iso(Date.now.addingTimeInterval(days * 86_400))
    }

    static let sends: [Send] = [
        Send(
            id: "s1", kind: .text, name: "WLAN für Gäste", notes: "Für den Besuch am Wochenende",
            text: "Netz: Gartenhaus\nPasswort: kirschblüte-42", hidden: true, fileName: nil, size: nil,
            maxAccessCount: 5, accessCount: 2, hasPassword: false, authType: .anyone, emails: [],
            disabled: false, hideEmail: false, revisionDate: date(-0.1), expirationDate: nil,
            deletionDate: date(6.8), entry: false,
            link: "https://vault.example.com/#/send/s1/k3y-sample", sendDomainId: nil),
        Send(
            id: "s2", kind: .file, name: "Mietvertrag.pdf", notes: nil, text: nil, hidden: false,
            fileName: "Mietvertrag.pdf", size: 2_481_152, maxAccessCount: nil, accessCount: 1,
            hasPassword: true, authType: .password, emails: [], disabled: false, hideEmail: true,
            revisionDate: date(-1), expirationDate: date(2), deletionDate: date(13), entry: false,
            link: "https://send.example.com/#/send/s2/k3y-sample", sendDomainId: "d1"),
        Send(
            id: "s3", kind: .text, name: "Server-Zugang", notes: nil,
            text: "Benutzername: mika\nWebsite: https://admin.example.net", hidden: true,
            fileName: nil, size: nil, maxAccessCount: 1, accessCount: 0, hasPassword: false,
            authType: .emails, emails: ["mika@example.org"], disabled: false, hideEmail: false,
            revisionDate: date(-2), expirationDate: nil, deletionDate: date(1), entry: true,
            link: "https://vault.example.com/#/send/s3/k3y-sample", sendDomainId: nil),
        Send(
            id: "s4", kind: .text, name: "Altes Rezept", notes: nil, text: "Zwei Eier, etwas Mehl …",
            hidden: false, fileName: nil, size: nil, maxAccessCount: 3, accessCount: 3,
            hasPassword: false, authType: .anyone, emails: [], disabled: false, hideEmail: false,
            revisionDate: date(-5), expirationDate: nil, deletionDate: date(2), entry: false,
            link: "https://vault.example.com/#/send/s4/k3y-sample", sendDomainId: nil),
        Send(
            id: "s5", kind: .file, name: "Urlaubsfotos.zip", notes: nil, text: nil, hidden: false,
            fileName: "Urlaubsfotos.zip", size: 48_200_000, maxAccessCount: nil, accessCount: 0,
            hasPassword: false, authType: .anyone, emails: [], disabled: true, hideEmail: false,
            revisionDate: date(-8), expirationDate: nil, deletionDate: date(20), entry: false,
            link: "https://vault.example.com/#/send/s5/k3y-sample", sendDomainId: nil),
    ]

    static let options = SendOptions(
        emails: true, domains: [SendDomain(id: "d1", url: "https://send.example.com")],
        defaultDomainId: nil)

    static let item = ItemDetail(
        summary: ItemSummary(
            id: "i1", kind: .login, name: "Mailkonto", subtitle: "mika@example.org",
            host: "mail.example.com", favorite: false, folderId: nil, organizationId: nil,
            collectionIds: [], deleted: false, reprompt: false, viewPassword: true, hasTotp: true,
            hasPassword: true, hasUsername: true, broken: false, revisionDate: nil),
        locked: false, notes: "Wiederherstellungscodes liegen im Safe.",
        login: LoginDetail(
            username: "mika@example.org", hasPassword: true, hasTotp: true,
            passwordRevisionDate: nil,
            uris: [
                LoginUri(uri: "https://mail.example.com", match: nil, host: "mail.example.com", openable: true),
                LoginUri(uri: "https://webmail.example.net", match: nil, host: "webmail.example.net", openable: true),
            ],
            passkeys: 0),
        card: nil, identity: nil, sshKey: nil,
        fields: [FieldDetail(index: 0, name: "PIN", kind: .hidden, value: nil, hasValue: true)],
        passwordHistory: nil, attachments: nil, creationDate: nil)

    /// Saves into the samples; answers the id.
    static func save(
        into sends: inout [Send]?, id: String?, draft: SendDraft, size: Int?, domain: SendLinkDomain
    ) -> String {
        var list = sends ?? []
        let domainId: String? = switch domain {
        case .unchanged: list.first { $0.id == id }?.sendDomainId
        case .server: nil
        case .domain(let chosen): chosen
        }
        let newId = id ?? "s\(Int.random(in: 100...999))"
        let old = list.first { $0.id == newId }
        let send = Send(
            id: newId, kind: draft.kind, name: draft.name, notes: draft.notes,
            text: old?.entry == true ? old?.text : draft.text, hidden: draft.hidden,
            fileName: draft.fileName, size: size ?? old?.size,
            maxAccessCount: draft.maxAccessCount, accessCount: old?.accessCount ?? 0,
            hasPassword: draft.authType == .password && (draft.password != nil || old?.hasPassword == true),
            authType: draft.authType, emails: draft.emails, disabled: draft.disabled,
            hideEmail: draft.hideEmail, revisionDate: FeatureDate.iso(.now),
            expirationDate: draft.expirationDate, deletionDate: draft.deletionDate,
            entry: old?.entry ?? false,
            link: "https://\(domainId == nil ? "vault" : "send").example.com/#/send/\(newId)/k3y-sample",
            sendDomainId: domainId)
        list.removeAll { $0.id == newId }
        list.insert(send, at: 0)
        sends = list
        return newId
    }
}
#endif
