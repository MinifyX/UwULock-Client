import SwiftUI
import UniformTypeIdentifiers
import UwUDesign

/// Moving in from another password manager's export (ImportDialog.tsx, mobile/pages/Import.tsx,
/// lib/importFlow.ts): Bitwarden (also password-protected), KeePass (KDBX, XML, CSV), 1Password,
/// Chrome, Firefox, Apple Passwörter, Proton Pass and LastPass. The file is read on this device;
/// the preview shows what comes before anything is in the vault. Presented as a sheet.
struct ImportFlowView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    @State private var flow: ImportFlowModel
    @State private var picking: ImportPickTarget?

    init() {
        #if DEBUG
        _flow = State(initialValue: ImportFlowModel(step: ImportFixtures.requested ?? .pick))
        #else
        _flow = State(initialValue: ImportFlowModel())
        #endif
    }

    #if DEBUG
    /// Opens on `step`, for previews.
    init(step: ImportStep) {
        _flow = State(initialValue: ImportFlowModel(step: step))
    }
    #endif

    var body: some View {
        NavigationStack {
            content
                .formStyle(.grouped)
                .navigationTitle(Text(features: "Importieren"))
                #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar { toolbar }
        }
        .uwuAnimation(value: flow.step.name)
        .interactiveDismissDisabled(flow.step.isRunning || flow.busy)
        .fileImporter(
            isPresented: Binding(get: { picking != nil }, set: { if !$0 { picking = nil } }),
            allowedContentTypes: [.data]
        ) { result in
            let target = picking
            picking = nil
            guard case .success(let url) = result, let target else { return }
            switch target {
            case .file:
                Task { await flow.pick(url) }
            case .keyFile(let handler):
                handler(url)
            }
        }
        .sensoryFeedback(trigger: flow.step.name) { _, new in
            new == "done" ? .success : nil
        }
        .sensoryFeedback(.error, trigger: flow.error) { _, new in new != nil }
        .featureToasts()
        .task { UwuStore.shared.attach(model.core) }
        #if os(macOS)
        .frame(minWidth: 520, idealWidth: 560, minHeight: 520, idealHeight: 640)
        #endif
    }

    // MARK: Steps

    @ViewBuilder private var content: some View {
        switch flow.step {
        case .pick, .reading:
            ImportPickStep(flow: flow, reading: flow.step.name == "reading") {
                picking = .file
            } exchange: { discard in
                Task { await flow.takeExchange(core: model.core, discard: discard) }
            }
            .task { await flow.checkExchange(core: model.core) }
        case .password(let file, let kind):
            ImportPasswordStep(flow: flow, file: file, kind: kind) { handler in
                picking = .keyFile(handler)
            } open: { password, keyFile in
                Task { await flow.unlock(password: password, keyFile: keyFile, core: model.core) }
            }
        case .preview(let parsed):
            ImportPreviewStep(parsed: parsed, error: flow.error)
        case .running(_, let progress):
            ImportRunningStep(progress: progress)
        case .done(let outcome):
            ImportDoneStep(outcome: outcome)
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        switch flow.step {
        case .done:
            ToolbarItem(placement: .cancellationAction) {
                Button { flow.reset() } label: { Text(features: "Noch eine Datei") }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button { dismiss() } label: { Text(features: "Fertig") }
                    .keyboardShortcut(.defaultAction)
            }
        case .running:
            ToolbarItem(placement: .principal) { EmptyView() }
        case .preview(let parsed):
            ToolbarItem(placement: .cancellationAction) { cancel }
            ToolbarItem(placement: .secondaryAction) {
                Button { flow.reset() } label: { Label(features: "Andere Datei", uwuIcon: .files) }
                    .help(Text(features: "Andere Datei"))
            }
            ToolbarItem(placement: .confirmationAction) {
                let count = parsed.data.items.count
                Button { Task { await flow.start(core: model.core) } } label: {
                    Text(features: "\(count) Einträge importieren")
                }
                .disabled(count == 0)
                .keyboardShortcut(.defaultAction)
            }
        default:
            ToolbarItem(placement: .cancellationAction) { cancel }
        }
    }

    private var cancel: some View {
        Button(role: .cancel) {
            flow.reset()
            dismiss()
        } label: {
            Text(features: "Abbrechen")
        }
        .disabled(flow.busy)
        .keyboardShortcut(.cancelAction)
    }
}

