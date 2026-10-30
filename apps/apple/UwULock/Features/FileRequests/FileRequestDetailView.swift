import SwiftUI
import UwUDesign

/// A request's page (Requests.tsx `RequestPage`, FileRequestsDialog.tsx `RequestView`): its link,
/// what arrived, its rules, switching it off and deleting it.
struct FileRequestDetailView: View {
    let store: FileRequestsModel
    let requestID: String
    var onEdit: (FileRequest) -> Void
    var onDeleted: () -> Void
    /// Pushed on a stack (iPhone): deleting goes back.
    var pushed = false

    @Environment(\.dismiss) private var dismiss
    @State private var uwu = UwuStore.shared
    @State private var busy = false
    @State private var shown: Submission?
    @State private var takingOver: Submission?
    @State private var deletingSubmission: Submission?
    @State private var deletingRequest = false

    private var request: FileRequest? { store.request(requestID) }

    var body: some View {
        Group {
            if let request {
                form(request)
            } else {
                FeatureEmptyState(mood: .sleepy, title: Text(features: "Gelöscht"))
            }
        }
        .navigationTitle(Text(verbatim: request.map(FileRequestText.name) ?? ""))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            if let request {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        onEdit(request)
                    } label: {
                        Label(features: "Ändern", uwuIcon: .edit)
                    }
                    .keyboardShortcut("e", modifiers: .command)
                    .help(Text(features: "Ändern"))
                }
            }
        }
        .task(id: requestID) { await store.loadSubmissions(requestID) }
        .sheet(item: $shown) { submission in
            if let request {
                FileRequestUploadSheet(store: store, request: request, submissionID: submission.id)
            }
        }
        .sheet(item: $takingOver) { submission in
            if let request {
                FileRequestTakeOverSheet(store: store, request: request, submission: submission)
            }
        }
        .confirmationDialog(
            Text(features: "Upload löschen?"),
            isPresented: Binding(get: { deletingSubmission != nil }, set: { if !$0 { deletingSubmission = nil } }),
            titleVisibility: .visible,
            presenting: deletingSubmission
        ) { submission in
            Button(role: .destructive) {
                deleteSubmission(submission)
            } label: {
                Text(features: "Löschen")
            }
            Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
        } message: { _ in
            Text(features: "Die Nachricht und die Dateien dieses Uploads verschwinden vom Server.")
        }
        .confirmationDialog(
            Text(features: "Dateianfrage löschen?"),
            isPresented: $deletingRequest,
            titleVisibility: .visible
        ) {
            Button(role: .destructive) {
                deleteRequest()
            } label: {
                Text(features: "Löschen")
            }
            Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
        } message: {
            Text(features: "Die Anfrage, ihr Link und alles, was darüber hochgeladen wurde, verschwinden vom Server.")
        }
        .onChange(of: uwu.notices) { Task { await store.loadSubmissions(requestID) } }
        .sensoryFeedback(.success, trigger: store.submissions[requestID]?.count)
    }

    // MARK: The page

    private func form(_ request: FileRequest) -> some View {
        Form {
            header(request)

            if request.foreignKey {
                Section {
                    Label {
                        Text(features: "Dieser Link verschlüsselt Uploads für einen Schlüssel, der nicht deiner ist – jemand, der das Link-Geheimnis kannte, hat ihn eingesetzt. Gib den Link nicht weiter; bearbeite die Anfrage mit einem neuen Link oder lösche sie.")
                    } icon: {
                        UwUIcon(.warning, size: .sm)
                    }
                    .foregroundStyle(Color.uwuDangerInk)
                    .listRowBackground(Color.uwuDangerTint)
                }
            }

            linkSection(request)

            if let note = request.note, !note.isEmpty {
                Section {
                    Text(verbatim: note)
                        .textSelection(.enabled)
                } header: {
                    Text(features: "Hinweis")
                }
            }

            uploads(request)
            rules(request)

            Section {
                Button {
                    toggleDisabled(request)
                } label: {
                    if request.disabled {
                        Label(features: "Wieder einschalten", uwuIcon: .start)
                    } else {
                        Label(features: "Abschalten", uwuIcon: .pending)
                            .foregroundStyle(Color.uwuDangerInk)
                    }
                }
                .disabled(busy)
                Button(role: .destructive) {
                    deletingRequest = true
                } label: {
                    Label(features: "Dateianfrage löschen", uwuIcon: .delete)
                        .foregroundStyle(Color.uwuDangerInk)
                }
                .disabled(busy)
            }
        }
        .formStyle(.grouped)
        .refreshable {
            await store.load()
            await store.loadSubmissions(requestID)
        }
    }

    private func header(_ request: FileRequest) -> some View {
        Section {
            VStack(spacing: UwUSpacing.sm) {
                UwUIcon(.inbox, size: .xl)
                    .foregroundStyle(Color.uwuPinkInk)
                    .frame(width: 64, height: 64)
                    .background(Color.uwuPinkTint, in: RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous))
                    .accessibilityHidden(true)
                Text(verbatim: FileRequestText.name(request))
                    .uwuFont(.title2, weight: .bold)
                    .multilineTextAlignment(.center)
                if let title = request.title, !title.isEmpty, request.label?.isEmpty == false {
                    VStack(spacing: 2) {
                        Text(features: "Titel für den Empfänger")
                            .uwuFont(.caption)
                            .foregroundStyle(Color.uwuMuted)
                        Text(verbatim: title)
                            .uwuFont(.subheadline)
                    }
                    .multilineTextAlignment(.center)
                }
                HStack(spacing: UwUSpacing.xs) {
                    FileRequestStateChip(FileRequestText.state(request))
                    Label {
                        Text(verbatim: FileRequestText.valid(request))
                    } icon: {
                        UwUIcon(.reminder, size: .xs)
                    }
                    .uwuFont(.caption, weight: .medium)
                    .foregroundStyle(Color.uwuMuted)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, UwUSpacing.xs)
            .listRowBackground(Color.clear)
        }
    }

    private func linkSection(_ request: FileRequest) -> some View {
        Section {
            if let link = request.link {
                Text(verbatim: link)
                    .uwuFont(.callout)
                    .fontDesign(.monospaced)
                    .textSelection(.enabled)
                    .lineLimit(3)
                    .truncationMode(.middle)
                HStack(spacing: UwUSpacing.sm) {
                    Button {
                        Task { await store.copyLink(link) }
                    } label: {
                        Label(features: "Link kopieren", uwuIcon: .copy)
                            .frame(maxWidth: .infinity)
                    }
                    .uwuProminentButton()
                    .keyboardShortcut("c", modifiers: [.command, .shift])
                    .help(Text(features: "Link kopieren"))
                    if let url = URL(string: link) {
                        ShareLink(item: url) {
                            Label(features: "Teilen", uwuIcon: .share)
                                .frame(maxWidth: .infinity)
                        }
                        .uwuGlassButton()
                    }
                }
                .buttonStyle(.borderless)
            } else {
                (request.foreignKey
                     ? Text(features: "Zurückgehalten: nicht für deinen Schlüssel.")
                     : Text(features: "Der Link lässt sich nicht mehr zeigen."))
                    .foregroundStyle(Color.uwuMuted)
            }
        } header: {
            Text(features: "Link")
        }
    }

    @ViewBuilder
    private func uploads(_ request: FileRequest) -> some View {
        Section {
            if let list = store.submissions[request.id] {
                if list.isEmpty {
                    VStack(spacing: UwUSpacing.xs) {
                        NyuView(mood: .sleepy).frame(width: 64)
                        Text(features: "Noch nichts angekommen")
                            .uwuFont(.headline)
                        Text(features: "Teile den Link, dann landen Uploads hier.")
                            .uwuFont(.subheadline)
                            .foregroundStyle(Color.uwuMuted)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, UwUSpacing.sm)
                } else {
                    ForEach(list) { submission in
                        submissionRow(request, submission)
                    }
                }
            } else {
                HStack {
                    Spacer()
                    ProgressView { Text(features: "Einen Moment …") }
                    Spacer()
                }
            }
        } header: {
            Text(features: "Eingegangen")
        }
    }

    private func submissionRow(_ request: FileRequest, _ submission: Submission) -> some View {
        Button {
            open(request, submission)
        } label: {
            HStack(spacing: UwUSpacing.sm) {
                UwUIcon(submission.broken ? .warning : .file, size: .md)
                    .foregroundStyle(submission.broken ? Color.uwuDangerInk : Color.uwuMuted)
                    .frame(width: 28)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: FileRequestText.senderTitle(submission))
                        .uwuFont(.body, weight: submission.seen ? .regular : .semibold)
                        .foregroundStyle(Color.uwuInk)
                    Text(verbatim: subtitle(submission))
                        .uwuFont(.subheadline)
                        .foregroundStyle(Color.uwuMuted)
                        .lineLimit(2)
                }
                Spacer(minLength: UwUSpacing.xs)
                if !submission.seen {
                    FileRequestStateChip(.new)
                }
                UwUIcon(.next, size: .xs)
                    .foregroundStyle(Color.uwuFaint)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(busy)
        .contextMenu {
            Button {
                open(request, submission)
            } label: {
                Label(features: "Öffnen", uwuIcon: .show)
            }
            if !submission.seen {
                Button {
                    Task { await store.markSeen(request, submission) }
                } label: {
                    Label(features: "Gesehen", uwuIcon: .done)
                }
            }
            Button {
                takingOver = submission
            } label: {
                Label(features: "Als Eintrag übernehmen", uwuIcon: .vault)
            }
            .disabled(submission.broken)
            Divider()
            Button(role: .destructive) {
                deletingSubmission = submission
            } label: {
                Label(features: "Upload löschen", uwuIcon: .delete)
            }
        }
        .swipeActions(edge: .trailing) {
            Button(role: .destructive) {
                deletingSubmission = submission
            } label: {
                Label(features: "Upload löschen", uwuIcon: .delete)
            }
            if !submission.broken {
                Button {
                    takingOver = submission
                } label: {
                    Label(features: "Als Eintrag übernehmen", uwuIcon: .vault)
                }
                .tint(.uwuPinkSolid)
            }
        }
        .swipeActions(edge: .leading) {
            if !submission.seen {
                Button {
                    Task { await store.markSeen(request, submission) }
                } label: {
                    Label(features: "Gesehen", uwuIcon: .done)
                }
                .tint(.uwuSuccess)
            }
        }
    }

    private func subtitle(_ submission: Submission) -> String {
        let names = submission.files.map(FileRequestText.fileName).joined(separator: ", ")
        return [names.isEmpty ? nil : names, FeatureDate.short(submission.creationDate)]
            .compactMap { $0 }
            .joined(separator: " · ")
    }

    private func rules(_ request: FileRequest) -> some View {
        Section {
            LabeledContent {
                Text(verbatim: request.maxFileBytes.map(\.fileSize) ?? "–")
            } label: {
                Text(features: "Größe je Datei")
            }
            LabeledContent {
                Text(verbatim: "\(request.maxFiles)")
            } label: {
                Text(features: "Dateien je Upload")
            }
            LabeledContent {
                if let max = request.maxSubmissions, max > 0 {
                    Text(verbatim: FileRequestText.uploads(request))
                } else {
                    Text(features: "bis zum Ablauf")
                }
            } label: {
                Text(features: "Uploads höchstens")
            }
            LabeledContent {
                request.passwordSet ? Text(features: "Ja") : Text(features: "Nein")
            } label: {
                Text(features: "Mit Passwort")
            }
            if let owner = request.owner, !owner.isEmpty {
                LabeledContent {
                    Text(verbatim: owner)
                } label: {
                    Text(features: "Dein Name, wie der Empfänger ihn sieht")
                }
            }
            Toggle(isOn: Binding(
                get: { request.textAllowed },
                set: { allowed in setTextAllowed(request, allowed) }
            )) {
                Text(features: "Eine Nachricht erlauben")
            }
            // Without files and without a message nothing could arrive.
            .disabled(busy || (request.textAllowed && request.maxFiles == 0))
        } header: {
            Text(features: "Regeln")
        } footer: {
            if let when = FeatureDate.short(request.deletionDate) {
                Text(features: "Der Server löscht die Anfrage mit allem darin am \(when).")
            }
        }
    }

    // MARK: Actions

    private func open(_ request: FileRequest, _ submission: Submission) {
        shown = submission
        // Opening an upload is seeing it (Requests.tsx).
        Task { await store.markSeen(request, submission) }
    }

    private func run(_ what: @escaping () async throws -> Void, done: String? = nil) {
        busy = true
        Task {
            defer { busy = false }
            do {
                try await what()
                if let done { FeatureToasts.shared.show(done) }
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }

    private func setTextAllowed(_ request: FileRequest, _ allowed: Bool) {
        run { try await store.change(request) { $0.textAllowed = allowed } }
    }

    private func toggleDisabled(_ request: FileRequest) {
        run(
            { try await store.change(request) { $0.disabled = !request.disabled } },
            done: request.disabled
                ? featureString("Wieder an ✧")
                : featureString("Abgeschaltet: der Link nimmt nichts mehr an"))
    }

    private func deleteSubmission(_ submission: Submission) {
        guard let request else { return }
        run({ try await store.deleteSubmission(request, submission) }, done: featureString("Gelöscht."))
    }

    private func deleteRequest() {
        guard let request else { return }
        busy = true
        Task {
            defer { busy = false }
            do {
                try await store.delete(request)
                FeatureToasts.shared.show(featureString("Dateianfrage gelöscht"))
                onDeleted()
                if pushed { dismiss() }
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }
}

#if DEBUG
#Preview("Dateianfrage") {
    NavigationStack {
        FileRequestDetailView(store: .preview(), requestID: "fr-tax", onEdit: { _ in }, onDeleted: {})
    }
    .featureToasts()
}
#endif
