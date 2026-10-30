#if os(iOS)
import SwiftUI
import UwUDesign

/// iPhone: the tabs of the mobile app (src/mobile/nav.ts): vault, check, generator, settings,
/// plus the system's search tab.
struct PhoneTabs: View {
    enum Page: Hashable { case vault, check, generator, settings, search }

    @Environment(AppModel.self) private var model
    @Environment(VaultModel.self) private var vault
    @State private var page = Page.vault
    @State private var vaultPath = NavigationPath()
    @State private var searchQuery = ""
    @FocusState private var searchFocused: Bool

    var body: some View {
        @Bindable var model = model
        TabView(selection: $page) {
            Tab(value: Page.vault) {
                NavigationStack(path: $vaultPath) { PhoneVaultOverview() }
            } label: {
                Label(vt("Tresor"), uwuIcon: .vault)
            }
            Tab(value: Page.check) {
                NavigationStack { PasswordCheckView() }
            } label: {
                Label(vt("Prüfung"), uwuIcon: .securityCheck)
            }
            Tab(value: Page.generator) {
                NavigationStack { GeneratorView() }
            } label: {
                Label(vt("Generator"), uwuIcon: .generate)
            }
            Tab(value: Page.settings) {
                NavigationStack { SettingsView() }
            } label: {
                Label(vt("Einstellungen"), uwuIcon: .settings)
            }
            Tab(value: Page.search, role: .search) {
                NavigationStack {
                    VaultSearchView(query: searchQuery)
                        // Pinned under the title: iOS 27 keeps the search tab in the bar and
                        // shows no field for `.searchable` on the TabView or the default placement.
                        .searchable(
                            text: $searchQuery,
                            placement: .navigationBarDrawer(displayMode: .always),
                            prompt: Text(vt("Im Tresor suchen")))
                        .searchFocused($searchFocused)
                }
            } label: {
                Label(vt("Suchen"), systemImage: "magnifyingglass")
            }
        }
        .tabBarMinimizeBehavior(.onScrollDown)
        .sensoryFeedback(.selection, trigger: page)
        // Opening the search tab goes straight to the field, as the system's search tab does.
        .onChange(of: page) { _, page in searchFocused = page == .search }
        .vaultDialogs(place: $model.selection)
        // Another screen opens an item (the check, a masked address, a taken-over file
        // request): the vault tab with the item on top.
        .environment(\.openVaultItem, PasswordCheckOpenItem { open($0) })
        .environment(\.passwordCheckOpenItem, PasswordCheckOpenItem { open($0) })
        #if DEBUG
        .task { debugLaunch() }
        #endif
    }

    private func open(_ id: String) {
        vault.query = ""
        page = .vault
        vaultPath = NavigationPath()
        vaultPath.append(id)
    }

    #if DEBUG
    private func debugLaunch() {
        switch DebugLaunch.tab {
        case "check": page = .check
        case "generator": page = .generator
        case "settings": page = .settings
        case "search": page = .search
        default: break
        }
        if let query = DebugLaunch.query { searchQuery = query }
        if let place = DebugLaunch.place {
            model.selection = place
            vaultPath.append(place)
        }
        if let item = DebugLaunch.item { vaultPath.append(item) }
        DebugLaunch.openSheet(vault)
    }
    #endif
}

/// The vault tab's first page: the sidebar's places, then a list, then an item.
struct PhoneVaultOverview: View {
    @Environment(AppModel.self) private var model
    @Environment(VaultModel.self) private var vault
    @State private var showAccounts = false

    var body: some View {
        @Bindable var model = model
        VaultSidebar(selection: $model.selection, pushes: true)
            .navigationDestination(for: SidebarItem.self) { place in
                if place.isItemFilter {
                    ItemListView(place: place, selection: .constant(nil), pushes: true)
                } else {
                    VaultFeaturePage(place: place)
                }
            }
            .navigationDestination(for: String.self) { ItemDetailView(itemId: $0) }
            .refreshable { vault.sync() }
            .navigationSubtitle(Text(SyncLine.text(model: model, vault: vault)))
            .sheet(isPresented: $showAccounts) { AccountsSheet() }
            .toolbar {
                if let status = model.status {
                    ToolbarItem(placement: .topBarLeading) {
                        Button {
                            showAccounts = true
                        } label: {
                            AccountAvatar(status: status, size: 28)
                        }
                        .accessibilityLabel(authStr("Konten"))
                    }
                    .sharedBackgroundVisibility(.hidden)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    NewItemMenu(place: nil)
                }
            }
    }
}
#endif
