// A suite record's detail (SuitePane.tsx `RecordDetail`, `HostBody`, `KeyBody`, `SecretRow`):
// what it holds, its links to other records, its secrets on demand and its actions.
import SwiftUI
import UwUDesign

#if os(macOS)
import AppKit
#endif

struct SuiteDetail: View {
    let store: SuiteSpaceStore
    let record: SuiteRecord
    let actions: SuiteActions

    /// A file a command just saved (the phone shares it).
    @State private var savedFile: URL?

    private var space: SuiteSpace { store.space }
    private var d: SuiteJSONObject? { record.data }
    private var index: [String: SuiteRecord] { store.index }

    private var users: [SuiteRecord] {
        (record.usedBy ?? []).compactMap { index[$0] }
    }

    var body: some View {
        Form {
            header
            if record.broken {
                Section {
                    Label {
                        Text(features: "Dieser Eintrag lässt sich nicht entschlüsseln. UwULock lässt ihn, wie er ist.")
                    } icon: {
                        UwUIcon(.warning, size: .sm)
                    }
                    .foregroundStyle(Color.uwuDangerInk)
                }
            }
            if d != nil {
                switch record.kind {
                case "host": hostBody
                case "group": groupBody
                case "identity": identityBody
                case "key": keyBody
                case "snippet": snippetBody
                case "known_host": knownHostBody
                case "port_forward":
                    Section {
                        SuiteInfo(label: featureString("Weiterleitung"), mono: true) {
                            Text(verbatim: suiteTunnelSpec(d))
                        }
                    }
                default: EmptyView()
                }
            }
            #if os(iOS)
            if let savedFile {
                Section {
                    ShareLink(item: savedFile) {
                        Label {
                            Text(verbatim: savedFile.lastPathComponent)
                        } icon: {
                            UwUIcon(.share, size: .sm)
                        }
                    }
                }
            }
            #endif
        }
        .formStyle(.grouped)
        .onChange(of: record.id) { savedFile = nil }
    }

    // MARK: Header

    private var header: some View {
        Section {
            VStack(alignment: .leading, spacing: UwUSpacing.sm) {
                Text(verbatim: record.shownTitle)
                    .uwuFont(.title2, weight: .bold)
                    .textSelection(.enabled)
                chips
                tools
                if !users.isEmpty, record.kind != "secret" {
                    Text(features: "Wird noch verwendet von: \(users.map(\.shownTitle).joined(separator: ", "))")
                        .uwuFont(.caption)
                        .foregroundStyle(Color.uwuMuted)
                }
            }
            .padding(.vertical, UwUSpacing.xxs)
        }
    }

    private var chips: some View {
        HStack(spacing: UwUSpacing.xs) {
            SuiteChip(text: suiteKindLabel(record.kind))
            if record.kind == "host" || record.kind == "group" {
                SuiteChip(text: suiteWorkspaceLabel(record.workspace))
            }
            if record.kind == "host", let group = index[SuiteJSON.ref(d, "group_id") ?? ""] {
                SuiteChip(text: group.shownTitle, icon: .folder)
            }
        }
    }

    private var siblings: [SuiteRecord] {
        record.kind == "host" || record.kind == "group" ? suiteSiblings(store.records, of: record) : []
    }

    @ViewBuilder private var tools: some View {
        let sorted = siblings.sorted { ($0.position, $0.title) < ($1.position, $1.title) }
        let place = sorted.firstIndex { $0.id == record.id } ?? -1
        UwUGlassContainer(spacing: UwUSpacing.xs) {
            HStack(spacing: UwUSpacing.xs) {
                if let edit = record.broken ? nil : SuiteSheet.edit(record), record.kind != "port_forward" {
                    Button {
                        actions.present(edit)
                    } label: {
                        Label(features: "Bearbeiten", uwuIcon: .edit)
                    }
                    .uwuProminentButton()
                    .foregroundStyle(Color.uwuOnPink)
                }
                if siblings.count > 1 {
                    Button {
                        move(-1)
                    } label: {
                        UwUIcon(.moveUp, size: .sm, label: Text(features: "Nach oben"))
                    }
                    .uwuGlassButton()
                    .disabled(place <= 0)
                    .help(Text(features: "Nach oben"))
                    Button {
                        move(1)
                    } label: {
                        UwUIcon(.moveDown, size: .sm, label: Text(features: "Nach unten"))
                    }
                    .uwuGlassButton()
                    .disabled(place < 0 || place >= sorted.count - 1)
                    .help(Text(features: "Nach unten"))
                }
                if record.kind != "secret" {
                    Button(role: .destructive) {
                        actions.askDelete(record)
                    } label: {
                        UwUIcon(.delete, size: .sm, label: Text(features: "Löschen"))
                    }
                    .uwuGlassButton()
                    .disabled(!users.isEmpty)
                    .help(Text(features: "Löschen"))
                }
            }
        }
    }

