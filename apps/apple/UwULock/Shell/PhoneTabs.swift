#if os(iOS)
import SwiftUI
import UwUDesign

/// iPhone: the tabs of the mobile app (src/mobile/nav.ts): vault, check, generator, settings,
/// plus the system's search tab.
struct PhoneTabs: View {
    enum Page: Hashable { case vault, check, generator, settings, search }

    @State private var page = Page.vault
    @State private var search = ""

    var body: some View {
        TabView(selection: $page) {
            Tab(value: Page.vault) {
                NavigationStack { PhoneVaultOverview() }
            } label: {
                Label("Tresor", uwuIcon: .vault)
            }
            Tab(value: Page.check) {
                NavigationStack { PhonePlaceholder(title: "Passwortprüfung", mood: .happy) }
            } label: {
                Label("Prüfung", uwuIcon: .securityCheck)
            }
            Tab(value: Page.generator) {
                NavigationStack { PhonePlaceholder(title: "Passwort-Generator", mood: .sparkle) }
            } label: {
                Label("Generator", uwuIcon: .generate)
            }
            Tab(value: Page.settings) {
                NavigationStack { SettingsView() }
            } label: {
                Label("Einstellungen", uwuIcon: .settings)
            }
            Tab(value: Page.search, role: .search) {
                NavigationStack {
                    PhonePlaceholder(title: "Suchen", mood: .puzzled)
                        .searchable(text: $search, prompt: Text("Im Tresor suchen"))
                }
            }
        }
        .tabBarMinimizeBehavior(.onScrollDown)
    }
}

/// The vault tab's first page: the sidebar's places as a list.
struct PhoneVaultOverview: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        List {
            ForEach(SidebarSection.all) { section in
                Section {
                    ForEach(section.items, id: \.self) { item in
                        NavigationLink(value: item) {
                            Label {
                                Text(item.title)
                            } icon: {
                                UwUIcon(item.icon).foregroundStyle(.tint)
                            }
                        }
                    }
                } header: {
                    if let title = section.title { Text(title) }
                }
            }
        }
        .navigationTitle(Text("Tresor"))
        .navigationDestination(for: SidebarItem.self) { ItemListPlaceholder(place: $0) }
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Sperren", systemImage: "lock") { model.lock() }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    // Milestone 4: the editor.
                } label: {
                    Label("Neuer Eintrag", uwuIcon: .add)
                }
            }
        }
    }
}

struct PhonePlaceholder: View {
    let title: LocalizedStringResource
    let mood: NyuMood

    var body: some View {
        VStack(spacing: UwUSpacing.md) {
            NyuView(mood: mood).frame(width: 110)
            Text("Kommt bald.").uwuFont(.headline)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .navigationTitle(Text(title))
    }
}
#endif
