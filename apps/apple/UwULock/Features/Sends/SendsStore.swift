// The account's Sends as the screen holds them, and the calls behind them (lib/sends.ts):
// list, save, open to anybody again, delete, copy a link. With fixtures on, the samples.
import Foundation
import SwiftUI

@MainActor
@Observable
final class SendsStore {
    /// Newest change first; nil until the first answer.
    private(set) var sends: [Send]?
    /// What the last load said.
    private(set) var error: String?

    @ObservationIgnored private var loading = false

    /// Loads the Sends again (after a change here or a sync).
    func load(_ core: Core) async {
        #if DEBUG
        if FeatureFixtures.isOn {
            if sends == nil { sends = sendsSorted(SendsFixtures.sends) }
            return
        }
        #endif
        guard !loading else { return }
        loading = true
        defer { loading = false }
        do {
            sends = sendsSorted(try await core.sends())
            error = nil
        } catch {
            self.error = Core.failure(error).message
            if sends == nil { sends = [] }
        }
    }

    func send(_ id: String?) -> Send? {
        guard let id else { return nil }
        return sends?.first { $0.id == id }
    }

    /// Saves a Send (a new one without `id`), with the bytes of a new file Send; answers its id.
    func save(
        _ core: Core, id: String?, draft: SendDraft, file: Data?, domain: SendLinkDomain
    ) async throws -> String {
        #if DEBUG
        if FeatureFixtures.isOn {
            return SendsFixtures.save(into: &sends, id: id, draft: draft, size: file?.count, domain: domain)
        }
        #endif
        if draft.kind == .file && id == nil {
            try await core.stageSendFile(file)
        }
        let saved = try await core.saveSend(id: id, draft: draft, domain: domain)
        await load(core)
        return saved
    }

    /// Anybody with the link may open it again: no password, no addresses.
    func removeAuth(_ core: Core, _ send: Send) async {
        let done = send.authType == .emails
            ? featureString("Adressen entfernt.") : featureString("Passwort entfernt.")
        await act(core, done: done) {
            #if DEBUG
            if FeatureFixtures.isOn {
                self.mutate(send.id) {
                    $0.authType = .anyone
                    $0.hasPassword = false
                    $0.emails = []
                }
                return
            }
            #endif
            try await core.removeSendAuth(id: send.id)
        }
    }

    /// Deletes a Send; true when it is gone.
    @discardableResult
    func delete(_ core: Core, _ send: Send) async -> Bool {
        await act(core, done: featureString("Gelöscht.")) {
            #if DEBUG
            if FeatureFixtures.isOn {
                self.sends?.removeAll { $0.id == send.id }
                return
            }
            #endif
            try await core.deleteSend(id: send.id)
        }
    }

    @discardableResult
    private func act(_ core: Core, done: String, _ what: () async throws -> Void) async -> Bool {
        do {
            try await what()
            FeatureToasts.shared.show(done)
            await load(core)
            return true
        } catch {
            FeatureToasts.shared.show(error: error)
            return false
        }
    }

    #if DEBUG
    private func mutate(_ id: String, _ change: (inout Send) -> Void) {
        guard let index = sends?.firstIndex(where: { $0.id == id }) else { return }
        change(&sends![index])
    }
    #endif
}

enum SendsLink {
    /// Copies a link through the core, which clears the clipboard later — the link opens the
    /// Send (its key is in it).
    @MainActor
    static func copy(_ core: Core, _ link: String) async {
        if !FeatureFixtures.isOn {
            do {
                try await core.copyGenerated(text: link)
            } catch {
                FeatureToasts.shared.show(error: error)
                return
            }
        }
        FeatureToasts.shared.show(copiedLinkText())
    }

    /// Copies a new Send's link without a toast; true when it worked.
    @MainActor
    static func copyQuietly(_ core: Core, _ link: String) async -> Bool {
        if FeatureFixtures.isOn { return true }
        return (try? await core.copyGenerated(text: link)) != nil
    }
}
