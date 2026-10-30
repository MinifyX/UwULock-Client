// The suite's editors (SuiteEditors.tsx): hosts, groups, logins, keys (renaming, a new one made
// here or imported), snippets and port forwards. Each sends only what it changed; the core lays
// it over the record, so fields of a newer app stay.
import SwiftUI
import UniformTypeIdentifiers
import UwUDesign

/// The sheet for `sheet`.
struct SuiteEditorHost: View {
    let store: SuiteSpaceStore
    let sheet: SuiteSheet
    let actions: SuiteActions

    var body: some View {
        switch sheet {
        case .host(let record):
            SuiteHostEditor(store: store, host: record) { actions.saved($0, "host") }
        case .group(let record):
            SuiteGroupEditor(store: store, group: record) { actions.saved($0, "group") }
        case .identity(let record):
            SuiteIdentityEditor(store: store, identity: record) { actions.saved($0, "identity") }
        case .renameKey(let record):
            SuiteKeyEditor(store: store, key: record) { actions.saved($0, "key") }
        case .newKey:
            SuiteNewKeySheet(store: store) { actions.saved($0, "key") }
        case .snippet(let record):
            SuiteSnippetEditor(store: store, snippet: record) { actions.saved($0, "snippet") }
        case .tunnel(let hostId, let record):
            SuiteTunnelEditor(store: store, hostId: hostId, tunnel: record) { actions.saved($0, "port_forward") }
        }
    }
}

// MARK: Frame

/// An editor's sheet: its form, Cancel and Save, the question before throwing away changes.
struct SuiteEditorFrame<Content: View>: View {
    let title: String
    let dirty: Bool
    let busy: Bool
    let canSave: Bool
    /// What keeps it from saving (shown once something was typed).
    var problem: String?
    let save: () async -> Void
    @ViewBuilder var content: Content

    @Environment(\.dismiss) private var dismiss
    @State private var askClose = false

    var body: some View {
        NavigationStack {
            Form {
                content
                if let problem {
                    Section {
                        Label {
                            Text(verbatim: problem)
                        } icon: {
                            UwUIcon(.error, size: .sm)
                        }
                        .foregroundStyle(Color.uwuDangerInk)
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(Text(verbatim: title))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button {
                        if dirty { askClose = true } else { dismiss() }
                    } label: {
                        Text(features: "Abbrechen")
                    }
                    .confirmationDialog(
                        Text(features: "Wirklich schließen?"), isPresented: $askClose, titleVisibility: .visible
                    ) {
                        Button(role: .destructive) {
                            dismiss()
                        } label: {
                            Text(features: "Schließen")
                        }
                        Button(role: .cancel) {} label: { Text(features: "Weiter bearbeiten") }
                    } message: {
                        Text(features: "Was du bisher eingegeben hast, geht dabei verloren.")
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        Task { await save() }
                    } label: {
                        if busy {
                            Text(features: "Speichert …")
                        } else {
                            Text(features: "Speichern")
                        }
                    }
                    .disabled(!canSave || busy)
                    .keyboardShortcut(.defaultAction)
                }
            }
            .disabled(busy)
        }
        .interactiveDismissDisabled(dirty || busy)
        #if os(macOS)
        .frame(minWidth: 480, idealWidth: 540, minHeight: 520, idealHeight: 640)
        #endif
        .presentationDetents([.large])
    }
}

/// A password field with the generator next to it.
struct SuitePasswordInput: View {
    let store: SuiteSpaceStore
    @Binding var text: String
    var placeholder: String?

    @Environment(VaultModel.self) private var vault: VaultModel?
    @State private var generating = false

    var body: some View {
        HStack(spacing: UwUSpacing.xs) {
            TextField(text: $text, prompt: placeholder.map { Text(verbatim: $0) }) {
                Text(features: "Passwort")
            }
            .font(.uwuMono(.body))
            .autocorrectionDisabled()
            #if os(iOS)
            .textInputAutocapitalization(.never)
            #endif
            Button {
                if vault != nil {
                    generating = true
                } else {
                    Task { if let value = await store.generatePassword() { text = value } }
                }
            } label: {
                UwUIcon(.generate, size: .sm, label: Text(features: "Passwort erzeugen"))
            }
            .buttonStyle(.borderless)
            .help(Text(features: "Passwort erzeugen"))
        }
        .sheet(isPresented: $generating) {
            if let vault {
                GeneratorSheet { value in
                    text = value
                    generating = false
                }
                .environment(vault)
            }
        }
    }
}

/// A text field with the label shown on the Mac's grouped form and as a prompt on the phone.
private struct SuiteTextField: View {
    let label: String
    @Binding var text: String
    var prompt: String?
    var mono = false
    var plain = false

    var body: some View {
        TextField(text: $text, prompt: prompt.map { Text(verbatim: $0) }) {
            Text(verbatim: label)
        }
        .font(mono ? .uwuMono(.body) : nil)
        .autocorrectionDisabled(plain || mono)
        #if os(iOS)
        .textInputAutocapitalization(plain || mono ? .never : .sentences)
        #endif
    }
}

/// A picker of records (a group, a login, a key) with a "none" choice.
private struct SuiteRecordPicker: View {
    let label: String
    let records: [SuiteRecord]
    @Binding var selection: String
    let none: String
    var extra: [(String, String)] = []

