import SwiftUI
import UwUDesign

/// Masked addresses (MaskedDialog.tsx, mobile/pages/Masked.tsx): addresses from UwUMail that
/// forward to the real one, one per site. Listed, copied, switched off and on, linked to an item,
/// deleted and made here; connecting the account to UwUMail happens in the web vault.
struct MaskedAddressesView: View {
    @Environment(AppModel.self) private var model
    @Environment(VaultModel.self) private var vault: VaultModel?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.openVaultItem) private var openVaultItem
    @State private var store = UwuStore.shared

    @State private var connection: MaskedConnection?
    @State private var addresses: [MaskedAddress]?
    @State private var items: [ItemSummary] = []
    @State private var error: String?
    @State private var busy = false
    @State private var query = ""
    @State private var selection: MaskedAddress.ID?
    @State private var creating = false
    @State private var linking: MaskedAddress?
    @State private var deleting: MaskedAddress?
    @State private var toggled = 0

    private var available: Bool { store.status.has(.maskedAddresses) }

    private var connected: Bool {
        guard let connection else { return false }
        return connection.connected && connection.status != "revoked"
    }

    private var canAdd: Bool { available && connected }

    /// Newest first, filtered by the search words.
    private var shown: [MaskedAddress] {
        let words = query.trimmingCharacters(in: .whitespaces).lowercased()
        return (addresses ?? [])
            .filter { address in
                guard !words.isEmpty else { return true }
                return [address.email, address.forDomain ?? "", address.description ?? "",
                        address.itemName ?? ""]
                    .joined(separator: " ").lowercased().contains(words)
            }
            .sorted { ($0.createdAt ?? "") > ($1.createdAt ?? "") }
    }

    var body: some View {
        content
            .navigationTitle(Text(features: "Maskierte Adressen"))
            .toolbar { toolbar }
            .task {
                UwuStore.shared.attach(model.core)
                await load()
            }
            .onChange(of: store.suiteChanges) { Task { await load() } }
            .onChange(of: store.statusChanges) { Task { await load() } }
            .onChange(of: available) { Task { await load() } }
            .sheet(isPresented: $creating) {
                MaskedNewAddressSheet(
                    to: connection?.username?.contains("@") == true ? connection?.username : nil,
                    items: items
                ) { site, description, cipherId in
                    try await create(site: site, description: description, cipherId: cipherId)
                }
            }
            .sheet(item: $linking) { address in
                MaskedLinkSheet(address: address, items: items) { cipherId in
                    try await link(address, to: cipherId)
                }
            }
            .alert(
                Text(features: "Adresse löschen?"),
                isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                presenting: deleting
            ) { address in
                Button(role: .destructive) {
                    remove(address)
                } label: {
                    Text(features: "Löschen")
                }
                Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
            } message: { address in
                Text(features: "\(address.email) nimmt dann für immer keine Mail mehr an und wird nie wieder vergeben. Zum Pausieren reicht „Abschalten“.")
            }
            .sensoryFeedback(.selection, trigger: toggled)
            .featureToasts()
    }

    // MARK: Content

    @ViewBuilder
    private var content: some View {
        if !available {
            FeatureEmptyState(
                mood: .sleepy, title: Text(features: "Keine maskierten Adressen"),
                message: Text(features: "Dieser Server bietet keine maskierten Adressen an."))
        } else if connection == nil, let error {
            FeatureEmptyState(mood: .sad, title: Text(features: "Das hat nicht geklappt"),
                              message: Text(verbatim: error)) {
                Button {
                    Task { await load() }
                } label: {
                    Label(features: "Nochmal versuchen", uwuIcon: .refresh)
                }
                .uwuGlassButton()
            }
        } else if let connection, !connected {
            MaskedNotConnectedView(connection: connection)
        } else if connection == nil {
            ProgressView { Text(features: "Einen Moment …") }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            list
        }
    }

    @ViewBuilder
    private var list: some View {
        let all = addresses ?? []
        List(selection: $selection) {
            if let error {
                Section {
                    Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                        .foregroundStyle(.uwuDangerInk)
                }
            }
            if connection?.status == "unreachable" {
                Section {
                    Label { Text(features: "UwUMail war zuletzt nicht erreichbar.") } icon: {
                        UwUIcon(.warning, size: .sm)
                    }
                    .foregroundStyle(.uwuWarningInk)
                }
            }
            Section {
                ForEach(shown) { address in
                    MaskedAddressRow(
                        address: address, itemName: itemName(for: address), busy: busy,
                        onToggle: { toggle(address) }
                    )
                    .tag(address.id)
                    #if os(iOS)
                    // A tap copies (the list's primary action only fires on the Mac).
                    .contentShape(Rectangle())
                    .onTapGesture { copy(address.email) }
                    .swipeActions(edge: .trailing) {
                        Button(role: .destructive) {
                            deleting = address
                        } label: {
                            Label(features: "Löschen", uwuIcon: .delete)
                        }
                    }
                    .swipeActions(edge: .leading) {
                        Button {
                            copy(address.email)
                        } label: {
                            Label(features: "Kopieren", uwuIcon: .copy)
                        }
                        .tint(.uwuPinkSolid)
                    }
                    #endif
                }
            } header: {
                if let subtitle = forwardedTo {
                    Text(verbatim: subtitle)
                        .textCase(nil)
                }
            } footer: {
                if !all.isEmpty {
                    #if os(iOS)
                    Text(features: "Antippen kopiert die Adresse. Abgeschaltete Adressen nehmen keine Mails mehr an.")
                    #else
                    Text(features: "Doppelklick kopiert die Adresse. Abgeschaltete Adressen nehmen keine Mails mehr an.")
                    #endif
                }
            }
        }
        .overlay {
            if all.isEmpty {
                FeatureEmptyState(
                    mood: .uwu, title: Text(features: "Noch keine Adressen."),
                    message: newHint
                ) {
                    Button {
                        creating = true
                    } label: {
                        Label(features: "Neue Adresse", uwuIcon: .add)
                    }
                    .uwuProminentButton()
                }
            } else if shown.isEmpty {
                ContentUnavailableView.search(text: query)
            }
        }
        .modifier(MaskedSearch(enabled: all.count > 6, query: $query))
        .contextMenu(forSelectionType: MaskedAddress.ID.self) { ids in
            if let id = ids.first, let address = all.first(where: { $0.id == id }) {
                menu(for: address)
            }
        } primaryAction: { ids in
            if let id = ids.first, let address = all.first(where: { $0.id == id }) {
                copy(address.email)
            }
        }
        #if os(macOS)
        .onDeleteCommand {
            if let address = selected { deleting = address }
        }
        .onCopyCommand {
            guard let address = selected else { return [] }
            copy(address.email)
            return []
        }
        #endif
        .refreshable { await load() }
    }

    private var newHint: Text {
        #if os(iOS)
        Text(features: "Tipp auf + für eine neue.")
        #else
        Text(features: "Mit ⌘N legst du eine neue an.")
        #endif
    }

    private var selected: MaskedAddress? {
        guard let selection else { return nil }
        return addresses?.first { $0.id == selection }
    }

    /// Where the mails go, when UwUMail's user name is an address (mobile `forwardedTo`).
    private var forwardedTo: String? {
        guard let connection, let user = connection.username, !user.isEmpty else { return nil }
        if user.contains("@") {
            return featureString("Weitergeleitet an \(user)")
        }
        let server = (connection.server ?? "")
            .replacingOccurrences(of: "^https?://", with: "", options: .regularExpression)
        return featureString("Verbunden mit \(server) als \(user).")
    }

    private func itemName(for address: MaskedAddress) -> String? {
        guard let cipherId = address.cipherId else { return nil }
        if let item = items.first(where: { $0.id == cipherId }) {
            return item.name.isEmpty ? featureString("(ohne Namen)") : item.name
        }
        if let name = address.itemName {
            return name.isEmpty ? featureString("(ohne Namen)") : name
        }
        return nil
    }

    @ViewBuilder
    private func menu(for address: MaskedAddress) -> some View {
        let on = MaskedAddressRow.isOn(address)
        Button {
            copy(address.email)
        } label: {
            Label(features: "Kopieren", uwuIcon: .copy)
        }
        Button {
            toggle(address)
        } label: {
            if on {
                Label(features: "Abschalten", uwuIcon: .notificationsOff)
            } else {
                Label(features: "Einschalten", uwuIcon: .notifications)
            }
        }
        .disabled(busy)
        if let cipherId = address.cipherId, let name = itemName(for: address), vault != nil {
            Button {
                openItem(cipherId)
            } label: {
                Label { Text(verbatim: name) } icon: { UwUIcon(.link) }
            }
        }
        Button {
            linking = address
        } label: {
            if address.cipherId == nil {
                Label(features: "Mit Eintrag verknüpfen …", uwuIcon: .link)
            } else {
                Label(features: "Anderen Eintrag verknüpfen …", uwuIcon: .link)
            }
        }
        .disabled(busy)
        Divider()
        Button(role: .destructive) {
            deleting = address
        } label: {
            Label(features: "Löschen", uwuIcon: .delete)
        }
        .disabled(busy)
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            Button {
                creating = true
            } label: {
                Label(features: "Neue Adresse", uwuIcon: .add)
            }
            .keyboardShortcut("n", modifiers: .command)
            .disabled(!canAdd || busy)
            .help(Text(features: "Neue Adresse"))
        }
        #if os(macOS)
        ToolbarItem {
            Button {
                Task { await load() }
            } label: {
                Label(features: "Neu laden", uwuIcon: .refresh)
            }
            .keyboardShortcut("r", modifiers: .command)
            .disabled(!available)
            .help(Text(features: "Neu laden"))
        }
        #endif
    }

    // MARK: Actions

    private func load() async {
        #if DEBUG
        if FeatureFixtures.isOn {
            if connection == nil {
                connection = MaskedFixtures.connection
                addresses = MaskedFixtures.addresses
                items = MaskedFixtures.items
            }
            return
        }
        #endif
        guard available else { return }
        do {
            let next = try await model.core.maskedConnection()
            connection = next
            if next.connected && next.status != "revoked" {
                addresses = try await model.core.maskedAddresses()
                if let all = try? await model.core.vaultItems() {
                    items = all.filter { !$0.deleted }
                }
            }
            error = nil
        } catch {
            self.error = Core.failure(error).message
            addresses = []
        }
    }

    /// Runs `what`, shows `done`, loads again (MaskedDialog `act`).
    private func act(_ done: String? = nil, _ what: @escaping () async throws -> Void) {
        busy = true
        Task {
            defer { busy = false }
            do {
                try await what()
                if let done { FeatureToasts.shared.show(done) }
                await load()
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }

    private func copy(_ email: String) {
        Task { await MaskedClipboard.copy(email, core: model.core) }
    }

    private func toggle(_ address: MaskedAddress) {
        let on = MaskedAddressRow.isOn(address)
        toggled += 1
        #if DEBUG
        if FeatureFixtures.isOn {
            update(address.id) { $0.state = on ? "disabled" : "enabled" }
            FeatureToasts.shared.show(on ? featureString("Abgeschaltet.") : featureString("Wieder an ✧"))
            return
        }
        #endif
        act(on ? featureString("Abgeschaltet.") : featureString("Wieder an ✧")) {
            let changed = try await model.core.setMaskedState(
                id: address.id, state: on ? .disabled : .enabled)
            update(address.id) { $0 = changed }
        }
    }

    private func remove(_ address: MaskedAddress) {
        #if DEBUG
        if FeatureFixtures.isOn {
            withUwUAnimation(reduceMotion: reduceMotion) {
                addresses?.removeAll { $0.id == address.id }
            }
            FeatureToasts.shared.show(featureString("Gelöscht."))
            return
        }
        #endif
        act(featureString("Gelöscht.")) {
            try await model.core.deleteMaskedAddress(id: address.id)
            if selection == address.id { selection = nil }
        }
    }

    private func create(site: String?, description: String?, cipherId: String?) async throws {
        #if DEBUG
        if FeatureFixtures.isOn {
            let made = MaskedAddress(
                id: UUID().uuidString, email: "new.kitten.\(Int.random(in: 10...99))@mask.example.com",
                state: "enabled", forDomain: site, description: description,
                createdAt: FeatureDate.iso(.now), lastMessageAt: nil, cipherId: cipherId,
                itemName: items.first { $0.id == cipherId }?.name)
            withUwUAnimation(reduceMotion: reduceMotion) { addresses?.insert(made, at: 0) }
            FeatureToasts.shared.show(copiedAddressText())
            return
        }
        #endif
        let made = try await model.core.createMaskedAddress(
            forDomain: site, description: description, cipherId: cipherId)
        await MaskedClipboard.copy(made.email, core: model.core)
        await load()
    }

    private func link(_ address: MaskedAddress, to cipherId: String) async throws {
        #if DEBUG
        if FeatureFixtures.isOn {
            update(address.id) {
                $0.cipherId = cipherId
                $0.itemName = items.first { $0.id == cipherId }?.name
            }
            FeatureToasts.shared.show(featureString("Verknüpft ✧"))
            return
        }
        #endif
        let changed = try await model.core.linkMaskedAddress(id: address.id, cipherId: cipherId)
        update(address.id) { $0 = changed }
        FeatureToasts.shared.show(featureString("Verknüpft ✧"))
        await load()
    }

    private func update(_ id: String, _ change: (inout MaskedAddress) -> Void) {
        guard let index = addresses?.firstIndex(where: { $0.id == id }) else { return }
        change(&addresses![index])
    }

    /// Shows the linked item in the vault (MaskedDialog `onOpenItem`).
    private func openItem(_ id: String) {
        if let openVaultItem { return openVaultItem(id) }
        guard let vault else { return }
        model.selection = .all
        vault.selectedItemId = id
    }
}

/// Copies an address through the core and says so (MaskedDialog `copyAddress`).
@MainActor
enum MaskedClipboard {
    static func copy(_ email: String, core: Core) async {
        #if DEBUG
        if FeatureFixtures.isOn {
            FeatureToasts.shared.show(copiedAddressText())
            return
        }
        #endif
        do {
            try await core.copyText(text: email)
            FeatureToasts.shared.show(copiedAddressText())
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }
}

/// `.searchable` only once there are more than six addresses, as the page does.
private struct MaskedSearch: ViewModifier {
    let enabled: Bool
    @Binding var query: String

    func body(content: Content) -> some View {
        if enabled {
            content.searchable(text: $query, prompt: Text(features: "Adressen durchsuchen"))
        } else {
            content
        }
    }
}

// MARK: Row

/// One address: the address, what it is for, its linked item, its last mail, and its switch.
struct MaskedAddressRow: View {
    let address: MaskedAddress
    let itemName: String?
    let busy: Bool
    let onToggle: () -> Void

    static func isOn(_ address: MaskedAddress) -> Bool {
        address.state == "enabled" || address.state == "pending"
    }

    private var on: Bool { Self.isOn(address) }

    private var subtitle: String {
        var parts: [String] = []
        if let description = address.description, !description.isEmpty { parts.append(description) }
        if let site = address.forDomain, !site.isEmpty { parts.append(site) }
        if let itemName { parts.append(itemName) }
        if let when = FeatureDate.relative(address.lastMessageAt) {
            parts.append(featureString("letzte Mail \(when)"))
        }
        if !on { parts.append(featureString("abgeschaltet")) }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: UwUSpacing.sm) {
            UwUIcon(.maskedAddress, size: .md)
                .foregroundStyle(on ? Color.uwuPinkSolid : Color.uwuMuted)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: address.email)
                    .uwuFont(.callout, weight: .medium)
                    .monospaced()
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                    .truncationMode(.middle)
                    .foregroundStyle(on ? Color.uwuInk : Color.uwuMuted)
                if !subtitle.isEmpty {
                    Text(verbatim: subtitle)
                        .uwuFont(.caption)
                        .foregroundStyle(.uwuMuted)
                        .lineLimit(2)
                }
            }
            Spacer(minLength: UwUSpacing.xs)
            if address.state == "pending" {
                UwUIcon(.pending, size: .sm)
                    .foregroundStyle(.uwuMuted)
                    .help(Text(features: "Wird eingerichtet"))
                    .accessibilityLabel(Text(features: "Wird eingerichtet"))
            }
            Toggle(isOn: Binding(get: { on }, set: { _ in onToggle() })) {
                Text(features: "\(address.email) an")
            }
            .labelsHidden()
            .toggleStyle(.switch)
            .disabled(busy)
            #if os(macOS)
            .controlSize(.small)
            #endif
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityHint(Text(features: "Antippen kopiert die Adresse."))
        #if os(macOS)
        .help(Text(features: "\(address.email) kopieren"))
        #endif
    }
}

