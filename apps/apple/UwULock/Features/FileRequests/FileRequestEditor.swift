import SwiftUI
import UwUDesign

/// The editor of a file request (Requests.tsx `RequestEditor`, FileRequestsDialog.tsx
/// `RequestForm`): every option the server takes, within its limits.
struct FileRequestEditor: View {
    let store: FileRequestsModel
    let request: FileRequest?
    /// The saved request; `true`: it is a new one.
    var onSaved: (FileRequest, Bool) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var form: Values
    @State private var dirty = false
    @State private var busy = false
    @State private var error: String?
    @State private var confirmDiscard = false
    @FocusState private var focus: Field?

    private enum Field: Hashable {
        case title, label, note, owner, size, password
    }

    private static let days = [1, 3, 7, 14, 30, 90]

    struct Values: Equatable {
        var label: String
        var title: String
        var note: String
        var owner: String
        /// 0: keep the date (only when changing).
        var days: Int
        /// 0: until it expires.
        var maxSubmissions: Int
        var maxFiles: Int
        var maxFileMib: String
        var textAllowed: Bool
        var password: String
        var removePassword: Bool
        var sendDomainId: String
        var disabled: Bool
        var newLink: Bool
    }

    init(store: FileRequestsModel, request: FileRequest?, onSaved: @escaping (FileRequest, Bool) -> Void) {
        self.store = store
        self.request = request
        self.onSaved = onSaved
        let limits = Self.limits
        // A week, or the longest the server allows when that is shorter.
        let newDays = min(7, Self.days.filter { $0 <= limits.maxDays }.last ?? 1)
        _form = State(initialValue: Values(
            label: request?.label ?? "",
            title: request?.title ?? "",
            note: request?.note ?? "",
            owner: request?.owner ?? "",
            days: request == nil ? newDays : 0,
            maxSubmissions: request.map { $0.maxSubmissions ?? 0 } ?? 1,
            maxFiles: min(request?.maxFiles ?? 10, limits.maxFiles),
            maxFileMib: request?.maxFileBytes.map { String($0 / 1024 / 1024) }
                ?? String(min(limits.serverMib ?? 100, 100)),
            textAllowed: request?.textAllowed ?? true,
            password: "",
            removePassword: false,
            sendDomainId: request?.sendDomainId ?? "",
            disabled: request?.disabled ?? false,
            newLink: request?.foreignKey ?? false))
    }

    /// The server's limits (lib/uwu.ts `limits`), with the pages' defaults.
    @MainActor
    private static var limits: (maxDays: Int, maxFiles: Int, serverMib: Int?) {
        let limits = UwuStore.shared.status.limits
        return (
            limits?.fileRequestMaxDays ?? 90,
            min(20, limits?.fileRequestMaxFiles ?? 20),
            limits?.maxFileBytes.map { $0 / 1024 / 1024 })
    }

    private var dayOptions: [Int] {
        (request == nil ? [] : [0]) + Self.days.filter { $0 <= Self.limits.maxDays }
    }

    private var ready: Bool {
        !busy && !form.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !(form.maxFiles == 0 && !form.textAllowed)
    }