    var body: some View {
        Picker(selection: $selection) {
            Text(verbatim: none).tag("")
            ForEach(records) { record in
                Text(verbatim: record.shownTitle).tag(record.id)
            }
            ForEach(extra, id: \.0) { value, title in
                Text(verbatim: title).tag(value)
            }
            if !selection.isEmpty, !records.contains(where: { $0.id == selection }),
                !extra.contains(where: { $0.0 == selection })
            {
                Text(features: "(nicht vorhanden)").tag(selection)
            }
        } label: {
            Text(verbatim: label)
        }
    }
}

/// The workspace picker (plus the record's own, when a newer app made it).
private struct SuiteWorkspacePicker: View {
    @Binding var workspace: String

    var body: some View {
        Picker(selection: $workspace) {
            ForEach(suiteWorkspaces + (suiteWorkspaces.contains(workspace) ? [] : [workspace]), id: \.self) {
                Text(verbatim: suiteWorkspaceLabel($0)).tag($0)
            }
        } label: {
            Text(features: "Arbeitsbereich")
        }
    }
}

/// A choice of values (plus the record's own, when it's none of them).
private struct SuiteValuePicker: View {
    let label: String
    @Binding var value: String
    let options: [(String, String)]

    var body: some View {
        Picker(selection: $value) {
            ForEach(options, id: \.0) { option, title in
                Text(verbatim: title).tag(option)
            }
            if !options.contains(where: { $0.0 == value }) {
                Text(verbatim: value).tag(value)
            }
        } label: {
            Text(verbatim: label)
        }
    }
}

// MARK: Drives

/// Drive redirection (UwURDP): taken from the group, off, or these drives.
private struct SuiteDrivesFields: View {
    @Binding var drives: SuiteDrives
    let inherit: Bool

    var body: some View {
        Picker(selection: $drives.mode) {
            if inherit { Text(features: "Wie die Gruppe").tag(SuiteDrives.Mode.inherit) }
            Text(features: "Aus").tag(SuiteDrives.Mode.off)
            Text(features: "An").tag(SuiteDrives.Mode.on)
        } label: {
            Text(features: "Laufwerke umleiten")
        }
        if drives.mode == .on {
            ForEach($drives.list) { $drive in
                VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                    SuiteTextField(label: featureString("Name auf dem Server"), text: $drive.name, plain: true)
                    HStack {
                        SuiteTextField(
                            label: featureString("Ordner auf diesem Gerät (* = alle Laufwerke)"), text: $drive.path,
                            mono: true)
                        Button(role: .destructive) {
                            drives.list.removeAll { $0.id == drive.id }
                        } label: {
                            UwUIcon(.delete, size: .sm, label: Text(features: "Entfernen"))
                        }
                        .buttonStyle(.borderless)
                        .help(Text(features: "Entfernen"))
                    }
                }
            }
            Button {
                drives.list.append(SuiteDrive(name: "", path: "", orig: [:]))
            } label: {
                Label(features: "Laufwerk hinzufügen", uwuIcon: .add)
            }
        }
    }
}

// MARK: Host

private let suiteNewLogin = "__new__"

struct SuiteHostEditor: View {
    let store: SuiteSpaceStore
    let host: SuiteRecord?
    let onSaved: (String) -> Void

    private struct Fields: Equatable {
        var name, address, port, workspace, groupId, identityId: String
        var newUser = "", newDomain = "", newPassword = ""
        var comment, gatewayIdentityId, display, width, height: String
        var smartSizing: Bool
        var colorDepth, audio: String
        var clipboard, admin, nla, wallpaper, graphicsPipeline: Bool
        var gatewayOn: Bool
        var gatewayAddress, gatewayPort: String
        var gatewayUseHostLogin, gatewayBypassLocal: Bool
        var drives: SuiteDrives
    }

    @Environment(\.dismiss) private var dismiss
    @State private var form: Fields
    private let initial: Fields
    @State private var busy = false

    init(store: SuiteSpaceStore, host: SuiteRecord?, onSaved: @escaping (String) -> Void) {
        self.store = store
        self.host = host
        self.onSaved = onSaved
        let d = host?.data
        let rdp = SuiteJSON.obj(d, "rdp")
        let gateway = SuiteJSON.obj(rdp, "gateway")
        let display = SuiteJSON.str(rdp, "display")
        let audio = SuiteJSON.str(rdp, "audio")
        let workspace = SuiteJSON.str(d, "workspace")
        let fields = Fields(
            name: SuiteJSON.str(d, "name"), address: SuiteJSON.str(d, "address"),
            port: String(SuiteJSON.int(d, "port", store.space.defaultPort)),
            workspace: workspace.isEmpty ? "private" : workspace,
            groupId: SuiteJSON.ref(d, "group_id") ?? "", identityId: SuiteJSON.ref(d, "identity_id") ?? "",
            comment: SuiteJSON.str(d, "comment"), gatewayIdentityId: SuiteJSON.ref(d, "gateway_identity_id") ?? "",
            display: display.isEmpty ? "fit" : display,
            width: String(SuiteJSON.int(rdp, "width", 1920)), height: String(SuiteJSON.int(rdp, "height", 1080)),
            smartSizing: SuiteJSON.bool(rdp, "smartSizing", true),
            colorDepth: String(SuiteJSON.int(rdp, "colorDepth", 32)), audio: audio.isEmpty ? "local" : audio,
            clipboard: SuiteJSON.bool(rdp, "clipboard", true), admin: SuiteJSON.bool(rdp, "admin", false),
            nla: SuiteJSON.bool(rdp, "nla", true), wallpaper: SuiteJSON.bool(rdp, "wallpaper", true),
            graphicsPipeline: SuiteJSON.bool(rdp, "graphicsPipeline", true), gatewayOn: gateway != nil,
            gatewayAddress: SuiteJSON.str(gateway, "address"),
            gatewayPort: String(SuiteJSON.int(gateway, "port", 443)),
            gatewayUseHostLogin: SuiteJSON.bool(gateway, "useHostLogin", false),
            gatewayBypassLocal: SuiteJSON.bool(gateway, "bypassLocal", false),
            drives: SuiteDrives(SuiteJSON.obj(rdp, "drives"), inherit: true))
        _form = State(initialValue: fields)
        initial = fields
    }

