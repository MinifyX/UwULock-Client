// UwUSSH's or UwURDP's records in the vault (SuitePane.tsx, mobile/pages/Suite.tsx): tabs by
// kind, the hosts by workspace and group, a record's detail (inspector on regular width, a
// sheet on the phone) and the editors.
import SwiftUI
import UwUDesign

struct SuiteView: View {
    enum Kind: Hashable, Sendable { case ssh, rdp }
    let kind: Kind

    var body: some View {
        SuiteScreen(space: kind == .ssh ? .ssh : .rdp)
            .id(kind)
    }
}

/// A tab of the list.
struct SuiteTab: Identifiable, Hashable {
    let kind: String
    let icon: UwUIconName
    var id: String { kind }

    var label: String {
        switch kind {
        case "host": featureString("Hosts")
        case "identity": featureString("Anmeldungen")
        case "key": featureString("SSH-Schlüssel")
        case "snippet": featureString("Snippets")
        default: featureString("Bekannte Hosts")
        }
    }

    static let all = [
        SuiteTab(kind: "host", icon: .computer), SuiteTab(kind: "identity", icon: .account),
        SuiteTab(kind: "key", icon: .sshKey), SuiteTab(kind: "snippet", icon: .terminal),
        SuiteTab(kind: "known_host", icon: .fingerprint),
    ]

    static func icon(_ kind: String, space: SuiteSpace) -> UwUIconName {
        switch kind {
        case "host": space == .rdp ? .computer : .terminal
        case "group": .folder
        case "identity": .account
        case "key": .sshKey
        case "snippet": .terminal
        case "known_host": .fingerprint
        case "port_forward": .network
        default: .secret
        }
    }
}

/// What a sheet edits or makes.
enum SuiteSheet: Identifiable {
    case host(SuiteRecord?)
    case group(SuiteRecord?)
    case identity(SuiteRecord?)
    case renameKey(SuiteRecord)
    case newKey
    case snippet(SuiteRecord?)
    case tunnel(hostId: String, SuiteRecord?)

    var id: String {
        switch self {
        case .host(let r): "host-\(r?.id ?? "new")"
        case .group(let r): "group-\(r?.id ?? "new")"
        case .identity(let r): "identity-\(r?.id ?? "new")"
        case .renameKey(let r): "key-\(r.id)"
        case .newKey: "key-new"
        case .snippet(let r): "snippet-\(r?.id ?? "new")"
        case .tunnel(let host, let r): "tunnel-\(host)-\(r?.id ?? "new")"
        }
    }

    /// The editor for a record's kind (nil: known hosts and secrets aren't edited).
    static func edit(_ record: SuiteRecord) -> SuiteSheet? {
        switch record.kind {
        case "host": .host(record)
        case "group": .group(record)
        case "identity": .identity(record)
        case "key": .renameKey(record)
        case "snippet": .snippet(record)
        case "port_forward":
            .tunnel(hostId: SuiteJSON.ref(record.data, "host_id") ?? "", record)
        default: nil
        }
    }
}

/// What the screen does for the rows, the detail and the sheets.
@MainActor
struct SuiteActions {
    var show: (String) -> Void
    var present: (SuiteSheet) -> Void
    var askDelete: (SuiteRecord) -> Void
    var saved: (String, String) -> Void
}

struct SuiteScreen: View {
    let space: SuiteSpace

