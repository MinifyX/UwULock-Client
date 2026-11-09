import Foundation

/// Which password a file in the `password` step needs.
enum ImportPasswordKind: Sendable {
    /// A KeePass database: its master password and maybe a key file.
    case keepass
    /// Bitwarden's password-protected export: the password chosen when exporting.
    case bitwarden
}

/// Where the import stands (importFlow.ts `ImportStep`).
enum ImportStep {
    case pick
    /// The file is being read into memory.
    case reading
    case password(file: ImportFile, kind: ImportPasswordKind)
    case preview(ImportParsed)
    case running(parsed: ImportParsed, progress: ImportProgress?)
    /// Only the outcome: the file's plaintext isn't kept once it is in the vault.
    case done(ImportOutcome)

    var name: String {
        switch self {
        case .pick: "pick"
        case .reading: "reading"
        case .password: "password"
        case .preview: "preview"
        case .running: "running"
        case .done: "done"
        }
    }

    var isRunning: Bool {
        if case .running = self { return true }
        return false
    }
}

/// The import's steps (lib/importFlow.ts `useImportFlow`): pick a file, open it with its password
/// when it has one, look at what comes, import it.
@MainActor
@Observable
final class ImportFlowModel {
    /// The app the file comes from; nil: detect it.
    var source: ImportSource?
    private(set) var step: ImportStep
    var error: String?
    private(set) var busy = false
    /// A cancel while a file is being read: what it reads is dropped.
    private var generation = 0

    init(step: ImportStep = .pick) {
        self.step = step
    }

    func reset() {
        generation += 1
        step = .pick
        error = nil
        busy = false
    }

    /// A file the user picked (`.fileImporter`, security-scoped).
    func pick(_ url: URL) async {
        generation += 1
        let run = generation
        error = nil
        let access = url.startAccessingSecurityScopedResource()
        defer { if access { url.stopAccessingSecurityScopedResource() } }
        do {
            // Before the file is read into memory at all.
            if let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize {
                try ImportLimits.checkFileSize(size)
            }
            step = .reading
            let source = source
            let (file, kind) = try await Task.detached(priority: .userInitiated) {
                () async throws -> (ImportFile, ImportPasswordKind?) in
                let bytes: [UInt8]
                do {
                    bytes = [UInt8](try Data(contentsOf: url, options: .mappedIfSafe))
                } catch {
                    throw ImportProblem(featureString(
                        "Die Datei ließ sich nicht lesen: Sie ist beschädigt oder anders aufgebaut als erwartet."))
                }
                try ImportLimits.checkFileSize(bytes.count)
                let file = ImportFile(name: url.lastPathComponent, bytes: bytes)
                if ImportReading.needsPassword(bytes) && (source == nil || source == .keepass) {
                    return (file, .keepass)
                }
                if ImportReading.isProtectedExport(bytes) && (source == nil || source == .bitwarden) {
                    return (file, .bitwarden)
                }
                return (file, nil)
            }.value
            guard run == generation else { return }
            if let kind {
                step = .password(file: file, kind: kind)
                return
            }
            let parsed = try await Self.read(file, source: source)
            if run == generation { step = .preview(parsed) }
        } catch {
            guard run == generation else { return }
            self.error = Self.errorText(error)
            step = .pick
        }
    }

    /// The password (and, for KeePass, the key file) of the file in the `password` step.
    func unlock(password: String, keyFile: [UInt8]?, core: Core) async {
        guard case .password(let file, let kind) = step, !busy else { return }
        let run = generation
        busy = true
        error = nil
        do {
            let parsed: ImportParsed
            if FeatureFixtures.isOn {
                #if DEBUG
                try await Task.sleep(for: .milliseconds(600))
                parsed = ImportFixtures.parsed
                #else
                return
                #endif
            } else if kind == .keepass {
                parsed = try await Self.read(
                    file, source: .keepass, credentials: ImportCredentials(password: password, keyFile: keyFile),
                    kdf: ImportKdf.core(core))
            } else {
                let text = try await core.importOpenBitwarden(text: ImportBytes.text(file.bytes), password: password)
                parsed = try await Self.read(ImportFile(name: file.name, bytes: Array(text.utf8)), source: .bitwarden)
            }
            if run == generation { step = .preview(parsed) }
        } catch {
            if run == generation { self.error = Self.errorText(error) }
        }
        if run == generation { busy = false }
    }

