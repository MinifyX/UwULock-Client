import SwiftUI
import UwUDesign

/// Dateianfragen (components/FileRequestsDialog.tsx, mobile/pages/Requests.tsx): links over which
/// somebody without an account uploads files and a message, encrypted for this account. The list
/// with how many uploads are new; a request's page (link, uploads, rules) opens beside it (Mac,
/// iPad) or pushed (iPhone); the editor and an upload open as sheets.
struct FileRequestsView: View {
    @Environment(AppModel.self) private var model
    @State private var store = FileRequestsModel()
    @State private var uwu = UwuStore.shared
    @State private var selection: String?
    @State private var editing: FileRequestEditorTarget?
    @State private var deleting: FileRequest?
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    var body: some View {
        content
            .navigationTitle(Text(features: "Dateianfragen"))
            .toolbar { toolbar }
            .sheet(item: $editing) { target in
                FileRequestEditor(store: store, request: target.request) { saved, created in
                    editing = nil
                    if created {
                        #if os(iOS)
                        // A new request's link is what the person wants next (Requests.tsx).
                        if let link = saved.link { Task { await store.copyLink(link) } }
                        #endif
                        selection = saved.id
                    }
                }
            }
            .confirmationDialog(
                Text(features: "Dateianfrage löschen?"),
                isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                titleVisibility: .visible,
                presenting: deleting
            ) { request in
                Button(role: .destructive) {
                    Task { await delete(request) }
                } label: {
                    Text(features: "Löschen")
                }
                Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
            } message: { _ in
                Text(features: "Die Anfrage, ihr Link und alles, was darüber hochgeladen wurde, verschwinden vom Server.")
            }
            .featureToasts()
            .task {
                UwuStore.shared.attach(model.core)
                store.attach(model.core)
                await store.load()
            }
            .onChange(of: uwu.notices) { Task { await store.reloadAll() } }
            .onChange(of: uwu.vaultChanges) { Task { await store.reloadAll() } }
            .onChange(of: uwu.statusChanges) { Task { await store.load() } }
    }

