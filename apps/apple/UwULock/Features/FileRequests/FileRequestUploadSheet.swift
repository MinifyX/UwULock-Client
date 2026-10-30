import QuickLook
import SwiftUI
import UwUDesign
#if os(macOS)
import AppKit
#endif

/// One upload (Requests.tsx `ShortSheet`, FileRequestsDialog.tsx `submission`): the sender (not
/// checked), the message, the files to save, taking it over into an item, deleting it.
struct FileRequestUploadSheet: View {
    let store: FileRequestsModel
    let request: FileRequest
    let submissionID: String

    @Environment(\.dismiss) private var dismiss
    @State private var busy = false
    /// The files to save once confirmed (risky ones always; on the Mac every one, as the desktop).
    @State private var asking: [SubmissionFile]?
    @State private var deleting = false
    /// Where the saved files are, by file id.
    @State private var saved: [String: URL] = [:]
    @State private var preview: URL?
    @State private var takingOver: Submission?

    private var submission: Submission? {
        store.submissions[request.id]?.first { $0.id == submissionID }
    }

    var body: some View {
        NavigationStack {
            Group {
                if let submission {
                    form(submission)
                } else {
                    FeatureEmptyState(mood: .sleepy, title: Text(features: "Gelöscht"))
                }
            }
            .navigationTitle(Text(features: "Upload von \(submission.map(senderName) ?? featureString("Unbekannt"))"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        dismiss()
                    } label: {
                        Text(features: "Fertig")
                    }
                }
            }
        }
        .quickLookPreview($preview)
        .sheet(item: $takingOver) { submission in
            FileRequestTakeOverSheet(store: store, request: request, submission: submission) {
                takingOver = nil
                dismiss()
            }
        }
        .alert(
            Text(features: "Datei speichern?"),
            isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
            presenting: asking
        ) { files in
            Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
            Button {
                save(files)
            } label: {
                Text(features: "Speichern")
            }
        } message: { files in
            if files.contains(where: \.risky) {
                Text(features: "Achtung: Diese Datei ist ein Programm, ein Skript oder ein Dokument mit Makros. Sie kann beim Öffnen alles tun, was du auf diesem Rechner darfst. Öffne sie nur, wenn du weißt, von wem sie ist und dass du sie erwartest.")
            } else {
                Text(features: "„\(files.map(FileRequestText.fileName).joined(separator: ", "))“ wird entschlüsselt in deinem Ordner „Downloads“ gespeichert. Öffne Dateien von Unbekannten mit Vorsicht.")
            }
        }
        .confirmationDialog(
            Text(features: "Upload löschen?"),
            isPresented: $deleting,
            titleVisibility: .visible
        ) {
            Button(role: .destructive) {
                deleteUpload()
            } label: {
                Text(features: "Löschen")
            }
            Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
        } message: {
            Text(features: "Die Nachricht und die Dateien dieses Uploads verschwinden vom Server.")
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        #else
        .frame(minWidth: 460, idealWidth: 520, minHeight: 480, idealHeight: 560)
        #endif
    }

    private func senderName(_ submission: Submission) -> String {
        if let name = submission.senderName, !name.isEmpty { return name }
        if let email = submission.senderEmail, !email.isEmpty { return email }
        return featureString("Unbekannt")
    }

    // MARK: The upload

    private func form(_ submission: Submission) -> some View {
        Form {
            Section {
                HStack(spacing: UwUSpacing.sm) {
                    UwUIcon(.account, size: .md)
                        .foregroundStyle(Color.uwuMuted)
                        .frame(width: 28)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: FileRequestText.sender(submission) ?? "–")
                            .uwuFont(.body, weight: .medium)
                            .textSelection(.enabled)
                        Text(verbatim: [featureString("Absender (nicht geprüft)"), FeatureDate.short(submission.creationDate)]
                            .compactMap { $0 }
                            .joined(separator: " · "))
                            .uwuFont(.subheadline)
                            .foregroundStyle(Color.uwuMuted)
                    }
                }
                .help(Text(features: "Das hat der Absender selbst eingetragen."))
                if let text = submission.text, !text.isEmpty {
                    Text(verbatim: text)
                        .uwuFont(.body)
                        .textSelection(.enabled)
                }
            }

