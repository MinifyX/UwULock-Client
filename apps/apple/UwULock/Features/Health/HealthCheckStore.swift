// The password check's state (HealthPane.tsx, mobile/pages/Check.tsx `useHealth`): one answer
// of `health_report` for the report and the review, the progress while the sources are asked,
// and the commands that change it.
import SwiftUI
import UwUDesign

@MainActor
@Observable
final class HealthCheckStore {
    private(set) var view: HealthView?
    /// What the check is doing right now ("Prüft …", the progress), or nil.
    private(set) var busy: String?
    /// The progress per source while the sources are asked.
    private(set) var progress: CheckProgress?
    /// Why the last load failed (the core's words).
    private(set) var error: String?
    /// The addresses' answers, once checked.
    private(set) var emailResults: [EmailResult]?
    private(set) var emailBusy = false
    /// Logins that got a new password in this session (review: their password problems are solved).
    private(set) var renewed: Set<String> = []
    /// Ticks when a fresh check finished (Nyu cheers, a haptic).
    private(set) var checked = 0

    @ObservationIgnored private var core: Core?
    @ObservationIgnored private var listening: Task<Void, Never>?
    @ObservationIgnored private var quietRunning = false

    /// Starts with `core`: the first load (without asking the sources) and the events.
    func attach(_ core: Core) async {
        guard self.core !== core else { return }
        self.core = core
        #if DEBUG
        if FeatureFixtures.isOn {
            view = HealthFixtures.view
            return
        }
        #endif
        listening?.cancel()
        listening = Task { [weak self] in
            for await event in core.events() {
                guard let self else { return }
                switch event {
                case .healthProgress(let progress):
                    if self.busy != nil {
                        self.progress = progress
                        self.busy = HealthWords.progress(progress)
                    }
                case .vaultChanged:
                    // A sync or a change elsewhere: the report follows, without asking the sources.
                    await self.refreshQuietly()
                case .vaultStatus(let status):
                    if status.state != .unlocked {
                        self.view = nil
                        self.emailResults = nil
                        self.renewed = []
                    } else {
                        await self.refreshQuietly()
                    }
                default:
                    break
                }
            }
        }
        await load(fresh: false)
    }

    #if DEBUG
    /// A store with the samples, for previews.
    static func preview() -> HealthCheckStore {
        let store = HealthCheckStore()
        store.view = HealthFixtures.view
        return store
    }
    #endif

    /// Stops following the core's events (the screen went away).
    func detach() {
        listening?.cancel()
        listening = nil
        core = nil
    }

    /// `fresh`: ask the breach sources again; else their last answers count.
    func load(fresh: Bool) async {
        error = nil
        progress = nil
        busy = fresh ? featureString("Prüft …") : featureString("Lädt …")
        defer {
            busy = nil
            progress = nil
        }
        #if DEBUG
        if FeatureFixtures.isOn {
            try? await Task.sleep(for: .milliseconds(fresh ? 600 : 0))
            view = view ?? HealthFixtures.view
            if fresh {
                view?.checkedAt = FeatureDate.iso(.now)
                checked += 1
            }
            return
        }
        #endif
        guard let core else { return }
        do {
            view = try await core.healthReport(fresh: fresh)
            if fresh { checked += 1 }
        } catch {
            self.error = Core.failure(error).message
        }
    }

    /// The report again without asking the sources; failures wait for the next change.
    func refreshQuietly() async {
        guard !quietRunning, !FeatureFixtures.isOn, let core else { return }
        quietRunning = true
        defer { quietRunning = false }
        if let fresh = try? await core.healthReport(fresh: false) {
            view = fresh
        }
    }