    /// Into the vault on screen, with the core's `import-progress` on the way.
    func start(core: Core) async {
        guard case .preview(let parsed) = step else { return }
        error = nil
        step = .running(parsed: parsed, progress: nil)
        #if DEBUG
        if FeatureFixtures.isOn {
            let total = parsed.data.items.count
            for done in stride(from: 0, through: total, by: max(1, total / 8)) {
                try? await Task.sleep(for: .milliseconds(250))
                progressed(ImportProgress(done: done, total: total))
            }
            step = .done(ImportFixtures.outcome)
            return
        }
        #endif
        let events = core.events()
        let follow = Task { [weak self] in
            for await event in events {
                if case .importProgress(let progress) = event { self?.progressed(progress) }
            }
        }
        defer { follow.cancel() }
        do {
            let outcome = try await core.importVault(format: parsed.submitFormat, text: parsed.submitText)
            step = .done(outcome)
        } catch {
            // Nothing came in: back to the preview, with the reason.
            self.error = Self.errorText(error)
            step = .preview(parsed)
        }
    }

    // MARK: Credential Exchange (CredentialExchangeCard.tsx)

    /// Credentials Apple Passwords handed over (iOS), waiting to be looked at.
    private(set) var exchangePending = false

    func checkExchange(core: Core) async {
        guard !FeatureFixtures.isOn else { return }
        #if os(iOS)
        exchangePending = (try? await core.credentialExchangePending()) ?? false
        #endif
    }

    /// "Ansehen" takes them from the system (only now, only once) and shows them in the preview;
    /// "Verwerfen" drops them.
    func takeExchange(core: Core, discard: Bool) async {
        guard !busy else { return }
        generation += 1
        let run = generation
        busy = true
        error = nil
        do {
            let json = try await core.credentialExchangeImport(discard: discard)
            exchangePending = false
            if !discard {
                let parsed = try await Task.detached(priority: .userInitiated) { try ImportCxf.read(json) }.value
                if run == generation { step = .preview(parsed) }
            }
        } catch {
            exchangePending = false
            if run == generation { self.error = Self.errorText(error) }
        }
        if run == generation { busy = false }
    }

    private func progressed(_ progress: ImportProgress) {
        if case .running(let parsed, _) = step { step = .running(parsed: parsed, progress: progress) }
    }

    /// Reads off the main actor: a big file takes a while.
    private static func read(_ file: ImportFile, source: ImportSource?, credentials: ImportCredentials? = nil,
                             kdf: ImportKdf? = nil) async throws -> ImportParsed {
        try await Task.detached(priority: .userInitiated) {
            try await ImportReading.read(file, source: source, credentials: credentials, kdf: kdf)
        }.value
    }

    // MARK: Words (lib/import/run.ts)

    /// An import's error in words: the reader's own, or one from the core.
    nonisolated static func errorText(_ error: any Error) -> String {
        if let problem = error as? ImportProblem { return problem.message }
        let failure = (error as? CoreFailure) ?? Core.failure(error)
        switch failure.kind {
        case "import-password":
            return featureString("Das Passwort der Datei stimmt nicht.")
        case "import-account-bound":
            return featureString(
                "Dieser Export ist mit dem Konto verschlüsselt, das ihn erstellt hat. Exportiere noch einmal als JSON – unverschlüsselt oder „Passwortgeschützt“ – und importiere diese Datei.")
        case "import-unsupported":
            let what = failure.message
            return featureString("Diese Datei verlangt etwas, das UwULock nicht kann: \(what)")
        case "import-file":
            return featureString(
                "Die Datei ließ sich nicht lesen: Sie ist beschädigt oder anders aufgebaut als erwartet.")
        default:
            return failure.message
        }
    }

    /// Why an item stayed out of the vault.
    nonisolated static func skippedText(_ reason: SkippedReason) -> String {
        switch reason {
        case .tooLong: featureString("Ein Wert (meist die Notiz) ist länger, als der Server annimmt.")
        case .invalid: featureString("Der Eintrag ist unvollständig (etwa ein SSH-Schlüssel ohne alle Teile).")
        }
    }

    /// The line under a finished import.
    nonisolated static func outcomeText(_ outcome: ImportOutcome) -> String {
        let n = outcome.imported
        if outcome.error != nil {
            return n == 1
                ? featureString("Ein Eintrag importiert, dann ist der Import stehen geblieben.")
                : featureString("\(n) Einträge importiert, dann ist der Import stehen geblieben.")
        }
        return n == 1 ? featureString("Ein Eintrag importiert ✧") : featureString("\(n) Einträge importiert ✧")
    }

    /// Below a partial import: what stopped it, and what that means.
    nonisolated static func partialText(_ outcome: ImportOutcome) -> String {
        let reason = outcome.error.map { errorText($0) } ?? ""
        return featureString(
            "\(reason) Was schon im Tresor ist, bleibt dort; importiere den Rest nicht doppelt.")
    }
}
