import Foundation
import Observation
import SwiftUI
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// How the list is ordered. The desktop only sorts by name; the date is the native extra.
enum VaultSort: String, CaseIterable, Identifiable, Sendable {
    case name, changed
    var id: String { rawValue }
    var label: String {
        switch self {
        case .name: vt("Name")
        case .changed: vt("Zuletzt geändert")
        }
    }
}

/// A message at the bottom of the window (lib/toast.ts).
struct VaultToast: Equatable, Identifiable {
    enum Tone { case info, success, danger }
    let id = UUID()
    var text: String
    var tone: Tone = .info
}

/// The editor's sheet: an item (`id`) or a new one of `kind`.
struct EditorRequest: Identifiable, Equatable {
    var id: String { itemId ?? "new-\(kind.rawValue)-\(folderId ?? "")" }
    var itemId: String?
    var kind: ItemKind
    var folderId: String?
}

/// The folder dialog: a new folder (`folder` nil) or a new name for one.
struct FolderRequest: Identifiable, Equatable {
    var id: String { folder?.id ?? "new" }
    var folder: Folder?
}

/// The vault on screen: the items and folders of the account, what the sidebar counts, the
/// list's search and order, and the actions every screen shares (components/VaultScreen.tsx,
/// mobile/state.tsx). Secrets never live here: they're revealed by the rows that show them.
@MainActor
@Observable
final class VaultModel {
    private(set) var items: [ItemSummary] = []
    private(set) var overview: Overview?
    private(set) var uwu = UwuStatus.empty
    private(set) var loaded = false
    /// Grows with every reload: the detail reloads with it.
    private(set) var revision = 0
    private(set) var syncing = false
    private(set) var icons: [String: Image] = [:]

    var query = ""
    var sort: VaultSort = VaultSort(rawValue: UserDefaults.standard.string(forKey: "vault.sort") ?? "") ?? .name {
        didSet { UserDefaults.standard.set(sort.rawValue, forKey: "vault.sort") }
    }
    /// The item in the detail column.
    var selectedItemId: String?
    var toast: VaultToast?
    var editor: EditorRequest?
    var folderRequest: FolderRequest?
    var folderToDelete: Folder?
    var itemToDelete: ItemSummary?
    /// The Mac's generator popover / sheet.
    var showGenerator = false
    /// The last items opened (mobile Search's "Zuletzt geöffnet"), in memory only.
    private(set) var recent: [String] = []

    @ObservationIgnored let backend: VaultBackend
    @ObservationIgnored private var latest = 0
    @ObservationIgnored private var accountId: String?
    @ObservationIgnored private var iconTask: Task<Void, Never>?

    init(backend: VaultBackend) {
        self.backend = backend
    }

    /// Settings of the desktop (lib/settings.ts) the vault reads.
    var siteIcons: Bool { UserDefaults.standard.object(forKey: "vault.siteIcons") as? Bool ?? true }
    var showTrash: Bool { UserDefaults.standard.object(forKey: "vault.showTrash") as? Bool ?? true }

    // MARK: Loading

    /// Loads the vault, then follows the core: changes, sync state, an account switch, the
    /// server's extras. Runs as long as the vault is on screen.
    func run(status: Status?) async {
        accountId = status?.accountId
        syncing = status?.syncing ?? false
        let events = backend.events()
        async let first: Void = reload()
        async let extras: Void = loadUwu()
        _ = await (first, extras)
        for await event in events {
            switch event {
            case .vaultChanged:
                await reload()
            case .vaultStatus(let status):
                syncing = status.syncing
                if status.accountId != accountId {
                    accountId = status.accountId
                    items = []
                    overview = nil
                    icons = [:]
                    loaded = false
                    selectedItemId = nil
                    recent = []
                    await reload()
                }
            case .uwuChanged(let status):
                uwu = status
            default:
                break
            }
        }
    }

    func reload() async {
        latest += 1
        let mine = latest
        do {
            let newItems = try await backend.vaultItems()
            let newOverview = try await backend.vaultOverview()
            guard mine == latest else { return }
            items = newItems
            overview = newOverview
            revision += 1
            loadIcons()
        } catch {
            if mine == latest { show(error) }
        }
        if mine == latest { loaded = true }
    }

    private func loadUwu() async {
        if let status = try? await backend.uwuStatus() { uwu = status }
    }

