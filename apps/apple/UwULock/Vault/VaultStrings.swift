import Foundation
import SwiftUI
import UwUDesign

/// A string of the vault's own catalog (Resources/Vault.xcstrings): German source, English.
func vt(_ value: String.LocalizationValue) -> String {
    String(localized: value, table: "Vault")
}

// The names the vault's kinds and fields go by (lib/items.ts).

extension ItemKind {
    /// KIND_LABEL: one item of this kind.
    var label: String {
        switch self {
        case .login: vt("Login")
        case .card: vt("Karte")
        case .identity: vt("Identität")
        case .note: vt("Sichere Notiz")
        case .sshKey: vt("SSH-Schlüssel")
        case .wifi: vt("WLAN")
        }
    }

    /// The sidebar's "Typen": all items of this kind.
    var plural: String {
        switch self {
        case .login: vt("Logins")
        case .card: vt("Karten")
        case .identity: vt("Identitäten")
        case .note: vt("Notizen")
        case .sshKey: vt("SSH-Schlüssel")
        case .wifi: vt("WLAN")
        }
    }

    var icon: UwUIconName {
        switch self {
        case .login: .website
        case .card: .card
        case .identity: .identity
        case .note: .note
        case .sshKey: .sshKey
        case .wifi: .wifi
        }
    }

    /// The order of the "Neu" menu and the sidebar.
    static let ordered: [ItemKind] = [.login, .card, .identity, .note, .sshKey, .wifi]
}

/// IDENTITY_FIELDS with their labels, in the order they're shown.
enum IdentityFields {
    static let all: [(name: String, sensitive: Bool)] = [
        ("title", false), ("firstName", false), ("middleName", false), ("lastName", false),
        ("username", false), ("company", false), ("email", false), ("phone", false),
        ("address1", false), ("address2", false), ("address3", false), ("postalCode", false),
        ("city", false), ("state", false), ("country", false),
        ("ssn", true), ("passportNumber", true), ("licenseNumber", true),
    ]

    static func label(_ name: String) -> String {
        switch name {
        case "title": vt("Anrede")
        case "firstName": vt("Vorname")
        case "middleName": vt("Zweiter Vorname")
        case "lastName": vt("Nachname")
        case "username": vt("Benutzername")
        case "company": vt("Firma")
        case "email": vt("E-Mail")
        case "phone": vt("Telefon")
        case "address1": vt("Adresse")
        case "address2": vt("Adresse 2")
        case "address3": vt("Adresse 3")
        case "postalCode": vt("Postleitzahl")
        case "city": vt("Ort")
        case "state": vt("Bundesland")
        case "country": vt("Land")
        case "ssn": vt("Sozialversicherungsnummer")
        case "passportNumber": vt("Reisepassnummer")
        case "licenseNumber": vt("Führerscheinnummer")
        default: name
        }
    }
}

/// MATCH_LABEL: how Bitwarden decides an address belongs to a site (nil: the default).
enum UriMatch {
    static let all: [Int] = [0, 1, 2, 3, 4, 5]

    static func label(_ match: Int?) -> String {
        switch match {
        case 0: vt("Domain")
        case 1: vt("Host")
        case 2: vt("Beginnt mit")
        case 3: vt("Genau")
        case 4: vt("Regulärer Ausdruck")
        case 5: vt("Nie")
        default: vt("Standard")
        }
    }
}

/// CARD_BRANDS.
let cardBrands = [
    "Visa", "Mastercard", "American Express", "Discover", "Diners Club", "JCB", "Maestro",
    "UnionPay", "RuPay",
]

// lib/format.ts

/// `when`: medium date, short time.
func whenText(_ iso: String?) -> String? {
    guard let date = parseDate(iso) else { return nil }
    return date.formatted(date: .abbreviated, time: .shortened)
}

func parseDate(_ iso: String?) -> Date? {
    guard let iso, !iso.isEmpty else { return nil }
    let full = ISO8601DateFormatter()
    full.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = full.date(from: iso) { return date }
    full.formatOptions = [.withInternetDateTime]
    return full.date(from: iso)
}

/// `spacedCode`: a numeric code of 6+ digits in two halves.
func spacedCode(_ code: String) -> String {
    guard code.count >= 6, code.allSatisfy(\.isNumber) else { return code }
    let half = (code.count + 1) / 2
    return "\(code.prefix(half)) \(code.dropFirst(half))"
}

/// `copiedText`: what the toast says after a copy.
func copiedText(_ field: String) -> String {
    switch field {
    case "username": vt("Benutzername kopiert ✧")
    case "password": vt("Passwort kopiert ✧")
    case "totp", "totp-next": vt("Code kopiert ✧")
    default: vt("Kopiert ✧")
    }
}

/// A password with letters, digits and symbols in their own colours (`charClasses`).
func coloredSecret(_ text: String) -> Text {
    var result = AttributedString()
    for char in text {
        var part = AttributedString(String(char))
        if char.isNumber {
            part.foregroundColor = .uwuPinkInk
        } else if char.isLetter {
            part.foregroundColor = .uwuInk
        } else {
            part.foregroundColor = .uwuWarningInk
        }
        result.append(part)
    }
    return Text(result)
}
