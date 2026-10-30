// Sends without views: the status a list shows, the dates the editor offers, the draft it
// saves and what an item can share (lib/sendModel.ts, components/ItemExtras.tsx `choices`).
import Foundation

// MARK: Availability

extension UwuStatus {
    /// Whether the server has Sends: every Bitwarden and Vaultwarden does (lib/sends.ts).
    var sendsAvailable: Bool { !uwu || has(.sends) }
}

// MARK: Status

/// Whether a Send opens right now, and until when.
enum SendsStatus: Equatable {
    case disabled
    case expired
    case usedUp
    case active(until: String?)

    init(_ send: Send, now: Date = .now) {
        if send.disabled {
            self = .disabled
        } else if let expires = FeatureDate.parse(send.expirationDate), expires < now {
            self = .expired
        } else if let max = send.maxAccessCount, max > 0, send.accessCount >= max {
            self = .usedUp
        } else {
            self = .active(until: send.expirationDate ?? send.deletionDate)
        }
    }

    var isActive: Bool {
        if case .active = self { return true }
        return false
    }
}

extension Send {
    var status: SendsStatus { SendsStatus(self) }

    /// Its name, or "(ohne Namen)".
    var shownName: String {
        name.isEmpty ? featureString("(ohne Namen)") : name
    }

    /// Whether it opens right now, in a few words.
    var statusText: String {
        switch status {
        case .disabled: featureString("Deaktiviert")
        case .expired: featureString("Abgelaufen")
        case .usedUp: featureString("So oft geöffnet, wie erlaubt")
        case .active(let until):
            featureString("Bis \(FeatureDate.short(until) ?? "")")
        }
    }

    /// "3 von 5 Mal geöffnet".
    var openedText: String {
        if let max = maxAccessCount, max > 0 {
            featureString("\(accessCount) von \(max) Mal geöffnet")
        } else {
            featureString("\(accessCount) Mal geöffnet")
        }
    }

    /// Who may open it, in words.
    var accessText: String {
        SendsAccessOption.label(authType)
    }
}

enum SendsAccessOption {
    static func label(_ access: SendAccess) -> String {
        switch access {
        case .anyone: featureString("Jeder mit dem Link")
        case .password: featureString("Mit Passwort")
        case .emails: featureString("Nur bestimmte Adressen")
        }
    }
}

// MARK: Days and addresses

enum SendsDays {
    /// The days the editor offers for deleting and expiring (a Send lives at most 31).
    static let all = [1, 2, 3, 7, 14, 30]

    /// "1 Tag", "7 Tage".
    static func label(_ days: Int) -> String {
        days == 1 ? featureString("1 Tag") : featureString("\(days) Tage")
    }

    /// In `days` days, as the server takes it.
    static func iso(in days: Int, now: Date = .now) -> String {
        FeatureDate.iso(now.addingTimeInterval(Double(days) * 86_400))
    }

    /// A date as the nearest of `all` from now; none: 7.
    static func until(_ text: String?, now: Date = .now) -> Int {
        guard let date = FeatureDate.parse(text) else { return 7 }
        let days = Int((date.timeIntervalSince(now) / 86_400).rounded())
        var best = 7
        for n in all where abs(n - days) < abs(best - days) {
            best = n
        }
        return best
    }
}

enum SendsAddresses {
    /// Addresses as somebody types them: separated by commas, spaces or lines.
    static func split(_ text: String) -> [String] {
        text.split(whereSeparator: { $0.isWhitespace || $0 == "," || $0 == ";" })
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
            .filter { !$0.isEmpty }
    }

    /// Whether an address can take the mailed code (uwulock-core's `looks_like_address`).
    static func looksValid(_ address: String) -> Bool {
        guard let at = address.firstIndex(of: "@"), at > address.startIndex else { return false }
        let domain = address[address.index(after: at)...]
        return domain.contains(".") && !domain.hasPrefix(".") && !domain.hasSuffix(".")
            && !address.contains(where: { $0.isWhitespace || $0 == "," })
    }

