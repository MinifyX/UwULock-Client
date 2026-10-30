// File requests (components/FileRequestsDialog.tsx, mobile/pages/Requests.tsx): the requests of
// the account and their uploads, loaded from the core (or the fixtures), and how they read.
import SwiftUI
import UwUDesign

/// The file requests on screen and the uploads of the ones opened.
@MainActor
@Observable
final class FileRequestsModel {
    /// nil until the first answer.
    private(set) var requests: [FileRequest]?
    /// The last failure loading the list, in the core's words.
    private(set) var error: String?
    /// The uploads by request id; missing: not loaded yet.
    private(set) var submissions: [String: [Submission]] = [:]

    @ObservationIgnored private weak var core: Core?

    init() {
        #if DEBUG
        if FeatureFixtures.isOn {
            requests = FileRequestFixtures.requests
            submissions = FileRequestFixtures.submissions
        }
        #endif
    }

    #if DEBUG
    /// Samples, for previews.
    static func preview() -> FileRequestsModel {
        let model = FileRequestsModel()
        model.requests = FileRequestFixtures.requests
        model.submissions = FileRequestFixtures.submissions
        model.fixtures = true
        return model
    }
    @ObservationIgnored private var fixtures = false
    #endif

    private var usesFixtures: Bool {
        #if DEBUG
        fixtures || FeatureFixtures.isOn
        #else
        false
        #endif
    }

    /// Newest expiry first, as the pages sort.
    var sorted: [FileRequest] {
        (requests ?? []).sorted { ($0.expirationDate ?? "") > ($1.expirationDate ?? "") }
    }

    func request(_ id: String?) -> FileRequest? {
        guard let id else { return nil }
        return requests?.first { $0.id == id }
    }

    func attach(_ core: Core) {
        self.core = core
    }

    private func requireCore() throws -> Core {
        guard let core else { throw CoreFailure.unsupported(featureString("Der Kern antwortet nicht.")) }
        return core
    }

    // MARK: Loading

    func load() async {
        guard !usesFixtures else { return }
        do {
            requests = try await requireCore().fileRequests()
            error = nil
        } catch {
            self.error = Core.failure(error).message
            if requests == nil { requests = [] }
        }
    }

    /// The uploads of `requestId`; a failure shows as a toast and an empty list.
    func loadSubmissions(_ requestId: String) async {
        guard !usesFixtures else {
            if submissions[requestId] == nil { submissions[requestId] = [] }
            return
        }
        do {
            submissions[requestId] = try await requireCore().fileRequestSubmissions(requestId: requestId)
        } catch {
            FeatureToasts.shared.show(error: error)
            submissions[requestId] = []
        }
    }

    /// The list, the uploads already loaded and the extras' unseen count again.
    func reloadAll() async {
        await load()
        for id in submissions.keys {
            await loadSubmissions(id)
        }
        await UwuStore.shared.refresh()
    }

    // MARK: Requests

    func save(_ request: FileRequest?, input: FileRequestInput, newLink: Bool, removePassword: Bool) async throws -> FileRequest {
        let saved: FileRequest
        if usesFixtures {
            saved = FileRequestFixtures.apply(input, to: request, newLink: newLink, removePassword: removePassword)
        } else if let request {
            saved = try await requireCore().updateFileRequest(
                id: request.id, input: input, newLink: newLink, removePassword: removePassword)
        } else {
            saved = try await requireCore().createFileRequest(input: input)
        }
        var list = (requests ?? []).filter { $0.id != saved.id }
        list.append(saved)
        requests = list
        await UwuStore.shared.refresh()
        return saved
    }

    /// Changes one thing of `request` (the switches on its page); the date and password stay.
    func change(_ request: FileRequest, _ patch: (inout FileRequestInput) -> Void) async throws {
        var input = FileRequestText.input(of: request)
        patch(&input)
        _ = try await save(request, input: input, newLink: false, removePassword: false)
    }

