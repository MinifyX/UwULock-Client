// The extras' commands, through the core, or the samples of ExtrasFixtures.swift when
// `FeatureFixtures.isOn` (screenshots, previews).
import Foundation

@MainActor
struct ExtrasBackend {
    let core: Core

    private var fixtures: Bool { FeatureFixtures.isOn }

    func item(_ id: String) async throws -> ItemDetail {
        #if DEBUG
        if fixtures { return ExtrasFixtures.item(id) }
        #endif
        return try await core.vaultItem(id: id)
    }

    func icon(_ id: String, automatic: Bool) async -> String? {
        #if DEBUG
        if fixtures { return ExtrasFixtures.ownIcons[id] ?? ExtrasFixtures.png(hue: 0.92) }
        #endif
        return (try? await core.itemIcons(ids: [id], automatic: automatic))?[id]
    }

    func setOwnIcon(_ id: String, png: String) async throws {
        #if DEBUG
        if fixtures { ExtrasFixtures.ownIcons[id] = png; return }
        #endif
        try await core.setOwnIcon(id: id, png: png)
    }

    func fetchDeviceIcon(_ id: String) async throws {
        #if DEBUG
        if fixtures { ExtrasFixtures.ownIcons[id] = ExtrasFixtures.png(hue: 0.55); return }
        #endif
        try await core.fetchDeviceIcon(id: id)
    }

    func deleteOwnIcon(_ id: String) async throws {
        #if DEBUG
        if fixtures { ExtrasFixtures.ownIcons[id] = nil; return }
        #endif
        try await core.deleteOwnIcon(id: id)
    }

    func iconLibrary() async throws -> IconLibrary {
        #if DEBUG
        if fixtures { return ExtrasFixtures.library }
        #endif
        return try await core.iconLibrary()
    }

    func libraryIcon(_ icon: LibraryIcon, variant: String) async throws -> String {
        #if DEBUG
        if fixtures {
            let base = Double(abs(icon.id.hashValue % 100)) / 100
            return ExtrasFixtures.png(hue: base, dark: variant == "dark", light: variant == "light")
        }
        #endif
        return try await core.libraryIcon(icon, variant: variant)
    }

    func versions(_ id: String) async throws -> [UwuVersion] {
        #if DEBUG
        if fixtures { return ExtrasFixtures.versions }
        #endif
        return try await core.itemVersions(id: id)
    }

    func revealVersionField(_ id: String, versionId: String?, field: String) async throws -> String {
        #if DEBUG
        if fixtures { return versionId == nil ? "kirsch-blüte-42" : "alt-und-vergessen-7" }
        #endif
        return try await core.revealVersionField(id: id, versionId: versionId, field: field)
    }

    func restoreVersion(_ id: String, versionId: String) async throws {
        #if DEBUG
        if fixtures { return }
        #endif
        try await core.restoreVersion(id: id, versionId: versionId)
    }

    func deleteVersions(_ id: String, versionId: String?) async throws {
        #if DEBUG
        if fixtures {
            if let versionId {
                ExtrasFixtures.versions.removeAll { $0.id == versionId }
            } else {
                ExtrasFixtures.versions = []
            }
            return
        }
        #endif
        try await core.deleteVersions(id: id, versionId: versionId)
    }

    func setReminder(_ id: String, due: String?, everyMonths: Int?) async throws {
        #if DEBUG
        if fixtures {
            ExtrasFixtures.reminders[id] = ReminderMark(due: due ?? "2027-04-30", everyMonths: everyMonths, isDue: false)
            return
        }
        #endif
        try await core.setReminder(id: id, due: due, everyMonths: everyMonths)
    }

    func deleteReminder(_ id: String) async throws {
        #if DEBUG
        if fixtures { ExtrasFixtures.reminders[id] = nil; return }
        #endif
        try await core.deleteReminder(id: id)
    }

    func copyText(_ text: String) async throws {
        #if DEBUG
        if fixtures { return }
        #endif
        try await core.copyText(text: text)
    }

    func openWebVault(_ place: WebVaultPlace, id: String? = nil) async throws {
        #if DEBUG
        if fixtures { return }
        #endif
        try await core.openWebVaultAt(place: place, id: id)
    }

    func extrasKeySeen() async throws {
        #if DEBUG
        if fixtures { return }
        #endif
        try await core.extrasKeySeen()
    }

    func travel() async -> UwuTravel? {
        #if DEBUG
        if fixtures { return UwuTravel(enabled: true, hiddenCount: 3) }
        #endif
        return try? await core.uwuTravel()
    }

    func emailOptIn() async -> EmailOptIn? {
        #if DEBUG
        if fixtures { return ExtrasFixtures.optIn }
        #endif
        return try? await core.emailOptIn()
    }

    func setEmailOptIn(_ on: Bool) async throws -> EmailOptIn {
        #if DEBUG
        if fixtures {
            ExtrasFixtures.optIn = EmailOptIn(optedIn: on, since: on ? FeatureDate.iso(.now) : nil)
            return ExtrasFixtures.optIn
        }
        #endif
        return try await core.setEmailOptIn(optedIn: on)
    }

    // MARK: Marks of the status

    /// The reminder on an item (the samples' in fixtures).
    static func reminder(_ id: String, in status: UwuStatus) -> ReminderMark? {
        #if DEBUG
        if FeatureFixtures.isOn { return ExtrasFixtures.reminders[id] }
        #endif
        return status.reminders[id]
    }

    static func masked(_ id: String, in status: UwuStatus) -> MaskedLink? {
        #if DEBUG
        if FeatureFixtures.isOn { return ExtrasFixtures.masked }
        #endif
        return status.masked[id]
    }

    static func hasOwnIcon(_ id: String, in status: UwuStatus) -> Bool {
        #if DEBUG
        if FeatureFixtures.isOn { return ExtrasFixtures.ownIcons[id] != nil }
        #endif
        return status.ownIcons.keys.contains(id)
    }
}
