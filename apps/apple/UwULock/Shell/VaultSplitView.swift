import SwiftUI
import UwUDesign

/// iPad and Mac: sidebar → list → detail, like Apple's Passwords app.
struct VaultSplitView: View {
    @Environment(AppModel.self) private var model
    @State private var columns = NavigationSplitViewVisibility.all
    @State private var search = ""

    var body: some View {
        @Bindable var model = model
        NavigationSplitView(columnVisibility: $columns) {
            VaultSidebar(selection: $model.selection)
                .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 320)
        } content: {
            ItemListPlaceholder(place: model.selection ?? .all)
                .navigationSplitViewColumnWidth(min: 260, ideal: 320, max: 420)
        } detail: {
            DetailPlaceholder()
        }
        .searchable(text: $search, placement: .sidebar, prompt: Text("Im Tresor suchen"))
    }
}

struct VaultSidebar: View {
    @Binding var selection: SidebarItem?
    @Environment(AppModel.self) private var model

    var body: some View {
        List(selection: $selection) {
            ForEach(SidebarSection.all) { section in
                if let title = section.title {
                    Section(title) { rows(section) }
                } else {
                    Section { rows(section) }
                }
            }
        }
        .navigationTitle(Text("Tresor"))
        .toolbar {
            ToolbarItem {
                Button("Sperren", systemImage: "lock") { model.lock() }
                    .labelStyle(.iconOnly)
                    .help(Text("Sperren"))
            }
        }
    }

    @ViewBuilder
    private func rows(_ section: SidebarSection) -> some View {
        ForEach(section.items, id: \.self) { item in
            Label {
                Text(item.title)
            } icon: {
                UwUIcon(item.icon)
            }
            .tag(item)
        }
        if section.id == "folders" {
            Button {
                // Milestone 4: create a folder.
            } label: {
                Label("Neuer Ordner", uwuIcon: .newFolder)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
        }
    }
}

/// The middle column until the vault arrives from the Rust core.
struct ItemListPlaceholder: View {
    let place: SidebarItem

    var body: some View {
        VStack(spacing: UwUSpacing.md) {
            NyuView(mood: .puzzled).frame(width: 96)
            Text("Hier ist noch nichts.")
                .uwuFont(.headline)
            Text("Sobald dein Tresor geladen ist, stehen die Einträge hier.")
                .uwuFont(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding(UwUSpacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .navigationTitle(Text(place.title))
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    // Milestone 4: the editor.
                } label: {
                    Label("Neuer Eintrag", uwuIcon: .add)
                }
                .help(Text("Neuer Eintrag"))
            }
        }
    }
}

struct DetailPlaceholder: View {
    var body: some View {
        ContentUnavailableView {
            Label("Nichts ausgewählt", uwuIcon: .vault, size: .xl)
        } description: {
            Text("Wähle links einen Eintrag.")
        }
    }
}
