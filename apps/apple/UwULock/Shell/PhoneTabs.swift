#if os(iOS)
import SwiftUI
import UwUDesign

/// iPhone: the tabs of the mobile app (src/mobile/nav.ts): vault, check, generator, settings,
/// plus the system's search tab.
struct PhoneTabs: View {
    enum Page: Hashable { case vault, check, generator, settings, search }

    @Environment(AppModel.self) private var model
    @State private var page = Page.vault

    var body: some View {
        @Bindable var model = model
        TabView(selection: $page) {
            Tab(value: Page.vault) {
                NavigationStack { PhoneVaultOverview() }
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
                NavigationStack { VaultSearchView() }
            } label: {
                Label(vt("Suchen"), systemImage: "magnifyingglass")
            }
        }
        .tabBarMinimizeBehavior(.onScrollDown)
        .sensoryFeedback(.selection, trigger: page)
        .vaultDialogs(place: $model.selection)
    }
}

/// The vault tab's first page: the sidebar's places, then a list, then an item.
struct PhoneVaultOverview: View {
    @Environment(AppModel.self) private var model
    @Environment(VaultModel.self) private var vault

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
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    NewItemMenu(place: nil)
                }
            }
    }
}
#endif
