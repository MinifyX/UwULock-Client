import SwiftUI
import UwUDesign

/// The middle column: the items of a place, or the search's (VaultScreen.tsx's list,
/// mobile/pages/ItemList.tsx).
struct ItemListView: View {
    let place: SidebarItem
    @Binding var selection: String?
    /// iPhone: rows push the detail (`NavigationLink(value: id)`) instead of selecting it.
    var pushes = false
    @Environment(VaultModel.self) private var vault

    private var searching: Bool { !vault.query.trimmingCharacters(in: .whitespaces).isEmpty }

    var body: some View {
        let list = vault.visible(place)
        Group {
            // A selection binding would swallow the taps that should push (iPhone).
            if pushes {
                List { rows(list) }
            } else {
                List(selection: $selection) { rows(list) }
            }
        }
        .overlay {
            if vault.loaded, list.isEmpty { empty }
        }
        .refreshable { await syncAndWait() }
        .navigationTitle(searching ? vt("Suche") : vault.title(of: place))
        .navigationSubtitle(list.count == 1 ? vt("1 Eintrag") : vt("\(list.count) Einträge"))
        .toolbar { toolbar }
        .onChange(of: list.map(\.id)) { _, ids in keep(ids) }
        .onAppear { keep(list.map(\.id)) }
    }

    @ViewBuilder
    private func rows(_ list: [ItemSummary]) -> some View {
        ForEach(list) { item in
            if pushes {
                NavigationLink(value: item.id) { ItemRow(item: item) }
            } else {
                ItemRow(item: item)
                    .tag(item.id)
            }
        }
        if place == .trash, !list.isEmpty {
            Section {
            } footer: {
                Text(vt("Der Server hebt gelöschte Einträge 30 Tage auf, dann sind sie weg."))
            }
        }
    }

    /// Keeps the selection while it's still in the list, else the first item (desktop only).
    private func keep(_ ids: [String]) {
        #if os(macOS)
        if let selected = selection, ids.contains(selected) { return }
        selection = ids.first
        #endif
    }

    private func syncAndWait() async {
        vault.sync()
        while vault.syncing { try? await Task.sleep(for: .milliseconds(150)) }
    }

    @ViewBuilder
    private var empty: some View {
        if searching {
            emptyState(.puzzled, vt("Nichts gefunden"), vt("Nichts gefunden für „\(vault.query.trimmingCharacters(in: .whitespaces))“."))
        } else if vault.items.contains(where: { !$0.deleted }) || place == .trash {
            emptyState(.sleepy, vt("Leer"), vt("Hier ist nichts. (˘ω˘)"))
        } else {
            emptyState(.uwu, vt("Leer"), vt("Dein Tresor ist noch leer. Leg oben rechts den ersten Eintrag an."))
        }
    }

    private func emptyState(_ mood: NyuMood, _ title: String, _ text: String) -> some View {
        ContentUnavailableView {
            VStack(spacing: UwUSpacing.sm) {
                NyuView(mood: mood).frame(width: 88)
                Text(title).uwuFont(.headline)
            }
        } description: {
            Text(text)
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            if place != .trash, place != .due {
                NewItemMenu(place: place)
            }
        }
        ToolbarItem(placement: .secondaryAction) {
            Menu {
                Picker(selection: Bindable(vault).sort) {
                    ForEach(VaultSort.allCases) { sort in Text(sort.label).tag(sort) }
                } label: {
                    Text(vt("Sortieren nach"))
                }
                .pickerStyle(.inline)
            } label: {
                Label(vt("Sortieren"), systemImage: "arrow.up.arrow.down")
            }
            .help(vt("Sortieren"))
        }
    }
}

/// "Neu": a menu of the kinds, plus a new folder (VaultScreen.tsx, mobile NewItem.tsx).
struct NewItemMenu: View {
    let place: SidebarItem?
    @Environment(VaultModel.self) private var vault

    var body: some View {
        Menu {
            ForEach(ItemKind.ordered, id: \.self) { kind in
                Button {
                    vault.newItem(kind: kind, in: place)
                } label: {
                    Label(kind.label, uwuIcon: kind.icon)
                }
            }
            Divider()
            Button {
                vault.folderRequest = FolderRequest(folder: nil)
            } label: {
                Label(vt("Neuer Ordner"), uwuIcon: .newFolder)
            }
        } label: {
            Label(vt("Neuer Eintrag"), uwuIcon: .add)
        } primaryAction: {
            vault.newItem(in: place)
        }
        .help(vt("Neuer Eintrag"))
    }
}

/// One item in a list: tile, name, subtitle and its marks; swipe actions, context menu, drag.
struct ItemRow: View {
    let item: ItemSummary
    @Environment(VaultModel.self) private var vault