    private func move(_ step: Int) {
        Task { await store.save(suiteMoveOps(siblings, id: record.id, step: step), done: nil) }
    }

    // MARK: Links

    /// A pointer at another record: its title, tapping shows it.
    @ViewBuilder private func link(_ id: String?) -> some View {
        if let id {
            if let target = index[id] {
                Button {
                    actions.show(id)
                } label: {
                    Text(verbatim: target.shownTitle)
                        .uwuFont(.body, weight: .medium)
                }
                .buttonStyle(.borderless)
                .tint(.uwuPinkInk)
            } else {
                Text(features: "(nicht vorhanden)").foregroundStyle(Color.uwuMuted)
            }
        } else {
            Text(verbatim: "—").foregroundStyle(Color.uwuMuted)
        }
    }

    @ViewBuilder private var usedBy: some View {
        if !users.isEmpty {
            Section {
                ForEach(users) { user in
                    link(user.id)
                }
            } header: {
                Text(features: "Wird verwendet von")
            } footer: {
                Text(features: "Löschen geht erst, wenn nichts mehr darauf zeigt.")
            }
        }
    }

    // MARK: Hosts

    private var login: SuiteRecord? { suiteLoginOf(record, index) }

    private var command: String {
        space == .ssh ? suiteSshCommand(d, login?.data) : suiteRdpAddress(d)
    }

    @ViewBuilder private var hostBody: some View {
        Section {
            UwUGlassContainer(spacing: UwUSpacing.xs) {
                HStack(spacing: UwUSpacing.xs) {
                    Button {
                        Task { await store.copyText(command) }
                    } label: {
                        if space == .ssh {
                            Label(features: "Befehl kopieren", uwuIcon: .copy)
                        } else {
                            Label(features: "Adresse kopieren", uwuIcon: .copy)
                        }
                    }
                    .uwuGlassButton()
                    if space == .rdp {
                        Button {
                            Task { saved(await store.saveRdp(record.id)) }
                        } label: {
                            Label(features: ".rdp-Datei", uwuIcon: .download)
                        }
                        .uwuGlassButton()
                    }
                    #if os(macOS)
                    Button {
                        Task { await store.openInApp(record.id) }
                    } label: {
                        Label(features: "In \(space.appName) öffnen", uwuIcon: .openExternal)
                    }
                    .uwuGlassButton()
                    #endif
                }
                .uwuFont(.subheadline)
            }
            SuiteInfo(label: featureString("Adresse"), mono: true) {
                Text(verbatim: SuiteJSON.str(d, "address"))
            } actions: {
                SuiteCopyButton(store: store, text: SuiteJSON.str(d, "address"), label: featureString("Adresse"))
            }
            SuiteInfo(label: featureString("Port"), mono: true) {
                Text(verbatim: String(SuiteJSON.int(d, "port", space.defaultPort)))
            }
            SuiteInfo(label: featureString("Anmeldung")) {
                if let own = SuiteJSON.ref(d, "identity_id") {
                    link(own)
                } else if let login {
                    HStack(spacing: UwUSpacing.xxs) {
                        link(login.id)
                        Text(features: "(von der Gruppe)").foregroundStyle(Color.uwuMuted)
                    }
                } else {
                    Text(features: "Keine").foregroundStyle(Color.uwuMuted)
                }
            }
            if let login, !SuiteJSON.str(login.data, "username").isEmpty {
                SuiteInfo(label: featureString("Benutzername"), mono: true) {
                    Text(
                        verbatim: [SuiteJSON.str(login.data, "domain"), SuiteJSON.str(login.data, "username")]
                            .filter { !$0.isEmpty }.joined(separator: "\\"))
                }
            }
            if space == .ssh {
                SuiteInfo(label: featureString("Befehl"), mono: true) {
                    Text(verbatim: command)
                }
            }
            let comment = SuiteJSON.str(d, "comment")
            if !comment.isEmpty {
                SuiteInfo(label: featureString("Notiz")) { Text(verbatim: comment) }
            }
        }
        if space == .rdp { rdpSection }
        if space == .ssh { tunnelSection }
    }

