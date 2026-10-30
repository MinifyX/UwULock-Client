import SwiftUI
import UwUDesign

/// The settings: tabs in the Mac's Settings window (SettingsDialog.tsx), a list of pages on the
/// iPhone (mobile/pages/Settings.tsx). Everything is stored on this device only.
struct SettingsView: View {
    var body: some View {
        #if os(macOS)
        MacSettings()
        #else
        PhoneSettings()
        #endif
    }
}

#if os(macOS)
private struct MacSettings: View {
    @AppStorage("settings.tab") private var tab = "appearance"

    var body: some View {
        TabView(selection: $tab) {
            Tab(value: "appearance") { AppearanceSettings() } label: {
                Label(prefStr("Darstellung"), uwuIcon: .appearance)
            }
            Tab(value: "security") { SecuritySettings() } label: {
                Label(prefStr("Sicherheit"), uwuIcon: .locked)
            }
            Tab(value: "account") { AccountSettings() } label: {
                Label(prefStr("Konten"), uwuIcon: .account)
            }
            Tab(value: "autofill") { AutofillSettings() } label: {
                Label(prefStr("AutoFill"), uwuIcon: .passkey)
            }
            Tab(value: "updates") { UpdatesSettings() } label: {
                Label(prefStr("Updates"), uwuIcon: .download)
            }
            Tab(value: "about") { AboutSettings() } label: {
                Label(prefStr("Über UwULock"), uwuIcon: .info)
            }
        }
        .frame(width: 600, height: 560)
    }
}

private struct UpdatesSettings: View {
    var body: some View {
        Form {
            Section { UpdatesSettingsSection() }
        }
        .formStyle(.grouped)
    }
}
#else
/// The iPhone's settings page (PhoneTabs puts it in a NavigationStack).
private struct PhoneSettings: View {
    @Environment(AppModel.self) private var model
    @AppStorage(AppSettingsKey.theme) private var theme: AppTheme = .system
    @State private var showAccounts = false
    @State private var confirmLogout = false
    @State private var importing = false
    @State private var provider: ProviderView?

    var body: some View {
        let status = model.status
        List {
            if let status, let email = status.email {
                Section {
                    Button { showAccounts = true } label: {
                        HStack(spacing: UwUSpacing.sm) {
                            AccountAvatar(status: status, size: 44)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(verbatim: status.label.flatMap { $0.isEmpty ? nil : $0 } ?? email)
                                    .uwuFont(.headline)
                                Text(verbatim: "\(email) · \(status.serverName)")
                                    .uwuFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Text(prefStr("Konto wechseln"))
                                .uwuFont(.caption, weight: .medium)
                                .foregroundStyle(.tint)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }

            Section {
                NavigationLink { AppearanceSettings() } label: {
                    LabeledContent { Text(theme.title) } label: { Label(prefStr("Darstellung"), uwuIcon: .appearance) }
                }
                NavigationLink { SecuritySettings() } label: {
                    LabeledContent {
                        Text(status?.hello == true ? Biometrics.name(status?.helloKind) : prefStr("Aus"))
                    } label: {
                        Label(prefStr("Sicherheit"), uwuIcon: .locked)
                    }
                }
                NavigationLink { AutofillSettings() } label: {
                    LabeledContent {
                        if let provider, provider.supported {
                            Text(provider.enabled == true ? prefStr("An") : prefStr("Aus"))
                        }
                    } label: {
                        Label(prefStr("AutoFill"), uwuIcon: .passkey)
                    }
                }
            }

            Section {
                NavigationLink { AccountSettings() } label: { Label(prefStr("Konto"), uwuIcon: .account) }
                if status?.state == .unlocked {
                    Button { importing = true } label: { Label(prefStr("Importieren"), uwuIcon: .import) }
                }
                NavigationLink { AboutSettings() } label: { Label(prefStr("Über UwULock"), uwuIcon: .info) }
            }

            if let status, status.state != .loggedOut {
                Section {
                    Button { model.lock() } label: {
                        Label(status.accounts.count > 1 ? prefStr("Alle sperren") : prefStr("Jetzt sperren"), uwuIcon: .locked)
                    }
                    .disabled(model.phase != .unlocked)
                    Button(role: .destructive) { confirmLogout = true } label: {
                        Label(prefStr("Abmelden"), uwuIcon: .signOut).foregroundStyle(.uwuDangerInk)
                    }
                }
            }
        }
        .navigationTitle(Text(prefStr("Einstellungen")))
        .sheet(isPresented: $showAccounts) { AccountsSheet() }
        .sheet(isPresented: $importing) { ImportFlowView() }
        .logoutConfirmation(isPresented: $confirmLogout, email: nil) { model.logOut() }
        .task {
            guard model.core.isAvailable else { return }
            provider = try? await model.core.autofillProviderStatus()
        }
    }
}
#endif
