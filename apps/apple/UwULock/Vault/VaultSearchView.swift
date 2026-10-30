import SwiftUI
import UwUDesign

/// Searching the vault on a phone (mobile/pages/Search.tsx): before anything is typed the items
/// opened last and a few lists to jump to, then the hits.
struct VaultSearchView: View {
    @Environment(VaultModel.self) private var vault
    @State private var query = ""

    private static let suggestions: [(label: String, place: SidebarItem)] = [
        (vt("Logins"), .type(.login)),
        (vt("Favoriten"), .favorites),
        (vt("Karten"), .type(.card)),
        (vt("Notizen"), .type(.note)),
        (vt("Fällig"), .due),
    ]

    private var clean: String { query.trimmingCharacters(in: .whitespaces) }

    var body: some View {
        let found = clean.isEmpty ? [] : vault.visible(.all, query: clean)
        List {
            if clean.isEmpty {
                let recent = vault.recent.compactMap { vault.item($0) }.filter { !$0.deleted }.prefix(3)
                if !recent.isEmpty {
                    Section(vt("Zuletzt geöffnet")) {
                        ForEach(Array(recent)) { item in
                            NavigationLink(value: item.id) { ItemRow(item: item) }
                        }
                    }
                }
                Section(vt("Vorschläge")) {
                    FlowLayout(spacing: UwUSpacing.xs) {
                        ForEach(Self.suggestions, id: \.label) { suggestion in
                            NavigationLink(value: suggestion.place) {
                                Label(suggestion.label, uwuIcon: suggestion.place.icon)
                                    .uwuFont(.subheadline, weight: .medium)
                                    .padding(.horizontal, UwUSpacing.sm)
                                    .padding(.vertical, UwUSpacing.xs)
                                    .uwuGlass(in: Capsule())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .listRowBackground(Color.clear)
                }
            } else if !found.isEmpty {
                Section(found.count == 1 ? vt("1 Treffer") : vt("\(found.count) Treffer")) {
                    ForEach(found) { item in
                        NavigationLink(value: item.id) { ItemRow(item: item) }
                    }
                }
            }
        }
        .overlay {
            if !clean.isEmpty, found.isEmpty {
                ContentUnavailableView {
                    VStack(spacing: UwUSpacing.sm) {
                        NyuView(mood: .puzzled).frame(width: 88)
                        Text(vt("Nichts gefunden")).uwuFont(.headline)
                    }
                } description: {
                    Text(vt("Kein Eintrag passt zu „\(clean)“."))
                }
            }
        }
        .navigationTitle(vt("Suche"))
        .searchable(text: $query, prompt: Text(vt("Im Tresor suchen")))
        .navigationDestination(for: SidebarItem.self) { place in
            ItemListView(place: place, selection: .constant(nil), pushes: true)
        }
        .navigationDestination(for: String.self) { ItemDetailView(itemId: $0) }
    }
}