    @ViewBuilder private var rdpSection: some View {
        let rdp = SuiteJSON.obj(d, "rdp")
        let gateway = SuiteJSON.obj(rdp, "gateway")
        let display = SuiteJSON.str(rdp, "display").isEmpty ? "fit" : SuiteJSON.str(rdp, "display")
        let audio = SuiteJSON.str(rdp, "audio").isEmpty ? "local" : SuiteJSON.str(rdp, "audio")
        Section {
            SuiteInfo(label: featureString("Größe")) {
                switch display {
                case "fixed":
                    Text(verbatim: "\(SuiteJSON.int(rdp, "width", 1920)) × \(SuiteJSON.int(rdp, "height", 1080))")
                case "fullscreen": Text(features: "Vollbild")
                case "fit": Text(features: "An das Fenster anpassen")
                default: Text(verbatim: display)
                }
            }
            SuiteInfo(label: featureString("Farbtiefe")) {
                Text(features: "\(SuiteJSON.int(rdp, "colorDepth", 32)) Bit")
            }
            SuiteInfo(label: featureString("Ton")) {
                switch audio {
                case "remote": Text(features: "Auf dem Server lassen")
                case "off": Text(features: "Aus")
                case "local": Text(features: "Hier abspielen")
                default: Text(verbatim: audio)
                }
            }
            SuiteInfo(label: featureString("Optionen")) {
                SuiteFlow {
                    ForEach(SuiteRdpOption.all, id: \.field) { option in
                        let on = SuiteJSON.bool(rdp, option.field, option.fallback)
                        SuiteChip(text: "\(on ? "✓" : "✗") \(option.label)", muted: !on)
                            .help(on ? Text(features: "An") : Text(features: "Aus"))
                    }
                }
            }
            SuiteInfo(label: featureString("Gateway"), mono: true) {
                VStack(alignment: .leading, spacing: 2) {
                    if let gateway {
                        Text(
                            verbatim: suiteHostPort(
                                SuiteJSON.str(gateway, "address"), SuiteJSON.int(gateway, "port", 443)))
                        if SuiteJSON.bool(gateway, "bypassLocal") {
                            Text(features: "Für lokale Adressen umgehen")
                                .uwuFont(.caption)
                                .foregroundStyle(Color.uwuMuted)
                        }
                    } else {
                        Text(features: "Keins")
                    }
                }
            }
            if let gateway {
                SuiteInfo(label: featureString("Anmeldung am Gateway")) {
                    if SuiteJSON.bool(gateway, "useHostLogin") {
                        Text(features: "Wie der Host")
                    } else {
                        link(SuiteJSON.ref(d, "gateway_identity_id"))
                    }
                }
            }
            SuiteInfo(label: featureString("Laufwerke umleiten")) {
                Text(verbatim: suiteDrivesText(SuiteJSON.obj(rdp, "drives"), inherit: true))
            }
        } header: {
            Text(features: "Remote Desktop")
        }
    }

