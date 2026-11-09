import Foundation
import SwiftUI
import UwUDesign

/// A string from the auth area's catalog (Resources/Auth.xcstrings). German is the source.
func authStr(_ key: String.LocalizationValue) -> String {
    String(localized: key, table: "Auth")
}

extension CoreFailure {
    /// The failure in the user's language, by kind (errors.ts). Security errors stay plain.
    var authMessage: String {
        let m = message
        switch kind {
        case "code-rejected":
            return m
        case "network":
            return authStr("Der Server ist nicht erreichbar: \(m)")
        case "refused":
            if m.contains("SSO login took too long") {
                return authStr("Die Anmeldung im Browser hat zu lange gedauert. Starte sie noch einmal.")
            }
            if m.contains("SSO is not enabled") || m.contains("doesn't offer SSO") {
                return authStr("Dieser Server bietet keine Anmeldung mit SSO an.")
            }
            if let count = m.firstMatch(of: /master password needs at least (\d+) characters/)?.1 {
                let count = String(count)
                return authStr("Das Master-Passwort braucht mindestens \(count) Zeichen.")
            }
            if let level = m.firstMatch(of: /too easy to guess: this server wants strength (\d)/)?.1 {
                let level = String(level)
                return authStr(
                    "Das Master-Passwort ist zu leicht zu erraten. Dieser Server verlangt Stärke \(level) von 4 – nimm ein längeres oder eine Passphrase aus mehreren Wörtern.")
            }
            if m.contains("https://") {
                return authStr(
                    "Der Server muss per https:// erreichbar sein. Unverschlüsseltes http:// geht nur zu diesem Rechner (localhost).")
            }
            if m.contains("empty") { return authStr("Bitte gib die Adresse deines Servers ein.") }
            if m.contains("web address") || m.contains("host name") {
                return authStr("Das ist keine gültige Server-Adresse.")
            }
            if m.range(of: "incorrect|wrong|invalid", options: [.regularExpression, .caseInsensitive]) != nil {
                return authStr("E-Mail-Adresse oder Master-Passwort stimmt nicht.")
            }
            return authStr("Der Server hat die Anmeldung abgelehnt: \(m)")
        case "cancelled", "biometric-cancelled":
            return authStr("Abgebrochen.")
        case "sso-restart":
            return authStr(
                "Dieses Konto hat schon ein Master-Passwort – vermutlich hat ein früherer Versuch geklappt. Melde dich noch einmal mit SSO an.")
        case "wrong-password":
            return authStr("Das Master-Passwort ist falsch.")
        case "session-expired":
            return authStr("Die Sitzung ist abgelaufen. Bitte melde dich neu an.")
        case "server":
            return authStr("Der Server hat mit einem Fehler geantwortet: \(m)")
        case "unsupported":
            let reason = m.replacing(#/^not supported yet: /#, with: "")
            return authStr("Das kann diese Beta noch nicht: \(reason)")
        case "hello":
            if m.contains("key has changed") {
                return authStr(
                    "Der Schlüssel zum Entsperren hat sich geändert. Entsperre mit dem Master-Passwort und schalte das Entsperren mit Fingerabdruck oder Gesicht wieder ein.")
            }
            return m
        case "biometric-lockout":
            return authStr(
                "Zu viele Versuche: Das Gerät lässt Face ID oder Touch ID gerade nicht zu. Entsperre mit dem Master-Passwort.")
        case "biometric-changed":
            return authStr(
                "Auf diesem Gerät ist ein Fingerabdruck oder Gesicht dazugekommen, deshalb gilt das Entsperren damit nicht mehr. Entsperre mit dem Master-Passwort und schalte es in den Einstellungen wieder ein.")
        case "weaker-kdf":
            return authStr(
                "Der Server verlangt für dieses Konto eine schwächere Schlüsselableitung als bei der letzten Anmeldung, deshalb hat UwULock nichts gesendet. Wenn du sie selbst gesenkt hast, melde das Konto auf diesem Gerät ab und füge es neu hinzu.")
        case "crypto":
            return authStr("Etwas ließ sich nicht entschlüsseln: \(m)")
        case "locked":
            return authStr("Der Tresor ist gesperrt.")
        case "rate-limited":
            return authStr("Zu viele Versuche in kurzer Zeit. Warte einen Moment.")
        case "upstream":
            return authStr("Ein Dienst hinter dem Server antwortet gerade nicht: \(m)")
        case "no-key-pair":
            return authStr("Deinem Konto fehlt ein Schlüsselpaar. Melde dich einmal im Web-Tresor an.")
        case "invalid":
            if m.hasPrefix("That doesn't look like an email") {
                return authStr("Das sieht nicht nach einer E-Mail-Adresse aus.")
            }
            return m
        default:
            return m
        }
    }
}

extension UnlockPrompt {
    /// What the system's Face ID / Touch ID sheet says (unlock.ts).
    static var standard: UnlockPrompt {
        UnlockPrompt(
            title: authStr("UwULock entsperren"),
            subtitle: authStr("Bestätige, dass du es bist."),
            cancel: authStr("Master-Passwort"),
            reason: authStr("deinen Tresor zu entsperren"))
    }
}

/// Words for unlocking with biometrics (unlock.ts), by the core's `helloKind`.
enum Biometrics {
    static func unlockLabel(_ kind: String?) -> String {
        switch kind {
        case "faceId": authStr("Mit Face ID entsperren")
        case "touchId": authStr("Mit Touch ID entsperren")
        case "opticId": authStr("Mit Optic ID entsperren")
        default: authStr("Mit Fingerabdruck oder Gesicht entsperren")
        }
    }

    /// The short name, for the iPhone's settings row ("Sicherheit: Face ID").
    static func name(_ kind: String?) -> String {
        switch kind {
        case "faceId": "Face ID"
        case "touchId": "Touch ID"
        case "opticId": "Optic ID"
        default: authStr("Biometrie")
        }
    }

    static var description: String {
        authStr(
            "Statt des Master-Passworts. Der Schlüssel dafür liegt geschützt auf diesem Gerät und gilt nicht mehr, sobald ein Finger oder Gesicht dazukommt.")
    }

    static func icon(_ kind: String?) -> UwUIconName {
        kind == "faceId" || kind == "opticId" ? .profile : .fingerprint
    }
}

/// "vor 5 Min." (format.ts `ago`), from unix seconds.
enum Ago {
    static func text(_ seconds: Int?) -> String {
        guard let seconds, seconds > 0 else { return authStr("noch nie") }
        let date = Date(timeIntervalSince1970: TimeInterval(seconds))
        let elapsed = max(0, Date().timeIntervalSince(date))
        if elapsed < 60 { return authStr("gerade eben") }
        if elapsed < 86_400 {
            let formatter = RelativeDateTimeFormatter()
            formatter.unitsStyle = .short
            return formatter.localizedString(for: date, relativeTo: Date())
        }
        return date.formatted(date: .abbreviated, time: .omitted)
    }
}

extension Status {
    /// "bitwarden.eu" or the self-hosted server's host, for the lock screen and the account card.
    var serverName: String { server ?? serverUrl ?? "" }
}