    private var rdpSpace: Bool { store.space == .rdp }
    private var dirty: Bool { form != initial }

    private var problem: String? {
        if form.address.trimmingCharacters(in: .whitespaces).isEmpty { return featureString("Die Adresse fehlt.") }
        if suitePort(form.port) == nil { return featureString("Der Port ist 1 bis 65535.") }
        if form.identityId == suiteNewLogin && form.newUser.trimmingCharacters(in: .whitespaces).isEmpty {
            return featureString("Der Benutzername fehlt.")
        }
        if rdpSpace && form.display == "fixed" && (suitePort(form.width) == nil || suitePort(form.height) == nil) {
            return featureString("Breite und Höhe sind Zahlen.")
        }
        if rdpSpace && form.gatewayOn
            && (form.gatewayAddress.trimmingCharacters(in: .whitespaces).isEmpty || suitePort(form.gatewayPort) == nil)
        {
            return featureString("Das Gateway braucht eine Adresse und einen Port.")
        }
        return nil
    }

    var body: some View {
        let groups = suiteOfKind(store.records, "group").filter { $0.workspace == form.workspace }
        let identities = suiteOfKind(store.records, "identity")
        SuiteEditorFrame(
            title: host == nil ? featureString("Neuer Host") : featureString("Host bearbeiten"), dirty: dirty,
            busy: busy, canSave: problem == nil, problem: dirty ? problem : nil, save: save
        ) {
            Section {
                SuiteTextField(label: featureString("Name"), text: $form.name)
                SuiteWorkspacePicker(workspace: $form.workspace)
                    .onChange(of: form.workspace) { form.groupId = "" }
                SuiteTextField(
                    label: featureString("Adresse"), text: $form.address,
                    prompt: rdpSpace ? "desktop.example.com" : "server.example.com", mono: true)
                SuiteTextField(label: featureString("Port"), text: $form.port, mono: true)
                SuiteRecordPicker(
                    label: featureString("Gruppe"), records: groups, selection: $form.groupId,
                    none: featureString("Ohne Gruppe"))
                SuiteRecordPicker(
                    label: featureString("Anmeldung"), records: identities, selection: $form.identityId,
                    none: rdpSpace ? featureString("Wie die Gruppe") : featureString("Keine (fragt beim Verbinden)"),
                    extra: [(suiteNewLogin, featureString("Neue Anmeldung …"))])
            }
            if form.identityId == suiteNewLogin {
                Section {
                    SuiteTextField(label: featureString("Benutzername"), text: $form.newUser, plain: true)
                    if rdpSpace {
                        SuiteTextField(label: featureString("Domäne"), text: $form.newDomain, plain: true)
                    }
                    SuitePasswordInput(store: store, text: $form.newPassword)
                } header: {
                    Text(features: "Neue Anmeldung")
                }
            }
            if rdpSpace { rdpFields(identities) }
        }
    }

    @ViewBuilder private func rdpFields(_ identities: [SuiteRecord]) -> some View {
        Section {
            SuiteValuePicker(
                label: featureString("Größe"), value: $form.display,
                options: [
                    ("fit", featureString("An das Fenster anpassen")), ("fixed", featureString("Feste Auflösung")),
                    ("fullscreen", featureString("Vollbild")),
                ])
            if form.display == "fixed" {
                SuiteTextField(label: featureString("Breite"), text: $form.width, mono: true)
                SuiteTextField(label: featureString("Höhe"), text: $form.height, mono: true)
            }
            SuiteValuePicker(
                label: featureString("Farbtiefe"), value: $form.colorDepth,
                options: ["15", "16", "24", "32"].map { ($0, featureString("\(Int($0) ?? 0) Bit")) })
            SuiteValuePicker(
                label: featureString("Ton"), value: $form.audio,
                options: [
                    ("local", featureString("Hier abspielen")), ("remote", featureString("Auf dem Server lassen")),
                    ("off", featureString("Aus")),
                ])
            Toggle(isOn: $form.smartSizing) { Text(features: "Verkleinern statt scrollen") }
            Toggle(isOn: $form.clipboard) { Text(features: "Zwischenablage teilen") }
            Toggle(isOn: $form.admin) { Text(features: "Konsolensitzung (/admin)") }
            Toggle(isOn: $form.nla) { Text(features: "Netzwerkebenen-Authentifizierung (NLA)") }
            Toggle(isOn: $form.wallpaper) { Text(features: "Hintergrundbild zeigen") }
            Toggle(isOn: $form.graphicsPipeline) { Text(features: "Grafik-Pipeline (RDPEGFX)") }
        } header: {
            Text(features: "Anzeige")
        }
        Section {
            Toggle(isOn: $form.gatewayOn) { Text(features: "Über ein Remotedesktop-Gateway verbinden") }
            if form.gatewayOn {
                SuiteTextField(
                    label: featureString("Gateway-Adresse"), text: $form.gatewayAddress, prompt: "gateway.example.com",
                    mono: true)
                SuiteTextField(label: featureString("Port"), text: $form.gatewayPort, mono: true)
                Toggle(isOn: $form.gatewayUseHostLogin) { Text(features: "Mit der Anmeldung des Hosts") }
                Toggle(isOn: $form.gatewayBypassLocal) { Text(features: "Für lokale Adressen umgehen") }
                if !form.gatewayUseHostLogin {
                    SuiteRecordPicker(
                        label: featureString("Anmeldung am Gateway"), records: identities,
                        selection: $form.gatewayIdentityId, none: featureString("Keine"))
                }
            }
        } header: {
            Text(features: "Gateway")
        }
        Section {
            SuiteDrivesFields(drives: $form.drives, inherit: true)
        }
        Section {
            TextField(text: $form.comment, axis: .vertical) { Text(features: "Notiz") }
                .lineLimit(2...6)
        } header: {
            Text(features: "Notiz")
        }
    }