    @ViewBuilder private var tunnelSection: some View {
        let tunnels = suiteTunnels(store.records, hostId: record.id)
        Section {
            if tunnels.isEmpty {
                Text(features: "Keine Weiterleitungen.").foregroundStyle(Color.uwuMuted)
            }
            ForEach(tunnels) { tunnel in
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: tunnel.shownTitle)
                            .uwuFont(.caption, weight: .semibold)
                            .foregroundStyle(Color.uwuMuted)
                        HStack(spacing: UwUSpacing.xs) {
                            Text(verbatim: suiteTunnelSpec(tunnel.data))
                                .font(.uwuMono(.callout))
                                .textSelection(.enabled)
                            if SuiteJSON.bool(tunnel.data, "autostart") {
                                SuiteChip(text: featureString("startet mit"))
                            }
                        }
                    }
                    Spacer(minLength: 0)
                    Button {
                        actions.present(.tunnel(hostId: record.id, tunnel))
                    } label: {
                        UwUIcon(.edit, size: .sm, label: Text(features: "Bearbeiten"))
                    }
                    .buttonStyle(.borderless)
                    .help(Text(features: "Bearbeiten"))
                    Button(role: .destructive) {
                        actions.askDelete(tunnel)
                    } label: {
                        UwUIcon(.delete, size: .sm, label: Text(features: "Löschen"))
                    }
                    .buttonStyle(.borderless)
                    .help(Text(features: "Löschen"))
                }
                .contextMenu {
                    Button {
                        actions.present(.tunnel(hostId: record.id, tunnel))
                    } label: {
                        Label(features: "Bearbeiten", uwuIcon: .edit)
                    }
                    Button(role: .destructive) {
                        actions.askDelete(tunnel)
                    } label: {
                        Label(features: "Löschen", uwuIcon: .delete)
                    }
                }
            }
        } header: {
            HStack {
                Text(features: "Weiterleitungen")
                Spacer()
                Button {
                    actions.present(.tunnel(hostId: record.id, nil))
                } label: {
                    UwUIcon(.add, size: .sm, label: Text(features: "Neue Weiterleitung"))
                }
                .buttonStyle(.borderless)
                .help(Text(features: "Neue Weiterleitung"))
            }
        }
    }

    // MARK: Other kinds

    @ViewBuilder private var groupBody: some View {
        Section {
            SuiteInfo(label: featureString("Hosts")) {
                Text(
                    verbatim: String(
                        store.records.filter {
                            $0.kind == "host" && SuiteJSON.ref($0.data, "group_id") == record.id
                        }.count))
            }
            if space == .rdp {
                SuiteInfo(label: featureString("Anmeldung für alle Hosts")) {
                    link(SuiteJSON.ref(d, "identity_id"))
                }
                SuiteInfo(label: featureString("Laufwerke umleiten")) {
                    Text(verbatim: suiteDrivesText(SuiteJSON.obj(d, "drives"), inherit: false))
                }
            }
        }
    }

    @ViewBuilder private var identityBody: some View {
        Section {
            SuiteInfo(label: featureString("Benutzername"), mono: true) {
                Text(verbatim: SuiteJSON.str(d, "username"))
            } actions: {
                SuiteCopyButton(store: store, text: SuiteJSON.str(d, "username"), label: featureString("Benutzername"))
            }
            if space == .rdp, !SuiteJSON.str(d, "domain").isEmpty {
                SuiteInfo(label: featureString("Domäne")) { Text(verbatim: SuiteJSON.str(d, "domain")) }
            }
            SuiteInfo(label: featureString("Anmelden mit")) {
                Text(verbatim: suiteAuthLabel(SuiteJSON.str(d, "auth_type")))
            }
            if let key = SuiteJSON.ref(d, "key_id") {
                SuiteInfo(label: featureString("Schlüssel")) { link(key) }
            }
            if let secret = SuiteJSON.ref(d, "password_secret_id"), index[secret]?.kind == "secret" {
                SuiteSecretRow(store: store, id: secret, label: featureString("Passwort"), password: true)
            }
        }
        usedBy
    }

    @ViewBuilder private var keyBody: some View {
        let publicKey = SuiteJSON.str(d, "public_key")
        Section {
            SuiteInfo(label: featureString("Typ"), mono: true) {
                let type = SuiteJSON.str(d, "key_type")
                Text(verbatim: type.isEmpty ? "—" : type)
            }
            if !publicKey.isEmpty {
                SuiteInfo(label: featureString("Öffentlicher Schlüssel"), mono: true) {
                    Text(verbatim: publicKey).lineLimit(6)
                } actions: {
                    SuiteCopyButton(store: store, text: publicKey, label: featureString("Öffentlicher Schlüssel"))
                    saveButton(.public, label: featureString("Öffentlicher Schlüssel"))
                }
            }
            if let privateId = SuiteJSON.ref(d, "private_secret_id"), index[privateId]?.kind == "secret" {
                SuiteSecretRow(store: store, id: privateId, label: featureString("Privater Schlüssel"), multiline: true) {
                    saveButton(.private, label: featureString("Privater Schlüssel"))
                }
            }
            if let phrase = SuiteJSON.ref(d, "passphrase_secret_id"), index[phrase]?.kind == "secret" {
                SuiteSecretRow(store: store, id: phrase, label: featureString("Passphrase"))
            }
        }
        usedBy
    }

    private func saveButton(_ half: SuiteKeyHalf, label: String) -> some View {
        Button {
            Task { saved(await store.saveKey(record.id, half: half)) }
        } label: {
            UwUIcon(.download, size: .sm, label: Text(features: "\(label) als Datei speichern"))
        }
        .buttonStyle(.borderless)
        .help(Text(features: "\(label) als Datei speichern"))
    }

    /// A file the core saved: the phone offers to share it, the Mac shows it in the Finder.
    private func saved(_ path: String?) {
        guard let path else { return }
        let url = URL(fileURLWithPath: path)
        #if os(macOS)
        NSWorkspace.shared.activateFileViewerSelecting([url])
        #else
        savedFile = url
        #endif
    }

    @ViewBuilder private var snippetBody: some View {
        Section {
            let path = SuiteJSON.str(d, "group_path")
            if !path.isEmpty {
                SuiteInfo(label: featureString("Ordner")) { Text(verbatim: path) }
            }
            SuiteInfo(label: featureString("Befehl"), mono: true) {
                Text(verbatim: SuiteJSON.str(d, "body"))
            } actions: {
                SuiteCopyButton(store: store, text: SuiteJSON.str(d, "body"), label: featureString("Befehl"))
            }
        }
    }

    @ViewBuilder private var knownHostBody: some View {
        Section {
            SuiteInfo(label: featureString("Adresse"), mono: true) {
                Text(verbatim: suiteHostPort(SuiteJSON.str(d, "address"), SuiteJSON.int(d, "port", 22)))
            }
            SuiteInfo(label: featureString("Algorithmus"), mono: true) {
                Text(verbatim: SuiteJSON.str(d, "algorithm"))
            }
            SuiteInfo(label: featureString("Fingerabdruck"), mono: true) {
                Text(verbatim: SuiteJSON.str(d, "fingerprint_sha256"))
            } actions: {
                SuiteCopyButton(
                    store: store, text: SuiteJSON.str(d, "fingerprint_sha256"), label: featureString("Fingerabdruck"))
            }
            SuiteInfo(label: featureString("Öffentlicher Schlüssel"), mono: true) {
                Text(verbatim: SuiteJSON.str(d, "public_key")).lineLimit(6)
            }
            let seen = SuiteJSON.num(d, "first_seen_ms")
            if seen > 0 {
                SuiteInfo(label: featureString("Zuerst gesehen")) {
                    Text(Date(timeIntervalSince1970: seen / 1000), format: .dateTime)
                }
            }
        } footer: {
            Text(features: "Löschen heißt: Beim nächsten Verbinden fragt \(space.appName) wieder, ob du dem Server vertraust.")
        }
    }
}

