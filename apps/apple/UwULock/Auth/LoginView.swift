import SwiftUI
import UwUDesign

/// Logging in (LoginScreen.tsx): server, email and master password, then whatever the account
/// asks for — two-step login, a new device's code, SSO and its master password.
struct LoginView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        AuthScaffold(mood: mood, title: title, lead: model.loginStage == .form ? lead : nil) {
            Group {
                switch model.loginStage {
                case .form:
                    LoginForm(mode: model.loginMode, status: model.status)
                case .browser:
                    SsoWaiting()
                case .twoFactor(let methods, let message):
                    TwoFactorStep(methods: methods, message: message)
                case .newDevice:
                    NewDeviceStep()
                case .ssoPassword(let email, let newAccount, let policy):
                    SsoPasswordStep(email: email, newAccount: newAccount, policy: policy)
                }
            }
            .transition(.opacity)
        }
        .uwuAnimation(value: model.loginStage)
        .uwuHaptic(.error, trigger: model.failure)
    }

    private var mood: NyuMood {
        if model.failure != nil { return .puzzled }
        return model.loginStage == .form ? .happy : .sparkle
    }

    private var title: String {
        switch model.loginMode {
        case .again: authStr("Einmal neu anmelden, bitte")
        case .adding: authStr("Noch ein Konto ✧")
        case .first: authStr("Hallo! Ich bin Nyu ✧")
        }
    }

    private var lead: String {
        switch model.loginMode {
        case .again:
            authStr("Der Server kennt die Sitzung dieses Geräts nicht mehr. Nach der Anmeldung geht es weiter wie vorher.")
        case .adding:
            authStr("Privat und Arbeit nebeneinander: Jedes Konto behält seinen Server, seinen Tresor und sein eigenes Master-Passwort.")
        case .first:
            authStr("UwULock öffnet deinen Tresor von Vaultwarden oder Bitwarden. Dein Master-Passwort verlässt dieses Gerät nie – der Server bekommt nur einen Hash davon.")
        }
    }
}

/// The first step: which server, which account.
private struct LoginForm: View {
    @Environment(AppModel.self) private var model
    let mode: LoginMode
    @State private var kind: ServerKind
    @State private var url: String
    @State private var email: String
    @State private var password = ""
    @State private var problem: String?
    @FocusState private var focus: Field?

    private enum Field { case url, email, password }

    init(mode: LoginMode, status: Status?) {
        self.mode = mode
        switch mode {
        case .again:
            _kind = State(initialValue: status?.serverKind ?? LastLogin.serverKind)
            _url = State(initialValue: status?.serverUrl ?? "")
            _email = State(initialValue: status?.email ?? "")
        case .adding:
            _kind = State(initialValue: LastLogin.serverKind)
            _url = State(initialValue: "")
            _email = State(initialValue: "")
        case .first:
            _kind = State(initialValue: LastLogin.serverKind)
            _url = State(initialValue: LastLogin.serverUrl)
            _email = State(initialValue: LastLogin.email)
        }
    }

    private var busy: Bool { model.busy }
    private var locked: Bool { mode == .again }