    private func save() async {
        let d = host?.data
        let id = host?.id ?? suiteNewId()
        var ops: [SuiteOp] = []
        var identityId: String? = form.identityId.isEmpty ? nil : form.identityId
        if form.identityId == suiteNewLogin {
            let newIdentity = suiteNewId()
            identityId = newIdentity
            var secret: String?
            if !form.newPassword.isEmpty {
                secret = suiteNewId()
                ops.append(.secret(id: secret!, text: form.newPassword, seq: nil))
            }
            let user = form.newUser.trimmingCharacters(in: .whitespaces)
            var patch: SuiteJSONObject = [
                "label": .string(user), "username": .string(user), "auth_type": "password",
                "password_secret_id": SuiteJSON.optional(secret),
            ]
            let domain = form.newDomain.trimmingCharacters(in: .whitespaces)
            if rdpSpace && !domain.isEmpty { patch["domain"] = .string(domain) }
            ops.append(.put(id: newIdentity, kind: "identity", seq: nil, patch: patch))
        }
        let groupId: String? = form.groupId.isEmpty ? nil : form.groupId
        var patch: SuiteJSONObject = [
            "name": .string(form.name.trimmingCharacters(in: .whitespaces)),
            "address": .string(form.address.trimmingCharacters(in: .whitespaces)),
            "port": SuiteJSON.number(suitePort(form.port) ?? store.space.defaultPort),
            "workspace": .string(form.workspace),
            "group_id": SuiteJSON.optional(groupId),
            "identity_id": SuiteJSON.optional(identityId),
        ]
        let oldWorkspace = SuiteJSON.str(d, "workspace").isEmpty ? "private" : SuiteJSON.str(d, "workspace")
        if host == nil || groupId != SuiteJSON.ref(d, "group_id") || form.workspace != oldWorkspace {
            let probe = SuiteRecord(
                id: id, kind: "host", seq: 0, updatedMs: 0,
                data: ["workspace": .string(form.workspace), "group_id": SuiteJSON.optional(groupId)], broken: false,
                usedBy: nil)
            patch["position"] = SuiteJSON.number(
                suiteNextPosition(suiteSiblings(store.records, of: probe).filter { $0.id != id }))
        }
        if rdpSpace {
            if !form.comment.trimmingCharacters(in: .whitespaces).isEmpty || SuiteJSON.has(d, "comment") {
                patch["comment"] = .string(form.comment)
            }
            if !form.gatewayIdentityId.isEmpty || SuiteJSON.has(d, "gateway_identity_id") {
                patch["gateway_identity_id"] = SuiteJSON.optional(
                    form.gatewayIdentityId.isEmpty ? nil : form.gatewayIdentityId)
            }
            // Settings of a newer UwURDP stay.
            let oldRdp = SuiteJSON.obj(d, "rdp") ?? [:]
            var rdp = oldRdp
            rdp["display"] = .string(form.display)
            rdp["width"] = SuiteJSON.number(suitePort(form.width) ?? 1920)
            rdp["height"] = SuiteJSON.number(suitePort(form.height) ?? 1080)
            rdp["smartSizing"] = .bool(form.smartSizing)
            rdp["colorDepth"] = SuiteJSON.number(Int(form.colorDepth) ?? 32)
            rdp["audio"] = .string(form.audio)
            rdp["clipboard"] = .bool(form.clipboard)
            rdp["admin"] = .bool(form.admin)
            rdp["nla"] = .bool(form.nla)
            rdp["wallpaper"] = .bool(form.wallpaper)
            rdp["graphicsPipeline"] = .bool(form.graphicsPipeline)
            if form.gatewayOn {
                var gateway = SuiteJSON.obj(oldRdp, "gateway") ?? [:]
                gateway["address"] = .string(form.gatewayAddress.trimmingCharacters(in: .whitespaces))
                gateway["port"] = SuiteJSON.number(suitePort(form.gatewayPort) ?? 443)
                gateway["useHostLogin"] = .bool(form.gatewayUseHostLogin)
                gateway["bypassLocal"] = .bool(form.gatewayBypassLocal)
                rdp["gateway"] = .object(gateway)
            } else {
                rdp["gateway"] = .null
            }
            rdp["drives"] = form.drives.written ?? .null
            patch["rdp"] = .object(rdp)
        }
        ops.append(.put(id: id, kind: "host", seq: host?.seq, patch: patch))
        busy = true
        let ok = await store.save(ops, done: host == nil ? featureString("Host angelegt ✧") : featureString("Gespeichert ✧"))
        busy = false
        dismiss()
        if ok { onSaved(id) }
    }
}

// MARK: Group

struct SuiteGroupEditor: View {
    let store: SuiteSpaceStore
    let group: SuiteRecord?
    let onSaved: (String) -> Void

