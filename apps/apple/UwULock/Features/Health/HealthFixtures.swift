#if DEBUG
// Sample data of the password check, for screenshots (`-UwULockFixtures YES`) and previews.
import Foundation

enum HealthFixtures {
    static let switches = BreachSwitches(
        hibp: true, xonPasswords: true, siteBreaches: true, emailCheck: true, changePassword: true)

    static let findings: [Finding] = [
        Finding(
            id: "f-shop", name: "Example Shop", subtitle: "nyu@example.com", bits: 28, weak: true,
            reused: 2, unsecured: false, breached: 12_408, breachSources: ["hibp", "xon"],
            host: "shop.example.com", uri: "https://shop.example.com/login",
            passwordChanged: "2023-02-11T09:12:00Z"),
        Finding(
            id: "f-forum", name: "Example Forum", subtitle: "nyu", bits: 44, weak: false,
            reused: 2, unsecured: true, breached: 0, breachSources: [],
            host: "forum.example.org", uri: "http://forum.example.org/",
            passwordChanged: nil),
        Finding(
            id: "f-mail", name: "Mail", subtitle: "nyu@example.net", bits: 52, weak: false,
            reused: 2, unsecured: false, breached: 0, breachSources: [],
            host: "mail.example.net", uri: "https://mail.example.net/",
            passwordChanged: "2024-06-01T10:00:00Z"),
        Finding(
            id: "f-router", name: "Router", subtitle: "admin", bits: 19, weak: true,
            reused: 0, unsecured: false, breached: 3, breachSources: ["hibp"],
            host: nil, uri: nil, passwordChanged: nil),
        Finding(
            id: "f-cloud", name: "Cloud Drive", subtitle: "nyu@example.com", bits: 96, weak: false,
            reused: 0, unsecured: false, breached: 0, breachSources: [],
            host: "drive.example.com", uri: "https://drive.example.com/",
            passwordChanged: "2022-09-20T08:00:00Z"),
    ]

    static let siteBreach = SiteBreach(
        domain: "drive.example.com", title: "Example Drive", date: "2025-11-04",
        added: "2025-12-01T00:00:00Z", records: 2_400_000, passwords: true,
        dataClasses: ["Email addresses", "Passwords"], sources: ["hibp": "ExampleDrive"])

    static let twofa = MissingTwofa(
        itemId: "f-mail", name: "Mail", host: "mail.example.net",
        entry: TwofaEntry(
            domain: "example.net", name: "Example Mail", methods: ["totp"],
            documentation: "https://help.example.net/2fa"))

    static var view: HealthView {
        let byId = Dictionary(uniqueKeysWithValues: findings.map { ($0.id, $0) })
        return HealthView(
            uwu: true, switches: switches,
            report: HealthReport(findings: findings, checked: 42, breachesChecked: true, breachesIncomplete: false),
            checkedAt: "2026-10-29T18:30:00Z",
            siteBreaches: ["f-cloud": siteBreach],
            siteSources: [HealthSource(id: "hibp", name: "Have I Been Pwned", url: "https://haveibeenpwned.com", license: "CC BY 4.0")],
            sitesFailed: false,
            twofa: [twofa],
            twofaSource: HealthSource(id: "2fa", name: "2FA Directory", url: "https://2fa.directory", license: "MIT"),
            twofaFailed: false,
            ignored: [IgnoredEntry(itemId: "f-router", kind: .breached, since: "2026-10-01T12:00:00Z")],
            cards: [
                HealthCard(finding: byId["f-shop"]!, problems: [
                    .breached(count: 12_408, sources: ["hibp", "xon"]), .reused(others: 2), .weak(bits: 28),
                ]),
                HealthCard(finding: byId["f-cloud"]!, problems: [.siteBreach(siteBreach)]),
                HealthCard(finding: byId["f-forum"]!, problems: [.reused(others: 2), .unsecured]),
                HealthCard(finding: byId["f-mail"]!, problems: [
                    .reused(others: 2), .twofa(documentation: "https://help.example.net/2fa"),
                ]),
                HealthCard(finding: byId["f-router"]!, problems: [
                    .breached(count: 3, sources: ["hibp"]), .weak(bits: 19),
                ]),
            ],
            emailOptIn: EmailOptIn(optedIn: true, since: "2026-09-01T00:00:00Z"))
    }

    static let emails: [EmailResult] = [
        EmailResult(email: "nyu@example.com", status: "found", breaches: ["ExampleDrive", "ExampleShop"]),
        EmailResult(email: "nyu@example.net", status: "clean", breaches: []),
        EmailResult(email: "admin@example.org", status: "later", breaches: []),
    ]
}
#endif
