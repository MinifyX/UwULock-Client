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
        Group {
            if showsList {
                NavigationSplitView(columnVisibility: $columns) {
                    sidebar
                } content: {
                    ItemListView(place: listPlace, selection: $vault.selectedItemId)
                        .navigationSplitViewColumnWidth(min: 260, ideal: 320, max: 440)
                } detail: {
                    detail
                }
            } else {
                // A page that isn't a list takes list and detail together, the sidebar stays.
                NavigationSplitView(columnVisibility: $columns) {
                    sidebar
                } detail: {
                    NavigationStack { VaultFeaturePage(place: place) }
                }
            }
        }
        .searchable(text: $vault.query, placement: .sidebar, prompt: Text(vt("Im Tresor suchen")))
        .searchFocused($searchFocused)
        .toolbar { toolbar }
        .background { shortcuts }
        .onChange(of: model.selection) { _, _ in vault.query = "" }
        // The search started on a page keeps its field when the list comes in.
        .onChange(of: showsList) { _, list in if list, !vault.query.isEmpty { searchFocused = true } }
        .vaultDialogs(place: $model.selection)
        #if DEBUG
        .task {
            // After the first load, which picks its own selection.
            while !vault.loaded { try? await Task.sleep(for: .milliseconds(100)) }
            if let place = DebugLaunch.place { model.selection = place }
            if let item = DebugLaunch.item { vault.selectedItemId = item }
            if let query = DebugLaunch.query { vault.query = query }
            DebugLaunch.openSheet(vault)
        }
        #endif
    }

    private var sidebar: some View {
        @Bindable var model = model
        return VaultSidebar(selection: $model.selection)
            .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 320)
    }

    @ViewBuilder
    private var detail: some View {
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
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if UwuStore.shared.status.travel.enabled {
            ToolbarItem(placement: .primaryAction) { TravelModeBadge() }
        }
        ToolbarItemGroup(placement: .primaryAction) {
            Button {
                vault.showGenerator = true
            } label: {
                Label(vt("Generator"), uwuIcon: .generate)
            }
            .help(vt("Passwort-Generator"))
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

    /// Keyboard shortcuts with no button of their own: a new item and the search. Copying and
    /// the generator are the menu bar's (AppCommands).
    private var shortcuts: some View {
        Group {
            Button(vt("Neuer Eintrag")) { vault.newItem(in: place) }
                .keyboardShortcut("n", modifiers: .command)
            Button(vt("Suchen")) { searchFocused = true }
                .keyboardShortcut("f", modifiers: .command)
        }
        .frame(width: 0, height: 0)
        .opacity(0)
        .accessibilityHidden(true)
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
