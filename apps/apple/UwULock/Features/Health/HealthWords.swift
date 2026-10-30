// The password check's words (lib/health.ts): problem titles and details, the progress line,
// the groups of the report (mobile/pages/Check.tsx `groupInfo`) and the score.
import SwiftUI
import UwUDesign

// MARK: Problems

extension ProblemKind {
    /// `problemTitle` of lib/health.ts.
    var healthTitle: String {
        switch self {
        case .breached: featureString("Passwort in Datenlecks")
        case .siteBreach: featureString("Datenleck nach deiner letzten Passwortänderung")
        case .reused: featureString("Mehrfach benutzt")
        case .weak: featureString("Schwach")
        case .unsecured: featureString("Ohne https")
        case .twofa: featureString("2FA möglich, nicht eingerichtet")
        }
    }

    /// Whether a new password solves the problem (review.ts `aboutThePassword`).
    var isAboutThePassword: Bool {
        switch self {
        case .breached, .siteBreach, .reused, .weak: true
        case .unsecured, .twofa: false
        }
    }
}

enum HealthWords {
    private static let sourceNames = ["hibp": "Have I Been Pwned", "xon": "XposedOrNot"]

    /// "Have I Been Pwned, XposedOrNot" for the sources' ids.
    static func sourceNames(_ sources: [String]) -> String {
        sources.map { sourceNames[$0] ?? $0 }.joined(separator: ", ")
    }

    /// "Seen 1,234 times".
    static func seen(_ count: Int) -> String {
        let number = count.formatted()
        return featureString("\(number) Mal gesehen")
    }

    /// "Example Site, 3 Mar 2026 – Have I Been Pwned".
    static func breachText(_ breach: SiteBreach) -> String {
        let date = breach.date.flatMap(FeatureDate.parse)?
            .formatted(date: .abbreviated, time: .omitted) ?? "?"
        let site = breach.title
        let sources = sourceNames(breach.sources.keys.sorted())
        return featureString("\(site), \(date) – \(sources)")
    }

    /// `problemDetail` of lib/health.ts.
    static func detail(_ problem: HealthProblem) -> String {
        switch problem {
        case .breached(let count, let sources):
            return ([seen(count)] + (sources.isEmpty ? [] : [sourceNames(sources)]))
                .joined(separator: " · ")
        case .siteBreach(let breach):
            return breachText(breach)
        case .reused(let others):
            return featureString("noch \(others) Mal im Tresor")
        case .weak(let bits):
            return featureString("\(bits) Bit")
        case .unsecured:
            return featureString("Die Adresse beginnt mit http://")
        case .twofa:
            return featureString("Die Website bietet Einmal-Codes aus einer Authenticator-App an.")
        }
    }

    /// `progressText` of lib/health.ts: per source when the event has them, else in all.
    static func progress(_ progress: CheckProgress) -> String {
        let parts: [String] = [("hibp", progress.hibp), ("xon", progress.xon)].compactMap { key, source in
            guard let source else { return nil }
            let name = sourceNames[key] ?? key
            let done = source.done
            let total = source.total
            let count = featureString("\(name) \(done) von \(total)")
            return source.waiting > 0 ? featureString("\(count), wartet auf den Server") : count
        }
        if parts.isEmpty {
            let done = progress.done
            let total = progress.total
            return featureString("Fragt nach Datenlecks … \(done) von \(total)")
        }
        let sources = parts.joined(separator: " · ")
        return featureString("Fragt nach Datenlecks … \(sources)")
    }

    /// "Name (License), Name" for the lists of breaches.
    static func sourcesWithLicenses(_ sources: [HealthSource]) -> String {
        sources.map { source in
            source.license.map { "\(source.name) (\($0))" } ?? source.name
        }.joined(separator: ", ")
    }

    /// A login's name, or "(ohne Namen)".
    static func name(_ name: String) -> String {
        name.isEmpty ? featureString("(ohne Namen)") : name
    }

    /// What an address's check said.
    static func emailResult(_ result: EmailResult) -> String {
        switch result.status {
        case "found": result.breaches.joined(separator: ", ")
        case "clean": featureString("In keinem bekannten Datenleck")
        case "later": featureString("Später – das Kontingent des Servers ist gerade aufgebraucht")
        default: featureString("XposedOrNot hat nicht geantwortet")
        }
    }
}

// MARK: Groups

/// A group of the report (mobile/pages/Check.tsx `FindingGroup`).
enum HealthGroup: String, CaseIterable, Identifiable, Hashable {
    case breached, siteBreach, reused, weak, unsecured, twofa

    var id: String { rawValue }

    var kind: ProblemKind {
        switch self {
        case .breached: .breached
        case .siteBreach: .siteBreach
        case .reused: .reused
        case .weak: .weak
        case .unsecured: .unsecured
        case .twofa: .twofa
        }
    }

    var title: LocalizedStringKey {
        switch self {
        case .breached: "In Datenlecks"
        case .siteBreach: "Datenleck nach deiner letzten Passwortänderung"
        case .reused: "Mehrfach benutzt"
        case .weak: "Schwach"
        case .unsecured: "Ohne https"
        case .twofa: "2FA möglich, nicht eingerichtet"
        }
    }

    var lead: LocalizedStringKey {
        switch self {
        case .breached: "Diese Passwörter tauchen in bekannten Datenlecks auf. Ändere sie zuerst."
        case .siteBreach:
            "Bei diesen Websites wurden Passwörter gestohlen, nachdem du deines zuletzt geändert hast. Ändere es dort."
        case .reused: "Wird eines davon bekannt, sind die anderen Konten mit offen."
        case .weak: "Zu kurz oder zu leicht zu erraten."
        case .unsecured: "Die Adresse beginnt mit http://: Das Passwort geht unverschlüsselt über das Netz."
        case .twofa:
            "Diese Websites bieten Einmal-Codes aus einer Authenticator-App an, im Eintrag ist aber keiner hinterlegt. Richte die Zwei-Schritt-Anmeldung dort ein und trag den Schlüssel im Eintrag ein."
        }
    }

