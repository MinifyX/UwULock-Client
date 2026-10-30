import SwiftUI
import UwUDesign

/// Settings → AutoFill & Passkeys (PasskeySettings.tsx, CredentialExchangeCard.tsx): UwULock as
/// the system's AutoFill provider, the sealed list for the extension, Safari, Apple Passwords'
/// hand-over and the extension's log.
struct AutofillSettings: View {
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @State private var provider: ProviderView?
    @State private var passkeys: PasskeyProviderStatus?
    @State private var safari: SafariView?
    @State private var log: AutofillLog?
    @State private var exchangePending = false
    @State private var busy = false
    @State private var problem: String?
    @State private var showLog = false
    @State private var importing = false

    var body: some View {
        Form {
            if exchangePending {
                exchangeCard
            }
            if let provider, provider.supported {
                providerSection(provider)
            }
            if let passkeys {
                listSection(passkeys)
            }
            if let safari, safari.available, safari.platform != .none {
                safariSection(safari)
            }
            if log?.supported == true {
                Section {
                    Button(prefStr("AutoFill-Protokoll anzeigen")) { showLog = true }
                } footer: {
                    Text(prefStr("Was die AutoFill-Erweiterung zuletzt getan hat, ohne Passwörter – für eine Fehlermeldung."))
                }
            }
            if provider == nil && passkeys == nil && problem == nil {
                Section { ProgressView().frame(maxWidth: .infinity) }
            }
            if let problem {
                Section { Text(problem).foregroundStyle(.uwuDangerInk) }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(Text(prefStr("AutoFill")))
        .task(id: scenePhase == .active) { if scenePhase == .active { await load() } }
        .sheet(isPresented: $showLog) { AutofillLogView() }
        .sheet(isPresented: $importing, onDismiss: { Task { await load() } }) { ImportFlowView() }
        .featureToasts()
    }

    // MARK: Sections

    private var exchangeCard: some View {
        Section {
            VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                Label(prefStr("Daten aus Apple Passwörter übernehmen?"), uwuIcon: .import)
                    .uwuFont(.headline)
                Text(prefStr("Du siehst alle Einträge, bevor etwas in deinem Tresor gespeichert wird."))
                    .uwuFont(.subheadline)
                    .foregroundStyle(.secondary)
                HStack {
                    Button(prefStr("Verwerfen"), role: .destructive) { discardExchange() }
                        .uwuGlassButton()
                    Button(prefStr("Ansehen")) { importing = true }
                        .uwuProminentButton()
                }
                .padding(.top, UwUSpacing.xxs)
            }
            .padding(.vertical, UwUSpacing.xxs)
        }
    }

    private func providerSection(_ view: ProviderView) -> some View {
        Section {
            LabeledContent(prefStr("Standard für AutoFill")) {
                if view.enabled == true {
                    Text(Self.stateText(view))
                } else {
                    Button(view.direct ? prefStr("Als Standard festlegen") : prefStr("Einstellungen öffnen")) {
                        askProvider()
                    }
                    .disabled(busy)
                }
            }
        } footer: {
            if view.enabled != true {
                Text(prefStr("\(Self.stateText(view)) · UwULock dort einschalten: \(Self.settingsPath(view))"))
            }
        }
    }

    private func listSection(_ status: PasskeyProviderStatus) -> some View {
        let on = status.settings.appleExtension
        return Section {
            Toggle(prefStr("Passwörter und Passkeys für AutoFill"), isOn: Binding(get: { on }, set: setList))
                .disabled(busy || model.phase != .unlocked)
        } footer: {
            VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                Text(prefStr("Hinterlegt Passwörter und Passkeys versiegelt für UwULocks AutoFill-Erweiterung; die öffnet sie nur nach Face ID, Touch ID oder dem Gerätecode. Logins mit erneuter Master-Passwort-Abfrage bleiben draußen."))
                if on, let warning = status.warning {
                    Text(Self.warningText(warning)).foregroundStyle(.uwuDangerInk)
                }
                if on, let problem = status.problem {
                    Text(problem).foregroundStyle(.uwuDangerInk)
                }
            }
        }
    }

    private func safariSection(_ view: SafariView) -> some View {
        Section {
            LabeledContent(prefStr("Safari-Erweiterung")) {
                if view.enabled == true {
                    Text(prefStr("Eingeschaltet"))
                } else if view.platform == .macos {
                    Button(prefStr("In Safari aktivieren")) { openSafari() }.disabled(busy)
                } else {
                    Text(view.enabled == false ? prefStr("Ausgeschaltet") : prefStr("Unbekannt"))
                }
            }
        } footer: {
            if view.enabled == true {
                Text(prefStr("In Safari eingeschaltet. Melde dich in der Erweiterung an; Passkeys kommen in Safari aus dieser App."))
            } else if view.platform == .ios {
                Text(prefStr("In Einstellungen → Apps → Safari → Erweiterungen einschalten. Passkeys kommen in Safari aus dieser App."))
            } else {
                Text(prefStr("Dieselbe Erweiterung wie in Chrome und Firefox, mit eigener Anmeldung. Passkeys kommen in Safari aus dieser App."))
            }
        }
    }

    // MARK: Words (lib/autofill.ts, lib/passkeys.ts)

    static func stateText(_ view: ProviderView) -> String {
        guard view.supported else { return prefStr("Nicht verfügbar") }
        switch view.enabled {
        case true?: return prefStr("Eingeschaltet")
        case false?: return prefStr("Ausgeschaltet")
        case nil: return prefStr("Unbekannt")
        }
    }

    static func settingsPath(_ view: ProviderView) -> String {
        view.platform == .macos
            ? prefStr("Systemeinstellungen → Allgemein → AutoFill & Passwörter")
            : prefStr("Einstellungen → Allgemein → AutoFill & Passwörter")
    }