    var body: some View {
        NavigationStack {
            content
                .formStyle(.grouped)
                .navigationTitle(request == nil ? Text(features: "Neue Dateianfrage") : Text(features: "Dateianfrage ändern"))
                #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button {
                            if dirty { confirmDiscard = true } else { dismiss() }
                        } label: {
                            Text(features: "Abbrechen")
                        }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button {
                            save()
                        } label: {
                            if busy {
                                Text(features: "Speichert …")
                            } else if request == nil {
                                Text(features: "Anlegen")
                            } else {
                                Text(features: "Speichern")
                            }
                        }
                        .disabled(!ready)
                        .keyboardShortcut(.return, modifiers: .command)
                    }
                }
                .confirmationDialog(
                    Text(features: "Änderungen verwerfen?"),
                    isPresented: $confirmDiscard,
                    titleVisibility: .visible
                ) {
                    Button(role: .destructive) {
                        dismiss()
                    } label: {
                        Text(features: "Verwerfen")
                    }
                    Button(role: .cancel) {} label: { Text(features: "Weiter bearbeiten") }
                }
        }
        .interactiveDismissDisabled(dirty || busy)
        .onChange(of: form) { dirty = true }
        .onAppear { focus = request == nil ? .title : nil }
        #if os(iOS)
        .presentationDetents([.large])
        #else
        .frame(minWidth: 520, idealWidth: 560, minHeight: 600, idealHeight: 720)
        #endif
    }

    private var content: some View {
        Form {
            if let error {
                Section {
                    Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                        .foregroundStyle(Color.uwuDangerInk)
                }
            }

            Section {
                TextField(text: limited(\.label, 200), prompt: Text(verbatim: form.title.isEmpty ? featureString("z. B. Steuerunterlagen") : form.title)) {
                    Text(features: "Dein Name dafür (nur für dich)")
                }
                .focused($focus, equals: .label)
            } header: {
                Text(features: "Dein Name dafür (nur für dich)")
            }

            Section {
                TextField(text: limited(\.title, 200), prompt: Text(features: "z. B. Scan des Reisepasses")) {
                    Text(features: "Titel für den Empfänger")
                }
                .focused($focus, equals: .title)
                TextField(text: $form.note, prompt: Text(features: "Hinweis für den Empfänger (optional)"), axis: .vertical) {
                    Text(features: "Hinweis für den Empfänger (optional)")
                }
                .lineLimit(2...6)
                .focused($focus, equals: .note)
                TextField(text: limited(\.owner, 100), prompt: Text(features: "Dein Name, wie der Empfänger ihn sieht (optional)")) {
                    Text(features: "Dein Name, wie der Empfänger ihn sieht (optional)")
                }
                .focused($focus, equals: .owner)
                #if os(iOS)
                .textContentType(.name)
                #endif
            } header: {
                Text(features: "Was der Empfänger sieht")
            }

            Section {
                Picker(selection: $form.days) {
                    ForEach(dayOptions, id: \.self) { days in
                        dayText(days).tag(days)
                    }
                } label: {
                    Label(features: "Gültig für", uwuIcon: .reminder)
                }
                Stepper(value: $form.maxSubmissions, in: 0...100) {
                    LabeledContent {
                        form.maxSubmissions > 0 ? Text(verbatim: "\(form.maxSubmissions)") : Text(features: "bis zum Ablauf")
                    } label: {
                        Label(features: "Uploads höchstens", uwuIcon: .inbox)
                    }
                }
                Stepper(value: $form.maxFiles, in: 0...Self.limits.maxFiles) {
                    LabeledContent {
                        Text(verbatim: "\(form.maxFiles)")
                    } label: {
                        Label(features: "Dateien je Upload", uwuIcon: .files)
                    }
                }
                LabeledContent {
                    TextField(text: sizeBinding, prompt: Text(verbatim: Self.limits.serverMib.map(String.init) ?? "100")) {
                        Text(features: "Größe je Datei (MB)")
                    }
                    .multilineTextAlignment(.trailing)
                    .labelsHidden()
                    .frame(maxWidth: 120)
                    .focused($focus, equals: .size)
                    #if os(iOS)
                    .keyboardType(.numberPad)
                    #endif
                } label: {
                    Label(features: "Größe je Datei (MB)", uwuIcon: .file)
                }
                Toggle(isOn: $form.textAllowed) {
                    Text(features: "Eine Nachricht erlauben")
                }
            } header: {
                Text(features: "Regeln")
            } footer: {
                if form.maxFiles == 0 && !form.textAllowed {
                    Text(features: "Ohne Dateien und ohne Nachricht käme nichts an.")
                        .foregroundStyle(Color.uwuDangerInk)
                } else if let mib = Self.limits.serverMib {
                    Text(features: "Der Server nimmt höchstens \(mib) MB je Datei.")
                }
            }

            Section {
                SecureField(
                    text: $form.password,
                    prompt: request?.passwordSet == true ? nil : Text(features: "Ohne Passwort")
                ) {
                    request?.passwordSet == true
                        ? Text(features: "Neues Passwort (leer: bleibt)")
                        : Text(features: "Passwort (optional)")
                }
                .textContentType(.newPassword)
                .disabled(form.removePassword)
                .focused($focus, equals: .password)
                if request?.passwordSet == true {
                    Toggle(isOn: Binding(
                        get: { form.removePassword },
                        set: { form.removePassword = $0; if $0 { form.password = "" } }
                    )) {
                        Text(features: "Passwort entfernen")
                    }
                }
                let domains = UwuStore.shared.status.sendDomains
                if !domains.isEmpty {
                    Picker(selection: $form.sendDomainId) {
                        Text(features: "Hauptadresse des Servers").tag("")
                        ForEach(domains) { domain in
                            Text(verbatim: Self.host(domain.url)).tag(domain.id)
                        }
                    } label: {
                        Label(features: "Link-Adresse", uwuIcon: .website)
                    }
                }
            }

            if request != nil {
                Section {
                    Toggle(isOn: $form.disabled) {
                        Text(features: "Abschalten (der Link nimmt nichts mehr an)")
                    }
                    Toggle(isOn: $form.newLink) {
                        Text(features: "Neuer Link – alle bisherigen Links funktionieren dann nicht mehr")
                    }
                }
            }
        }
    }

    private func dayText(_ days: Int) -> Text {
        switch days {
        case 0: Text(features: "Datum behalten")
        case 1: Text(features: "1 Tag")
        default: Text(features: "\(days) Tage")
        }
    }

    static func host(_ url: String) -> String {
        url.replacing(/^https?:\/\//, with: "")
    }

    /// A text field's binding cut to `max` characters (the pages' `maxLength`).
    private func limited(_ key: WritableKeyPath<Values, String>, _ max: Int) -> Binding<String> {
        Binding(
            get: { form[keyPath: key] },
            set: { form[keyPath: key] = String($0.prefix(max)) })
    }

    /// Digits only, at most what the server takes.
    private var sizeBinding: Binding<String> {
        Binding(
            get: { form.maxFileMib },
            set: { text in
                var digits = String(text.filter(\.isNumber).prefix(7))
                if let server = Self.limits.serverMib, let value = Int(digits), value > server {
                    digits = String(server)
                }
                form.maxFileMib = digits
            })
    }

    private func save() {
        guard ready else { return }
        busy = true
        error = nil
        let trimmed = { (text: String) in text.trimmingCharacters(in: .whitespacesAndNewlines) }
        let note = trimmed(form.note)
        let owner = trimmed(form.owner)
        let mib = Int(form.maxFileMib).map { Swift.max(1, $0) }
        let input = FileRequestInput(
            label: trimmed(form.label),
            title: trimmed(form.title),
            note: note.isEmpty ? nil : note,
            owner: owner.isEmpty ? nil : owner,
            expiresInDays: form.days > 0 ? form.days : nil,
            maxSubmissions: form.maxSubmissions > 0 ? form.maxSubmissions : nil,
            maxFiles: form.maxFiles,
            maxFileMib: mib,
            textAllowed: form.textAllowed,
            password: form.password.isEmpty ? nil : form.password,
            sendDomainId: form.sendDomainId.isEmpty ? nil : form.sendDomainId,
            disabled: form.disabled)
        Task {
            defer { busy = false }
            do {
                let saved = try await store.save(
                    request, input: input, newLink: form.newLink, removePassword: form.removePassword)
                FeatureToasts.shared.show(request == nil ? featureString("Angelegt ✧") : featureString("Gespeichert ✧"))
                onSaved(saved, request == nil)
                dismiss()
            } catch {
                self.error = Core.failure(error).message
            }
        }
    }
}

#if DEBUG
#Preview("Neu") {
    FileRequestEditor(store: .preview(), request: nil) { _, _ in }
}

#Preview("Ändern") {
    FileRequestEditor(store: .preview(), request: FileRequestFixtures.requests[0]) { _, _ in }
}
#endif
