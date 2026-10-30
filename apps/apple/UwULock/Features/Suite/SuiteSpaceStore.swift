// One suite space on screen (SuitePane.tsx `load`, `took`, `save`): the records, loading
// again when UwUSSH or UwURDP changed something, and the commands that change them.
import SwiftUI
import UwUDesign

@MainActor
@Observable
final class SuiteSpaceStore {
    let space: SuiteSpace
    private(set) var view: SuiteSpaceView?
    /// Why the last load failed (the core's words), or nil.
    private(set) var loadError: String?
    private(set) var loading = false
    private(set) var creating = false

    @ObservationIgnored private var core: Core?
    #if DEBUG
    /// The secrets of the samples (fixture mode).
    @ObservationIgnored private var fixtureSecrets: [String: String] = [:]
    #endif

    init(space: SuiteSpace) {
        self.space = space
    }

    var records: [SuiteRecord] { view?.records ?? [] }

    var index: [String: SuiteRecord] {
        Dictionary(records.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// Starts with `core`: the first load.
    func attach(_ core: Core) async {
        guard self.core !== core else { return }
        self.core = core
        #if DEBUG
        if FeatureFixtures.isOn {
            view = SuiteFixtures.view(space)
            fixtureSecrets = SuiteFixtures.secrets
            return
        }
        #endif
        await load()
    }

    /// Asks the core for the space again.
    func load() async {
        #if DEBUG
        if FeatureFixtures.isOn { return }
        #endif
        guard let core else { return }
        loading = true
        defer { loading = false }
        do {
            view = try await core.suiteView(space: space)
            loadError = nil
        } catch {
            loadError = suiteFailureText(Core.failure(error))
        }
    }

    /// Makes the space no app has made yet.
    func create() async {
        #if DEBUG
        if FeatureFixtures.isOn {
            view = SuiteSpaceView(space: space, exists: true, records: [])
            return
        }
        #endif
        guard let core else { return }
        creating = true
        defer { creating = false }
        do {
            view = try await core.suiteCreate(space: space)
        } catch {
            FeatureToasts.shared.show(suiteFailureText(Core.failure(error)), danger: true)
        }
    }

    /// Takes a save's answer; false (with a note) when the server had newer copies.
    @discardableResult
    func took(_ saved: SuiteSaved) -> Bool {
        view = saved.view
        if !saved.conflicts.isEmpty {
            FeatureToasts.shared.show(
                suiteFailureText(CoreFailure(kind: "suite-conflict", message: "")), danger: true)
            return false
        }
        return true
    }

    /// Applies `ops`; `done` is the note on success.
    func save(_ ops: [SuiteOp], done: String?) async -> Bool {
        guard !ops.isEmpty else { return true }
        #if DEBUG
        if FeatureFixtures.isOn {
            applyFixture(ops)
            if let done { FeatureToasts.shared.show(done) }
            return true
        }
        #endif
        guard let core else { return false }
        do {
            let saved = try await core.suiteSave(space: space, ops: ops)
            guard took(saved) else { return false }
            if let done { FeatureToasts.shared.show(done) }
            return true
        } catch {
            let failure = Core.failure(error)
            FeatureToasts.shared.show(suiteFailureText(failure), danger: true)
            if failure.kind == "suite-conflict" || failure.kind == "space-changed" {
                await load()
            }
            return false
        }
    }

    /// A secret's text, or nil (with a note) when it didn't open.
    func reveal(_ id: String) async -> String? {
        #if DEBUG
        if FeatureFixtures.isOn { return fixtureSecrets[id] ?? "" }
        #endif
        guard let core else { return nil }
        do {
            return try await core.suiteReveal(space: space, id: id)
        } catch {
            FeatureToasts.shared.show(suiteFailureText(Core.failure(error)), danger: true)
            return nil
        }
    }

    /// Copies a secret through the core (the clipboard clears itself later).
    func copySecret(_ id: String, password: Bool) async {
        #if DEBUG
        if FeatureFixtures.isOn {
            FeatureToasts.shared.show(password ? copiedText("password") : copiedText("text"))
            return
        }
        #endif
        guard let core else { return }
        do {
            try await core.suiteCopy(space: space, id: id)
            FeatureToasts.shared.show(password ? copiedText("password") : copiedText("text"))
        } catch {
            FeatureToasts.shared.show(suiteFailureText(Core.failure(error)), danger: true)
        }
    }

    /// Copies plain text (a command, an address, a public key).
    func copyText(_ text: String) async {
        #if DEBUG
        if FeatureFixtures.isOn {
            FeatureToasts.shared.show(copiedText("text"))
            return
        }
        #endif
        guard let core else { return }
        do {
            try await core.copyGenerated(text: text)
            FeatureToasts.shared.show(copiedText("text"))
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }

    /// A fresh password with the generator's defaults.
    func generatePassword() async -> String? {
        #if DEBUG
        if FeatureFixtures.isOn { return "fixture-Pa55word-not-real" }
        #endif
        guard let core else { return nil }
        do {
            return try await core.generatePassword(options: GeneratorOptions()).password
        } catch {
            FeatureToasts.shared.show(error: error)
            return nil
        }
    }

    /// Makes an Ed25519 key; throws the core's failure.
    func generateKey(label: String, comment: String, passphrase: String?) async throws -> SuiteSaved {
        #if DEBUG
        if FeatureFixtures.isOn { return fixtureKey(label: label, type: "ssh-ed25519") }
        #endif
        guard let core else { throw CoreFailure(kind: "unavailable", message: "") }
        return try await core.suiteGenerateKey(
            space: space, label: label, comment: comment, passphrase: passphrase)
    }

    /// Imports a private key; throws the core's failure.
    func importKey(label: String, privateKey: String, passphrase: String?) async throws -> SuiteSaved {
        #if DEBUG
        if FeatureFixtures.isOn { return fixtureKey(label: label, type: "ssh-ed25519") }
        #endif
        guard let core else { throw CoreFailure(kind: "unavailable", message: "") }
        return try await core.suiteImportKey(
            space: space, label: label, privateKey: privateKey, passphrase: passphrase)
    }

    /// Saves a key's half as a file; the path, or nil (with a note).
    func saveKey(_ id: String, half: SuiteKeyHalf) async -> String? {
        await savedPath { try await $0.suiteSaveKey(space: self.space, id: id, half: half) }
    }

    /// Saves a host as an .rdp file; the path, or nil (with a note).
    func saveRdp(_ id: String) async -> String? {
        await savedPath { try await $0.suiteSaveRdp(id: id) }
    }

    private func savedPath(_ command: (Core) async throws -> String) async -> String? {
        #if DEBUG
        if FeatureFixtures.isOn { return nil }
        #endif
        guard let core else { return nil }
        do {
            let path = try await command(core)
            FeatureToasts.shared.show(featureString("Gespeichert unter \(path) ✧"))
            return path
        } catch {
            FeatureToasts.shared.show(suiteFailureText(Core.failure(error)), danger: true)
            return nil
        }
    }

    /// Opens the host in UwUSSH or UwURDP (computers).
    func openInApp(_ id: String) async {
        #if DEBUG
        if FeatureFixtures.isOn { return }
        #endif
        guard let core else { return }
        do {
            try await core.suiteOpenInApp(space: space, id: id)
        } catch {
            FeatureToasts.shared.show(suiteFailureText(Core.failure(error)), danger: true)
        }
    }

    // MARK: Fixtures

    #if DEBUG
    /// A store with the samples, for previews.
    static func preview(_ space: SuiteSpace) -> SuiteSpaceStore {
        let store = SuiteSpaceStore(space: space)
        store.view = SuiteFixtures.view(space)
        store.fixtureSecrets = SuiteFixtures.secrets
        return store
    }

    private func applyFixture(_ ops: [SuiteOp]) {
        var list = records
        for op in ops {
            switch op {
            case .put(let id, let kind, _, let patch):
                if let at = list.firstIndex(where: { $0.id == id }) {
                    var data = list[at].data ?? [:]
                    for (key, value) in patch { data[key] = value }
                    list[at].data = data
                    list[at].seq += 1
                } else {
                    list.append(
                        SuiteRecord(
                            id: id, kind: kind, seq: 1, updatedMs: 0, data: patch, broken: false,
                            usedBy: nil))
                }
            case .secret(let id, let text, _):
                fixtureSecrets[id] = text
                if !list.contains(where: { $0.id == id }) {
                    list.append(
                        SuiteRecord(
                            id: id, kind: "secret", seq: 1, updatedMs: 0, data: nil, broken: false,
                            usedBy: nil))
                }
            case .delete(let id, _):
                list.removeAll { $0.id == id }
            }
        }
        view = SuiteSpaceView(space: space, exists: true, records: SuiteFixtures.withUsedBy(list))
    }

    private func fixtureKey(label: String, type: String) -> SuiteSaved {
        let id = suiteNewId()
        applyFixture([
            .put(
                id: id, kind: "key", seq: nil,
                patch: [
                    "label": .string(label), "key_type": .string(type),
                    "public_key": .string("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFixtureOnly \(label)"),
                ])
        ])
        return SuiteSaved(view: view ?? SuiteSpaceView(space: space, exists: true, records: []), conflicts: [], id: id)
    }
    #endif
}