    private struct Fields: Equatable {
        var name, workspace, identityId: String
        var drives: SuiteDrives
    }

    @Environment(\.dismiss) private var dismiss
    @State private var form: Fields
    private let initial: Fields
    @State private var busy = false

    init(store: SuiteSpaceStore, group: SuiteRecord?, onSaved: @escaping (String) -> Void) {
        self.store = store
        self.group = group
        self.onSaved = onSaved
        let d = group?.data
        let workspace = SuiteJSON.str(d, "workspace")
        let fields = Fields(
            name: SuiteJSON.str(d, "name"), workspace: workspace.isEmpty ? "private" : workspace,
            identityId: SuiteJSON.ref(d, "identity_id") ?? "",
            drives: SuiteDrives(SuiteJSON.obj(d, "drives"), inherit: false))
        _form = State(initialValue: fields)
        initial = fields
    }

    var body: some View {
        SuiteEditorFrame(
            title: group == nil ? featureString("Neue Gruppe") : featureString("Gruppe bearbeiten"),
            dirty: form != initial, busy: busy,
            canSave: !form.name.trimmingCharacters(in: .whitespaces).isEmpty, save: save
        ) {
            Section {
                SuiteTextField(label: featureString("Name"), text: $form.name)
                SuiteWorkspacePicker(workspace: $form.workspace)
            } footer: {
                if group != nil { Text(features: "Die Hosts der Gruppe ziehen mit um.") }
            }
            if store.space == .rdp {
                Section {
                    SuiteRecordPicker(
                        label: featureString("Anmeldung für alle Hosts der Gruppe"),
                        records: suiteOfKind(store.records, "identity"), selection: $form.identityId,
                        none: featureString("Keine"))
                } footer: {
                    Text(features: "Ein Host mit eigener Anmeldung nimmt seine.")
                }
                Section {
                    SuiteDrivesFields(drives: $form.drives, inherit: false)
                }
            }
        }
    }

    private func save() async {
        let d = group?.data
        let id = group?.id ?? suiteNewId()
        var patch: SuiteJSONObject = [
            "name": .string(form.name.trimmingCharacters(in: .whitespaces)), "workspace": .string(form.workspace),
        ]
        let oldWorkspace = group?.workspace
        let moved = group != nil && oldWorkspace != form.workspace
        if group == nil || moved {
            let siblings = store.records.filter {
                $0.kind == "group" && $0.workspace == form.workspace && $0.id != id
            }
            patch["position"] = SuiteJSON.number(suiteNextPosition(siblings))
        }
        if store.space == .rdp {
            if !form.identityId.isEmpty || SuiteJSON.has(d, "identity_id") {
                patch["identity_id"] = SuiteJSON.optional(form.identityId.isEmpty ? nil : form.identityId)
            }
            let drives = form.drives.written
            if drives != nil || SuiteJSON.has(d, "drives") { patch["drives"] = drives ?? .null }
        }
        var ops: [SuiteOp] = [.put(id: id, kind: "group", seq: group?.seq, patch: patch)]
        if moved {
            // The group's hosts move along.
            for host in store.records where host.kind == "host" && SuiteJSON.ref(host.data, "group_id") == id {
                ops.append(.put(id: host.id, kind: "host", seq: host.seq, patch: ["workspace": .string(form.workspace)]))
            }
        }
        busy = true
        let ok = await store.save(
            ops, done: group == nil ? featureString("Gruppe angelegt ✧") : featureString("Gespeichert ✧"))
        busy = false
        dismiss()
        if ok { onSaved(id) }
    }
}

// MARK: Identity

struct SuiteIdentityEditor: View {
    let store: SuiteSpaceStore
    let identity: SuiteRecord?
    let onSaved: (String) -> Void

    private enum Secret: Equatable {
        case keep
        case value(String)
    }

    private struct Fields: Equatable {
        var label, username, domain, authType, keyId: String
        var password: Secret
    }

    @Environment(\.dismiss) private var dismiss
    @State private var form: Fields
    private let initial: Fields
    private let secretId: String?
    private let hasSecret: Bool
    @State private var busy = false

    init(store: SuiteSpaceStore, identity: SuiteRecord?, onSaved: @escaping (String) -> Void) {
        self.store = store
        self.identity = identity
        self.onSaved = onSaved
        let d = identity?.data
        secretId = SuiteJSON.ref(d, "password_secret_id")
        hasSecret = secretId.map { store.index[$0]?.kind == "secret" } ?? false
        let auth = SuiteJSON.str(d, "auth_type")
        let fields = Fields(
            label: SuiteJSON.str(d, "label"), username: SuiteJSON.str(d, "username"),
            domain: SuiteJSON.str(d, "domain"), authType: auth.isEmpty ? "password" : auth,
            keyId: SuiteJSON.ref(d, "key_id") ?? "", password: hasSecret ? .keep : .value(""))
        _form = State(initialValue: fields)
        initial = fields
    }

    private var usesKey: Bool { form.authType == "key" || form.authType == "cert" }

    private var passwordText: Binding<String> {
        Binding(
            get: {
                if case .value(let text) = form.password { return text }
                return ""
            },
            set: { text in
                if text.isEmpty, form.password == .keep { return }
                form.password = .value(text)
            })
    }