    var icon: UwUIconName {
        switch self {
        case .breached: .warning
        case .siteBreach: .error
        case .reused: .repeat
        case .weak: .score
        case .unsecured: .unlocked
        case .twofa: .oneTimeCode
        }
    }

    var tone: Color {
        switch self {
        case .breached, .siteBreach: .uwuDangerInk
        case .reused, .weak: .uwuWarningInk
        case .unsecured, .twofa: .uwuMuted
        }
    }

    var tint: Color {
        switch self {
        case .breached, .siteBreach: .uwuDangerTint
        case .reused, .weak: .uwuWarningTint
        case .unsecured, .twofa: .uwuPinkTint
        }
    }
}

/// One login in a group of the report.
struct HealthEntry: Identifiable, Hashable {
    var id: String
    var name: String
    /// The line under the name (subtitle and the problem's detail).
    var detail: String
    /// Where 2FA is documented (the 2FA group only).
    var documentation: URL?
}

/// One ignored hint.
struct HealthIgnoredEntry: Identifiable, Hashable {
    var itemId: String
    var kind: ProblemKind
    var name: String

    var id: String { "\(itemId):\(kind.rawValue)" }
}

extension HealthView {
    /// Whether the server asks breach sources about passwords at all.
    var hasSources: Bool { switches.hibp || switches.xonPasswords }

    func isIgnored(_ itemId: String, _ kind: ProblemKind) -> Bool {
        ignored?.contains { $0.itemId == itemId && $0.kind == kind } ?? false
    }

    /// The groups the check can fill on this server (the others would always be empty).
    var shownGroups: [HealthGroup] {
        HealthGroup.allCases.filter { group in
            switch group {
            case .breached: hasSources
            case .siteBreach: switches.siteBreaches
            default: true
            }
        }
    }

    /// The logins of a group, without what was ignored (HealthPane.tsx's report).
    func entries(_ group: HealthGroup) -> [HealthEntry] {
        func line(_ finding: Finding, _ detail: String?) -> String {
            [finding.subtitle, detail].compactMap { $0?.isEmpty == false ? $0 : nil }
                .joined(separator: " · ")
        }
        func pick(_ test: (Finding) -> Bool, _ detail: (Finding) -> String?) -> [HealthEntry] {
            report.findings
                .filter { test($0) && !isIgnored($0.id, group.kind) }
                .map { HealthEntry(id: $0.id, name: $0.name, detail: line($0, detail($0))) }
        }
        switch group {
        case .breached:
            return pick({ ($0.breached ?? 0) > 0 }) { finding in
                let sources = finding.breachSources.isEmpty
                    ? nil : HealthWords.sourceNames(finding.breachSources)
                return [HealthWords.seen(finding.breached ?? 0), sources].compactMap { $0 }
                    .joined(separator: " · ")
            }
        case .siteBreach:
            return pick({ siteBreaches[$0.id] != nil }) { finding in
                siteBreaches[finding.id].map(HealthWords.breachText)
            }
        case .reused:
            return pick({ $0.reused > 0 }) { featureString("noch \($0.reused) Mal im Tresor") }
        case .weak:
            return pick({ $0.weak }) { featureString("\($0.bits) Bit") }
        case .unsecured:
            return pick({ $0.unsecured }) { _ in nil }
        case .twofa:
            return twofa.filter { !isIgnored($0.itemId, .twofa) }.map { missing in
                let other = missing.entry.name != missing.name ? missing.entry.name : nil
                return HealthEntry(
                    id: missing.itemId, name: missing.name,
                    detail: [missing.host, other].compactMap { $0 }.filter { !$0.isEmpty }
                        .joined(separator: " · "),
                    documentation: missing.entry.documentation.flatMap(URL.init(string:)))
            }
        }
    }

    /// The ignored hints of logins still in the report.
    var ignoredEntries: [HealthIgnoredEntry] {
        var names: [String: String] = [:]
        for finding in report.findings { names[finding.id] = finding.name }
        for missing in twofa { names[missing.itemId] = missing.name }
        return (ignored ?? []).compactMap { entry in
            names[entry.itemId].map { HealthIgnoredEntry(itemId: entry.itemId, kind: entry.kind, name: $0) }
        }
    }

    /// 0–100: the share of checked passwords without an open hint about the password itself.
    func score(_ groups: [HealthGroup: [HealthEntry]]) -> Int {
        guard report.checked > 0 else { return 100 }
        let about: [HealthGroup] = [.breached, .siteBreach, .reused, .weak, .unsecured]
        let affected = Set(about.flatMap { groups[$0] ?? [] }.map(\.id))
        return max(0, Int((100 * Double(report.checked - affected.count) / Double(report.checked)).rounded()))
    }
}

/// The score's colour: success from 80, warning from 50, else danger.
func healthScoreTone(_ score: Int) -> Color {
    score >= 80 ? .uwuSuccessInk : score >= 50 ? .uwuWarningInk : .uwuDangerInk
}

/// The headline under the score (Check.tsx `headline`).
func healthHeadline(score: Int, open: Int) -> String {
    guard open > 0 else { return featureString("Alles gut: nichts gefunden ✧") }
    let tier = score >= 80 ? featureString("Gut") : score >= 50 ? featureString("Geht so") : featureString("Ausbaufähig")
    return open == 1 ? featureString("\(tier), mit einer Baustelle") : featureString("\(tier), mit \(open) Baustellen")
}