    /// Ignores a problem of a login everywhere, or stops ignoring it.
    func setIgnored(_ itemId: String, _ kind: ProblemKind, _ ignored: Bool) async {
        #if DEBUG
        if FeatureFixtures.isOn {
            var list = view?.ignored ?? []
            list.removeAll { $0.itemId == itemId && $0.kind == kind }
            if ignored { list.append(IgnoredEntry(itemId: itemId, kind: kind, since: FeatureDate.iso(.now))) }
            view?.ignored = list
            return
        }
        #endif
        guard let core else { return }
        do {
            let list = try await core.setIgnored(itemId: itemId, kind: kind, ignored: ignored)
            view?.ignored = list
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }

    /// Opens the change-password page (or the login) in the browser.
    func openChangePage(_ itemId: String) async {
        guard !FeatureFixtures.isOn, let core else { return }
        do {
            try await core.openChangePage(itemId: itemId)
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }

    /// Saves a new password to the login; the old one goes into its history.
    func saveNewPassword(_ itemId: String, _ password: String) async {
        if !FeatureFixtures.isOn {
            guard let core else { return }
            do {
                try await core.saveNewPassword(itemId: itemId, password: password)
            } catch {
                FeatureToasts.shared.show(error: error)
                return
            }
        }
        renewed.insert(itemId)
        FeatureToasts.shared.show(featureString("Neues Passwort gespeichert; das alte steht im Verlauf des Eintrags."))
        await refreshQuietly()
    }

    /// A password from the generator (the sheet of the review).
    func generate(_ options: GeneratorOptions) async -> Generated? {
        #if DEBUG
        if FeatureFixtures.isOn {
            let letters = Array("abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%&*")
            let password = String((0..<options.length).map { _ in letters.randomElement()! })
            return Generated(password: password, bits: options.length * 6, length: options.length, required: 4)
        }
        #endif
        guard let core else { return nil }
        do {
            return try await core.generatePassword(options: options)
        } catch {
            FeatureToasts.shared.show(error: error)
            return nil
        }
    }

    /// Copies a generated password (the core clears the clipboard later).
    func copyGenerated(_ password: String) async {
        guard !FeatureFixtures.isOn, let core else { return }
        do {
            try await core.copyGenerated(text: password)
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }

    // MARK: Addresses

    /// The consent may have changed elsewhere (Settings → Konto).
    func refreshEmailOptIn() async {
        guard !FeatureFixtures.isOn, let core, view?.switches.emailCheck == true else { return }
        if let optIn = try? await core.emailOptIn() {
            view?.emailOptIn = optIn
        }
    }

    /// Gives or withdraws the consent to the check of addresses.
    func setEmailOptIn(_ optedIn: Bool) async {
        #if DEBUG
        if FeatureFixtures.isOn {
            view?.emailOptIn = EmailOptIn(optedIn: optedIn, since: optedIn ? FeatureDate.iso(.now) : nil)
            if !optedIn { emailResults = nil }
            return
        }
        #endif
        guard let core else { return }
        do {
            view?.emailOptIn = try await core.setEmailOptIn(optedIn: optedIn)
            if !optedIn { emailResults = nil }
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }

    /// Asks about the account's addresses.
    func checkEmails() async {
        emailBusy = true
        defer { emailBusy = false }
        #if DEBUG
        if FeatureFixtures.isOn {
            try? await Task.sleep(for: .milliseconds(500))
            emailResults = HealthFixtures.emails
            return
        }
        #endif
        guard let core else { return }
        do {
            emailResults = try await core.checkEmails().results
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }
}

/// Logins put off with "Später" stay put off while the app runs (HealthPane.tsx `LATER_KEY`).
@MainActor
enum HealthLater {
    static var ids: Set<String> = []
}

extension EnvironmentValues {
    /// Opens an item of the vault (the shell's detail); nil hides "Eintrag öffnen"/"Öffnen".
    @Entry var passwordCheckOpenItem: PasswordCheckOpenItem? = nil
}

/// How the shell opens a vault item from the password check:
/// `.environment(\.passwordCheckOpenItem, PasswordCheckOpenItem { id in … })`.
struct PasswordCheckOpenItem {
    let open: @MainActor (String) -> Void
    init(_ open: @escaping @MainActor (String) -> Void) { self.open = open }
    @MainActor func callAsFunction(_ id: String) { open(id) }
}