    var body: some View {
        let password = featureString("Passwort")
        SuiteEditorFrame(
            title: identity == nil ? featureString("Neue Anmeldung") : featureString("Anmeldung bearbeiten"),
            dirty: form != initial, busy: busy,
            canSave: !form.username.trimmingCharacters(in: .whitespaces).isEmpty
                || !form.label.trimmingCharacters(in: .whitespaces).isEmpty,
            save: save
        ) {
            Section {
                SuiteTextField(label: featureString("Bezeichnung"), text: $form.label)
                SuiteTextField(label: featureString("Benutzername"), text: $form.username, plain: true)
            }
            if store.space == .rdp {
                Section {
                    SuiteTextField(label: featureString("Domäne"), text: $form.domain, plain: true)
                } footer: {
                    Text(features: "Leer für ein lokales Konto.")
                }
            }
            Section {
                SuiteValuePicker(
                    label: featureString("Anmelden mit"), value: $form.authType,
                    options: suiteAuthTypes.map { ($0, suiteAuthLabel($0)) })
                if usesKey {
                    SuiteRecordPicker(
                        label: featureString("Schlüssel"), records: suiteOfKind(store.records, "key"),
                        selection: $form.keyId, none: featureString("Keiner"))
                }
            }
            Section {
                HStack(spacing: UwUSpacing.xs) {
                    SuitePasswordInput(
                        store: store, text: passwordText, placeholder: form.password == .keep ? "••••••••••••" : nil)
                    if form.password == .keep, let secretId {
                        Button {
                            Task {
                                if let text = await store.reveal(secretId) { form.password = .value(text) }
                            }
                        } label: {
                            UwUIcon(.show, size: .sm, label: Text(features: "\(password) zeigen"))
                        }
                        .buttonStyle(.borderless)
                        .help(Text(features: "\(password) zeigen"))
                    }
                    if form.password != .keep && hasSecret {
                        Button {
                            form.password = .keep
                        } label: {
                            UwUIcon(.undo, size: .sm, label: Text(features: "\(password) unverändert lassen"))
                        }
                        .buttonStyle(.borderless)
                        .help(Text(features: "\(password) unverändert lassen"))
                    }
                }
            } header: {
                Text(features: "Passwort")
            } footer: {
                if form.password == .keep {
                    Text(features: "Bleibt, wie es ist.")
                } else if hasSecret && form.password == .value("") {
                    Text(features: "Wird beim Speichern entfernt.").foregroundStyle(Color.uwuWarningInk)
                }
            }
        }
    }

    private func save() async {
        let d = identity?.data
        let id = identity?.id ?? suiteNewId()
        var ops: [SuiteOp] = []
        let username = form.username.trimmingCharacters(in: .whitespaces)
        let label = form.label.trimmingCharacters(in: .whitespaces)
        var patch: SuiteJSONObject = [
            "label": .string(label.isEmpty ? username : label), "username": .string(username),
            "auth_type": .string(form.authType),
            // Only a key login picks a key; any other leaves the pointer as it was.
            "key_id": SuiteJSON.optional(usesKey ? (form.keyId.isEmpty ? nil : form.keyId) : SuiteJSON.ref(d, "key_id")),
        ]
        let domain = form.domain.trimmingCharacters(in: .whitespaces)
        if store.space == .rdp && (!domain.isEmpty || SuiteJSON.has(d, "domain")) {
            patch["domain"] = .string(domain)
        }
        if case .value(let value) = form.password {
            if !value.isEmpty {
                if hasSecret, let secretId {
                    ops.append(.secret(id: secretId, text: value, seq: store.index[secretId]?.seq))
                } else {
                    let fresh = suiteNewId()
                    ops.append(.secret(id: fresh, text: value, seq: nil))
                    patch["password_secret_id"] = .string(fresh)
                }
            } else if let secretId {
                patch["password_secret_id"] = .null
                if hasSecret { ops.append(.delete(id: secretId, seq: nil)) }
            } else if identity == nil {
                patch["password_secret_id"] = .null
            }
        }
        // The identity first: the secret it lets go of is deleted after.
        ops.insert(.put(id: id, kind: "identity", seq: identity?.seq, patch: patch), at: 0)
        busy = true
        let ok = await store.save(
            ops, done: identity == nil ? featureString("Anmeldung angelegt ✧") : featureString("Gespeichert ✧"))
        busy = false
        dismiss()
        if ok { onSaved(id) }
    }
}

// MARK: Keys

/// Renaming a key; its halves never change.
struct SuiteKeyEditor: View {
    let store: SuiteSpaceStore
    let key: SuiteRecord
    let onSaved: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var label: String
    @State private var busy = false

    init(store: SuiteSpaceStore, key: SuiteRecord, onSaved: @escaping (String) -> Void) {
        self.store = store
        self.key = key
        self.onSaved = onSaved
        _label = State(initialValue: SuiteJSON.str(key.data, "label"))
    }

    var body: some View {
        SuiteEditorFrame(
            title: featureString("Schlüssel bearbeiten"), dirty: label != SuiteJSON.str(key.data, "label"), busy: busy,
            canSave: !label.trimmingCharacters(in: .whitespaces).isEmpty, save: save
        ) {
            Section {
                SuiteTextField(label: featureString("Bezeichnung"), text: $label)
            }
        }
    }

    private func save() async {
        busy = true
        let ok = await store.save(
            [.put(id: key.id, kind: "key", seq: key.seq, patch: ["label": .string(label.trimmingCharacters(in: .whitespaces))])],
            done: featureString("Gespeichert ✧"))
        busy = false
        dismiss()
        if ok { onSaved(key.id) }
    }
}

/// A new SSH key: made here (Ed25519), or an existing private key taken in.
struct SuiteNewKeySheet: View {
    let store: SuiteSpaceStore
    let onSaved: (String) -> Void

    private enum Mode: Hashable { case generate, `import` }