    /// SSO comes back to a listener on this device: self-hosted servers only. The Mac uses the
    /// system browser, iOS the page in the app (SsoBrowser).
    private var ssoOffered: Bool { kind == .selfHosted }

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.md) {
            AuthCardTitle(authStr("Anmelden"))

            VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                Text(authStr("Server")).uwuFont(.subheadline, weight: .medium)
                Picker(authStr("Server"), selection: $kind) {
                    Text(authStr("Selbst gehostet")).tag(ServerKind.selfHosted)
                    Text(verbatim: "bitwarden.com").tag(ServerKind.bitwardenUS)
                    Text(verbatim: "bitwarden.eu").tag(ServerKind.bitwardenEU)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .disabled(locked || busy)
            }

            if kind == .selfHosted {
                AuthField(label: authStr("Server-Adresse"), hint: authStr("Die Adresse, unter der du deinen Web-Tresor öffnest.")) {
                    TextField(authStr("Server-Adresse"), text: $url, prompt: Text(verbatim: "https://vault.example.com"))
                        .textContentType(.URL)
                        #if os(iOS)
                        .keyboardType(.URL)
                        #endif
                        .focused($focus, equals: .url)
                        .submitLabel(.next)
                        .onSubmit { focus = .email }
                        .disabled(locked || busy)
                }
                .transition(.opacity)
            }

            AuthField(label: authStr("E-Mail-Adresse")) {
                TextField(authStr("E-Mail-Adresse"), text: $email, prompt: Text(verbatim: "name@example.com"))
                    .textContentType(.username)
                    #if os(iOS)
                    .keyboardType(.emailAddress)
                    #endif
                    .focused($focus, equals: .email)
                    .submitLabel(.next)
                    .onSubmit { focus = .password }
                    .disabled(locked || busy)
            }

            AuthField(label: authStr("Master-Passwort")) {
                SecureField(authStr("Master-Passwort"), text: $password)
                    .textContentType(.password)
                    .focused($focus, equals: .password)
                    .submitLabel(.go)
                    .onSubmit(logIn)
                    .disabled(busy)
            }

            AuthErrorText(text: problem ?? model.loginNotice ?? model.failure?.authMessage)

            HStack(spacing: UwUSpacing.sm) {
                if mode != .first {
                    Button(authStr("Abbrechen")) { model.cancelLogin() }
                        .uwuGlassButton()
                        .disabled(busy)
                        .keyboardShortcut(.cancelAction)
                }
                Button(action: logIn) {
                    BusyLabel(title: authStr("Anmelden"), busy: busy ? authStr("Nyu leitet deinen Schlüssel ab …") : nil)
                }
                .uwuProminentButton()
                .disabled(busy || password.isEmpty)
                .keyboardShortcut(.defaultAction)
            }
            .controlSize(.large)

            if ssoOffered {
                Button(action: startSso) {
                    Label(authStr("Mit SSO anmelden"), uwuIcon: .organization).frame(maxWidth: .infinity)
                }
                .uwuGlassButton()
                .controlSize(.large)
                .disabled(busy)
            }
        }
        .uwuAnimation(value: kind)
        .onAppear {
            focus = locked || (!email.isEmpty && (kind != .selfHosted || !url.isEmpty))
                ? .password
                : (kind == .selfHosted && url.isEmpty ? .url : .email)
        }
        .onChange(of: kind) { problem = nil }
    }

    private var server: ServerInput {
        kind == .selfHosted
            ? ServerInput(kind: .selfHosted, url: url.trimmingCharacters(in: .whitespaces))
            : ServerInput(kind: kind)
    }

    private func logIn() {
        guard !busy, !password.isEmpty else { return }
        problem = nil
        let secret = password
        password = ""
        model.logIn(server: server, email: email.trimmingCharacters(in: .whitespaces), password: secret)
    }

    private func startSso() {
        problem = nil
        guard !url.trimmingCharacters(in: .whitespaces).isEmpty else {
            problem = authStr("Bitte gib die Adresse deines Servers ein.")
            focus = .url
            return
        }
        let email = email.trimmingCharacters(in: .whitespaces)
        model.logInWithSso(server: server, email: email.isEmpty ? nil : email)
    }
}

/// SSO: the browser is open; the core waits for it to come back.
private struct SsoWaiting: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.md) {
            AuthCardTitle(authStr("Anmeldung im Browser"))
            AuthLead(authStr("Dein Browser hat sich geöffnet. Melde dich dort an – danach geht es hier von selbst weiter."))
            ProgressView()
                .frame(maxWidth: .infinity)
                .accessibilityLabel(Text(authStr("Anmeldung im Browser")))
            Button(authStr("Abbrechen")) { model.backToForm() }
                .uwuGlassButton()
                .controlSize(.large)
                .keyboardShortcut(.cancelAction)
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
    }
}