    var body: some View {
        HStack(spacing: UwUSpacing.sm) {
            ItemTile(item: item)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.name.isEmpty ? vt("(ohne Namen)") : item.name)
                    .uwuFont(.body, weight: .medium)
                    .lineLimit(1)
                if let subtitle = item.subtitle ?? item.host, !subtitle.isEmpty {
                    Text(subtitle)
                        .uwuFont(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            badges
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .draggable(item.id) {
            Label(item.name, uwuIcon: item.kind.icon).padding(UwUSpacing.xs)
        }
        .swipeActions(edge: .leading, allowsFullSwipe: true) {
            if !item.deleted {
                Button {
                    vault.toggleFavorite(item)
                } label: {
                    Label(item.favorite ? vt("Entfernen") : vt("Favorit"), uwuIcon: .favorite)
                }
                .tint(.uwuWarning)
            }
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if item.deleted {
                Button {
                    vault.restore(item)
                } label: {
                    Label(vt("Wiederherstellen"), uwuIcon: .restore)
                }
                .tint(.uwuSuccess)
            } else {
                Button(role: .destructive) {
                    vault.trash(item)
                } label: {
                    Label(vt("Löschen"), uwuIcon: .delete)
                }
                if let field = vault.mainField(item) {
                    Button {
                        vault.copy(item.id, field: field)
                    } label: {
                        Label(vt("Kopieren"), uwuIcon: .copy)
                    }
                    .tint(.uwuPinkSolid)
                }
            }
        }
        .contextMenu { ItemMenu(item: item) }
    }

    @ViewBuilder
    private var badges: some View {
        HStack(spacing: UwUSpacing.xxs) {
            if item.broken { badge(.warning, vt("Nicht alles ließ sich entschlüsseln"), .uwuWarningInk) }
            if item.reprompt { badge(.masterPassword, vt("Fragt nach dem Master-Passwort")) }
            if vault.due.contains(item.id) { badge(.reminder, vt("Neues Passwort fällig"), .uwuWarningInk) }
            if vault.uwu.masked[item.id] != nil { badge(.maskedAddress, vt("Mit maskierter Adresse")) }
            if item.hasTotp { badge(.oneTimeCode, vt("Mit Einmal-Code")) }
            if item.organizationId != nil { badge(.organization, vt("Organisation")) }
            if item.favorite { badge(.favorite, vt("Favorit"), .uwuPinkSolid) }
        }
    }

    private func badge(_ icon: UwUIconName, _ label: String, _ tint: Color = .secondary) -> some View {
        UwUIcon(icon, size: .xs, label: Text(label))
            .foregroundStyle(tint)
            .help(label)
    }
}

/// The actions of an item (mobile ItemRow.tsx's long-press menu, the desktop's context menu).
struct ItemMenu: View {
    let item: ItemSummary
    @Environment(VaultModel.self) private var vault

    var body: some View {
        if item.deleted {
            Button { vault.restore(item) } label: { Label(vt("Wiederherstellen"), uwuIcon: .restore) }
            Button(role: .destructive) { vault.itemToDelete = item } label: {
                Label(vt("Endgültig löschen …"), uwuIcon: .delete)
            }
        } else {
            if item.kind == .login {
                if item.hasPassword, item.viewPassword {
                    Button { vault.copy(item.id, field: "password") } label: { Label(vt("Passwort kopieren"), uwuIcon: .secret) }
                }
                if item.hasUsername {
                    Button { vault.copy(item.id, field: "username") } label: { Label(vt("Benutzername kopieren"), uwuIcon: .profile) }
                }
                if item.hasTotp {
                    Button { vault.copy(item.id, field: "totp") } label: { Label(vt("Code kopieren"), uwuIcon: .oneTimeCode) }
                }
                if item.host != nil {
                    Button { vault.openUri(item.id, index: 0) } label: { Label(vt("Website öffnen"), uwuIcon: .openExternal) }
                }
            } else if let field = vault.mainField(item) {
                Button { vault.copy(item.id, field: field) } label: { Label(vt("Kopieren"), uwuIcon: .copy) }
            }
            Divider()
            Button { vault.toggleFavorite(item) } label: {
                Label(item.favorite ? vt("Aus Favoriten entfernen") : vt("Zu Favoriten"), uwuIcon: .favorite)
            }
            Button { vault.edit(item) } label: { Label(vt("Bearbeiten"), uwuIcon: .edit) }
                .disabled(item.broken)
            if item.organizationId == nil {
                MoveMenu(item: item)
            }
            Divider()
            Button(role: .destructive) { vault.trash(item) } label: {
                Label(vt("In den Papierkorb"), uwuIcon: .delete)
            }
        }
    }
}

/// "Verschieben": the own vault's folders (mobile Move.tsx).
struct MoveMenu: View {
    let item: ItemSummary
    @Environment(VaultModel.self) private var vault

    var body: some View {
        Menu {
            Picker(selection: Binding(
                get: { item.folderId ?? "" },
                set: { vault.move(item, to: $0.isEmpty ? nil : $0) }
            )) {
                Text(vt("Ohne Ordner")).tag("")
                ForEach(vault.sortedFolders) { folder in Text(folder.name).tag(folder.id) }
            } label: {
                Text(vt("In Ordner verschieben"))
            }
            .pickerStyle(.inline)
        } label: {
            Label(vt("Verschieben"), uwuIcon: .move)
        }
    }
}
