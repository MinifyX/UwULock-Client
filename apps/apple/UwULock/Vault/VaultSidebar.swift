import SwiftUI
import UwUDesign

/// The sidebar (VaultScreen.tsx): the vault's places with their counts, types, folders,
/// organizations with their collections, the UwU apps, the extras and the trash.
struct VaultSidebar: View {
    @Binding var selection: SidebarItem?
    /// iPhone: rows push the place (`NavigationLink(value:)`) instead of selecting it.
    var pushes = false
    @Environment(VaultModel.self) private var vault
    @Environment(AppModel.self) private var model
    @AppStorage(AppSettingsKey.showTrash) private var showTrash = true
    @State private var cards = PromptCards()

    var body: some View {
        Group {
            // A selection binding would swallow the taps that should push (iPhone).
            if pushes {
                List {
                    if cards.visible {
                        Section {
                            VaultPromptCards(cards: cards)
                                .listRowInsets(EdgeInsets())
                                .listRowBackground(Color.clear)
                        }
                    }
                    places
                }
            } else {
                List(selection: $selection) { places }
                    // The account in the sidebar's bottom left (VaultScreen.tsx's AccountCard),
                    // the cards after an unlock above it.
                    .safeAreaInset(edge: .bottom, spacing: 0) {
                        VStack(alignment: .leading, spacing: UwUSpacing.sm) {
                            VaultPromptCards(cards: cards)
                            HStack(spacing: UwUSpacing.xs) {
                                if let status = model.status { AccountAvatar(status: status, size: 22) }
                                AccountMenu(nameOnly: true)
                                    .menuStyle(.button)
                                    .buttonStyle(.borderless)
                                    .menuIndicator(.hidden)
                                    .foregroundStyle(.primary)
                                Spacer(minLength: 0)
                            }
                                .padding(.horizontal, UwUSpacing.sm)
                                .padding(.vertical, UwUSpacing.xs)
                                .uwuGlass(in: RoundedRectangle(cornerRadius: UwURadius.control, style: .continuous))
                        }
                        .padding(UwUSpacing.sm)
                    }
            }
        }
        .promptCards(cards)
        .navigationTitle(vt("Tresor"))
        .toolbar {
            ToolbarItem {
                Button {
                    model.lock()
                } label: {
                    Label(vt("Sperren"), uwuIcon: .locked)
                }
                .help(vt("Sperren"))
            }
        }
    }

    /// The iPhone overview's favourites (Overview.tsx): the items themselves, or a note.
    @ViewBuilder
    private var favorites: some View {
        let list = vault.visible(.favorites, query: "")
        Section(vt("Favoriten")) {
            if list.isEmpty {
                Text(vt("Noch keine Favoriten")).foregroundStyle(.secondary)
            } else {
                ForEach(list) { item in
                    NavigationLink(value: item.id) { ItemRow(item: item) }
                }
            }
        }
    }

    @ViewBuilder
    private var places: some View {
            Section {
                row(.all)
                if !pushes { row(.favorites) }
                if vault.uwu.has(.reminders), vault.count(.due) > 0 || selection == .due {
                    row(.due)
                }
                // The iPhone has the check as a tab.
                if !pushes { row(.passwordCheck, count: false) }
                if pushes { TravelModeBadge(row: true) }
            }

            if pushes { favorites }

            Section(vt("Typen")) {
                ForEach(ItemKind.ordered.filter { $0 == .login || vault.count(.type($0)) > 0 }, id: \.self) { kind in
                    row(.type(kind))
                }
            }

            Section {
                ForEach(vault.sortedFolders) { folder in
                    row(.folder(folder.id))
                        .contextMenu {
                            Button { vault.folderRequest = FolderRequest(folder: folder) } label: {
                                Label(vt("Umbenennen"), uwuIcon: .edit)
                            }
                            Button(role: .destructive) { vault.folderToDelete = folder } label: {
                                Label(vt("Löschen"), uwuIcon: .delete)
                            }
                        }
                }
                if pushes || (!vault.sortedFolders.isEmpty && vault.count(.folder(nil)) > 0) {
                    row(.folder(nil))
                }
            } header: {
                HStack {
                    Text(vt("Ordner"))
                    Spacer()
                    Button {
                        vault.folderRequest = FolderRequest(folder: nil)
                    } label: {
                        UwUIcon(.newFolder, size: .sm, label: Text(vt("Neuer Ordner")))
                    }
                    .buttonStyle(.borderless)
                    .help(vt("Neuer Ordner"))
                }
            }

            ForEach(organizations) { org in
                Section {
                    row(.organization(org.id), title: vt("Alle Einträge"))
                    ForEach(collections(of: org.id)) { collection in
                        row(.collection(collection.id))
                    }
                } header: {
                    HStack {
                        Text(org.name)
                        Spacer()
                        Button {
                            vault.perform { try await vault.backend.openWebVaultAt(place: .organization, id: org.id) }
                        } label: {
                            UwUIcon(.openExternal, size: .sm, label: Text(vt("\(org.name) im Web-Tresor verwalten")))
                        }
                        .buttonStyle(.borderless)
                        .help(vt("Im Web-Tresor verwalten"))
                    }
                }
            }

            if vault.uwu.has(.suite) {
                Section(vt("UwU-Apps")) {
                    row(.uwuSSH, count: false)
                    row(.uwuRDP, count: false)
                }
            }

            if vault.uwu.has(.sends) || vault.uwu.has(.fileRequests) || vault.uwu.has(.maskedAddresses) {
                Section(vt("Extras")) {
                    if vault.uwu.has(.sends) { row(.sends, count: false) }
                    if vault.uwu.has(.fileRequests) {
                        row(.fileRequests, count: false)
                            .badge(vault.uwu.unseen.fileRequestSubmissions)
                    }
                    if vault.uwu.has(.maskedAddresses) { row(.maskedAddresses, count: false) }
                }
            }

            if showTrash, vault.count(.trash) > 0 || selection == .trash {
                Section { row(.trash) }
            }

            if pushes {
                Section {
                } footer: {
                    Text(vt("Zum Synchronisieren nach unten ziehen"))
                        .frame(maxWidth: .infinity)
                }
            }
    }

    private var organizations: [Organization] {
        (vault.overview?.organizations ?? []).sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    private func collections(of org: String) -> [VaultCollection] {
        (vault.overview?.collections ?? []).filter { $0.organizationId == org }
            .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    @ViewBuilder
    private func row(_ place: SidebarItem, title: String? = nil, count: Bool = true) -> some View {
        let label = Label(title ?? vault.title(of: place), uwuIcon: place.icon)
        Group {
            if pushes {
                NavigationLink(value: place) {
                    if count { label.badge(vault.count(place)) } else { label }
                }
            } else if count {
                label.badge(vault.count(place))
            } else {
                label
            }
        }
        .tag(place)
        .dropDestination(for: String.self) { ids, _ in
            guard case .folder(let folderId) = place else { return false }
            return vault.move(ids: ids, to: folderId)
        }
    }
}