    private struct Fields: Equatable {
        var label = "", comment = "", passphrase = "", repeated = "", privateKey = ""
    }

    @Environment(\.dismiss) private var dismiss
    @State private var mode = Mode.generate
    @State private var form = Fields()
    @State private var busy = false
    @State private var error: String?
    @State private var picking = false

    private var problem: String? {
        if form.label.trimmingCharacters(in: .whitespaces).isEmpty { return featureString("Die Bezeichnung fehlt.") }
        if mode == .generate && form.passphrase != form.repeated {
            return featureString("Die Passphrasen stimmen nicht überein.")
        }
        if mode == .import && form.privateKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return featureString("Der private Schlüssel fehlt.")
        }
        return nil
    }

    var body: some View {
        let dirty = form != Fields()
        SuiteEditorFrame(
            title: featureString("Neuer Schlüssel"), dirty: dirty, busy: busy, canSave: problem == nil,
            problem: error ?? (dirty ? problem : nil), save: save
        ) {
            Section {
                Picker(selection: $mode) {
                    Text(features: "Neu erzeugen").tag(Mode.generate)
                    Text(features: "Importieren").tag(Mode.import)
                } label: {
                    Text(features: "Schlüssel")
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                SuiteTextField(label: featureString("Bezeichnung"), text: $form.label)
            }
            if mode == .generate {
                Section {
                    SuiteTextField(label: featureString("Kommentar"), text: $form.comment, plain: true)
                } footer: {
                    Text(features: "Steht hinten im öffentlichen Schlüssel, oft benutzer@rechner.")
                }
                Section {
                    SecureField(text: $form.passphrase) { Text(features: "Passphrase (optional)") }
                    SecureField(text: $form.repeated) { Text(features: "Passphrase wiederholen") }
                } footer: {
                    if busy && !form.passphrase.isEmpty {
                        Text(features: "Verschlüsselt den Schlüssel mit der Passphrase … das dauert einen Moment.")
                    } else {
                        Text(
                            features:
                                "Ed25519, wie ssh-keygen ihn macht. Der private Schlüssel bleibt verschlüsselt in deinem Tresor.")
                    }
                }
            } else {
                Section {
                    TextField(text: $form.privateKey, prompt: Text(verbatim: "-----BEGIN OPENSSH PRIVATE KEY-----"), axis: .vertical) {
                        Text(features: "Privater Schlüssel")
                    }
                    .font(.uwuMono(.caption))
                    .lineLimit(4...12)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
                    Button {
                        picking = true
                    } label: {
                        Label(features: "Datei wählen …", uwuIcon: .file)
                    }
                    SecureField(text: $form.passphrase) { Text(features: "Passphrase, falls der Schlüssel eine hat") }
                } header: {
                    Text(features: "Privater Schlüssel")
                }
            }
        }
        .onChange(of: form) { error = nil }
        .onChange(of: mode) { error = nil }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.data, .text]) { result in
            if case .success(let url) = result { pick(url) }
        }
    }

    private func pick(_ url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        guard size <= 64 * 1024, let data = try? Data(contentsOf: url), data.count <= 64 * 1024 else {
            error = featureString("Das ist kein privater Schlüssel (zu groß).")
            return
        }
        let text = String(decoding: data, as: UTF8.self)
        var name = url.lastPathComponent
        for suffix in [".pem", ".key", ".ppk"] where name.lowercased().hasSuffix(suffix) {
            name = String(name.dropLast(suffix.count))
        }
        form.privateKey = text
        if form.label.isEmpty { form.label = name }
        DispatchQueue.main.async { error = nil }
    }

    private func save() async {
        busy = true
        error = nil
        defer { busy = false }
        do {
            let label = form.label.trimmingCharacters(in: .whitespaces)
            let passphrase = form.passphrase.isEmpty ? nil : form.passphrase
            let saved =
                mode == .generate
                ? try await store.generateKey(
                    label: label, comment: form.comment.trimmingCharacters(in: .whitespaces), passphrase: passphrase)
                : try await store.importKey(label: label, privateKey: form.privateKey, passphrase: passphrase)
            if store.took(saved), let id = saved.id {
                dismiss()
                onSaved(id)
            } else {
                dismiss()
            }
        } catch {
            let failure = Core.failure(error)
            if failure.kind == "key-format" || failure.kind == "key-passphrase" {
                self.error = suiteFailureText(failure)
            } else {
                FeatureToasts.shared.show(suiteFailureText(failure), danger: true)
            }
        }
    }
}

// MARK: Snippet

struct SuiteSnippetEditor: View {
    let store: SuiteSpaceStore
    let snippet: SuiteRecord?
    let onSaved: (String) -> Void

    private struct Fields: Equatable {
        var label, groupPath, body: String
    }

    @Environment(\.dismiss) private var dismiss
    @State private var form: Fields
    private let initial: Fields
    @State private var busy = false

    init(store: SuiteSpaceStore, snippet: SuiteRecord?, onSaved: @escaping (String) -> Void) {
        self.store = store
        self.snippet = snippet
        self.onSaved = onSaved
        let d = snippet?.data
        let fields = Fields(
            label: SuiteJSON.str(d, "label"), groupPath: SuiteJSON.str(d, "group_path"), body: SuiteJSON.str(d, "body"))
        _form = State(initialValue: fields)
        initial = fields
    }

