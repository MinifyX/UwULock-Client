import SwiftUI
import UwUDesign

/// iPad and Mac: sidebar → list → detail, like Apple's Passwords app (components/VaultScreen.tsx).
/// The places that aren't item lists (password check, apps, extras) take list and detail together.
struct VaultSplitView: View {
    @Environment(AppModel.self) private var model
    @Environment(VaultModel.self) private var vault
    @State private var columns = NavigationSplitViewVisibility.all
    @FocusState private var searchFocused: Bool

    private var place: SidebarItem { model.selection ?? .all }
    /// A search always looks through the items, whatever page is on screen.
    private var listPlace: SidebarItem {
        place.isItemFilter ? place : .all
    }
    private var showsList: Bool {
        place.isItemFilter || !vault.query.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        @Bindable var model = model
        @Bindable var vault = vault
        NavigationSplitView(columnVisibility: $columns) {
            VaultSidebar(selection: $model.selection)
                .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 320)
        } content: {
            ItemListView(place: listPlace, selection: $vault.selectedItemId)
                .navigationSplitViewColumnWidth(min: 260, ideal: 320, max: 440)
        } detail: {
            detail
        }
        .searchable(text: $vault.query, placement: .sidebar, prompt: Text(vt("Im Tresor suchen")))
        .searchFocused($searchFocused)
        .toolbar { toolbar }
        .background { shortcuts }
        .onChange(of: model.selection) { _, _ in
            vault.query = ""
            columns = showsList ? .all : .doubleColumn
        }
        .onChange(of: showsList) { _, list in
            columns = list ? .all : .doubleColumn
        }
        .onAppear { columns = showsList ? .all : .doubleColumn }
        .vaultDialogs(place: $model.selection)
    }

    @ViewBuilder
    private var detail: some View {
        if showsList {
            if let id = vault.selectedItemId, vault.item(id) != nil {
                NavigationStack {
                    ItemDetailView(itemId: id).id(id)
                }
            } else {
                ContentUnavailableView {
                    VStack(spacing: UwUSpacing.sm) {
                        NyuView(mood: .sleepy).frame(width: 96)
                        Text(vt("Nichts ausgewählt")).uwuFont(.headline)
                    }
                } description: {
                    Text(vt("Wähle links einen Eintrag."))
                }
            }
        } else {
            NavigationStack { VaultFeaturePage(place: place) }
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            Button {
                vault.showGenerator = true
            } label: {
                Label(vt("Generator"), uwuIcon: .generate)
            }
            .help(vt("Passwort-Generator"))
            .keyboardShortcut("g", modifiers: [.command, .shift])
            Button {
                vault.sync()
            } label: {
                Label(vault.syncing ? vt("Synchronisiert …") : vt("Synchronisieren"), uwuIcon: .sync)
                    .symbolEffect(.rotate, isActive: vault.syncing)
            }
            .help(vt("Synchronisieren"))
            .disabled(vault.syncing)
            .keyboardShortcut("r", modifiers: [.command, .shift])
        }
    }

    /// Keyboard shortcuts with no button of their own: a new item, the search, and copying the
    /// selected login's username, password and code (VaultScreen.tsx's Ctrl+U/P/T).
    private var shortcuts: some View {
        Group {
            Button(vt("Neuer Eintrag")) { vault.newItem(in: place) }
                .keyboardShortcut("n", modifiers: .command)
            Button(vt("Suchen")) { searchFocused = true }
                .keyboardShortcut("f", modifiers: .command)
            Button(vt("Benutzername kopieren")) { copySelected("username") }
                .keyboardShortcut("u", modifiers: [.command, .shift])
            Button(vt("Passwort kopieren")) { copySelected("password") }
                .keyboardShortcut("p", modifiers: [.command, .shift])
            Button(vt("Code kopieren")) { copySelected("totp") }
                .keyboardShortcut("t", modifiers: [.command, .shift])
        }
        .frame(width: 0, height: 0)
        .opacity(0)
        .accessibilityHidden(true)
    }

    private func copySelected(_ field: String) {
        guard let item = vault.item(vault.selectedItemId), item.kind == .login, !item.deleted else { return }
        vault.copy(item.id, field: field)
    }
}

/// A page of the sidebar that isn't a list of items: the other screens' views.
struct VaultFeaturePage: View {
    let place: SidebarItem

    var body: some View {
        switch place {
        case .passwordCheck: PasswordCheckView()
        case .uwuSSH: SuiteView(kind: .ssh)
        case .uwuRDP: SuiteView(kind: .rdp)
        case .sends: SendsView()
        case .fileRequests: FileRequestsView()
        case .maskedAddresses: MaskedAddressesView()
        default: EmptyView()
        }
    }
}
