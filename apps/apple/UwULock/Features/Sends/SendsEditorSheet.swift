import SwiftUI
import UniformTypeIdentifiers
import UwUDesign

/// A file picked for a new file Send: read only when it is saved.
struct SendsPickedFile: Equatable {
    let url: URL
    let name: String
    let size: Int
}

/// The Send editor (SendsDialog.tsx `SendEditor`, mobile/pages/Sends.tsx `SendEditor`): a new
/// text or file Send, or a change to one, with every option the web vault has.
struct SendsEditorSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let target: SendsEditorTarget
    let store: SendsStore
    let onSaved: (String) -> Void

    @State private var uwu = UwuStore.shared
    @State private var kind: SendKind
    @State private var form: SendsForm
    @State private var file: SendsPickedFile?
    @State private var options: SendOptions?
    /// Where the link points: a send domain's id, nil the server's own address.
    @State private var domainId: String?
    /// False until the account's default domain is known (a new Send).
    @State private var domainKnown: Bool
    @State private var domainChanged = false
    @State private var importing = false
    @State private var busy = false
    @State private var error: String?
    @State private var askDiscard = false
    private let initial: SendsForm

    init(target: SendsEditorTarget, store: SendsStore, onSaved: @escaping (String) -> Void) {
        self.target = target
        self.store = store
        self.onSaved = onSaved
        let form = SendsForm(target.send)
        initial = form
        _form = State(initialValue: form)
        _kind = State(initialValue: target.kind)
        _domainId = State(initialValue: target.send?.sendDomainId)
        _domainKnown = State(initialValue: target.send != nil)
    }

    private var send: Send? { target.send }
    private var domains: [SendDomain] {
        uwu.status.has(.sendDomains) ? (options?.domains ?? []) : []
    }
    private var mailOk: Bool { options?.emails == true || form.access == .emails }
    private var ready: Bool { options != nil && form.ready(send: send, kind: kind) }
    private var dirty: Bool { form != initial || file != nil || kind != target.kind || domainChanged }
    private var maxBytes: Int { SendsFileLimit.bytes(uwu.status.limits?.maxFileBytes) }

    var body: some View {
        NavigationStack {
            Form {
                if send == nil {
                    Section {
                        Picker(selection: $kind) {
                            Text(features: "Text").tag(SendKind.text)
                            Text(features: "Datei").tag(SendKind.file)
                        } label: {
                            Text(features: "Art des Sends")
                        }
                        .pickerStyle(.segmented)
                    }
                }
                if let error {
                    Section {
                        Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                            .foregroundStyle(Color.uwuDangerInk)
                            .accessibilityAddTraits(.isStaticText)
                    }
                }
                Section {
                    TextField(text: limited(\.name, 200), prompt: Text(features: "z. B. WLAN für Gäste")) {
                        Text(features: "Name des Sends")
                    }
                }
                carried
                validity
                access
                Section {
                    Toggle(isOn: $form.hideEmail) { Text(features: "Meine Adresse nicht zeigen") }
                    Toggle(isOn: $form.disabled) {
                        Text(features: "Deaktiviert")
                        Text(features: "Der Link öffnet vorerst nichts")
                    }
                }
                Section {
                    TextField(text: limited(\.notes, 1000), axis: .vertical) {
                        Text(features: "Notizen (nur für dich)")
                    }
                    .lineLimit(2...6)
                } header: {
                    Text(features: "Notizen (nur für dich)")
                }
            }
            .formStyle(.grouped)
            .navigationTitle(title)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    #if os(iOS)
                    // The system's ✕, so the title keeps its room.
                    Button(role: .cancel) {
                        if dirty { askDiscard = true } else { dismiss() }
                    }
                    #else
                    Button {
                        if dirty { askDiscard = true } else { dismiss() }
                    } label: { Text(features: "Abbrechen") }
                    #endif
                }
                ToolbarItem(placement: .confirmationAction) {
                    if busy {
                        ProgressView().controlSize(.small)
                    } else {
                        #if os(iOS)
                        Button(role: .confirm) {
                            Task { await save() }
                        }
                        .accessibilityLabel(send == nil ? Text(features: "Anlegen") : Text(features: "Speichern"))
                        .disabled(!ready)
                        .keyboardShortcut(.defaultAction)
                        #else
                        Button {
                            Task { await save() }
                        } label: {
                            send == nil ? Text(features: "Anlegen") : Text(features: "Speichern")
                        }
                        .disabled(!ready)
                        .keyboardShortcut(.defaultAction)
                        #endif
                    }
                }
            }
            .confirmationDialog(
                Text(features: "Änderungen verwerfen?"), isPresented: $askDiscard, titleVisibility: .visible
            ) {
                Button(role: .destructive) { dismiss() } label: { Text(features: "Verwerfen") }
            }
            .fileImporter(isPresented: $importing, allowedContentTypes: [.item]) { result in
                pick(result)
            }
            .task { await loadOptions() }
        }
        .interactiveDismissDisabled(dirty || busy)
        #if os(macOS)
        .frame(minWidth: 480, idealWidth: 540, minHeight: 560, idealHeight: 680)
        #else
        .presentationDetents([.large])
        #endif
        .sensoryFeedback(.error, trigger: error) { _, new in new != nil }
    }

    private var title: Text {
        if send != nil { return Text(features: "Send bearbeiten") }
        return kind == .file ? Text(features: "Neuer Datei-Send") : Text(features: "Neuer Text-Send")
    }

    // MARK: Sections

    @ViewBuilder
    private var carried: some View {
        if kind == .text, let send, send.entry {
            Section {
                Text(verbatim: send.text ?? "").textSelection(.enabled)
            } header: {
                Text(features: "Text · als Eintrag geteilt")
            } footer: {
                Text(features: "Ein geteilter Eintrag lässt sich nicht ändern: Sein Inhalt steckt noch ein zweites Mal im Send, für die Send-Seite. Um etwas zurückzunehmen, lösch diesen Send und teil den Eintrag neu.")
            }
        } else if kind == .text {
            Section {
                TextField(text: limited(\.text, 1000), prompt: Text(features: "Was geteilt wird"), axis: .vertical) {
                    Text(features: "Text")
                }
                .lineLimit(4...12)
                Toggle(isOn: $form.hidden) {
                    #if os(macOS)
                    Text(features: "Text erst nach einem Klick zeigen")
                    #else
                    Text(features: "Text erst nach Tipp zeigen")
                    #endif
                }
            } header: {
                Text(features: "Text")
            }
        } else if let send {
            Section {
                Label { Text(verbatim: send.fileName ?? "") } icon: { UwUIcon(.file, size: .sm) }
            } header: {
                Text(features: "Datei")
            } footer: {
                Text(features: "Die Datei eines Sends bleibt, wie sie ist: \(send.fileName ?? "")")
            }
        } else {
            Section {
                Button { importing = true } label: {
                    LabeledContent {
                        if let file { Text(verbatim: file.size.fileSize) }
                    } label: {
                        Label {
                            if let file {
                                Text(verbatim: file.name)
                            } else {
                                Text(features: "Datei wählen …")
                            }
                        } icon: { UwUIcon(.upload, size: .sm) }
                    }
                }
            } header: {
                Text(features: "Datei")
            } footer: {
                Text(features: "Höchstens \(maxBytes.fileSize).")
            }
        }
    }

    private var validity: some View {
        Section {
            Picker(selection: $form.deletionDays) {
                ForEach(SendsDays.all, id: \.self) { days in
                    Text(verbatim: SendsDays.label(days)).tag(days)
                }
            } label: {
                Label(features: "Löschen nach", uwuIcon: .delete)
            }
            Picker(selection: $form.expiresDays) {
                Text(features: "Nie (bis zum Löschen)").tag(0)
                ForEach(SendsDays.all, id: \.self) { days in
                    Text(verbatim: SendsDays.label(days)).tag(days)
                }
            } label: {
                Label(features: "Läuft ab nach", uwuIcon: .reminder)
            }
            Stepper(value: $form.maxAccess, in: 0...1000) {
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(features: "Höchstens so oft öffnen")
                        Group {
                            if form.maxAccess > 0 {
                                Text(verbatim: "\(form.maxAccess)")
                            } else {
                                Text(features: "unbegrenzt")
                            }
                        }
                        .uwuFont(.subheadline)
                        .foregroundStyle(Color.uwuMuted)
                        .contentTransition(.numericText())
                    }
                } icon: {
                    UwUIcon(.show)
                }
            }
        } header: {
            Text(features: "Gültigkeit")
        } footer: {
            if send != nil {
                Text(features: "Beide zählen ab jetzt: Speichern setzt die Daten neu.")
            }
        }
    }

    private var access: some View {
        Section {
            Picker(selection: $form.access) {
                Text(verbatim: SendsAccessOption.label(.anyone)).tag(SendAccess.anyone)
                Text(verbatim: SendsAccessOption.label(.password)).tag(SendAccess.password)
                if mailOk {
                    Text(verbatim: SendsAccessOption.label(.emails)).tag(SendAccess.emails)
                }
            } label: {
                Label(features: "Wer darf öffnen?", uwuIcon: accessIcon)
            }
            if form.access == .password {
                SecureField(text: $form.password) {
                    send?.hasPassword == true
                        ? Text(features: "Neues Passwort (leer lässt das alte)")
                        : Text(features: "Passwort")
                }
                .textContentType(.newPassword)
            }
            if form.access == .emails {
                TextField(
                    text: $form.emails,
                    prompt: Text(verbatim: "friend@example.com, family@example.org"),
                    axis: .vertical
                ) {
                    Text(features: "E-Mail-Adressen")
                }
                .lineLimit(2...5)
                .autocorrectionDisabled()
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .keyboardType(.emailAddress)
                #endif
            }
            if !domains.isEmpty {
                Picker(selection: domainBinding) {
                    Text(features: "Hauptadresse des Servers").tag("")
                    ForEach(domains) { domain in
                        Text(verbatim: sendsDomainLabel(domain)).tag(domain.id)
                    }
                } label: {
                    Label(features: "Link-Adresse", uwuIcon: .website)
                }
                .disabled(!domainKnown)
            }
        } header: {
            Text(features: "Wer darf öffnen?")
        } footer: {
            VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                if !mailOk {
                    Text(features: "Nur bestimmte Adressen braucht Mail auf dem Server; hier ist keine eingerichtet.")
                } else if form.access == .emails {
                    let count = form.addresses.count
                    if count > 0 {
                        Text(features: "\(count) Adressen")
                    }
                    Text(features: "Wer den Link öffnet, gibt seine Adresse an und bekommt einen Code per Mail. Der Server kennt dafür die Adressen.")
                }
                if !domains.isEmpty {
                    Text(features: "Der Send öffnet sich unter jeder dieser Adressen; diese steht im Link, den du kopierst.")
                }
            }
        }
    }

    private var accessIcon: UwUIconName {
        switch form.access {
        case .anyone: .link
        case .password: .locked
        case .emails: .account
        }
    }

    private var domainBinding: Binding<String> {
        Binding(
            get: { domainId ?? "" },
            set: {
                domainId = $0.isEmpty ? nil : $0
                domainChanged = true
            })
    }

    /// A text field of the form, cut at `limit` characters as the page's `maxLength`.
    private func limited(_ path: WritableKeyPath<SendsForm, String>, _ limit: Int) -> Binding<String> {
        Binding(
            get: { form[keyPath: path] },
            set: { form[keyPath: path] = String($0.prefix(limit)) })
    }

    // MARK: Actions

    private func loadOptions() async {
        #if DEBUG
        if FeatureFixtures.isOn {
            options = SendsFixtures.options
            if !domainKnown { domainId = SendsFixtures.options.defaultDomainId; domainKnown = true }
            return
        }
        #endif
        do {
            let found = try await model.core.sendOptions()
            options = found
            // A new Send starts on the account's default domain, which the server gives it anyway.
            if !domainKnown {
                domainId = found.defaultDomainId
                domainKnown = true
            }
        } catch {
            options = SendOptions(emails: false, domains: [], defaultDomainId: nil)
            domainKnown = true
        }
    }

    private func pick(_ result: Result<URL, any Error>) {
        guard case .success(let url) = result else { return }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        if size > maxBytes {
            error = featureString("Die Datei ist zu groß: höchstens \(maxBytes.fileSize).")
            return
        }
        error = nil
        file = SendsPickedFile(url: url, name: url.lastPathComponent, size: size)
        form.fileName = url.lastPathComponent
    }

    private func save() async {
        guard ready, !busy else { return }
        busy = true
        error = nil
        defer { busy = false }
        do {
            var bytes: Data?
            if kind == .file && send == nil {
                guard let file else { return }
                bytes = try await Self.read(file.url)
                if (bytes?.count ?? 0) > maxBytes {
                    error = featureString("Die Datei ist zu groß: höchstens \(maxBytes.fileSize).")
                    return
                }
            }
            let draft = form.draft(send: send, kind: kind)
            let choice: SendLinkDomain =
                domains.isEmpty ? .unchanged : (domainId.map { .domain(id: $0) } ?? .server)
            let id = try await store.save(model.core, id: send?.id, draft: draft, file: bytes, domain: choice)
            // A new Send's link is what the person wants next.
            var copied = false
            if send == nil, let link = store.send(id)?.link {
                copied = await SendsLink.copyQuietly(model.core, link)
            }
            FeatureToasts.shared.show(
                send != nil
                    ? featureString("Gespeichert ✧")
                    : copied
                        ? featureString("Send angelegt – der Link ist kopiert ✧")
                        : featureString("Send angelegt ✧"))
            onSaved(id)
            dismiss()
        } catch {
            self.error = Core.failure(error).message
        }
    }

    /// The picked file's bytes, read away from the main thread.
    private nonisolated static func read(_ url: URL) async throws -> Data {
        try await Task.detached(priority: .userInitiated) {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            return try Data(contentsOf: url)
        }.value
    }
}

#if DEBUG
#Preview("New Send") {
    UserDefaults.standard.set(true, forKey: "UwULockFixtures")
    return SendsEditorSheet(target: .new(.text), store: SendsStore()) { _ in }
        .environment(AppModel())
}

#Preview("Edit Send") {
    UserDefaults.standard.set(true, forKey: "UwULockFixtures")
    return SendsEditorSheet(target: .edit(SendsFixtures.sends[2]), store: SendsStore()) { _ in }
        .environment(AppModel())
}
#endif
