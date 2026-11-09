import SwiftUI
import UwUDesign

// The shell's shared pieces: opening an item from any screen, the vault for the menu bar's
// commands, the cards after an unlock (AutofillCard.tsx, CredentialExchangeCard.tsx) and the
// overview's sync line (mobile/pages/Overview.tsx).

extension EnvironmentValues {
    /// Shows an item of the vault from another screen: the masked addresses' "Eintrag öffnen", a
    /// file request's take-over (VaultScreen.tsx's `onOpenItem`, `onTakenOver`). The shell sets it.
    @Entry var openVaultItem: PasswordCheckOpenItem? = nil
}

extension FocusedValues {
    /// The vault of the focused window, for the menu bar's commands (copy, generator, import).
    @Entry var vaultModel: VaultModel?
}

// MARK: Sync line

/// "Synchronisiert vor 2 Min.", or what the sync is doing (mobile Overview.tsx's `SyncLine`).
struct SyncLine: View {
    @Environment(AppModel.self) private var model
    @Environment(VaultModel.self) private var vault

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { _ in
            Text(Self.text(model: model, vault: vault))
        }
    }

    static func text(model: AppModel, vault: VaultModel) -> String {
        let status = model.status
        if vault.syncing || status?.syncing == true { return vt("Synchronisiert gerade …") }
        if status?.syncError != nil { return vt("Sync fehlgeschlagen") }
        return vt("Synchronisiert \(Ago.text(status?.lastSync))")
    }
}

// MARK: Cards after an unlock

/// What the cards after an unlock show; the view that holds it loads it (`promptCards`).
@MainActor @Observable
final class PromptCards {
    var provider: ProviderView?
    var prompt = AutofillPrompt.read()
    var exchangePending = false
    var busy = false
    var problem: String?
    var importing = false

    var showsAutofill: Bool { provider.map { prompt.shows($0) } ?? false }
    var visible: Bool { exchangePending || showsAutofill }

    func load(_ core: Core) async {
        guard core.isAvailable else { return }
        exchangePending = (try? await core.credentialExchangePending()) ?? false
        if let view = try? await core.autofillProviderStatus() {
            provider = view
            prompt = prompt.seen(view)
        }
    }
}

extension View {
    /// Loads the cards whenever the app comes to the front, and shows the import they lead to.
    func promptCards(_ cards: PromptCards) -> some View {
        modifier(PromptCardsLoader(cards: cards))
    }
}

private struct PromptCardsLoader: ViewModifier {
    let cards: PromptCards
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase

    func body(content: Content) -> some View {
        @Bindable var cards = cards
        content
            .task(id: scenePhase == .active) {
                if scenePhase == .active { await cards.load(model.core) }
            }
            .sheet(isPresented: $cards.importing, onDismiss: { Task { await cards.load(model.core) } }) {
                ImportFlowView()
            }
    }
}

/// The cards App.tsx shows while the vault is open: Apple Passwords handed credentials over
/// (CredentialExchangeCard.tsx) and "UwULock als Standard für AutoFill" (AutofillCard.tsx).
struct VaultPromptCards: View {
    let cards: PromptCards
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(spacing: UwUSpacing.sm) {
            if cards.exchangePending { exchangeCard }
            if let provider = cards.provider, cards.showsAutofill { autofillCard(provider) }
        }
    }

    private func card<Buttons: View>(
        title: String, text: String, hint: String? = nil, @ViewBuilder buttons: () -> Buttons
    ) -> some View {
        VStack(alignment: .leading, spacing: UwUSpacing.xs) {
            HStack(alignment: .top, spacing: UwUSpacing.sm) {
                NyuView(mood: .sparkle).frame(width: 40)
                VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                    Text(title).uwuFont(.subheadline, weight: .semibold)
                    Text(text)
                        .uwuFont(.footnote)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            if let hint {
                Text(hint).uwuFont(.footnote).foregroundStyle(.secondary)
            }
            if let problem = cards.problem {
                Text(problem).uwuFont(.footnote).foregroundStyle(.uwuDangerInk)
            }
            // Side by side when they fit; a narrow sidebar (iPad) stacks them instead of wrapping.
            ViewThatFits(in: .horizontal) {
                HStack {
                    Spacer()
                    buttons()
                }
                .fixedSize(horizontal: true, vertical: false)
                VStack(spacing: UwUSpacing.xs) {
                    buttons().frame(maxWidth: .infinity)
                }
            }
            .controlSize(.small)
        }
        .padding(UwUSpacing.md)
        .uwuGlass(in: RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous))
    }

    private var exchangeCard: some View {
        card(
            title: prefStr("Daten aus Apple Passwörter übernehmen?"),
            text: prefStr("Du siehst alle Einträge, bevor etwas in deinem Tresor gespeichert wird.")
        ) {
            Button(prefStr("Verwerfen"), role: .destructive) { discardExchange() }
                .uwuGlassButton()
                .disabled(cards.busy)
            Button(prefStr("Ansehen")) { cards.importing = true }
                .uwuProminentButton()
        }
    }

    private func autofillCard(_ view: ProviderView) -> some View {
        card(
            title: vt("UwULock als Standard für AutoFill"),
            text: vt("Dann schlägt dein System UwULocks Passwörter und Passkeys direkt beim Anmelden vor – in Apps und im Browser. Jedes Ausfüllen fragt nach Face ID, Touch ID oder deiner Gerätesperre."),
            hint: view.direct ? nil : vt("UwULock dort einschalten: \(AutofillSettings.settingsPath(view))")
        ) {
            Button(vt("Später")) { cards.prompt = cards.prompt.later() }
                .uwuGlassButton()
            Button(view.direct ? vt("Als Standard festlegen") : vt("Einstellungen öffnen")) { turnOn() }
                .uwuProminentButton()
                .disabled(cards.busy)
        }
    }

    private func turnOn() {
        let cards = cards
        cards.busy = true
        cards.problem = nil
        Task {
            defer { cards.busy = false }
            do {
                let after = try await model.core.autofillProviderRequest(target: .credentials)
                cards.provider = after
                // Declined in the system's sheet (or the settings opened): counts as "Später".
                cards.prompt = AutofillPrompt.missing(after) ? cards.prompt.later() : cards.prompt.seen(after)
            } catch {
                cards.problem = Core.failure(error).message
            }
        }
    }

    private func discardExchange() {
        let cards = cards
        cards.busy = true
        Task {
            defer { cards.busy = false }
            do {
                _ = try await model.core.credentialExchangeImport(discard: true)
                cards.exchangePending = false
            } catch {
                cards.problem = Core.failure(error).message
            }
        }
    }
}