// MARK: Not connected

/// The account isn't connected to UwUMail, or no longer is (`MaskedNotConnected`).
struct MaskedNotConnectedView: View {
    @Environment(AppModel.self) private var model
    let connection: MaskedConnection

    private var revoked: Bool { connection.status == "revoked" }

    var body: some View {
        FeatureEmptyState(
            mood: revoked ? .sad : .puzzled,
            title: revoked ? Text(features: "Verbindung beendet") : Text(features: "Nicht verbunden"),
            message: revoked
                ? Text(features: "UwUMail hat die Verbindung zu deinem Konto beendet. Verbinde es im Web-Tresor neu.")
                : Text(features: "Maskierte Adressen kommen von UwUMail: für jede Website eine eigene Adresse, die an dein Postfach weiterleitet. Verbinde dein Konto einmal im Web-Tresor mit UwUMail, dann kannst du sie hier anlegen.")
        ) {
            Button {
                Task {
                    guard !FeatureFixtures.isOn else { return }
                    do {
                        try await model.core.openWebVaultAt(place: .masked)
                    } catch {
                        FeatureToasts.shared.show(error: error)
                    }
                }
            } label: {
                Label(features: "Im Web-Tresor verbinden", uwuIcon: .openExternal)
            }
            .uwuProminentButton()
        }
    }
}

#if DEBUG
#Preview("Maskierte Adressen") {
    NavigationStack {
        MaskedAddressesView()
    }
    .environment(AppModel())
    .onAppear { UserDefaults.standard.set(true, forKey: "UwULockFixtures") }
}

#Preview("Nicht verbunden") {
    MaskedNotConnectedView(connection: MaskedFixtures.notConnected)
        .environment(AppModel())
}

#Preview("Verbindung beendet") {
    MaskedNotConnectedView(connection: MaskedFixtures.revoked)
        .environment(AppModel())
}
#endif