/// The RDP options shown as chips (with the defaults UwURDP has).
struct SuiteRdpOption {
    let field: String
    let fallback: Bool
    let label: String

    static var all: [SuiteRdpOption] {
        [
            SuiteRdpOption(field: "smartSizing", fallback: true, label: featureString("Verkleinern statt scrollen")),
            SuiteRdpOption(field: "clipboard", fallback: true, label: featureString("Zwischenablage teilen")),
            SuiteRdpOption(field: "admin", fallback: false, label: featureString("Konsolensitzung (/admin)")),
            SuiteRdpOption(field: "nla", fallback: true, label: featureString("Netzwerkebenen-Authentifizierung (NLA)")),
            SuiteRdpOption(field: "wallpaper", fallback: true, label: featureString("Hintergrundbild zeigen")),
            SuiteRdpOption(field: "graphicsPipeline", fallback: true, label: featureString("Grafik-Pipeline (RDPEGFX)")),
        ]
    }
}

/// A field of a detail: its label above, the value, actions on the right.
struct SuiteInfo<Value: View, Actions: View>: View {
    let label: String
    var mono = false
    @ViewBuilder var value: Value
    @ViewBuilder var actions: Actions

    init(
        label: String, mono: Bool = false, @ViewBuilder value: () -> Value,
        @ViewBuilder actions: () -> Actions = { EmptyView() }
    ) {
        self.label = label
        self.mono = mono
        self.value = value()
        self.actions = actions()
    }

    var body: some View {
        HStack(alignment: .center, spacing: UwUSpacing.xs) {
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: label)
                    .uwuFont(.caption, weight: .semibold)
                    .foregroundStyle(Color.uwuMuted)
                Group {
                    if mono {
                        value.font(.uwuMono(.callout))
                    } else {
                        value.uwuFont(.body)
                    }
                }
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            actions
        }
    }
}

/// Copies plain text through the core.
struct SuiteCopyButton: View {
    let store: SuiteSpaceStore
    let text: String
    let label: String

    var body: some View {
        Button {
            Task { await store.copyText(text) }
        } label: {
            UwUIcon(.copy, size: .sm, label: Text(features: "\(label) kopieren"))
        }
        .buttonStyle(.borderless)
        .help(Text(features: "\(label) kopieren"))
    }
}

