// The UwU extras of one item inside its detail (ItemExtras.tsx, mobile/pages/Item.tsx): own icon
// and icon library, masked address, password reminder, earlier versions. Each part is gated by
// the server's features as the page gates it; nothing at all without UwULock Server's extras.
// Sharing as a Send lives in Features/Sends (ShareAsSendButton).
import SwiftUI
import UwUDesign

struct ItemExtrasSection: View {
    let itemId: String

    @Environment(AppModel.self) private var model
    @State private var loader = ExtrasItemLoader()

    private var store: UwuStore { UwuStore.shared }
    private var status: UwuStatus { store.status }
    private var backend: ExtrasBackend { ExtrasBackend(core: model.core) }
    private var detail: ItemDetail? { loader.detail(for: itemId) }
    private var summary: ItemSummary? { detail?.summary }
    private var live: Bool { !(summary?.deleted ?? false) }

    /// Reloads whenever the item, the vault or the extras change.
    private var loadKey: String { "\(itemId)#\(store.vaultChanges)#\(store.statusChanges)#\(loader.revision)" }

    var body: some View {
        if status.uwu {
            let _ = loader.revision
            if status.has(.ownIcons), live {
                ExtrasIconSection(itemId: itemId, detail: detail, icon: loader.icon, backend: backend) {
                    loader.reload()
                }
                .task(id: loadKey) { await load() }
            }
            if let masked = ExtrasBackend.masked(itemId, in: status) {
                ExtrasMaskedRow(masked: masked, backend: backend)
                    .task(id: loadKey) { await load() }
            }
            if status.has(.reminders), live, (summary?.kind ?? .login) == .login,
                ExtrasBackend.reminder(itemId, in: status) != nil || detail != nil
            {
                ExtrasReminderSection(
                    itemId: itemId, reminder: ExtrasBackend.reminder(itemId, in: status), backend: backend
                ) {
                    loader.reload()
                }
                .task(id: loadKey) { await load() }
            }
            if status.has(.versions), live {
                ExtrasVersionsSection(itemId: itemId, itemBroken: summary?.broken ?? false, backend: backend)
                    .task(id: loadKey) { await load() }
            }
        }
    }

    private func load() async {
        store.attach(model.core)
        await loader.load(itemId, key: loadKey, backend: backend, automatic: status.automaticIcons)
    }
}

/// The item's details and icon, loaded once per change however many sections ask.
@MainActor
@Observable
final class ExtrasItemLoader {
    private(set) var detail: ItemDetail?
    private(set) var icon: String?
    /// Ticks after a change only the samples know of (fixtures don't tick the store).
    private(set) var revision = 0

    @ObservationIgnored private var loadedKey: String?

    func detail(for id: String) -> ItemDetail? { detail?.summary.id == id ? detail : nil }

    func load(_ id: String, key: String, backend: ExtrasBackend, automatic: Bool) async {
        guard loadedKey != key else { return }
        loadedKey = key
        if detail?.summary.id != id {
            detail = nil
            icon = nil
        }
        do {
            detail = try await backend.item(id)
        } catch {
            detail = nil
        }
        icon = await backend.icon(id, automatic: automatic)
    }

    func reload() {
        revision += 1
    }
}

// MARK: Own icon

private struct ExtrasIconSection: View {
    let itemId: String
    let detail: ItemDetail?
    let icon: String?
    let backend: ExtrasBackend
    let changed: () -> Void

    @State private var picking = false
    @State private var busy = false
    @State private var saved = 0

    private var status: UwuStatus { UwuStore.shared.status }
    private var hasOwn: Bool { ExtrasBackend.hasOwnIcon(itemId, in: status) }
    private var uris: [String] { detail?.login?.uris.map { $0.host ?? $0.uri } ?? [] }
    private var local: Bool { uris.contains { ExtrasHost.isLocal(ExtrasHost.host(of: $0)) } }

