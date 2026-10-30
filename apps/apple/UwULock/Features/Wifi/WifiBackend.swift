// The Wi-Fi item's commands, through the core, or the samples of WifiFixtures when
// `FeatureFixtures.isOn` (screenshots, previews).
import Foundation

@MainActor
struct WifiBackend {
    let core: Core
    /// The item detail's backend when there is one (it knows the shell's sample items too).
    var vault: VaultBackend?

    func item(_ id: String) async throws -> ItemDetail {
        if let vault { return try await vault.vaultItem(id: id) }
        #if DEBUG
        if FeatureFixtures.isOn { return WifiFixtures.item(id) }
        #endif
        return try await core.vaultItem(id: id)
    }

    func reveal(_ id: String, field: String) async throws -> String {
        if let vault { return try await vault.revealField(id: id, field: field) }
        #if DEBUG
        if FeatureFixtures.isOn { return WifiFixtures.password }
        #endif
        return try await core.revealField(id: id, field: field)
    }

    func copy(_ id: String, field: String) async throws {
        if let vault { return try await vault.copyField(id: id, field: field) }
        #if DEBUG
        if FeatureFixtures.isOn { return }
        #endif
        try await core.copyField(id: id, field: field)
    }

    func connect(_ id: String) async throws -> WifiJoined {
        #if DEBUG
        if FeatureFixtures.isOn { return WifiJoined(outcome: .saved, reason: nil, message: nil) }
        #endif
        return try await core.wifiConnect(id: id)
    }

    /// Opens the system's Wi-Fi settings; false where the platform can't (Apple).
    func settings() async -> Bool {
        #if DEBUG
        if FeatureFixtures.isOn { return false }
        #endif
        return (try? await core.wifiSettings()) ?? false
    }
}

#if DEBUG
@MainActor
enum WifiFixtures {
    static let itemId = "fixture-wifi"
    static let password = "kirsch-blüte-sternschnuppe"

    static func item(_ id: String) -> ItemDetail {
        let fields = [
            FieldDetail(index: 0, name: WifiNetwork.marker, kind: .text, value: WifiNetwork.type, hasValue: true),
            FieldDetail(index: 1, name: WifiNetwork.Name.ssid, kind: .text, value: "Mias WLAN", hasValue: true),
            FieldDetail(index: 2, name: WifiNetwork.Name.password, kind: .hidden, value: nil, hasValue: true),
            FieldDetail(index: 3, name: WifiNetwork.Name.security, kind: .text, value: "WPA2/WPA3", hasValue: true),
            FieldDetail(index: 4, name: WifiNetwork.Name.hidden, kind: .boolean, value: "false", hasValue: true),
        ]
        return ItemDetail(
            summary: ItemSummary(
                id: id, kind: .wifi, name: "Mias WLAN", subtitle: "WPA2/WPA3", host: nil, favorite: false,
                folderId: nil, organizationId: nil, collectionIds: [], deleted: false, reprompt: false,
                viewPassword: true, hasTotp: false, hasPassword: false, hasUsername: false, broken: false,
                revisionDate: "2026-10-12T08:00:00Z"),
            locked: false, fields: fields)
    }
}
#endif