/// What the one `.fileImporter` is open for.
private enum ImportPickTarget {
    case file
    case keyFile(@MainActor (URL) -> Void)
}

// MARK: - Pick

private struct ImportPickStep: View {
    @Bindable var flow: ImportFlowModel
    let reading: Bool
    let choose: () -> Void
    let exchange: (_ discard: Bool) -> Void

    var body: some View {
        Form {
            if flow.exchangePending {
                Section {
                    HStack(alignment: .top, spacing: UwUSpacing.md) {
                        NyuView(mood: .sparkle)
                            .frame(width: 40, height: 40)
                            .accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                            Text(features: "Daten aus Apple Passwörter übernehmen?")
                                .uwuFont(.headline)
                            Text(features: "Du siehst alle Einträge, bevor etwas in deinem Tresor gespeichert wird.")
                                .uwuFont(.footnote)
                                .foregroundStyle(Color.uwuMuted)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    HStack(spacing: UwUSpacing.sm) {
                        Button { exchange(true) } label: {
                            Text(features: "Verwerfen").frame(maxWidth: .infinity)
                        }
                        .uwuGlassButton()
                        Button { exchange(false) } label: {
                            Text(features: "Ansehen").frame(maxWidth: .infinity)
                        }
                        .uwuProminentButton()
                        .foregroundStyle(Color.uwuOnPink)
                    }
                    .disabled(flow.busy)
                }
            }
            Section {
                HStack(alignment: .top, spacing: UwUSpacing.md) {
                    NyuView(mood: reading ? .puzzled : .uwu)
                        .frame(width: 56, height: 56)
                        .accessibilityHidden(true)
                    Text(features: "Aus Bitwarden, Vaultwarden, UwULock, KeePass, KeePassXC, 1Password, Chrome, Edge, Firefox, Apple Passwörter, Proton Pass oder LastPass. Die Datei wird nur hier auf diesem Gerät gelesen, und du siehst vorher, was kommt.")
                        .uwuFont(.callout)
                        .foregroundStyle(Color.uwuInk)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Section {
                Picker(selection: $flow.source) {
                    Text(features: "Automatisch erkennen").tag(ImportSource?.none)
                    ForEach(ImportSource.menu) { source in
                        Text(verbatim: source.label).tag(Optional(source))
                    }
                } label: {
                    Label(features: "Importieren aus", uwuIcon: .import)
                }
                .disabled(reading)
            } footer: {
                Text(features: "Anhänge kommen nicht mit: Lade sie danach im Web-Tresor hoch. Aus Apple Passwörter: Exportiere in den Einstellungen als CSV-Datei.")
            }
            if let error = flow.error {
                ImportErrorSection(text: error)
            }
            Section {
                Button(action: choose) {
                    HStack {
                        Spacer()
                        if reading {
                            ProgressView().controlSize(.small)
                            Text(features: "Liest …")
                        } else {
                            Label(features: "Datei wählen …", uwuIcon: .file)
                        }
                        Spacer()
                    }
                    .frame(maxWidth: .infinity)
                }
                .uwuProminentButton()
                .foregroundStyle(Color.uwuOnPink)
                .controlSize(.large)
                .disabled(reading)
                .keyboardShortcut("o", modifiers: .command)
                .help(Text(features: "Datei wählen …"))
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets())
        }
    }
}

// MARK: - Password

private struct ImportPasswordStep: View {
    let flow: ImportFlowModel
    let file: ImportFile
    let kind: ImportPasswordKind
    let chooseKeyFile: (@escaping @MainActor (URL) -> Void) -> Void
    let open: (String, [UInt8]?) -> Void

    @State private var password = ""
    @State private var keyFile: (name: String, bytes: [UInt8])?
    @FocusState private var focused: Bool

    var body: some View {
        Form {
            Section {
                Group {
                    if kind == .keepass {
                        Text(features: "„\(file.name)“ ist mit einem Passwort geschützt. UwULock öffnet die Datei hier auf diesem Gerät; das Passwort geht nirgendwohin.")
                    } else {
                        Text(features: "„\(file.name)“ ist ein passwortgeschützter Export von Bitwarden. Gib das Passwort ein, das beim Exportieren gewählt wurde.")
                    }
                }
                .uwuFont(.callout)
                .fixedSize(horizontal: false, vertical: true)
            }
            Section {
                SecureField(text: $password) { Text(features: "Passwort der Datei") }
                    .textContentType(.password)
                    .focused($focused)
                    .onSubmit(submit)
                    .disabled(flow.busy)
                    #if os(iOS)
                    .submitLabel(.go)
                    #endif
            } header: {
                Text(features: "Passwort der Datei")
            }
            if kind == .keepass {
                Section {
                    if let keyFile {
                        HStack {
                            Label { Text(verbatim: keyFile.name) } icon: { UwUIcon(.file) }
                            Spacer()
                            Button(role: .destructive) { self.keyFile = nil } label: {
                                Text(features: "Entfernen")
                            }
                            .accessibilityLabel(Text(features: "Schlüsseldatei entfernen"))
                            .help(Text(features: "Schlüsseldatei entfernen"))
                        }
                    }
                    Button {
                        chooseKeyFile { url in readKeyFile(url) }
                    } label: {
                        if keyFile == nil {
                            Label(features: "Schlüsseldatei wählen …", uwuIcon: .securityKey)
                        } else {
                            Label(features: "Andere wählen …", uwuIcon: .securityKey)
                        }
                    }
                    .disabled(flow.busy)
                } header: {
                    Text(features: "Schlüsseldatei (wenn die Datei eine hat)")
                }
            }
            if let error = flow.error {
                ImportErrorSection(text: error)
            }
            Section {
                HStack(spacing: UwUSpacing.sm) {
                    Button { flow.reset() } label: {
                        Text(features: "Zurück").frame(maxWidth: .infinity)
                    }
                    .uwuGlassButton()
                    .disabled(flow.busy)
                    Button(action: submit) {
                        HStack {
                            if flow.busy {
                                ProgressView().controlSize(.small)
                                Text(features: "Öffnet …")
                            } else {
                                Text(features: "Öffnen")
                            }
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .uwuProminentButton()
                    .foregroundStyle(Color.uwuOnPink)
                    .disabled(flow.busy || (password.isEmpty && keyFile == nil))
                    .keyboardShortcut(.defaultAction)
                }
                .controlSize(.large)
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets())
        }
        .onAppear { focused = true }
    }

    private func submit() {
        guard !flow.busy, !(password.isEmpty && keyFile == nil) else { return }
        open(password, keyFile?.bytes)
    }

    private func readKeyFile(_ url: URL) {
        let access = url.startAccessingSecurityScopedResource()
        defer { if access { url.stopAccessingSecurityScopedResource() } }
        // A key file is small; a big one is not a key file.
        if let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize, size > ImportLimits.maxFileBytes {
            flow.error = ImportLimits.tooLarge().message
            return
        }
        guard let data = try? Data(contentsOf: url) else {
            flow.error = featureString("Die Schlüsseldatei ist beschädigt.")
            return
        }
        keyFile = (url.lastPathComponent, [UInt8](data))
    }
}

// MARK: - Preview

private struct ImportPreviewStep: View {
    let parsed: ImportParsed
    let error: String?

    #if os(macOS)
    private let listLimit = 500
    #else
    private let listLimit = 100
    #endif

    var body: some View {
        let summary = ImportSummary(parsed)
        let total = summary.items.count
        Form {
            Section {
                ForEach(counts(summary), id: \.label) { count in
                    LabeledContent {
                        Text(verbatim: count.value.formatted())
                    } label: {
                        Label { Text(verbatim: count.label) } icon: { UwUIcon(count.icon) }
                    }
                }
                if summary.passkeys > 0 {
                    LabeledContent {
                        Text(verbatim: summary.passkeys.formatted())
                    } label: {
                        Label(features: "Passkeys", uwuIcon: .passkey)
                    }
                }
                if !summary.folders.isEmpty {
                    LabeledContent {
                        Text(verbatim: summary.folders.count.formatted())
                    } label: {
                        Label(features: "Ordner", uwuIcon: .folder)
                    }
                    let names = summary.folders.joined(separator: ", ")
                    Text(features: "Ordner: \(names)")
                        .uwuFont(.footnote)
                        .foregroundStyle(Color.uwuMuted)
                }
                if total == 0 {
                    Text(features: "In der Datei sind keine Einträge.")
                        .foregroundStyle(Color.uwuMuted)
                }
            } header: {
                let app = parsed.source.label
                let format = parsed.format
                Text(features: "\(app), \(format): \(total) Einträge.")
            } footer: {
                Text(features: "Noch ist nichts im Tresor; das passiert erst mit „Importieren“.")
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel(Text(features: "Einträge nach Art"))

            if !parsed.warnings.isEmpty {
                Section {
                    ForEach(Array(parsed.warnings.enumerated()), id: \.offset) { _, warning in
                        Label { Text(verbatim: warning) } icon: {
                            UwUIcon(.warning).foregroundStyle(Color.uwuWarningInk)
                        }
                        .uwuFont(.callout)
                    }
                } header: {
                    Text(features: "Hinweise")
                }
            }
            if let error {
                ImportErrorSection(text: error)
            }
            if total > 0 {
                Section {
                    ForEach(summary.items.prefix(listLimit)) { item in
                        ImportPreviewRow(item: item)
                    }
                    if total > listLimit {
                        let more = total - listLimit
                        Text(features: "… und \(more) weitere")
                            .foregroundStyle(Color.uwuMuted)
                    }
                } header: {
                    Text(features: "Einträge")
                }
            }
        }
    }

    private struct Count {
        let label: String
        let value: Int
        let icon: UwUIconName
    }

    /// What the preview counts, by kind; kinds with none left out (ImportDialog.tsx importCounts).
    private func counts(_ summary: ImportSummary) -> [Count] {
        [
            Count(label: featureString("Logins"), value: summary.logins, icon: .website),
            Count(label: featureString("Notizen"), value: summary.notes, icon: .note),
            Count(label: featureString("Karten"), value: summary.cards, icon: .card),
            Count(label: featureString("Identitäten"), value: summary.identities, icon: .identity),
            Count(label: featureString("SSH-Schlüssel"), value: summary.sshKeys, icon: .sshKey),
            Count(label: featureString("WLANs"), value: summary.wifi, icon: .wifi),
        ].filter { $0.value > 0 }
    }
}

private struct ImportPreviewRow: View {
    let item: ImportPreviewItem

    var body: some View {
        HStack(spacing: UwUSpacing.sm) {
            UwUIcon(icon)
                .foregroundStyle(Color.uwuMuted)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: item.name)
                    .lineLimit(1)
                Text(verbatim: detail)
                    .uwuFont(.footnote)
                    .foregroundStyle(Color.uwuMuted)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if item.totp {
                Text(verbatim: "TOTP")
                    .uwuFont(.caption2, weight: .semibold)
                    .padding(.horizontal, UwUSpacing.xs)
                    .padding(.vertical, 2)
                    .uwuGlass()
                    .help(Text(features: "Mit Einmalcodes (TOTP)"))
                    .accessibilityLabel(Text(features: "Mit Einmalcodes (TOTP)"))
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var icon: UwUIconName {
        if item.wifi { return .wifi }
        switch item.type {
        case .login: return .website
        case .note: return .note
        case .card: return .card
        case .identity: return .identity
        case .sshKey: return .sshKey
        }
    }

    private var detail: String {
        let type: String
        if item.wifi {
            type = featureString("WLAN")
        } else {
            switch item.type {
            case .login: type = featureString("Login")
            case .note: type = featureString("Notiz")
            case .card: type = featureString("Karte")
            case .identity: type = featureString("Identität")
            case .sshKey: type = featureString("Schlüssel (SSH)")
            }
        }
        return [type, item.detail, item.folder ?? featureString("Ohne Ordner")]
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }
}

// MARK: - Running

private struct ImportRunningStep: View {
    let progress: ImportProgress?

    var body: some View {
        VStack(spacing: UwUSpacing.lg) {
            NyuView(mood: .sparkle)
                .frame(width: 96, height: 96)
                .accessibilityHidden(true)
            Group {
                if let progress, progress.total > 0 {
                    ProgressView(value: Double(progress.done), total: Double(progress.total)) {
                        let done = progress.done
                        let total = progress.total
                        Text(features: "\(done) von \(total) Einträgen im Tresor")
                    }
                } else {
                    ProgressView {
                        Text(features: "Verschlüsselt die Einträge …")
                    }
                }
            }
            .uwuFont(.callout)
            Text(features: "Jeder Eintrag wird hier auf diesem Gerät verschlüsselt, bevor er zum Server geht. Lass UwULock so lange offen.")
                .uwuFont(.footnote)
                .foregroundStyle(Color.uwuMuted)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(UwUSpacing.lg)
        .frame(maxWidth: 440)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .combine)
    }
}

// MARK: - Done

private struct ImportDoneStep: View {
    let outcome: ImportOutcome

    var body: some View {
        Form {
            Section {
                VStack(spacing: UwUSpacing.sm) {
                    NyuView(mood: outcome.error == nil ? .cheer : .sad)
                        .frame(width: 88, height: 88)
                        .accessibilityHidden(true)
                    Text(verbatim: ImportFlowModel.outcomeText(outcome))
                        .uwuFont(.title3, weight: .semibold)
                        .multilineTextAlignment(.center)
                    if outcome.foldersCreated > 0 {
                        Group {
                            if outcome.foldersCreated == 1 {
                                Text(features: "Ein neuer Ordner angelegt.")
                            } else {
                                let n = outcome.foldersCreated
                                Text(features: "\(n) neue Ordner angelegt.")
                            }
                        }
                        .uwuFont(.callout)
                        .foregroundStyle(Color.uwuMuted)
                    }
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, UwUSpacing.sm)
                .accessibilityElement(children: .combine)
            }
            if outcome.error != nil {
                ImportErrorSection(text: ImportFlowModel.partialText(outcome))
            }
            if !outcome.skipped.isEmpty {
                Section {
                    ForEach(Array(outcome.skipped.prefix(50).enumerated()), id: \.offset) { _, item in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(verbatim: item.name).uwuFont(.body, weight: .semibold)
                            Text(verbatim: ImportFlowModel.skippedText(item.reason))
                                .uwuFont(.footnote)
                                .foregroundStyle(Color.uwuMuted)
                        }
                        .accessibilityElement(children: .combine)
                    }
                } header: {
                    if outcome.skipped.count == 1 {
                        Text(features: "Ein Eintrag ist draußen geblieben:")
                    } else {
                        let n = outcome.skipped.count
                        Text(features: "\(n) Einträge sind draußen geblieben:")
                    }
                }
            }
        }
    }
}

// MARK: - Pieces

private struct ImportErrorSection: View {
    let text: String

    var body: some View {
        Section {
            Label { Text(verbatim: text) } icon: { UwUIcon(.error) }
                .foregroundStyle(Color.uwuDangerInk)
                .uwuFont(.callout)
                .accessibilityAddTraits(.isStaticText)
        }
    }
}

#if DEBUG
#Preview("Datei wählen") {
    ImportFlowView(step: .pick).environment(AppModel())
}

#Preview("Passwort") {
    ImportFlowView(step: .password(file: ImportFixtures.file, kind: .keepass)).environment(AppModel())
}

#Preview("Vorschau") {
    ImportFlowView(step: .preview(ImportFixtures.parsed)).environment(AppModel())
}

#Preview("Läuft") {
    ImportFlowView(step: .running(parsed: ImportFixtures.parsed, progress: ImportProgress(done: 7, total: 12)))
        .environment(AppModel())
}

#Preview("Fertig") {
    ImportFlowView(step: .done(ImportFixtures.outcome)).environment(AppModel())
}

#Preview("Teilweise") {
    ImportFlowView(step: .done(ImportFixtures.partial)).environment(AppModel())
}
#endif