    @ViewBuilder
    private var content: some View {
        if !uwu.status.has(.fileRequests) {
            FeatureEmptyState(
                mood: .sleepy, title: Text(features: "Keine Dateianfragen"),
                message: Text(features: "Dieser Server bietet keine Dateianfragen an."))
        } else if store.requests == nil {
            ProgressView { Text(features: "Einen Moment …") }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if store.sorted.isEmpty {
            FeatureEmptyState(
                mood: store.error == nil ? .uwu : .sad,
                title: Text(features: "Noch keine Dateianfragen"),
                message: store.error.map { Text(verbatim: $0) } ?? Text(features: "Mit einer Dateianfrage schickst du jemandem einen Link, über den er dir Dateien und eine Nachricht hochlädt – verschlüsselt, nur du kannst sie öffnen. Ein Konto braucht er dafür nicht.")
            ) {
                Button {
                    editing = FileRequestEditorTarget(request: nil)
                } label: {
                    Label(features: "Neue Dateianfrage", uwuIcon: .add)
                }
                .uwuProminentButton()
                if store.error != nil {
                    Button {
                        Task { await store.load() }
                    } label: {
                        Label(features: "Nochmal versuchen", uwuIcon: .refresh)
                    }
                    .uwuGlassButton()
                }
            }
        } else if usesInspector {
            list
                .inspector(isPresented: inspectorShown) {
                    if let id = selection, store.request(id) != nil {
                        FileRequestDetailView(store: store, requestID: id) {
                            editing = FileRequestEditorTarget(request: $0)
                        } onDeleted: {
                            selection = nil
                        }
                        .inspectorColumnWidth(min: 340, ideal: 420, max: 560)
                    }
                }
        } else {
            list
        }
    }

    private var usesInspector: Bool {
        #if os(iOS)
        sizeClass == .regular
        #else
        true
        #endif
    }

    private var inspectorShown: Binding<Bool> {
        Binding(
            get: { selection.flatMap { store.request($0) } != nil },
            set: { if !$0 { selection = nil } })
    }

    // MARK: The list

    private var list: some View {
        List(selection: usesInspector ? $selection : .constant(nil)) {
            if let error = store.error {
                Section {
                    Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                        .foregroundStyle(Color.uwuDangerInk)
                }
            }
            Section {
                ForEach(store.sorted) { request in
                    row(request)
                }
            } footer: {
                Text(features: "Andere laden Dateien verschlüsselt zu dir hoch")
            }
        }
        #if os(macOS)
        .listStyle(.inset)
        .onDeleteCommand {
            if let request = store.request(selection) { deleting = request }
        }
        #else
        .listStyle(.insetGrouped)
        #endif
        .refreshable { await store.reloadAll() }
    }

    @ViewBuilder
    private func row(_ request: FileRequest) -> some View {
        if usesInspector {
            FileRequestRow(request: request)
                .tag(request.id)
                .contextMenu { menu(request) }
                .swipeActions(edge: .trailing) { swipes(request) }
        } else {
            NavigationLink {
                FileRequestDetailView(
                    store: store, requestID: request.id,
                    onEdit: { editing = FileRequestEditorTarget(request: $0) },
                    onDeleted: {}, pushed: true)
            } label: {
                FileRequestRow(request: request)
            }
            .contextMenu { menu(request) }
            .swipeActions(edge: .trailing) { swipes(request) }
            .swipeActions(edge: .leading) {
                if let link = request.link {
                    Button {
                        Task { await store.copyLink(link) }
                    } label: {
                        Label(features: "Link kopieren", uwuIcon: .copy)
                    }
                    .tint(.uwuPinkSolid)
                }
            }
        }
    }

    @ViewBuilder
    private func swipes(_ request: FileRequest) -> some View {
        Button(role: .destructive) {
            deleting = request
        } label: {
            Label(features: "Löschen", uwuIcon: .delete)
        }
        Button {
            editing = FileRequestEditorTarget(request: request)
        } label: {
            Label(features: "Ändern", uwuIcon: .edit)
        }
    }

    @ViewBuilder
    private func menu(_ request: FileRequest) -> some View {
        if let link = request.link {
            Button {
                Task { await store.copyLink(link) }
            } label: {
                Label(features: "Link kopieren", uwuIcon: .copy)
            }
            if let url = URL(string: link) {
                ShareLink(item: url) {
                    Label(features: "Link teilen", uwuIcon: .share)
                }
            }
            Divider()
        }
        Button {
            editing = FileRequestEditorTarget(request: request)
        } label: {
            Label(features: "Ändern", uwuIcon: .edit)
        }
        Button {
            Task {
                do {
                    try await store.change(request) { $0.disabled = !request.disabled }
                    FeatureToasts.shared.show(
                        request.disabled
                            ? featureString("Wieder an ✧")
                            : featureString("Abgeschaltet: der Link nimmt nichts mehr an"))
                } catch {
                    FeatureToasts.shared.show(error: error)
                }
            }
        } label: {
            if request.disabled {
                Label(features: "Wieder einschalten", uwuIcon: .start)
            } else {
                Label(features: "Abschalten", uwuIcon: .pending)
            }
        }
        Divider()
        Button(role: .destructive) {
            deleting = request
        } label: {
            Label(features: "Dateianfrage löschen", uwuIcon: .delete)
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            Button {
                editing = FileRequestEditorTarget(request: nil)
            } label: {
                Label(features: "Neue Dateianfrage", uwuIcon: .add)
            }
            .keyboardShortcut("n", modifiers: .command)
            .help(Text(features: "Neue Dateianfrage"))
            .disabled(!uwu.status.has(.fileRequests))
        }
        ToolbarItem(placement: .secondaryAction) {
            Button {
                Task { await store.openWebVault() }
            } label: {
                Label(features: "Im Web-Tresor öffnen", uwuIcon: .openExternal)
            }
            .help(Text(features: "Im Web-Tresor öffnen"))
            .disabled(!uwu.status.has(.fileRequests))
        }
        #if os(macOS)
        ToolbarItem(placement: .secondaryAction) {
            Button {
                Task { await store.reloadAll() }
            } label: {
                Label(features: "Neu laden", uwuIcon: .refresh)
            }
            .keyboardShortcut("r", modifiers: .command)
            .help(Text(features: "Neu laden"))
        }
        #endif
    }

    private func delete(_ request: FileRequest) async {
        do {
            try await store.delete(request)
            if selection == request.id { selection = nil }
            FeatureToasts.shared.show(featureString("Dateianfrage gelöscht"))
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }
}

/// Which request the editor shows; nil: a new one.
struct FileRequestEditorTarget: Identifiable {
    let id = UUID()
    let request: FileRequest?
}

/// A request in the list: name, uploads and validity, new uploads or its state.
struct FileRequestRow: View {
    let request: FileRequest

    var body: some View {
        HStack(spacing: UwUSpacing.sm) {
            UwUIcon(.inbox, size: .md)
                .foregroundStyle(Color.uwuPinkInk)
                .frame(width: 34, height: 34)
                .background(Color.uwuPinkTint, in: RoundedRectangle(cornerRadius: UwURadius.small, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: FileRequestText.name(request))
                    .uwuFont(.body, weight: .medium)
                    .lineLimit(1)
                Text(verbatim: FileRequestText.subtitle(request))
                    .uwuFont(.subheadline)
                    .foregroundStyle(Color.uwuMuted)
                    .lineLimit(2)
            }
            Spacer(minLength: UwUSpacing.xs)
            if request.foreignKey {
                UwUIcon(.warning, size: .sm)
                    .foregroundStyle(Color.uwuDangerInk)
                    .accessibilityLabel(Text(features: "Zurückgehalten: nicht für deinen Schlüssel."))
            }
            if request.unseen > 0 {
                FileRequestUnseenBadge(count: request.unseen)
            } else if FileRequestText.state(request) != .on {
                FileRequestStateChip(FileRequestText.state(request))
            }
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
    }
}

#if DEBUG
#Preview("Dateianfragen") {
    NavigationStack {
        FileRequestsView()
    }
    .environment(AppModel())
}

#Preview("Zeile") {
    List {
        ForEach(FileRequestFixtures.requests) { FileRequestRow(request: $0) }
    }
}
#endif