    static func allValid(_ addresses: [String]) -> Bool {
        !addresses.isEmpty && addresses.allSatisfy(looksValid)
    }
}

enum SendsFileLimit {
    /// Bitwarden's limit for a Send's file (`MAX_FILE` in sends.rs).
    static let bitwarden = 500 * 1024 * 1024

    /// The largest file a Send may carry: the server's limit when it names one, never more
    /// than Bitwarden's.
    static func bytes(_ serverMax: Int?) -> Int {
        if let serverMax, serverMax > 0 { return min(serverMax, bitwarden) }
        return bitwarden
    }
}

// MARK: The editor's form

/// The editor's fields, as it holds them (sendModel.ts `SendForm`).
struct SendsForm: Equatable {
    var name = ""
    var text = ""
    var hidden = false
    var notes = ""
    /// The picked file's name, for a new file Send.
    var fileName: String?
    var password = ""
    /// 0: unlimited.
    var maxAccess = 0
    /// One of `SendsDays.all`.
    var deletionDays = 7
    /// 0: never (until it is deleted).
    var expiresDays = 0
    var disabled = false
    var hideEmail = false
    var access: SendAccess = .anyone
    /// As typed.
    var emails = ""

    init() {}

    /// The form for a Send, or a new one.
    init(_ send: Send?, now: Date = .now) {
        name = send?.name ?? ""
        text = send?.text ?? ""
        hidden = send?.hidden ?? false
        notes = send?.notes ?? ""
        maxAccess = send?.maxAccessCount ?? 0
        deletionDays = SendsDays.until(send?.deletionDate, now: now)
        expiresDays = send?.expirationDate != nil ? SendsDays.until(send?.expirationDate, now: now) : 0
        disabled = send?.disabled ?? false
        hideEmail = send?.hideEmail ?? false
        access = send?.authType ?? .anyone
        emails = send?.emails.joined(separator: ", ") ?? ""
    }

    var addresses: [String] { SendsAddresses.split(emails) }

    /// Whether it can be saved: a name, the text (or for a new file Send the file), an address
    /// when only addresses may open it, a password when one is asked for and the Send has none.
    func ready(send: Send?, kind: SendKind) -> Bool {
        if name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return false }
        if kind == .text && !(send?.entry ?? false)
            && text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            return false
        }
        if kind == .file && send == nil && fileName == nil { return false }
        if access == .emails && !SendsAddresses.allValid(addresses) { return false }
        if access == .password && password.isEmpty && !(send?.hasPassword ?? false) { return false }
        return true
    }

    /// The draft the core seals. Expiring never comes after deleting.
    func draft(send: Send?, kind: SendKind, now: Date = .now) -> SendDraft {
        let trimmedNotes = notes.trimmingCharacters(in: .whitespacesAndNewlines)
        return SendDraft(
            kind: kind,
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            notes: trimmedNotes.isEmpty ? nil : trimmedNotes,
            text: kind == .text ? text : nil,
            hidden: hidden,
            fileName: fileName ?? send?.fileName,
            password: access == .password && !password.isEmpty ? password : nil,
            maxAccessCount: maxAccess > 0 ? min(maxAccess, 1_000_000) : nil,
            expirationDate: expiresDays > 0
                ? SendsDays.iso(in: min(expiresDays, deletionDays), now: now) : nil,
            deletionDate: SendsDays.iso(in: deletionDays, now: now),
            disabled: disabled,
            hideEmail: hideEmail,
            authType: access,
            emails: access == .emails ? addresses : [])
    }
}

/// Newest change first.
func sendsSorted(_ sends: [Send]) -> [Send] {
    sends.sorted { ($0.revisionDate ?? "") > ($1.revisionDate ?? "") }
}

/// A domain's address without its scheme.
func sendsDomainLabel(_ domain: SendDomain) -> String {
    var url = domain.url
    for scheme in ["https://", "http://"] where url.hasPrefix(scheme) {
        url.removeFirst(scheme.count)
    }
    return url
}

// MARK: What an item can share