    var body: some View {
        Section {
            HStack(spacing: UwUSpacing.sm) {
                ExtrasIconTile(
                    dataURL: icon,
                    fallback: String(detail?.summary.name.prefix(1).uppercased() ?? "?"), side: 44)
                Text(features: hint)
                    .uwuFont(.subheadline)
                    .foregroundStyle(Color.uwuMuted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .contextMenu { menu }

            Button {
                picking = true
            } label: {
                Label(features: "Icon ändern", uwuIcon: .image)
            }
            .disabled(busy || detail == nil)
            #if os(macOS)
            .help(Text(features: "Icon ändern"))
            #endif
            if hasOwn {
                Button(role: .destructive) {
                    apply(.remove)
                } label: {
                    Label(features: "Eigenes Icon entfernen", uwuIcon: .delete)
                }
                .disabled(busy)
            }
        } header: {
            Text(features: "Icon")
        }
        .sensoryFeedback(.success, trigger: saved)
        .sheet(isPresented: $picking) {
            if let detail {
                ExtrasIconPicker(summary: detail.summary, uris: uris, hasOwn: hasOwn) { choice in
                    apply(choice)
                }
            }
        }
    }

    private var hint: LocalizedStringKey {
        if hasOwn {
            return "Ein eigenes Icon, verschlüsselt gespeichert. Die offiziellen Bitwarden-Apps zeigen das der Website."
        }
        if local {
            return "Ein Gerät im Heimnetz: dein Server fragt es nie. Hol das Icon vom Gerät, nimm eins aus der Bibliothek oder ein eigenes Bild."
        }
        return "Das Icon der Website, geholt von deinem Server. Oder wähle ein eigenes."
    }

    @ViewBuilder
    private var menu: some View {
        Button { picking = true } label: { Label(features: "Icon ändern", uwuIcon: .image) }
        if local {
            Button { apply(.device) } label: { Label(features: "Icon vom Gerät holen", uwuIcon: .network) }
        }
        if hasOwn {
            Button(role: .destructive) { apply(.remove) } label: {
                Label(features: "Eigenes Icon entfernen", uwuIcon: .delete)
            }
        }
    }

    private func apply(_ choice: ExtrasIconChoice) {
        busy = true
        Task {
            defer { busy = false }
            do {
                switch choice {
                case .png(let png):
                    try await backend.setOwnIcon(itemId, png: png)
                    FeatureToasts.shared.show(featureString("Icon gespeichert ✧"))
                case .device:
                    try await backend.fetchDeviceIcon(itemId)
                    FeatureToasts.shared.show(featureString("Icon vom Gerät geholt ✧"))
                case .remove:
                    try await backend.deleteOwnIcon(itemId)
                    FeatureToasts.shared.show(featureString("Eigenes Icon entfernt."))
                }
                saved += 1
                changed()
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }
}

// MARK: Masked address

private struct ExtrasMaskedRow: View {
    let masked: MaskedLink
    let backend: ExtrasBackend

    var body: some View {
        Section {
            LabeledContent {
                Text(verbatim: masked.email)
                    .textSelection(.enabled)
                    .lineLimit(1)
                    .truncationMode(.middle)
            } label: {
                Label(features: "Maskierte Adresse", uwuIcon: .maskedAddress)
            }
            .contextMenu {
                Button { copy() } label: { Label(features: "Kopieren", uwuIcon: .copy) }
            }
            #if os(iOS)
            .swipeActions(edge: .trailing) {
                Button { copy() } label: { Label(features: "Kopieren", uwuIcon: .copy) }
                    .tint(.uwuPinkSolid)
            }
            #endif
        }
    }

    private func copy() {
        Task {
            do {
                try await backend.copyText(masked.email)
                FeatureToasts.shared.show(copiedAddressText())
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }
}

#if DEBUG
#Preview("Item extras") {
    Form {
        ItemExtrasSection(itemId: ExtrasFixtures.loginId)
    }
    .formStyle(.grouped)
    .featureToasts()
    .environment(AppModel())
}
#endif
