import SwiftUI
import UwUDesign

/// Settings → Konto (SettingsDialog.tsx's Account, the phone's Konto page): who is logged in,
/// syncing, the web vault, import and move, the extras, logging out. On the Mac also every
/// account of this device.
struct AccountSettings: View {
    @Environment(AppModel.self) private var model
    @State private var syncing = false
    @State private var syncProblem: String?
    @State private var confirmLogout = false
    @State private var renaming: AccountBrief?
    @State private var loggingOut: AccountBrief?
    @State private var importing = false
    @State private var moving = false

    var body: some View {
        Form {
            if let status = model.status, status.state != .loggedOut {
                #if os(macOS)
                accounts(status)
                #endif
                details(status)
                sync(status)
                webVault
                if status.state == .unlocked {
                    transfer
                    ExtrasSettingsSection()
                }
                logout
            } else {
                Section {
                    Text(prefStr("Nicht angemeldet."))
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(Text(prefStr("Konto")))
        .renameAccountSheet($renaming)
        .logoutConfirmation(item: $loggingOut)
        .logoutConfirmation(isPresented: $confirmLogout, email: nil) { model.logOut() }
        .sheet(isPresented: $importing) { ImportFlowView() }
        .sheet(isPresented: $moving) { MoveFlowView() }
        .featureToasts()
    }

    #if os(macOS)
    /// Every account on this Mac: a click switches, the menu renames or logs out.
    @ViewBuilder
    private func accounts(_ status: Status) -> some View {
        Section(prefStr("Konten auf diesem Gerät")) {
            ForEach(status.accounts) { account in
                HStack(spacing: UwUSpacing.sm) {
                    AccountAvatar(account: account, size: 28)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(verbatim: account.title).uwuFont(.body, weight: .medium)
                        Text(verbatim: "\(account.email) · \(account.server)")
                            .uwuFont(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if account.active {
                        Text(prefStr("Aktiv")).uwuFont(.caption, weight: .medium).foregroundStyle(.tint)
                    } else {
                        Button(prefStr("Wechseln")) { model.switchAccount(account.id) }
                            .disabled(model.busy)
                    }
                    Menu {
                        Button { renaming = account } label: { Label(authStr("Konto umbenennen"), uwuIcon: .edit) }
                        Button(role: .destructive) { loggingOut = account } label: {
                            Label(authStr("Abmelden"), uwuIcon: .signOut)
                        }
                    } label: {
                        UwUIcon(.more, size: .sm, label: Text(prefStr("Mehr")))
                    }
                    .menuStyle(.borderlessButton)
                    .menuIndicator(.hidden)
                    .fixedSize()
                }
            }
            Button { model.startAdding() } label: { Label(authStr("Konto hinzufügen"), uwuIcon: .add) }
        }
    }
    #endif

    @ViewBuilder
    private func details(_ status: Status) -> some View {
        Section {
            LabeledContent(prefStr("E-Mail")) { Text(verbatim: status.email ?? "").textSelection(.enabled) }
            LabeledContent(prefStr("Server")) { Text(verbatim: status.serverName) }
            if let active = status.accounts.first(where: \.active) {
                LabeledContent(prefStr("Name")) {
                    Button {
                        renaming = active
                    } label: {
                        HStack(spacing: UwUSpacing.xxs) {
                            Text(verbatim: active.title)
                            UwUIcon(.edit, size: .xs, label: Text(authStr("Konto umbenennen")))
                        }
                    }
                    .buttonStyle(.borderless)
                }
            }
        } header: {
            Text(prefStr("Angemeldet als"))
        }
    }

    @ViewBuilder
    private func sync(_ status: Status) -> some View {
        Section {
            Button(action: syncNow) {
                HStack {
                    Label(
                        syncing || status.syncing ? prefStr("Synchronisiert …") : prefStr("Jetzt synchronisieren"),
                        uwuIcon: .sync)
                    Spacer()
                    if syncing || status.syncing {
                        ProgressView().controlSize(.small)
                    }
                }
            }
            .disabled(syncing || status.syncing || status.state != .unlocked)
        } footer: {
            VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                if status.syncError != nil {
                    Text(prefStr("Sync fehlgeschlagen")).foregroundStyle(.uwuDangerInk)
                }
                let when = Ago.text(status.lastSync)
                Text(status.live != nil
                    ? prefStr("Zuletzt \(when) – Änderungen von deinen anderen Geräten kommen sofort an.")
                    : prefStr("Zuletzt \(when) – UwULock holt Änderungen beim Entsperren und danach alle fünf Minuten."))
                if let syncProblem {
                    Text(syncProblem).foregroundStyle(.uwuDangerInk)
                }
            }
        }
    }

    private var webVault: some View {
        Section {
            Button { openWebVault() } label: {
                LabeledContent {
                    Text(prefStr("im Web-Tresor"))
                } label: {
                    Label(prefStr("Master-Passwort ändern"), uwuIcon: .masterPassword)
                }
            }
            Button { openWebVault() } label: {
                Label(prefStr("Web-Tresor öffnen"), uwuIcon: .openExternal)
            }
        } footer: {
            Text(prefStr("Für alles, was UwULock noch nicht kann: zum Beispiel Organisationen verwalten."))
        }
    }

    private var transfer: some View {
        Section {
            Button { importing = true } label: { Label(prefStr("Importieren …"), uwuIcon: .import) }
            Button { moving = true } label: { Label(prefStr("Von Bitwarden umziehen …"), uwuIcon: .move) }
        } footer: {
            Text(prefStr("Importieren holt Einträge aus der Export-Datei eines anderen Passwort-Managers. Umziehen holt Einträge, Ordner, Anhänge, Sends und Organisationen aus Bitwarden oder Vaultwarden. Entschlüsselt wird nur auf diesem Gerät."))
        }
    }

    private var logout: some View {
        Section {
            Button(role: .destructive) { confirmLogout = true } label: {
                Label(prefStr("Abmelden …"), uwuIcon: .signOut)
                    .foregroundStyle(.uwuDangerInk)
            }
        } footer: {
            Text(prefStr("Löscht die Anmeldung und die verschlüsselte Kopie des Tresors von diesem Gerät. Auf dem Server bleibt alles, wie es ist."))
        }
    }

    private func syncNow() {
        syncing = true
        syncProblem = nil
        Task {
            defer { syncing = false }
            do {
                try await model.syncNow()
                FeatureToasts.shared.show(prefStr("Synchronisiert ✧"))
            } catch {
                syncProblem = Core.failure(error).authMessage
            }
        }
    }

    private func openWebVault() {
        Task {
            do {
                try await model.core.openWebVault()
            } catch {
                FeatureToasts.shared.show(Core.failure(error).authMessage, danger: true)
            }
        }
    }
}
