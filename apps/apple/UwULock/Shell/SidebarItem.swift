import SwiftUI
import UwUDesign

/// One place in the sidebar, as on the desktop (components/VaultScreen.tsx, lib/filters.ts):
/// the item filters, plus the pages that aren't item lists (password check, apps, extras).
enum SidebarItem: Hashable, Sendable {
    case all, favorites, due, passwordCheck
    case type(ItemKind)
    /// A folder of the own vault; nil: the items without one.
    case folder(String?)
    case organization(String)
    case collection(String)
    case uwuSSH, uwuRDP
    case sends, fileRequests, maskedAddresses
    case trash

    /// Whether the place is a list of items (`Filter` of lib/filters.ts).
    var isItemFilter: Bool {
        switch self {
        case .passwordCheck, .uwuSSH, .uwuRDP, .sends, .fileRequests, .maskedAddresses: false
        default: true
        }
    }

    var icon: UwUIconName {
        switch self {
        case .all: .vault
        case .favorites: .favorite
        case .due: .reminder
        case .passwordCheck: .securityCheck
        case .type(let kind): kind.icon
        case .folder: .folder
        case .organization: .organization
        case .collection: .collection
        case .uwuSSH: .terminal
        case .uwuRDP: .computer
        case .sends: .send
        case .fileRequests: .inbox
        case .maskedAddresses: .maskedAddress
        case .trash: .delete
        }
    }

    /// The title of a place that doesn't need the vault's data to be named.
    var fixedTitle: String? {
        switch self {
        case .all: vt("Alle Einträge")
        case .favorites: vt("Favoriten")
        case .due: vt("Neues Passwort fällig")
        case .passwordCheck: vt("Passwortprüfung")
        case .type(let kind): kind.plural
        case .folder(nil): vt("Ohne Ordner")
        case .uwuSSH: vt("SSH (UwUSSH)")
        case .uwuRDP: vt("Remote Desktop (UwURDP)")
        case .sends: vt("Sends")
        case .fileRequests: vt("Dateianfragen")
        case .maskedAddresses: vt("Maskierte Adressen")
        case .trash: vt("Papierkorb")
        case .folder, .organization, .collection: nil
        }
    }
}