/// When the AutoFill card shows (lib/autofillPrompt.ts): "Später" waits a week, at most three
/// times; once UwULock is the provider it never comes back. Per device, not secret.
struct AutofillPrompt: Codable, Equatable {
    var later = 0
    var until: Double = 0
    var done = false

    static let key = "uwulock.autofillPrompt"
    static let laterDays = 7.0
    static let maxLater = 3

    static func read() -> AutofillPrompt {
        guard let data = UserDefaults.standard.data(forKey: key),
            let state = try? JSONDecoder().decode(AutofillPrompt.self, from: data)
        else { return AutofillPrompt() }
        return state
    }

    private func saved() -> AutofillPrompt {
        if let data = try? JSONEncoder().encode(self) { UserDefaults.standard.set(data, forKey: Self.key) }
        return self
    }

    /// Something is still missing: the provider, or (iOS, macOS) UwULock's own list for it.
    static func missing(_ view: ProviderView) -> Bool {
        guard view.supported, view.platform != .none else { return false }
        if view.enabled != true { return true }
        return (view.platform == .ios || view.platform == .macos) && !view.list
    }

    func shows(_ view: ProviderView, now: Date = .now) -> Bool {
        if done || later >= Self.maxLater || now.timeIntervalSince1970 < until { return false }
        return Self.missing(view)
    }

    func later(now: Date = .now) -> AutofillPrompt {
        var next = self
        next.later += 1
        next.until = now.timeIntervalSince1970 + Self.laterDays * 86_400
        return next.saved()
    }

    func seen(_ view: ProviderView) -> AutofillPrompt {
        guard !done, view.supported, !Self.missing(view) else { return self }
        var next = self
        next.done = true
        return next.saved()
    }
}

// MARK: Debug launch

#if DEBUG
/// Screenshot hooks: `-UwULockTab vault|check|generator|settings|search` (iPhone),
/// `-UwULockPlace all|favorites|passwordCheck|sends|fileRequests|maskedAddresses|uwuSSH|uwuRDP|trash|type:<kind>|folder:<id>`,
/// `-UwULockItem <id>`, `-UwULockSettings appearance|security|account|autofill|about` and
/// `-UwULockSheet generator|import|move|accounts|editor`, `-UwULockQuery <text>` (the search). Combine with `-UwULockPhase unlocked
/// -UwULockFixture auth -UwULockVaultFixtures YES` for a logged-in account with made-up items.
enum DebugLaunch {
    private static var defaults: UserDefaults { .standard }

    static var tab: String? { defaults.string(forKey: "UwULockTab") }
    static var query: String? { defaults.string(forKey: "UwULockQuery") }
    static var item: String? { defaults.string(forKey: "UwULockItem") }
    static var settings: String? { defaults.string(forKey: "UwULockSettings") }
    static var sheet: String? { defaults.string(forKey: "UwULockSheet") }

    /// `-UwULockSheet`: the vault's own sheets.
    @MainActor
    static func openSheet(_ vault: VaultModel) {
        switch sheet {
        case "generator": vault.showGenerator = true
        case "import": vault.showImport = true
        default: break
        }
    }

    static var place: SidebarItem? {
        guard let raw = defaults.string(forKey: "UwULockPlace") else { return nil }
        let parts = raw.split(separator: ":", maxSplits: 1).map(String.init)
        switch parts[0] {
        case "all": return .all
        case "favorites": return .favorites
        case "due": return .due
        case "passwordCheck": return .passwordCheck
        case "sends": return .sends
        case "fileRequests": return .fileRequests
        case "maskedAddresses": return .maskedAddresses
        case "uwuSSH": return .uwuSSH
        case "uwuRDP": return .uwuRDP
        case "trash": return .trash
        case "type": return parts.count > 1 ? ItemKind(rawValue: parts[1]).map(SidebarItem.type) : nil
        case "folder": return .folder(parts.count > 1 ? parts[1] : nil)
        default: return nil
        }
    }
}
#endif