            if submission.broken {
                Section {
                    Label {
                        Text(features: "Dieser Upload ließ sich mit deinem Schlüssel nicht öffnen.")
                    } icon: {
                        UwUIcon(.warning, size: .sm)
                    }
                    .foregroundStyle(Color.uwuDangerInk)
                    .listRowBackground(Color.uwuDangerTint)
                }
            }

            if !submission.files.isEmpty {
                Section {
                    ForEach(submission.files) { file in
                        fileRow(submission, file)
                    }
                } header: {
                    Text(features: "Dateien")
                } footer: {
                    Text(features: "Öffne Dateien von Unbekannten mit Vorsicht.")
                }
            }

            Section {
                Button {
                    takingOver = submission
                } label: {
                    Label(features: "Als Eintrag übernehmen", uwuIcon: .vault)
                }
                .disabled(busy || submission.broken)
                if submission.files.contains(where: { $0.name != nil }) {
                    Button {
                        ask(submission.files)
                    } label: {
                        #if os(iOS)
                        Label(features: "In Dateien sichern", uwuIcon: .download)
                        #else
                        Label(features: "In „Downloads“ speichern", uwuIcon: .download)
                        #endif
                    }
                    .disabled(busy)
                }
                Button(role: .destructive) {
                    deleting = true
                } label: {
                    Label(features: "Upload löschen", uwuIcon: .delete)
                        .foregroundStyle(Color.uwuDangerInk)
                }
                .disabled(busy)
            }
        }
        .formStyle(.grouped)
    }

    private func fileRow(_ submission: Submission, _ file: SubmissionFile) -> some View {
        HStack(spacing: UwUSpacing.sm) {
            UwUIcon(file.risky ? .warning : .file, size: .md)
                .foregroundStyle(file.risky ? Color.uwuWarningInk : Color.uwuMuted)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: FileRequestText.fileName(file))
                    .uwuFont(.body)
                    .lineLimit(2)
                    .truncationMode(.middle)
                Text(verbatim: file.size.fileSize)
                    .uwuFont(.caption)
                    .foregroundStyle(Color.uwuMuted)
            }
            Spacer(minLength: UwUSpacing.xs)
            if let url = saved[file.id] {
                savedActions(url)
            } else {
                Button {
                    ask([file])
                } label: {
                    UwUIcon(.download, size: .md)
                }
                .buttonStyle(.borderless)
                .disabled(busy || file.name == nil)
                .accessibilityLabel(Text(features: "\(file.name ?? "") speichern"))
                .help(Text(features: "In „Downloads“ speichern"))
            }
        }
        .contextMenu {
            if let url = saved[file.id] {
                Button {
                    preview = url
                } label: {
                    Label(features: "Vorschau", uwuIcon: .show)
                }
                #if os(macOS)
                Button {
                    NSWorkspace.shared.activateFileViewerSelecting([url])
                } label: {
                    Label(features: "Im Finder zeigen", uwuIcon: .folder)
                }
                #endif
                ShareLink(item: url) {
                    Label(features: "Teilen", uwuIcon: .share)
                }
            } else if file.name != nil {
                Button {
                    ask([file])
                } label: {
                    Label(features: "In „Downloads“ speichern", uwuIcon: .download)
                }
            }
        }
    }

    @ViewBuilder
    private func savedActions(_ url: URL) -> some View {
        HStack(spacing: UwUSpacing.sm) {
            Button {
                preview = url
            } label: {
                UwUIcon(.show, size: .md)
            }
            .accessibilityLabel(Text(features: "Vorschau"))
            .help(Text(features: "Vorschau"))
            #if os(macOS)
            Button {
                NSWorkspace.shared.activateFileViewerSelecting([url])
            } label: {
                UwUIcon(.folder, size: .md)
            }
            .accessibilityLabel(Text(features: "Im Finder zeigen"))
            .help(Text(features: "Im Finder zeigen"))
            #endif
            ShareLink(item: url) {
                UwUIcon(.share, size: .md)
            }
            .accessibilityLabel(Text(features: "Teilen"))
            .help(Text(features: "Teilen"))
        }
        .buttonStyle(.borderless)
        .foregroundStyle(Color.uwuPinkInk)
    }

    // MARK: Actions

    /// Programs, scripts and documents with macros are asked about first; on the Mac every file,
    /// as the desktop does.
    private func ask(_ files: [SubmissionFile]) {
        let named = files.filter { $0.name != nil }
        guard !named.isEmpty else { return }
        #if os(macOS)
        asking = named
        #else
        if named.contains(where: \.risky) {
            asking = named
        } else {
            save(named)
        }
        #endif
    }

    private func save(_ files: [SubmissionFile]) {
        guard let submission else { return }
        busy = true
        Task {
            defer { busy = false }
            do {
                var last: URL?
                for file in files {
                    let url = try await store.saveFile(request, submission, file)
                    saved[file.id] = url
                    last = url
                }
                #if os(macOS)
                FeatureToasts.shared.show(featureString("Gespeichert: \(last?.path(percentEncoded: false) ?? "")"))
                #else
                FeatureToasts.shared.show(featureString("Gespeichert ✧"))
                // One harmless file: show it right away; the share sheet is one tap further.
                if files.count == 1, let last, files.first?.risky == false { preview = last }
                #endif
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }

    private func deleteUpload() {
        guard let submission else { return }
        busy = true
        Task {
            defer { busy = false }
            do {
                try await store.deleteSubmission(request, submission)
                FeatureToasts.shared.show(featureString("Gelöscht."))
                dismiss()
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }
}

/// "Als Eintrag übernehmen" (Requests.tsx `TakeOver`): a secure note with the message and the
/// sender, the files as attachments; the upload leaves the request.
struct FileRequestTakeOverSheet: View {
    let store: FileRequestsModel
    let request: FileRequest
    let submission: Submission
    var onDone: () -> Void = {}

    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var busy = false
    @State private var error: String?
    @FocusState private var focused: Bool

    init(store: FileRequestsModel, request: FileRequest, submission: Submission, onDone: @escaping () -> Void = {}) {
        self.store = store
        self.request = request
        self.submission = submission
        self.onDone = onDone
        let title = (request.title?.isEmpty == false ? request.title : request.label) ?? ""
        _name = State(initialValue: [title, submission.senderName ?? ""]
            .filter { !$0.isEmpty }
            .joined(separator: " – "))
    }

    private var ready: Bool {
        !busy && !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        NavigationStack {
            Form {
                if let error {
                    Section {
                        Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                            .foregroundStyle(Color.uwuDangerInk)
                    }
                }
                Section {
                    TextField(text: $name) {
                        Text(features: "Name des Eintrags")
                    }
                    .focused($focused)
                    .onChange(of: name) { _, new in
                        if new.count > 200 { name = String(new.prefix(200)) }
                    }
                    .onSubmit { if ready { run() } }
                } header: {
                    Text(features: "Name des Eintrags")
                } footer: {
                    Text(features: "Eine sichere Notiz mit der Nachricht und dem Absender, die Dateien als Anhänge. Der Upload verschwindet danach aus der Anfrage.")
                }
            }
            .formStyle(.grouped)
            .navigationTitle(Text(features: "Als Eintrag übernehmen"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button {
                        dismiss()
                    } label: {
                        Text(features: "Abbrechen")
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        run()
                    } label: {
                        busy ? Text(features: "Übernimmt …") : Text(features: "Übernehmen")
                    }
                    .disabled(!ready)
                }
            }
            .onAppear { focused = true }
        }
        .interactiveDismissDisabled(busy)
        #if os(iOS)
        .presentationDetents([.medium])
        #else
        .frame(minWidth: 420, idealWidth: 460, minHeight: 240)
        #endif
    }

    private func run() {
        busy = true
        error = nil
        Task {
            defer { busy = false }
            do {
                _ = try await store.takeOver(
                    request, submission, name: name.trimmingCharacters(in: .whitespacesAndNewlines))
                FeatureToasts.shared.show(featureString("Als Eintrag übernommen ✧"))
                dismiss()
                onDone()
            } catch {
                self.error = Core.failure(error).message
            }
        }
    }
}

#if DEBUG
#Preview("Upload") {
    FileRequestUploadSheet(
        store: .preview(), request: FileRequestFixtures.requests[0], submissionID: "s1")
}

#Preview("Übernehmen") {
    FileRequestTakeOverSheet(
        store: .preview(), request: FileRequestFixtures.requests[0],
        submission: FileRequestFixtures.submissions["fr-tax"]![0])
}
#endif