    @Environment(AppModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var store: SuiteSpaceStore
    @State private var uwu = UwuStore.shared
    @State private var tab = "host"
    @State private var query = ""
    @State private var selection: String?
    @State private var inspectorOn = true
    @State private var sheet: SuiteSheet?
    @State private var deleting: SuiteRecord?
    @State private var deleted = 0

    init(space: SuiteSpace) {
        self.space = space
        _store = State(initialValue: SuiteSpaceStore(space: space))
    }

    private var compact: Bool {
        #if os(iOS)
        sizeClass == .compact
        #else
        false
        #endif
    }

    /// On the phone the detail is a sheet: the editors then open from it.
    private var detailIsSheet: Bool { compact && selection != nil }

    private var actions: SuiteActions {
        SuiteActions(
            show: { id in
                if let record = store.index[id] { reveal(record) }
            },
            present: { sheet = $0 },
            askDelete: { deleting = $0 },
            saved: { id, kind in
                let target = kind == "group" ? "host" : kind == "port_forward" ? "host" : kind
                if SuiteTab.all.contains(where: { $0.kind == target }) { tab = target }
                if kind != "port_forward" { selection = id }
            })
    }

    private func reveal(_ record: SuiteRecord) {
        let target = record.kind == "group" ? "host" : record.kind
        if SuiteTab.all.contains(where: { $0.kind == target }) { tab = target }
        selection = record.id
    }

    var body: some View {
        content
            .navigationTitle(Text(verbatim: space.title))
            .toolbar { toolbar }
            .inspector(isPresented: inspectorShown) {
                inspector
                    .inspectorColumnWidth(min: 300, ideal: 380, max: 520)
                    .presentationDetents([.medium, .large])
                    .suitePresenting(
                        active: detailIsSheet, store: store, sheet: $sheet, deleting: $deleting,
                        actions: actions, onDeleted: deletedRecord)
            }
            .suitePresenting(
                active: !detailIsSheet, store: store, sheet: $sheet, deleting: $deleting,
                actions: actions, onDeleted: deletedRecord)
            .sensoryFeedback(.success, trigger: deleted)
            .task {
                UwuStore.shared.attach(model.core)
                await store.attach(model.core)
            }
            .onChange(of: uwu.suiteChanges) {
                Task { await store.load() }
            }
            .onChange(of: tab) {
                if let selected = selection, let record = store.index[selected],
                    !(record.kind == tab || (tab == "host" && record.kind == "group"))
                {
                    selection = nil
                }
            }
            .featureToasts()
    }

    private func deletedRecord(_ record: SuiteRecord) {
        if selection == record.id { selection = nil }
        deleted += 1
    }

    private var inspectorShown: Binding<Bool> {
        Binding(
            get: {
                guard available, store.view?.exists == true else { return false }
                return compact ? selection != nil : inspectorOn
            },
            set: { on in
                if compact {
                    if !on { selection = nil }
                } else {
                    inspectorOn = on
                }
            })
    }

    private var available: Bool { uwu.status.has(.suite) }

    // MARK: List

    @ViewBuilder private var content: some View {
        if !available {
            FeatureEmptyState(
                mood: .sleepy, title: Text(verbatim: space.title),
                message: Text(features: "Dein Server bietet die UwU-Apps nicht an."))
        } else if let view = store.view {
            if view.exists {
                list
            } else {
                FeatureEmptyState(
                    mood: .puzzled, title: Text(verbatim: space.title),
                    message: Text(
                        features:
                            "Hier ist noch nichts von \(space.appName). Sobald \(space.appName) mit UwULock synchronisiert, erscheint alles hier – oder du fängst hier an."
                    )
                ) {
                    Button {
                        Task { await store.create() }
                    } label: {
                        if store.creating {
                            Text(features: "Einen Moment …")
                        } else {
                            Text(features: "Bereich anlegen")
                        }
                    }
                    .uwuProminentButton()
                    .disabled(store.creating)
                }
            }
        } else if let error = store.loadError {
            FeatureEmptyState(mood: .sad, title: Text(verbatim: space.title), message: Text(verbatim: error)) {
                Button {
                    Task { await store.load() }
                } label: {
                    Label(features: "Neu laden", uwuIcon: .refresh)
                }
                .uwuGlassButton()
            }
        } else {
            ProgressView { Text(features: "Lädt …") }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private var tabs: [SuiteTab] {
        SuiteTab.all.filter { item in
            item.kind == "host" || item.kind == "identity" || space == .ssh
                || store.records.contains { $0.kind == item.kind }
        }
    }

    private func count(_ kind: String) -> Int {
        store.records.filter { $0.kind == kind }.count
    }

    private var list: some View {
        List(selection: $selection) {
            if tab == "host" {
                hostRows
            } else {
                flatRows
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if tabs.count > 1 { tabBar }
        }
        .searchable(text: $query, prompt: Text(verbatim: featureString("\(searchSection) durchsuchen")))
        .overlay {
            if isEmpty { emptyList }
        }
        .refreshable { await store.load() }
        #if os(macOS)
        .onDeleteCommand {
            if let id = selection, let record = store.index[id], record.kind != "secret" {
                deleting = record
            }
        }
        #endif
        .uwuAnimation(value: tab)
    }

    private var searchSection: String {
        tabs.first { $0.kind == tab }?.label ?? space.title
    }

    @ViewBuilder private var tabBar: some View {
        if compact {
            ScrollView(.horizontal, showsIndicators: false) {
                UwUGlassContainer(spacing: UwUSpacing.xs) {
                    HStack(spacing: UwUSpacing.xs) {
                        ForEach(tabs) { item in
                            tabButton(item)
                        }
                    }
                    .padding(.horizontal, UwUSpacing.md)
                    .padding(.vertical, UwUSpacing.xs)
                }
            }
            .background(.bar)
        } else {
            Picker(selection: $tab) {
                ForEach(tabs) { item in
                    Text(verbatim: "\(item.label) \(count(item.kind))").tag(item.kind)
                }
            } label: {
                Text(features: "Bereich")
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(.horizontal, UwUSpacing.md)
            .padding(.vertical, UwUSpacing.xs)
            .background(.bar)
        }
    }

    @ViewBuilder private func tabButton(_ item: SuiteTab) -> some View {
        let on = item.kind == tab
        let button = Button {
            withUwUAnimation(reduceMotion: reduceMotion) { tab = item.kind }
        } label: {
            Label {
                Text(verbatim: "\(item.label) \(count(item.kind))")
            } icon: {
                UwUIcon(item.icon, size: .sm)
            }
            .uwuFont(.subheadline, weight: on ? .semibold : .regular)
        }
        .accessibilityAddTraits(on ? .isSelected : [])
        if on {
            button.uwuProminentButton()
        } else {
            button.uwuGlassButton()
        }
    }

    private var grouped: [SuiteWorkspace] { suiteWorkspaceList(store.records, query: query) }

    private var flat: [SuiteRecord] {
        let words = suiteWords(query)
        return suiteOfKind(store.records, tab).filter { record in
            guard !words.isEmpty else { return true }
            let hay = [
                record.title, SuiteJSON.str(record.data, "username"), SuiteJSON.str(record.data, "body"),
                SuiteJSON.str(record.data, "fingerprint_sha256"),
            ].joined(separator: " ").lowercased()
            return words.allSatisfy { hay.contains($0) }
        }
    }

    private var isEmpty: Bool { tab == "host" ? grouped.isEmpty : flat.isEmpty }

    @ViewBuilder private var emptyList: some View {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        if !trimmed.isEmpty {
            FeatureEmptyState(mood: .puzzled, title: Text(features: "Nichts gefunden für „\(trimmed)“."))
        } else if !store.records.contains(where: { $0.kind != "secret" }) {
            FeatureEmptyState(mood: .sleepy, title: Text(features: "Noch nichts hier. Leg oben rechts etwas an."))
        } else {
            FeatureEmptyState(mood: .sleepy, title: Text(features: "Hier ist nichts. (˘ω˘)"))
        }
    }

    @ViewBuilder private var hostRows: some View {
        let index = store.index
        ForEach(grouped) { workspace in
            Section {
                ForEach(workspace.groups) { group in
                    if let record = group.group {
                        row(record, subtitle: nil, count: group.hosts.count)
                    } else {
                        Text(features: "Ohne Gruppe")
                            .uwuFont(.subheadline, weight: .semibold)
                            .foregroundStyle(Color.uwuMuted)
                            .selectionDisabled()
                    }
                    ForEach(group.hosts) { host in
                        row(host, subtitle: hostSubtitle(host, index), indent: group.group != nil)
                    }
                }
            } header: {
                Text(verbatim: suiteWorkspaceLabel(workspace.workspace))
            }
        }
    }

    @ViewBuilder private var flatRows: some View {
        ForEach(flat) { record in
            row(record, subtitle: subtitle(record))
        }
    }

    private func hostSubtitle(_ host: SuiteRecord, _ index: [String: SuiteRecord]) -> String {
        let user = SuiteJSON.str(suiteLoginOf(host, index)?.data, "username")
        let at = suiteHostPort(
            SuiteJSON.str(host.data, "address"), SuiteJSON.int(host.data, "port", space.defaultPort))
        return user.isEmpty ? at : "\(user)@\(at)"
    }

    private func subtitle(_ record: SuiteRecord) -> String {
        let d = record.data
        switch record.kind {
        case "identity":
            let who = [SuiteJSON.str(d, "domain"), SuiteJSON.str(d, "username")].filter { !$0.isEmpty }
                .joined(separator: "\\")
            return "\(who) · \(suiteAuthLabel(SuiteJSON.str(d, "auth_type")))"
        case "key":
            return SuiteJSON.str(d, "key_type")
        case "snippet":
            let path = SuiteJSON.str(d, "group_path")
            return path.isEmpty
                ? String(SuiteJSON.str(d, "body").split(separator: "\n", omittingEmptySubsequences: false).first ?? "")
                : path
        case "known_host":
            return SuiteJSON.str(d, "fingerprint_sha256")
        default:
            return ""
        }
    }

    private func row(_ record: SuiteRecord, subtitle: String?, count: Int? = nil, indent: Bool = false)
        -> some View
    {
        SuiteRow(record: record, icon: SuiteTab.icon(record.kind, space: space), subtitle: subtitle, count: count)
            .padding(.leading, indent ? UwUSpacing.md : 0)
            .tag(record.id)
            .contextMenu { menu(record) }
            #if os(iOS)
            // On the phone a list doesn't select by tap outside edit mode: the tap opens the sheet.
            .contentShape(Rectangle())
            .simultaneousGesture(TapGesture().onEnded { if compact { selection = record.id } })
            .swipeActions(edge: .trailing) {
                Button(role: .destructive) {
                    deleting = record
                } label: {
                    Label(features: "Löschen", uwuIcon: .delete)
                }
                .disabled(blocked(record))
                if let edit = editable(record) {
                    Button {
                        sheet = edit
                    } label: {
                        Label(features: "Bearbeiten", uwuIcon: .edit)
                    }
                    .tint(.uwuPinkSolid)
                }
            }
            #endif
    }

    /// The editor, when the record can be edited here.
    private func editable(_ record: SuiteRecord) -> SuiteSheet? {
        record.broken ? nil : SuiteSheet.edit(record)
    }

    /// Whether something that still exists points at the record.
    private func blocked(_ record: SuiteRecord) -> Bool {
        let index = store.index
        return (record.usedBy ?? []).contains { index[$0] != nil }
    }

    @ViewBuilder private func menu(_ record: SuiteRecord) -> some View {
        if let edit = editable(record) {
            Button {
                sheet = edit
            } label: {
                Label(features: "Bearbeiten", uwuIcon: .edit)
            }
        }
        if record.kind == "host", !record.broken {
            Button {
                Task { await copyHost(record) }
            } label: {
                if space == .ssh {
                    Label(features: "Befehl kopieren", uwuIcon: .copy)
                } else {
                    Label(features: "Adresse kopieren", uwuIcon: .copy)
                }
            }
            #if os(macOS)
            Button {
                Task { await store.openInApp(record.id) }
            } label: {
                Label(features: "In \(space.appName) öffnen", uwuIcon: .openExternal)
            }
            #endif
        }
        if record.kind == "host" || record.kind == "group" {
            let siblings = suiteSiblings(store.records, of: record)
            if siblings.count > 1 {
                Button {
                    Task { await store.save(suiteMoveOps(siblings, id: record.id, step: -1), done: nil) }
                } label: {
                    Label(features: "Nach oben", uwuIcon: .moveUp)
                }
                Button {
                    Task { await store.save(suiteMoveOps(siblings, id: record.id, step: 1), done: nil) }
                } label: {
                    Label(features: "Nach unten", uwuIcon: .moveDown)
                }
            }
        }
        if record.kind != "secret" {
            Divider()
            Button(role: .destructive) {
                deleting = record
            } label: {
                Label(features: "Löschen", uwuIcon: .delete)
            }
            .disabled(blocked(record))
        }
    }

    private func copyHost(_ host: SuiteRecord) async {
        if space == .ssh {
            await store.copyText(suiteSshCommand(host.data, suiteLoginOf(host, store.index)?.data))
        } else {
            await store.copyText(suiteRdpAddress(host.data))
        }
    }

    // MARK: Toolbar

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        if available, store.view?.exists == true {
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    newButton("host", label: featureString("Host"), icon: .computer)
                    newButton("group", label: featureString("Gruppe"), icon: .folder)
                    newButton("identity", label: featureString("Anmeldung"), icon: .account)
                    if space == .ssh {
                        newButton("key", label: featureString("Schlüssel"), icon: .sshKey)
                        newButton("snippet", label: featureString("Snippet"), icon: .terminal)
                    }
                } label: {
                    Label(features: "Neu", uwuIcon: .add)
                } primaryAction: {
                    open(tab == "known_host" ? "host" : tab)
                }
                .keyboardShortcut("n", modifiers: .command)
                .help(Text(features: "Neu"))
            }
            ToolbarItem {
                Button {
                    Task { await store.load() }
                } label: {
                    Label(features: "Neu laden", uwuIcon: .refresh)
                }
                .disabled(store.loading)
                .help(Text(features: "Neu laden"))
            }
            if !compact {
                ToolbarItem {
                    Button {
                        inspectorOn.toggle()
                    } label: {
                        Label(features: "Details", uwuIcon: .info)
                    }
                    .help(Text(features: "Details"))
                }
            }
        }
    }

    private func newButton(_ kind: String, label: String, icon: UwUIconName) -> some View {
        Button {
            open(kind)
        } label: {
            Label {
                Text(verbatim: label)
            } icon: {
                UwUIcon(icon)
            }
        }
    }

    private func open(_ kind: String) {
        switch kind {
        case "host": sheet = .host(nil)
        case "group": sheet = .group(nil)
        case "identity": sheet = .identity(nil)
        case "key": sheet = .newKey
        case "snippet": sheet = .snippet(nil)
        default: break
        }
    }

    // MARK: Detail

    @ViewBuilder private var inspector: some View {
        if let id = selection, let record = store.index[id] {
            SuiteDetail(store: store, record: record, actions: actions)
        } else {
            FeatureEmptyState(mood: .uwu, title: Text(verbatim: space.title))
        }
    }
}

/// A row of the list: the kind's tile, the title, a line below, a warning when it didn't open.
struct SuiteRow: View {
    let record: SuiteRecord
    let icon: UwUIconName
    var subtitle: String?
    var count: Int?

    var body: some View {
        HStack(spacing: UwUSpacing.sm) {
            UwUIcon(icon, size: .sm)
                .foregroundStyle(Color.uwuPinkInk)
                .frame(width: 30, height: 30)
                .background(Color.uwuPinkTint, in: RoundedRectangle(cornerRadius: UwURadius.small, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: record.shownTitle)
                    .uwuFont(.body, weight: record.kind == "group" ? .semibold : .medium)
                    .lineLimit(1)
                if let subtitle, !subtitle.isEmpty {
                    Text(verbatim: subtitle)
                        .uwuFont(.caption)
                        .foregroundStyle(Color.uwuMuted)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            Spacer(minLength: 0)
            if record.broken {
                UwUIcon(.warning, size: .xs, label: Text(features: "(lässt sich nicht öffnen)"))
                    .foregroundStyle(Color.uwuWarningInk)
            }
            if let count {
                Text(verbatim: String(count))
                    .uwuFont(.caption, weight: .semibold)
                    .foregroundStyle(Color.uwuMuted)
                    .monospacedDigit()
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }
}

/// The sheets and the delete question, attached where they can be shown (the screen, or the
/// detail when that is itself a sheet).
private struct SuitePresenting: ViewModifier {
    let active: Bool
    let store: SuiteSpaceStore
    @Binding var sheet: SuiteSheet?
    @Binding var deleting: SuiteRecord?
    let actions: SuiteActions
    let onDeleted: (SuiteRecord) -> Void

    func body(content: Content) -> some View {
        content
            .sheet(item: active ? $sheet : .constant(nil)) { sheet in
                SuiteEditorHost(store: store, sheet: sheet, actions: actions)
            }
            .confirmationDialog(
                Text(features: "Löschen?"),
                isPresented: Binding(
                    get: { active && deleting != nil },
                    set: { if !$0 { deleting = nil } }),
                titleVisibility: .visible,
                presenting: deleting
            ) { record in
                Button(role: .destructive) {
                    Task {
                        if await store.save([.delete(id: record.id, seq: record.seq)], done: featureString("Gelöscht.")) {
                            onDeleted(record)
                        }
                    }
                } label: {
                    Text(features: "Löschen")
                }
                Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
            } message: { record in
                Text(verbatim: suiteDeleteLead(record, records: store.records) + "\n"
                    + featureString("Auf deinen anderen Geräten verschwindet es beim nächsten Synchronisieren."))
            }
    }
}

extension View {
    fileprivate func suitePresenting(
        active: Bool, store: SuiteSpaceStore, sheet: Binding<SuiteSheet?>, deleting: Binding<SuiteRecord?>,
        actions: SuiteActions, onDeleted: @escaping (SuiteRecord) -> Void
    ) -> some View {
        modifier(
            SuitePresenting(
                active: active, store: store, sheet: sheet, deleting: deleting, actions: actions,
                onDeleted: onDeleted))
    }
}

/// What deleting a record takes with it.
func suiteDeleteLead(_ record: SuiteRecord, records: [SuiteRecord]) -> String {
    let name = record.shownTitle
    switch record.kind {
    case "host":
        let tunnels = suiteTunnels(records, hostId: record.id).count
        return tunnels > 0
            ? featureString("„\(name)“ wird gelöscht, mit seinen \(tunnels) Weiterleitungen. Anmeldung und Schlüssel bleiben.")
            : featureString("„\(name)“ wird gelöscht. Anmeldung und Schlüssel bleiben.")
    case "group":
        let hosts = records.filter { $0.kind == "host" && SuiteJSON.ref($0.data, "group_id") == record.id }.count
        return featureString("„\(name)“ wird gelöscht. Ihre \(hosts) Hosts bleiben – dann ohne Gruppe.")
    case "identity":
        return featureString("„\(name)“ wird gelöscht, mit ihrem Passwort.")
    case "key":
        return featureString("„\(name)“ wird gelöscht, mit dem privaten Schlüssel und der Passphrase.")
    default:
        return featureString("„\(name)“ wird gelöscht.")
    }
}

#if DEBUG
#Preview("UwUSSH") {
    NavigationStack { SuiteView(kind: .ssh) }
        .environment(AppModel())
}

#Preview("UwURDP") {
    NavigationStack { SuiteView(kind: .rdp) }
        .environment(AppModel())
}
#endif