    var body: some View {
        SuiteEditorFrame(
            title: snippet == nil ? featureString("Neues Snippet") : featureString("Snippet bearbeiten"),
            dirty: form != initial, busy: busy,
            canSave: !form.label.trimmingCharacters(in: .whitespaces).isEmpty, save: save
        ) {
            Section {
                SuiteTextField(label: featureString("Bezeichnung"), text: $form.label)
                SuiteTextField(label: featureString("Ordner"), text: $form.groupPath)
            }
            Section {
                TextField(text: $form.body, axis: .vertical) { Text(features: "Befehl") }
                    .font(.uwuMono(.body))
                    .lineLimit(3...12)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
            } header: {
                Text(features: "Befehl")
            }
        }
    }

    private func save() async {
        let id = snippet?.id ?? suiteNewId()
        let path = form.groupPath.trimmingCharacters(in: .whitespaces)
        busy = true
        let ok = await store.save(
            [
                .put(
                    id: id, kind: "snippet", seq: snippet?.seq,
                    patch: [
                        "label": .string(form.label.trimmingCharacters(in: .whitespaces)),
                        "group_path": SuiteJSON.optional(path.isEmpty ? nil : path), "body": .string(form.body),
                    ])
            ], done: snippet == nil ? featureString("Snippet angelegt ✧") : featureString("Gespeichert ✧"))
        busy = false
        dismiss()
        if ok { onSaved(id) }
    }
}

// MARK: Port forward

struct SuiteTunnelEditor: View {
    let store: SuiteSpaceStore
    let hostId: String
    let tunnel: SuiteRecord?
    let onSaved: (String) -> Void

    private struct Fields: Equatable {
        var name, kind, bindAddress, bindPort, targetHost, targetPort: String
        var autostart: Bool
    }

    @Environment(\.dismiss) private var dismiss
    @State private var form: Fields
    private let initial: Fields
    @State private var busy = false

    init(store: SuiteSpaceStore, hostId: String, tunnel: SuiteRecord?, onSaved: @escaping (String) -> Void) {
        self.store = store
        self.hostId = hostId
        self.tunnel = tunnel
        self.onSaved = onSaved
        let d = tunnel?.data
        let kind = SuiteJSON.str(d, "kind")
        let bindPort = SuiteJSON.int(d, "bind_port")
        let targetPort = SuiteJSON.int(d, "target_port")
        let fields = Fields(
            name: SuiteJSON.str(d, "name"), kind: kind.isEmpty ? "local" : kind,
            bindAddress: tunnel == nil ? "127.0.0.1" : SuiteJSON.str(d, "bind_address"),
            bindPort: bindPort > 0 ? String(bindPort) : "",
            targetHost: tunnel == nil ? "localhost" : SuiteJSON.str(d, "target_host"),
            targetPort: targetPort > 0 ? String(targetPort) : "", autostart: SuiteJSON.bool(d, "autostart"))
        _form = State(initialValue: fields)
        initial = fields
    }

    private var problem: String? {
        suitePort(form.bindPort) == nil || suitePort(form.targetPort) == nil
            || form.targetHost.trimmingCharacters(in: .whitespaces).isEmpty
            ? featureString("Ports sind 1 bis 65535, und das Ziel braucht eine Adresse.") : nil
    }

    var body: some View {
        let local = form.kind != "remote"
        let dirty = form != initial
        SuiteEditorFrame(
            title: tunnel == nil ? featureString("Neue Weiterleitung") : featureString("Weiterleitung bearbeiten"),
            dirty: dirty, busy: busy, canSave: problem == nil, problem: dirty ? problem : nil, save: save
        ) {
            Section {
                SuiteTextField(label: featureString("Name"), text: $form.name)
                SuiteValuePicker(
                    label: featureString("Richtung"), value: $form.kind,
                    options: [("local", featureString("Lokal (-L)")), ("remote", featureString("Entfernt (-R)"))])
            }
            Section {
                SuiteTextField(
                    label: local ? featureString("Lauscht hier auf") : featureString("Lauscht auf dem Server auf"),
                    text: $form.bindAddress, mono: true)
                SuiteTextField(label: featureString("Port"), text: $form.bindPort, mono: true)
            } header: {
                Text(verbatim: local ? featureString("Lauscht hier auf") : featureString("Lauscht auf dem Server auf"))
            }
            Section {
                SuiteTextField(
                    label: local ? featureString("Ziel, vom Server aus") : featureString("Ziel, von hier aus"),
                    text: $form.targetHost, mono: true)
                SuiteTextField(label: featureString("Port"), text: $form.targetPort, mono: true)
            } header: {
                Text(verbatim: local ? featureString("Ziel, vom Server aus") : featureString("Ziel, von hier aus"))
            }
            Section {
                Toggle(isOn: $form.autostart) { Text(features: "Mit jedem Terminal zu diesem Host starten") }
            }
        }
    }

    private func save() async {
        let id = tunnel?.id ?? suiteNewId()
        busy = true
        let ok = await store.save(
            [
                .put(
                    id: id, kind: "port_forward", seq: tunnel?.seq,
                    patch: [
                        "host_id": .string(hostId), "name": .string(form.name.trimmingCharacters(in: .whitespaces)),
                        "kind": .string(form.kind),
                        "bind_address": .string(form.bindAddress.trimmingCharacters(in: .whitespaces)),
                        "bind_port": SuiteJSON.number(suitePort(form.bindPort) ?? 0),
                        "target_host": .string(form.targetHost.trimmingCharacters(in: .whitespaces)),
                        "target_port": SuiteJSON.number(suitePort(form.targetPort) ?? 0),
                        "autostart": .bool(form.autostart),
                    ])
            ], done: featureString("Gespeichert ✧"))
        busy = false
        dismiss()
        if ok { onSaved(id) }
    }
}