/// A secret record: dots until the eye (hidden again after a minute); copied by the core.
struct SuiteSecretRow<Extra: View>: View {
    let store: SuiteSpaceStore
    let id: String
    let label: String
    var multiline = false
    var password = false
    @ViewBuilder var extra: Extra

    @State private var value: String?
    @State private var hiding: Task<Void, Never>?

    init(
        store: SuiteSpaceStore, id: String, label: String, multiline: Bool = false, password: Bool = false,
        @ViewBuilder extra: () -> Extra = { EmptyView() }
    ) {
        self.store = store
        self.id = id
        self.label = label
        self.multiline = multiline
        self.password = password
        self.extra = extra()
    }

    var body: some View {
        SuiteInfo(label: label, mono: true) {
            if let value {
                Text(verbatim: value)
                    .lineLimit(multiline ? nil : 3)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                Text(verbatim: "••••••••••••").accessibilityLabel(Text(verbatim: label))
            }
        } actions: {
            Button {
                Task { await toggle() }
            } label: {
                if value == nil {
                    UwUIcon(.show, size: .sm, label: Text(features: "\(label) zeigen"))
                } else {
                    UwUIcon(.hide, size: .sm, label: Text(features: "\(label) verbergen"))
                }
            }
            .buttonStyle(.borderless)
            .help(value == nil ? Text(features: "\(label) zeigen") : Text(features: "\(label) verbergen"))
            Button {
                Task { await store.copySecret(id, password: password) }
            } label: {
                UwUIcon(.copy, size: .sm, label: Text(features: "\(label) kopieren"))
            }
            .buttonStyle(.borderless)
            .help(Text(features: "\(label) kopieren"))
            extra
        }
        .onDisappear {
            hiding?.cancel()
            value = nil
        }
        .onChange(of: id) {
            hiding?.cancel()
            value = nil
        }
    }

    private func toggle() async {
        hiding?.cancel()
        if value != nil {
            value = nil
            return
        }
        guard let text = await store.reveal(id) else { return }
        value = text
        hiding = Task {
            try? await Task.sleep(for: .seconds(60))
            guard !Task.isCancelled else { return }
            value = nil
        }
    }
}

/// A small label (the kind, the workspace, an RDP option).
struct SuiteChip: View {
    let text: String
    var icon: UwUIconName?
    var muted = false

    var body: some View {
        HStack(spacing: UwUSpacing.xxs) {
            if let icon { UwUIcon(icon, size: .xs) }
            Text(verbatim: text)
        }
        .uwuFont(.caption, weight: .medium)
        .foregroundStyle(muted ? Color.uwuMuted : Color.uwuPinkInk)
        .padding(.horizontal, UwUSpacing.xs)
        .padding(.vertical, 3)
        .background(muted ? Color.uwuControl : Color.uwuPinkTint, in: Capsule())
    }
}

/// Chips that wrap onto new lines.
struct SuiteFlow: Layout {
    var spacing: CGFloat = UwUSpacing.xxs

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0
        var y: CGFloat = 0
        var line: CGFloat = 0
        var widest: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > 0 && x + size.width > width {
                y += line + spacing
                x = 0
                line = 0
            }
            x += size.width + spacing
            line = max(line, size.height)
            widest = max(widest, x - spacing)
        }
        return CGSize(width: proposal.width ?? widest, height: y + line)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX
        var y = bounds.minY
        var line: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > bounds.minX && x + size.width > bounds.maxX {
                y += line + spacing
                x = bounds.minX
                line = 0
            }
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            line = max(line, size.height)
        }
    }
}

#if DEBUG
@MainActor private let suitePreviewActions = SuiteActions(
    show: { _ in }, present: { _ in }, askDelete: { _ in }, saved: { _, _ in })

#Preview("Host (SSH)") {
    let store = SuiteSpaceStore.preview(.ssh)
    NavigationStack {
        SuiteDetail(store: store, record: store.index["host-web1"]!, actions: suitePreviewActions)
    }
}

#Preview("Host (RDP)") {
    let store = SuiteSpaceStore.preview(.rdp)
    NavigationStack {
        SuiteDetail(store: store, record: store.index["host-desk"]!, actions: suitePreviewActions)
    }
}

#Preview("Neuer Host") {
    SuiteHostEditor(store: .preview(.rdp), host: nil) { _ in }
}

#Preview("Neuer Schlüssel") {
    SuiteNewKeySheet(store: .preview(.ssh)) { _ in }
}
#endif