    static func warningText(_ warning: PasskeyProviderWarning) -> String {
        let holder = warning.holder ?? prefStr("ein unbekanntes Programm")
        switch warning.kind {
        case .held:
            return prefStr("Ein anderes Programm hält deinen UwULock-Sicherheitsschlüssel: \(holder). Browser reden gerade nicht mit UwULock. Kennst du das Programm nicht, beende es und prüfe deinen Rechner.")
        case .registry:
            return prefStr("Der Windows-Eintrag, der UwULock für Passkey-Anfragen startet, zeigte auf ein anderes Programm: \(holder). UwULock hat ihn zurückgesetzt. Kennst du das Programm nicht, prüfe deinen Rechner.")
        }
    }

    // MARK: Actions

    private func load() async {
        let core = model.core
        guard core.isAvailable else { return }
        do {
            async let provider = core.autofillProviderStatus()
            async let passkeys = core.passkeyProviderStatus()
            async let safari = core.safariExtensionStatus()
            async let log = core.autofillLog()
            async let pending = core.credentialExchangePending()
            self.provider = try await provider
            self.passkeys = try await passkeys
            self.safari = try? await safari
            self.log = try? await log
            self.exchangePending = (try? await pending) ?? false
            problem = nil
        } catch {
            problem = Core.failure(error).authMessage
        }
    }

    private func run(_ action: @escaping () async throws -> Void) {
        busy = true
        problem = nil
        Task {
            defer { busy = false }
            do {
                try await action()
            } catch {
                problem = Core.failure(error).authMessage
            }
        }
    }

    private func askProvider() {
        run { provider = try await model.core.autofillProviderRequest(target: .credentials) }
    }

    private func setList(_ on: Bool) {
        guard var settings = passkeys?.settings else { return }
        settings.appleExtension = on
        run { passkeys = try await model.core.setPasskeyProvider(settings: settings) }
    }

    private func openSafari() {
        run {
            try await model.core.safariExtensionOpen()
            safari = try await model.core.safariExtensionStatus()
        }
    }

    private func discardExchange() {
        run {
            _ = try await model.core.credentialExchangeImport(discard: true)
            exchangePending = false
        }
    }
}

/// The AutoFill extension's log (AutofillLog.tsx): read, copy for a bug report, clear.
struct AutofillLogView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var log: AutofillLog?
    @State private var confirmClear = false

    var body: some View {
        NavigationStack {
            Group {
                if let log {
                    if let problem = log.problem {
                        ContentUnavailableView {
                            Label(prefStr("AutoFill-Protokoll"), uwuIcon: .warning)
                        } description: {
                            Text(prefStr("Das Protokoll ließ sich nicht lesen: \(problem)"))
                        }
                    } else if log.lines.isEmpty {
                        ContentUnavailableView {
                            Label(prefStr("AutoFill-Protokoll"), uwuIcon: .history)
                        } description: {
                            Text(prefStr("Noch keine Einträge. Fülle einmal ein Passwort oder einen Passkey mit UwULock aus."))
                        }
                    } else {
                        ScrollView {
                            VStack(alignment: .leading, spacing: UwUSpacing.sm) {
                                Text(prefStr("Was UwULocks AutoFill-Erweiterung auf diesem Gerät getan hat: welcher Weg, welcher Schritt, welcher Fehlercode. Ohne Passwörter, Benutzernamen und vollständige Adressen – du kannst es für eine Fehlermeldung kopieren."))
                                    .uwuFont(.footnote)
                                    .foregroundStyle(.secondary)
                                Text(verbatim: log.lines.joined(separator: "\n"))
                                    .font(.uwuMono(.caption))
                                    .textSelection(.enabled)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .padding()
                        }
                    }
                } else {
                    ProgressView(prefStr("Lädt …"))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .navigationTitle(Text(prefStr("AutoFill-Protokoll")))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(prefStr("Schließen")) { dismiss() }
                }
                ToolbarItemGroup(placement: .primaryAction) {
                    Button { copy() } label: { Label(prefStr("Kopieren"), uwuIcon: .copy) }
                        .disabled(log?.lines.isEmpty ?? true)
                    Button(role: .destructive) { confirmClear = true } label: {
                        Label(prefStr("Löschen"), uwuIcon: .delete)
                    }
                    .disabled(log?.lines.isEmpty ?? true)
                }
            }
            .confirmationDialog(Text(prefStr("AutoFill-Protokoll löschen?")), isPresented: $confirmClear, titleVisibility: .visible) {
                Button(prefStr("Löschen"), role: .destructive, action: clear)
                Button(authStr("Abbrechen"), role: .cancel) {}
            } message: {
                Text(prefStr("Die Einträge auf diesem Gerät werden gelöscht."))
            }
        }
        #if os(macOS)
        .frame(minWidth: 560, minHeight: 420)
        #endif
        .featureToasts()
        .task { await load() }
    }

    private func load() async {
        do {
            log = try await model.core.autofillLog()
        } catch {
            log = AutofillLog(supported: true, lines: [], problem: Core.failure(error).message)
        }
    }

    private func copy() {
        guard let log else { return }
        Task {
            do {
                try await model.core.copyText(text: log.lines.joined(separator: "\n"))
                FeatureToasts.shared.show(prefStr("AutoFill-Protokoll kopiert"))
            } catch {
                FeatureToasts.shared.show(Core.failure(error).authMessage, danger: true)
            }
        }
    }

    private func clear() {
        Task {
            do {
                try await model.core.autofillLogClear()
                FeatureToasts.shared.show(prefStr("AutoFill-Protokoll gelöscht"))
                await load()
            } catch {
                FeatureToasts.shared.show(Core.failure(error).authMessage, danger: true)
            }
        }
    }
}