    /// The icons of the first live items (the desktop asks for the first 400–600).
    private func loadIcons() {
        iconTask?.cancel()
        let ids = items.filter { !$0.deleted }.prefix(600).map(\.id)
        guard !ids.isEmpty else { return }
        let automatic = siteIcons
        iconTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(150))
            guard let self, !Task.isCancelled,
                let found = try? await backend.itemIcons(ids: Array(ids), automatic: automatic)
            else { return }
            var images: [String: Image] = [:]
            for (id, url) in found {
                if let image = Self.image(dataURL: url) { images[id] = image }
            }
            icons = images
        }
    }

    private static func image(dataURL: String) -> Image? {
        guard let comma = dataURL.firstIndex(of: ","),
            let data = Data(base64Encoded: String(dataURL[dataURL.index(after: comma)...]))
        else { return nil }
        #if canImport(UIKit)
        return UIImage(data: data).map(Image.init(uiImage:))
        #else
        return NSImage(data: data).map(Image.init(nsImage:))
        #endif
    }

    // MARK: Filters (lib/filters.ts)

    var due: Set<String> {
        Set(uwu.reminders.filter { $0.value.isDue }.map(\.key))
    }

    func item(_ id: String?) -> ItemSummary? {
        guard let id else { return nil }
        return items.first { $0.id == id }
    }

    var sortedFolders: [Folder] {
        (overview?.folders ?? []).sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    func folderName(_ id: String?) -> String? {
        guard let id else { return nil }
        return overview?.folders.first { $0.id == id }?.name
    }

    func title(of place: SidebarItem) -> String {
        if let fixed = place.fixedTitle { return fixed }
        switch place {
        case .folder(let id): return folderName(id) ?? vt("Ohne Ordner")
        case .organization(let id):
            return overview?.organizations.first { $0.id == id }?.name ?? vt("Organisation")
        case .collection(let id):
            return overview?.collections.first { $0.id == id }?.name ?? vt("Sammlung")
        default: return vt("Tresor")
        }
    }

    func matches(_ item: ItemSummary, _ filter: SidebarItem) -> Bool {
        if case .trash = filter { return item.deleted }
        if item.deleted { return false }
        switch filter {
        case .all: return true
        case .favorites: return item.favorite
        case .type(let kind): return item.kind == kind
        case .folder(let id): return item.organizationId == nil && item.folderId == id
        case .collection(let id): return item.collectionIds.contains(id)
        case .organization(let id): return item.organizationId == id
        case .due: return due.contains(item.id)
        default: return false
        }
    }

    func count(_ filter: SidebarItem) -> Int {
        items.reduce(0) { $0 + (matches($1, filter) ? 1 : 0) }
    }

    /// The items of a place, or with a search all live items (the trash's with "Papierkorb").
    func visible(_ filter: SidebarItem, query: String? = nil) -> [ItemSummary] {
        let words = (query ?? self.query).lowercased().split(whereSeparator: \.isWhitespace)
        let list: [ItemSummary]
        if words.isEmpty {
            list = items.filter { matches($0, filter) }
        } else {
            let trash = filter == .trash
            list = items.filter { item in
                guard item.deleted == trash else { return false }
                let text = "\(item.name) \(item.subtitle ?? "") \(item.host ?? "")".lowercased()
                return words.allSatisfy { text.contains($0) }
            }
        }
        return sorted(list)
    }

    func sorted(_ list: [ItemSummary]) -> [ItemSummary] {
        switch sort {
        case .name:
            return list.sorted { a, b in
                let byName = a.name.localizedStandardCompare(b.name)
                if byName != .orderedSame { return byName == .orderedAscending }
                return (a.subtitle ?? "").localizedStandardCompare(b.subtitle ?? "") == .orderedAscending
            }
        case .changed:
            return list.sorted { (parseDate($0.revisionDate) ?? .distantPast) > (parseDate($1.revisionDate) ?? .distantPast) }
        }
    }

    // MARK: Messages

    func say(_ text: String, tone: VaultToast.Tone = .info) {
        toast = VaultToast(text: text, tone: tone)
    }

    func show(_ error: any Error) {
        let failure = Core.failure(error)
        if failure.kind == "not-found" { return }
        say(failure.message, tone: .danger)
    }

    /// Runs an action and says what went wrong.
    func perform(_ action: @escaping @MainActor () async throws -> Void) {
        Task {
            do { try await action() } catch { show(error) }
        }
    }

    // MARK: Actions

    func opened(_ id: String) {
        recent.removeAll { $0 == id }
        recent.insert(id, at: 0)
        if recent.count > 5 { recent.removeLast(recent.count - 5) }
    }

    func copy(_ id: String, field: String) {
        perform {
            try await self.backend.copyField(id: id, field: field)
            self.say(copiedText(field), tone: .success)
        }
    }

    /// The field a row's copy action takes (mobile ItemRow.tsx).
    func mainField(_ item: ItemSummary) -> String? {
        switch item.kind {
        case .login: item.hasPassword && item.viewPassword ? "password" : (item.hasUsername ? "username" : nil)
        case .note: "notes"
        case .card: "card-number"
        default: nil
        }
    }

    func toggleFavorite(_ item: ItemSummary) {
        perform {
            try await self.backend.setFavorite(id: item.id, favorite: !item.favorite)
            self.say(item.favorite ? vt("Kein Favorit mehr") : vt("Favorit ✧"), tone: item.favorite ? .info : .success)
        }
    }

    func trash(_ item: ItemSummary) {
        perform {
            try await self.backend.deleteItem(id: item.id, permanent: false)
            if self.selectedItemId == item.id { self.selectedItemId = nil }
            self.say(vt("Im Papierkorb."))
        }
    }

    func restore(_ item: ItemSummary) {
        perform {
            try await self.backend.restoreItem(id: item.id)
            self.say(vt("Aus dem Papierkorb geholt ✧"), tone: .success)
        }
    }

    /// Deletes for good; turns the linked masked address off when asked to.
    func deleteForever(_ item: ItemSummary, disableMasked: Bool) {
        perform {
            try await self.backend.deleteItem(id: item.id, permanent: true)
            if disableMasked, let masked = self.uwu.masked[item.id] {
                _ = try? await self.backend.setMaskedState(id: masked.id, state: .disabled)
            }
            if self.selectedItemId == item.id { self.selectedItemId = nil }
            self.say(vt("Gelöscht."))
        }
    }

    func move(_ item: ItemSummary, to folderId: String?) {
        guard item.organizationId == nil, item.folderId != folderId else { return }
        perform {
            try await self.backend.setItemFolder(id: item.id, folderId: folderId)
            self.say(vt("Verschoben ✧"), tone: .success)
        }
    }

    func move(ids: [String], to folderId: String?) -> Bool {
        let movable = ids.compactMap { self.item($0) }.filter { $0.organizationId == nil && $0.folderId != folderId }
        guard !movable.isEmpty else { return false }
        perform {
            for item in movable { try await self.backend.setItemFolder(id: item.id, folderId: folderId) }
            self.say(vt("Verschoben ✧"), tone: .success)
        }
        return true
    }

    func saveFolder(_ folder: Folder?, name: String) {
        let clean = String(name.trimmingCharacters(in: .whitespacesAndNewlines).prefix(100))
        guard !clean.isEmpty else { return }
        perform {
            _ = try await self.backend.saveFolder(id: folder?.id, name: clean)
            self.say(folder == nil ? vt("Ordner angelegt ✧") : vt("Ordner umbenannt ✧"), tone: .success)
        }
    }

    /// Deletes a folder; returns whether the place on screen was that folder.
    func deleteFolder(_ folder: Folder, selection: Binding<SidebarItem?>) {
        perform {
            try await self.backend.deleteFolder(id: folder.id)
            if selection.wrappedValue == .folder(folder.id) { selection.wrappedValue = .all }
            self.say(vt("Ordner gelöscht."))
        }
    }

    func sync() {
        guard !syncing else { return }
        syncing = true
        perform {
            defer { self.syncing = false }
            let status = try await self.backend.syncNow()
            if let error = status.syncError { self.say(error, tone: .danger) }
        }
    }

    func openUri(_ id: String, index: Int) {
        perform { try await self.backend.openItemUri(id: id, index: index) }
    }

    /// A new item: of `kind`, else of the type on screen; in the folder on screen.
    func newItem(kind: ItemKind? = nil, in place: SidebarItem?) {
        var folderId: String?
        var preset = ItemKind.login
        if case .folder(let id) = place { folderId = id }
        if case .type(let type) = place { preset = type }
        editor = EditorRequest(itemId: nil, kind: kind ?? preset, folderId: folderId)
    }

    func edit(_ item: ItemSummary) {
        guard !item.broken else { return }
        editor = EditorRequest(itemId: item.id, kind: item.kind, folderId: item.folderId)
    }
}
