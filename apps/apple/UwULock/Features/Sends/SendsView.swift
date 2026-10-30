import SwiftUI
import UwUDesign

/// Sends: a text or a file behind a link, for somebody without an account (SendsDialog.tsx,
/// mobile/pages/Sends.tsx). The owner sees their Sends, makes new ones, changes them, copies or
/// shares a link again, opens one to anybody again and deletes them. A Send's details show in
/// the inspector (a sheet on the iPhone), the editor in a sheet.
struct SendsView: View {
    @Environment(AppModel.self) private var model
    @State private var store = SendsStore()
    @State private var uwu = UwuStore.shared
    @State private var selection: Send.ID?
    @State private var editing: SendsEditorTarget?
    @State private var deleting: Send?
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    var body: some View {
        content
            .navigationTitle(Text(features: "Sends"))
            .toolbar {
                if uwu.status.sendsAvailable {
                    ToolbarItem(placement: .primaryAction) { newMenu }
                }
            }
            .inspector(isPresented: usesInspector ? inspectorShown : .constant(false)) {
                inspector
            }
            #if os(iOS)
            .sheet(isPresented: usesInspector ? .constant(false) : inspectorShown) {
                NavigationStack {
                    inspector
                        .navigationTitle(Text(features: "Send"))
                        .navigationBarTitleDisplayMode(.inline)
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button(role: .close) { selection = nil }
                            }
                        }
                }
            }
            #endif
            .sheet(item: $editing) { target in
                SendsEditorSheet(target: target, store: store) { id in
                    selection = id
                }
            }
            .confirmationDialog(
                Text(features: "Send löschen?"),
                isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                titleVisibility: .visible,
                presenting: deleting
            ) { send in
                Button(role: .destructive) {
                    Task {
                        if await store.delete(model.core, send), selection == send.id {
                            selection = nil
                        }
                    }
                } label: { Text(features: "Löschen") }
                Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
            } message: { _ in
                Text(features: "Der Link öffnet danach nichts mehr.")
            }
            .featureToasts()
            .task {
                UwuStore.shared.attach(model.core)
                await store.load(model.core)
            }
            .onChange(of: uwu.vaultChanges) { reload() }
            .onChange(of: uwu.statusChanges) { reload() }
            .onChange(of: store.sends) {
                // Deleted elsewhere while open: the details close.
                if let selection, store.sends != nil, store.send(selection) == nil {
                    self.selection = nil
                }
            }
            #if os(iOS)
            .sensoryFeedback(.selection, trigger: selection) { _, new in new != nil }
            #endif
    }

    private func reload() {
        Task { await store.load(model.core) }
    }

    // MARK: Content

    @ViewBuilder
    private var content: some View {
        if !uwu.status.sendsAvailable {
            FeatureEmptyState(
                mood: .sleepy, title: Text(features: "Keine Sends"),
                message: Text(features: "Dieser Server bietet keine Sends an."))
        } else if let sends = store.sends {
            if sends.isEmpty {
                FeatureEmptyState(
                    mood: store.error == nil ? .uwu : .sad,
                    title: Text(features: "Noch keine Sends"),
                    message: store.error.map { Text(verbatim: $0) } ?? Text(
                        features: "Ein Send ist ein Text oder eine Datei hinter einem Link – für jemanden ohne Konto, verschlüsselt, und nach einer Frist wieder weg.")
                ) {
                    UwUGlassContainer {
                        HStack {
                            Button { editing = .new(.text) } label: {
                                Label(features: "Text", uwuIcon: .add)
                            }
                            .uwuProminentButton()
                            .foregroundStyle(Color.uwuOnPink)
                            Button { editing = .new(.file) } label: {
                                Label(features: "Datei", uwuIcon: .add)
                            }
                            .uwuGlassButton()
                        }
                    }
                }
                .refreshable { await store.load(model.core) }
            } else {
                list(sends)
            }
        } else {
            ProgressView { Text(features: "Einen Moment …") }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func list(_ sends: [Send]) -> some View {
        List(selection: usesInspector ? $selection : .constant(nil)) {
            if let error = store.error {
                Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                    .foregroundStyle(Color.uwuDangerInk)
                    .uwuFont(.subheadline)
            }
            Section {
                ForEach(sends) { send in
                    row(send)
                        #if os(iOS)
                        .swipeActions(edge: .leading) {
                            Button {
                                Task { await SendsLink.copy(model.core, send.link) }
                            } label: { Label(features: "Link kopieren", uwuIcon: .copy) }
                            .tint(.uwuPinkSolid)
                        }
                        .swipeActions(edge: .trailing) {
                            Button(role: .destructive) { deleting = send } label: {
                                Label(features: "Löschen", uwuIcon: .delete)
                            }
                            Button { editing = .edit(send) } label: {
                                Label(features: "Bearbeiten", uwuIcon: .edit)
                            }
                        }
                        #endif
                }
            } footer: {
                Text(features: "Wer den Link hat, kann den Send öffnen. Abgelaufene Sends löscht der Server nach der Löschfrist.")
            }
        }
        .contextMenu(forSelectionType: Send.ID.self) { ids in
            if let id = ids.first, let send = store.send(id) {
                menu(for: send)
            }
        } primaryAction: { ids in
            guard let id = ids.first else { return }
            #if os(macOS)
            // A double click edits, as Finder opens.
            if let send = store.send(id) { editing = .edit(send) }
            #else
            selection = id
            #endif
        }
        #if os(macOS)
        .onDeleteCommand {
            if let send = store.send(selection) { deleting = send }
        }
        #endif
        .refreshable { await store.load(model.core) }
    }

    @ViewBuilder
    private func menu(for send: Send) -> some View {
        Button { editing = .edit(send) } label: { Label(features: "Bearbeiten", uwuIcon: .edit) }
        Button {
            Task { await SendsLink.copy(model.core, send.link) }
        } label: { Label(features: "Link kopieren", uwuIcon: .copy) }
        if let url = URL(string: send.link) {
            ShareLink(item: url) { Label(features: "Link teilen …", uwuIcon: .share) }
        }
        if send.authType != .anyone {
            Button {
                Task { await store.removeAuth(model.core, send) }
            } label: {
                send.authType == .emails
                    ? Label(features: "Adressen entfernen", uwuIcon: .unlocked)
                    : Label(features: "Passwort entfernen", uwuIcon: .unlocked)
            }
        }
        Divider()
        Button(role: .destructive) { deleting = send } label: {
            Label(features: "Löschen", uwuIcon: .delete)
        }
    }

    private var newMenu: some View {
        Menu {
            Button { editing = .new(.text) } label: { Label(features: "Neuer Text-Send", uwuIcon: .note) }
            Button { editing = .new(.file) } label: { Label(features: "Neuer Datei-Send", uwuIcon: .file) }
        } label: {
            Label(features: "Neuer Send", uwuIcon: .add)
        } primaryAction: {
            editing = .new(.text)
        }
        .keyboardShortcut("n", modifiers: .command)
        .help(Text(features: "Neuer Send"))
    }

    @ViewBuilder
    private func row(_ send: Send) -> some View {
        if usesInspector {
            SendsRow(send: send).tag(send.id)
        } else {
            // Compact width: the row opens the details in a sheet.
            Button { selection = send.id } label: { SendsRow(send: send) }
                .buttonStyle(.plain)
                .contentShape(Rectangle())
        }
    }

    // MARK: Details

    private var usesInspector: Bool {
        #if os(iOS)
        sizeClass == .regular
        #else
        true
        #endif
    }

    private var inspectorShown: Binding<Bool> {
        Binding(
            get: { selection != nil && store.send(selection) != nil },
            set: { if !$0 { selection = nil } })
    }

    @ViewBuilder
    private var inspector: some View {
        if let send = store.send(selection) {
            SendsDetailView(
                send: send, store: store,
                onEdit: { editing = .edit(send) },
                onDelete: { deleting = send })
                .inspectorColumnWidth(min: 300, ideal: 380, max: 520)
                .presentationDetents([.medium, .large])
        }
    }
}

/// What the editor sheet edits: a new Send of a kind, or one there is.
struct SendsEditorTarget: Identifiable {
    let id = UUID()
    let send: Send?
    let kind: SendKind

    static func new(_ kind: SendKind) -> SendsEditorTarget { SendsEditorTarget(send: nil, kind: kind) }
    static func edit(_ send: Send) -> SendsEditorTarget { SendsEditorTarget(send: send, kind: send.kind) }
}

// MARK: Row

/// A Send in the list: its kind, name, whether it opens and how often it was opened.
struct SendsRow: View {
    let send: Send

    var body: some View {
        HStack(spacing: UwUSpacing.sm) {
            SendsKindTile(kind: send.kind)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: UwUSpacing.xs) {
                    Text(verbatim: send.shownName)
                        .uwuFont(.body, weight: .medium)
                        .lineLimit(1)
                    if send.authType == .password {
                        UwUIcon(.locked, size: .xs, label: Text(features: "Mit Passwort"))
                            .foregroundStyle(Color.uwuMuted)
                    } else if send.authType == .emails {
                        UwUIcon(.account, size: .xs, label: Text(features: "Nur bestimmte Adressen"))
                            .foregroundStyle(Color.uwuMuted)
                    }
                }
                Text(verbatim: [send.statusText, send.openedText].joined(separator: " · "))
                    .uwuFont(.caption)
                    .foregroundStyle(Color.uwuMuted)
                    .lineLimit(1)
            }
            Spacer(minLength: UwUSpacing.xs)
            SendsStateChip(status: send.status)
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }
}

