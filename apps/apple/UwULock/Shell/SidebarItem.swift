import SwiftUI
import UwUDesign

/// The item types of the vault (sidebar "Typen").
enum ItemType: String, CaseIterable, Hashable, Sendable {
    case login, card, identity, note, sshKey, wifi

    var title: LocalizedStringResource {
        switch self {
        case .login: "Logins"
        case .card: "Karten"
        case .identity: "Identitäten"
        case .note: "Notizen"
        case .sshKey: "SSH-Schlüssel"
        case .wifi: "WLAN"
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
}

/// One place in the sidebar, as on the desktop (components/VaultScreen.tsx).
enum SidebarItem: Hashable, Sendable {
    case all, favorites, passwordDue, passwordCheck
    case type(ItemType)
    case noFolder
    case uwuSSH, uwuRDP
    case sends, fileRequests, maskedAddresses
    case trash

    var title: LocalizedStringResource {
        switch self {
        case .all: "Alle Einträge"
        case .favorites: "Favoriten"
        case .passwordDue: "Neues Passwort fällig"
        case .passwordCheck: "Passwortprüfung"
        case .type(let type): type.title
        case .noFolder: "Ohne Ordner"
        case .uwuSSH: "SSH (UwUSSH)"
        case .uwuRDP: "Remote Desktop (UwURDP)"
        case .sends: "Sends"
        case .fileRequests: "Dateianfragen"
        case .maskedAddresses: "Maskierte Adressen"
        case .trash: "Papierkorb"
        }
    }

    var icon: UwUIconName {
        switch self {
        case .all: .vault
        case .favorites: .favorite
        case .passwordDue: .reminder
        case .passwordCheck: .securityCheck
        case .type(let type): type.icon
        case .noFolder: .folder
        case .uwuSSH: .terminal
        case .uwuRDP: .computer
        case .sends: .send
        case .fileRequests: .inbox
        case .maskedAddresses: .maskedAddress
        case .trash: .delete
        }
    }
}

/// The sidebar's sections, in the desktop's order.
struct SidebarSection: Identifiable {
    let id: String
    let title: LocalizedStringResource?
    let items: [SidebarItem]

    static let all: [SidebarSection] = [
        SidebarSection(id: "vault", title: nil, items: [.all, .favorites, .passwordDue, .passwordCheck]),
        SidebarSection(id: "types", title: "Typen", items: ItemType.allCases.map(SidebarItem.type)),
        SidebarSection(id: "folders", title: "Ordner", items: [.noFolder]),
        SidebarSection(id: "apps", title: "UwU-Apps", items: [.uwuSSH, .uwuRDP]),
        SidebarSection(id: "extras", title: "Extras", items: [.sends, .fileRequests, .maskedAddresses]),
        SidebarSection(id: "trash", title: nil, items: [.trash]),
    ]
}
