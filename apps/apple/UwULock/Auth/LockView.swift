import SwiftUI
import UwUDesign

/// The locked vault (LockScreen.tsx): master password or Face ID / Touch ID, the other accounts,
/// logging out.
struct LockView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @State private var password = ""
    @State private var confirmLogout = false
    /// The account the biometric prompt came up for by itself (once per account).
    @State private var asked: String?
    @FocusState private var focused: Bool

    private var status: Status? { model.status }
    private var others: [AccountBrief] { status?.accounts.filter { !$0.active } ?? [] }
    private var helloOn: Bool { status?.hello == true }

    /// Asked by itself on the iPhone and with Touch ID (no extra click on a Mac with a sensor).
    private var autoAsk: Bool {
        #if os(iOS)
        helloOn
        #else
        helloOn && status?.helloKind == "touchId"
        #endif
    }

    var body: some View {
        AuthScaffold(mood: model.failure == nil ? .sleepy : .puzzled) {
            AuthCardTitle(authStr("Dein Tresor ist gesperrt"))
            if let status, let email = status.email {
                VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                    Text(verbatim: email).uwuFont(.headline)
                    Text(verbatim: [status.label, status.serverName].compactMap { $0 }.filter { !$0.isEmpty && $0 != email }.joined(separator: " · "))
                        .uwuFont(.subheadline)
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            }

            AuthField(label: authStr("Master-Passwort")) {
                SecureField(authStr("Master-Passwort"), text: $password)
                    .textContentType(.password)
                    .focused($focused)
                    .submitLabel(.go)
                    .onSubmit(unlock)
                    .disabled(model.busy)
            }

            AuthErrorText(text: model.failure?.authMessage)

            VStack(spacing: UwUSpacing.sm) {
                Button(action: unlock) {
                    BusyLabel(title: authStr("Entsperren"), busy: model.busy ? authStr("Entsperrt …") : nil)
                }
                .uwuProminentButton()
                .disabled(model.busy || password.isEmpty)
                .keyboardShortcut(.defaultAction)

                if helloOn {
                    Button { model.unlockWithBiometrics() } label: {
                        Label(Biometrics.unlockLabel(status?.helloKind), uwuIcon: Biometrics.icon(status?.helloKind))
                            .frame(maxWidth: .infinity)
                    }
                    .uwuGlassButton()
                    .disabled(model.busy)
                }
            }
            .controlSize(.large)

            HStack {
                Text(authStr("Zuletzt synchronisiert: \(Ago.text(status?.lastSync))"))
                    .uwuFont(.footnote)
                    .foregroundStyle(.secondary)
                Spacer()
                Button(authStr("Abmelden")) { confirmLogout = true }
                    .buttonStyle(.borderless)
                    .uwuFont(.footnote, weight: .medium)
            }

            if !others.isEmpty || status != nil {
                OtherAccounts(accounts: others)
            }
        }
        .uwuHaptic(.error, trigger: model.failure)
        .logoutConfirmation(isPresented: $confirmLogout, email: nil) { model.logOut() }
        .onAppear { focused = !autoAsk }
        .task(id: "\(status?.accountId ?? "")|\(scenePhase == .active)") { askOnce() }
        .onChange(of: model.phase) { password = "" }
    }

    private func unlock() {
        guard !password.isEmpty, !model.busy else { return }
        let secret = password
        password = ""
        model.unlock(password: secret)
    }

    private func askOnce() {
        guard autoAsk, !model.pinned, scenePhase == .active, let id = status?.accountId, asked != id else { return }
        asked = id
        model.unlockWithBiometrics()
    }
}

/// The lock screen's other accounts: open or locked, a tap switches; "Konto hinzufügen".
private struct OtherAccounts: View {
    @Environment(AppModel.self) private var model
    let accounts: [AccountBrief]

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.xs) {
            if !accounts.isEmpty {
                Divider()
                ForEach(accounts) { account in
                    Button { model.switchAccount(account.id) } label: {
                        HStack(spacing: UwUSpacing.sm) {
                            AccountAvatar(account: account)
                            VStack(alignment: .leading, spacing: 0) {
                                Text(verbatim: account.label.isEmpty ? account.email : account.label)
                                    .uwuFont(.subheadline, weight: .medium)
                                Text(verbatim: account.email)
                                    .uwuFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Text(account.unlocked ? authStr("offen") : authStr("gesperrt"))
                                .uwuFont(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(model.busy)
                    .accessibilityHint(Text(authStr("Konto wechseln")))
                }
            }
            Button { model.startAdding() } label: {
                Label(authStr("Konto hinzufügen"), uwuIcon: .add)
            }
            .buttonStyle(.borderless)
            .uwuFont(.subheadline, weight: .medium)
        }
    }
}