/// The kind's icon on a soft tile.
struct SendsKindTile: View {
    let kind: SendKind
    var large = false

    var body: some View {
        UwUIcon(kind == .file ? .file : .note, size: large ? .xl : .md)
            .foregroundStyle(Color.uwuPinkInk)
            .frame(width: large ? 56 : 34, height: large ? 56 : 34)
            .background(Color.uwuPinkTint, in: RoundedRectangle(cornerRadius: large ? 14 : 9, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// On, switched off, or no longer opening.
struct SendsStateChip: View {
    let status: SendsStatus

    var body: some View {
        Group {
            switch status {
            case .active: Text(features: "aktiv")
            case .disabled: Text(features: "abgeschaltet")
            case .expired, .usedUp: Text(features: "abgelaufen")
            }
        }
        .uwuFont(.caption2, weight: .semibold)
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .foregroundStyle(ink)
        .background(fill, in: Capsule())
    }

    private var ink: Color {
        switch status {
        case .active: .uwuSuccessInk
        case .disabled: .uwuMuted
        case .expired, .usedUp: .uwuWarningInk
        }
    }

    private var fill: Color {
        switch status {
        case .active: .uwuSuccessTint
        case .disabled: Color.uwuMuted.opacity(0.14)
        case .expired, .usedUp: .uwuWarningTint
        }
    }
}

#if DEBUG
#Preview("Sends") {
    UserDefaults.standard.set(true, forKey: "UwULockFixtures")
    return NavigationStack { SendsView() }
        .environment(AppModel())
}
#endif