/// One value of an item that can go into a Send.
struct SendsShareChoice: Identifiable, Equatable {
    /// `username`, `uri:0`, `field:2`, …
    var field: String
    /// What the recipient reads before the value.
    var label: String
    /// What the list shows, when that is more than the label (a website's address).
    var shown: String?
    var checked: Bool
    var secret: Bool

    var id: String { field }
}

enum SendsShareChoices {
    /// What an item has that can be shared — no secrets of an item whose organisation hides its
    /// passwords from this member. The one-time code only in an entry Send: never ticked by
    /// default, and ticking it asks once more.
    static func make(_ detail: ItemDetail, entry: Bool) -> [SendsShareChoice] {
        let viewPassword = detail.summary.viewPassword
        var out: [SendsShareChoice] = []
        func add(_ field: String, _ label: String, _ checked: Bool, secret: Bool = false, shown: String? = nil) {
            if !viewPassword && secret && !field.hasPrefix("identity:") { return }
            out.append(SendsShareChoice(field: field, label: label, shown: shown, checked: checked, secret: secret))
        }
        if let login = detail.login {
            if let username = login.username, !username.isEmpty { add("username", featureString("Benutzername"), true) }
            if login.hasPassword { add("password", featureString("Passwort"), true, secret: true) }
            if entry && login.hasTotp { add("totp", featureString("Einmal-Code"), false, secret: true) }
            let many = login.uris.count > 1
            for (index, uri) in login.uris.enumerated() {
                add(
                    "uri:\(index)",
                    many ? featureString("Website \(index + 1)") : featureString("Website"),
                    index == 0, shown: uri.uri)
            }
        }
        if let card = detail.card {
            if !(card.cardholderName ?? "").isEmpty { add("card-name", featureString("Karteninhaber"), true) }
            if !(card.numberEnding ?? "").isEmpty { add("card-number", featureString("Kartennummer"), true, secret: true) }
            if !(card.expMonth ?? "").isEmpty || !(card.expYear ?? "").isEmpty {
                add("card-expiry", featureString("Gültig bis"), true)
            }
            if card.hasCode { add("card-code", featureString("Prüfnummer"), false, secret: true) }
        }
        for field in detail.identity ?? [] {
            add("identity:\(field.name)", identityLabel(field.name), !field.sensitive, secret: field.sensitive)
        }
        if let key = detail.sshKey {
            if !(key.publicKey ?? "").isEmpty { add("ssh-public", featureString("Öffentlicher Schlüssel"), true) }
            if !(key.fingerprint ?? "").isEmpty { add("ssh-fingerprint", featureString("Fingerprint"), false) }
            if key.hasPrivateKey { add("ssh-private", featureString("Privater Schlüssel"), false, secret: true) }
        }
        for field in detail.fields ?? [] where field.kind != .linked && field.hasValue {
            let name = (field.name ?? "").isEmpty ? featureString("Feld \(field.index + 1)") : field.name!
            add("field:\(field.index)", name, false, secret: field.kind == .hidden)
        }
        if !(detail.notes ?? "").isEmpty {
            add("notes", featureString("Notizen"), detail.login == nil && detail.card == nil)
        }
        return out
    }

    /// An identity field's name in words (lib/items.ts `IDENTITY_LABEL`).
    static func identityLabel(_ name: String) -> String {
        switch name {
        case "title": featureString("Anrede")
        case "firstName": featureString("Vorname")
        case "middleName": featureString("Zweiter Vorname")
        case "lastName": featureString("Nachname")
        case "username": featureString("Benutzername")
        case "company": featureString("Firma")
        case "email": featureString("E-Mail")
        case "phone": featureString("Telefon")
        case "address1": featureString("Adresse")
        case "address2": featureString("Adresse 2")
        case "address3": featureString("Adresse 3")
        case "postalCode": featureString("Postleitzahl")
        case "city": featureString("Ort")
        case "state": featureString("Bundesland")
        case "country": featureString("Land")
        case "ssn": featureString("Sozialversicherungsnummer")
        case "passportNumber": featureString("Reisepassnummer")
        case "licenseNumber": featureString("Führerscheinnummer")
        default: name
        }
    }
}