    func delete(_ request: FileRequest) async throws {
        if !usesFixtures {
            try await requireCore().deleteFileRequest(id: request.id)
        }
        requests?.removeAll { $0.id == request.id }
        submissions[request.id] = nil
        await UwuStore.shared.refresh()
    }

    func copyLink(_ link: String) async {
        do {
            if !usesFixtures {
                try await requireCore().copyGenerated(text: link)
            }
            FeatureToasts.shared.show(featureString("Link kopiert ✧"))
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }

    func openWebVault() async {
        guard !usesFixtures else { return }
        do {
            try await requireCore().openWebVaultAt(place: .fileRequests)
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }

    // MARK: Uploads

    /// Opening an upload is seeing it (mobile/pages/Requests.tsx); the desktop's "Gesehen".
    func markSeen(_ request: FileRequest, _ submission: Submission) async {
        guard !submission.seen else { return }
        do {
            if !usesFixtures {
                try await requireCore().markSubmissionSeen(requestId: request.id, submissionId: submission.id)
            }
            updateSubmissions(request.id) { list in
                if let index = list.firstIndex(where: { $0.id == submission.id }) { list[index].seen = true }
            }
            updateRequest(request.id) { $0.unseen = max(0, $0.unseen - 1) }
            await reloadAfterUploadChange(request.id)
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }

    func deleteSubmission(_ request: FileRequest, _ submission: Submission) async throws {
        if !usesFixtures {
            try await requireCore().deleteSubmission(requestId: request.id, submissionId: submission.id)
        }
        updateSubmissions(request.id) { $0.removeAll { $0.id == submission.id } }
        updateRequest(request.id) {
            $0.submissionCount = max(0, $0.submissionCount - 1)
            if !submission.seen { $0.unseen = max(0, $0.unseen - 1) }
        }
        await reloadAfterUploadChange(request.id)
    }

    /// Saves one file into the host's download folder; answers where it is.
    func saveFile(_ request: FileRequest, _ submission: Submission, _ file: SubmissionFile) async throws -> URL {
        if usesFixtures {
            let url = FileManager.default.temporaryDirectory.appending(path: file.name ?? file.id)
            if !FileManager.default.fileExists(atPath: url.path) {
                try Data("UwULock sample file\n".utf8).write(to: url)
            }
            return url
        }
        let path = try await requireCore().saveSubmissionFile(
            requestId: request.id, submissionId: submission.id, fileId: file.id, allowRisky: file.risky)
        return URL(filePath: path)
    }

    /// Takes the upload over into a secure note; answers the new item's id.
    func takeOver(_ request: FileRequest, _ submission: Submission, name: String) async throws -> String {
        let id: String
        if usesFixtures {
            id = UUID().uuidString
        } else {
            id = try await requireCore().takeOverSubmission(
                requestId: request.id, submissionId: submission.id, name: name,
                senderLabel: featureString("Absender (nicht geprüft)"))
        }
        updateSubmissions(request.id) { $0.removeAll { $0.id == submission.id } }
        updateRequest(request.id) {
            $0.submissionCount = max(0, $0.submissionCount - 1)
            if !submission.seen { $0.unseen = max(0, $0.unseen - 1) }
        }
        await reloadAfterUploadChange(request.id)
        return id
    }

    private func reloadAfterUploadChange(_ requestId: String) async {
        guard !usesFixtures else { return }
        await load()
        await loadSubmissions(requestId)
        await UwuStore.shared.refresh()
    }

    private func updateSubmissions(_ requestId: String, _ change: (inout [Submission]) -> Void) {
        guard var list = submissions[requestId] else { return }
        change(&list)
        submissions[requestId] = list
    }

    private func updateRequest(_ id: String, _ change: (inout FileRequest) -> Void) {
        guard let index = requests?.firstIndex(where: { $0.id == id }) else { return }
        change(&requests![index])
    }
}

/// How a request reads (Requests.tsx `validText`, `uploadsText`, `requestName`, `requestState`).
enum FileRequestText {
    enum State {
        case on, off, gone
    }

    static func expired(_ request: FileRequest) -> Bool {
        guard let date = FeatureDate.parse(request.expirationDate) else { return false }
        return date < .now
    }

    static func state(_ request: FileRequest) -> State {
        request.disabled ? .off : expired(request) ? .gone : .on
    }

    static func name(_ request: FileRequest) -> String {
        if let label = request.label, !label.isEmpty { return label }
        if let title = request.title, !title.isEmpty { return title }
        return featureString("Dateianfrage")
    }

    static func valid(_ request: FileRequest) -> String {
        if request.disabled { return featureString("abgeschaltet") }
        if expired(request) { return featureString("abgelaufen") }
        let when = FeatureDate.short(request.expirationDate) ?? ""
        return featureString("bis \(when)")
    }

    static func uploads(_ request: FileRequest) -> String {
        if let max = request.maxSubmissions, max > 0 {
            return featureString("\(request.submissionCount) von \(max) Uploads")
        }
        return featureString("\(request.submissionCount) Uploads")
    }

    static func subtitle(_ request: FileRequest) -> String {
        [
            request.submissionCount > 0 ? uploads(request) : featureString("Noch nichts angekommen"),
            valid(request),
        ].joined(separator: " · ")
    }

    static func sender(_ submission: Submission) -> String? {
        let parts = [submission.senderName, submission.senderEmail].compactMap { $0?.isEmpty == false ? $0 : nil }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    static func senderTitle(_ submission: Submission) -> String {
        if let name = submission.senderName, !name.isEmpty { return name }
        if let email = submission.senderEmail, !email.isEmpty { return email }
        return featureString("Ohne Absender")
    }

    static func fileName(_ file: SubmissionFile) -> String {
        file.name ?? featureString("(unlesbarer Name)")
    }

    /// The request as the editor saves it; the date and password stay.
    static func input(of request: FileRequest) -> FileRequestInput {
        FileRequestInput(
            label: request.label ?? "", title: request.title ?? "", note: request.note,
            owner: request.owner, expiresInDays: nil, maxSubmissions: request.maxSubmissions,
            maxFiles: request.maxFiles,
            maxFileMib: request.maxFileBytes.map { $0 / 1024 / 1024 },
            textAllowed: request.textAllowed, password: nil, sendDomainId: request.sendDomainId,
            disabled: request.disabled)
    }
}

/// The state chip of a request or an upload (mobile/ui.tsx `StateChip`).
struct FileRequestStateChip: View {
    enum Kind {
        case on, off, gone, new
    }

    let kind: Kind

    init(_ kind: Kind) {
        self.kind = kind
    }

    init(_ state: FileRequestText.State) {
        switch state {
        case .on: kind = .on
        case .off: kind = .off
        case .gone: kind = .gone
        }
    }

    var body: some View {
        text
            .uwuFont(.caption, weight: .semibold)
            .foregroundStyle(ink)
            .padding(.horizontal, UwUSpacing.xs)
            .padding(.vertical, 2)
            .background(tint, in: Capsule())
    }

    private var text: Text {
        switch kind {
        case .on: Text(features: "aktiv")
        case .off: Text(features: "abgeschaltet")
        case .gone: Text(features: "abgelaufen")
        case .new: Text(features: "neu")
        }
    }

    private var ink: Color {
        switch kind {
        case .on: .uwuSuccessInk
        case .off: .uwuMuted
        case .gone: .uwuWarningInk
        case .new: .uwuPinkInk
        }
    }

    private var tint: Color {
        switch kind {
        case .on: .uwuSuccessTint
        case .off: .uwuControl
        case .gone: .uwuWarningTint
        case .new: .uwuPinkTint
        }
    }
}

/// The count of new uploads on a row.
struct FileRequestUnseenBadge: View {
    let count: Int

    var body: some View {
        Text(verbatim: "\(count)")
            .uwuFont(.caption, weight: .bold)
            .monospacedDigit()
            .foregroundStyle(Color.uwuOnPink)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(Color.uwuPinkSolid, in: Capsule())
            .accessibilityLabel(Text(features: "\(count) neu"))
    }
}
