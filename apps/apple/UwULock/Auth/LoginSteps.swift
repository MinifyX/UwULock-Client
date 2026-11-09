import SwiftUI
import UwUDesign

extension TwoFactorKind {
    /// LoginScreen.tsx's METHOD_LABEL.
    var title: String {
        switch self {
        case .authenticator: authStr("Authenticator-App")
        case .email: authStr("E-Mail")
        case .yubikey: "YubiKey OTP"
        case .duo: "Duo"
        case .webauthn: "Passkey / FIDO2"
        case .u2f: "FIDO U2F"
        case .other: "?"
        }
    }
}

/// The account asks for a second step: authenticator, email code, YubiKey OTP.
struct TwoFactorStep: View {
    @Environment(AppModel.self) private var model
    let methods: [TwoFactorMethod]
    let message: String?
    @State private var provider: Int?
    @State private var code = ""
    @State private var remember = true
    @State private var sent = false
    @State private var sendProblem: String?
    @FocusState private var focused: Bool

    init(methods: [TwoFactorMethod], message: String?) {
        self.methods = methods
        self.message = message
        _provider = State(initialValue: methods.first(where: \.supported)?.provider)
    }

    private var usable: [TwoFactorMethod] { methods.filter(\.supported) }
    private var method: TwoFactorMethod? { usable.first { $0.provider == provider } }

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.md) {
            AuthCardTitle(authStr("Zweistufige Anmeldung"))
            if let method {
                form(method)
            } else {
                let names = methods.map(\.kind.title).joined(separator: ", ")
                AuthLead(authStr("Dein Konto verlangt eine Methode, die UwULock noch nicht kann (\(names)). Richte im Web-Tresor zusätzlich eine Authenticator-App oder E-Mail-Codes ein."))
                Button(authStr("Zurück")) { model.backToForm() }
                    .uwuGlassButton()
                    .controlSize(.large)
                    .keyboardShortcut(.cancelAction)
            }
        }
    }

    @ViewBuilder
    private func form(_ method: TwoFactorMethod) -> some View {
        if usable.count > 1 {
            Picker(authStr("Methode"), selection: $provider) {
                ForEach(usable) { option in
                    Text(option.kind.title).tag(Optional(option.provider))
                }
            }
            .pickerStyle(.segmented)
            .onChange(of: provider) {
                code = ""
                sendProblem = nil
                focused = true
            }
        }

        if let lead = lead(method) {
            AuthLead(lead)
        }

        let unsupported = methods.filter { !$0.supported }
        if !unsupported.isEmpty {
            let names = unsupported.map(\.kind.title).joined(separator: ", ")
            Text(authStr("Noch nicht unterstützt: \(names)."))
                .uwuFont(.footnote)
                .foregroundStyle(.secondary)
        }

        AuthField(label: authStr("Code")) {
            TextField(authStr("Code"), text: $code)
                .textContentType(.oneTimeCode)
                #if os(iOS)
                .keyboardType(method.kind == .yubikey ? .asciiCapable : .numberPad)
                #endif
                .font(.uwuMono(.title3))
                .focused($focused)
                .onSubmit(submit)
                .disabled(model.busy)
        }

        Toggle(authStr("Auf diesem Gerät merken"), isOn: $remember)
            #if os(macOS)
            .toggleStyle(.checkbox)
            #endif

        AuthErrorText(text: sendProblem ?? model.failure?.authMessage ?? message)

        ViewThatFits(in: .horizontal) {
            HStack(spacing: UwUSpacing.sm) { buttons(method) }
            VStack(spacing: UwUSpacing.sm) { buttons(method) }
        }
        .controlSize(.large)
        .onAppear { focused = true }
    }

    @ViewBuilder
    private func buttons(_ method: TwoFactorMethod) -> some View {
        Button(authStr("Zurück")) { model.backToForm() }
            .uwuGlassButton()
            .disabled(model.busy)
            .keyboardShortcut(.cancelAction)
        if method.kind == .email {
            Button(sent ? authStr("Nochmal senden") : authStr("Code senden"), action: sendEmail)
                .uwuGlassButton()
                .disabled(model.busy)
        }
        Button(action: submit) {
            BusyLabel(title: authStr("Weiter"), busy: model.busy ? authStr("Prüft …") : nil)
        }
        .uwuProminentButton()
        .disabled(model.busy || code.trimmingCharacters(in: .whitespaces).isEmpty)
        .keyboardShortcut(.defaultAction)
    }

    private func lead(_ method: TwoFactorMethod) -> String? {
        switch method.kind {
        case .authenticator:
            return authStr("Gib den sechsstelligen Code aus deiner Authenticator-App ein.")
        case .email:
            let address = method.hint ?? authStr("deine E-Mail-Adresse")
            return sent
                ? authStr("Der Code ist unterwegs an \(address).")
                : authStr("Lass dir einen Code an \(address) schicken und gib ihn hier ein.")
        case .yubikey:
            return authStr("Stecke deinen YubiKey ein und tippe ihn an.")
        default:
            return nil
        }
    }

    private func submit() {
        guard let provider, !model.busy else { return }
        let entered = code.replacingOccurrences(of: " ", with: "")
        guard !entered.isEmpty else { return }
        sendProblem = nil
        code = ""
        model.submitTwoFactor(provider: provider, code: entered, remember: remember)
    }

    private func sendEmail() {
        sendProblem = nil
        Task {
            do {
                try await model.sendEmailCode()
                sent = true
                focused = true
            } catch {
                sendProblem = Core.failure(error).authMessage
            }
        }
    }
}

/// Bitwarden doesn't know this device yet and mailed a code.
struct NewDeviceStep: View {
    @Environment(AppModel.self) private var model
    @State private var code = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.md) {
            AuthCardTitle(authStr("Neues Gerät bestätigen"))
            AuthLead(authStr("Bitwarden kennt dieses Gerät noch nicht und hat dir einen Code per E-Mail geschickt."))
            AuthField(label: authStr("Code aus der E-Mail")) {
                TextField(authStr("Code aus der E-Mail"), text: $code)
                    .textContentType(.oneTimeCode)
                    #if os(iOS)
                    .keyboardType(.numberPad)
                    #endif
                    .font(.uwuMono(.title3))
                    .focused($focused)
                    .onSubmit(submit)
                    .disabled(model.busy)
            }
            AuthErrorText(text: model.failure?.authMessage)
            HStack(spacing: UwUSpacing.sm) {
                Button(authStr("Zurück")) { model.backToForm() }
                    .uwuGlassButton()
                    .disabled(model.busy)
                    .keyboardShortcut(.cancelAction)
                Button(action: submit) {
                    BusyLabel(title: authStr("Weiter"), busy: model.busy ? authStr("Prüft …") : nil)
                }
                .uwuProminentButton()
                .disabled(model.busy || code.trimmingCharacters(in: .whitespaces).isEmpty)
                .keyboardShortcut(.defaultAction)
            }
            .controlSize(.large)
        }
        .onAppear { focused = true }
    }

    private func submit() {
        let entered = code.replacingOccurrences(of: " ", with: "")
        guard !entered.isEmpty, !model.busy else { return }
        code = ""
        model.submitNewDevice(code: entered)
    }
}

/// After SSO: the master password opens the vault — or, for an account without a vault yet,
/// is chosen now. The keys are made on this device; only the hash goes to the server.
struct SsoPasswordStep: View {
    @Environment(AppModel.self) private var model
    let email: String
    let newAccount: Bool
    let policy: PasswordPolicy
    @State private var password = ""
    @State private var repeated = ""
    @FocusState private var focused: Bool

    private var minLength: Int { max(12, policy.minLength) }
    private var minComplexity: Int { min(4, max(0, policy.minComplexity)) }
    private var tooShort: Bool { newAccount && password.count < minLength }
    private var mismatch: Bool { newAccount && !repeated.isEmpty && repeated != password }

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.md) {
            AuthCardTitle(newAccount ? authStr("Master-Passwort festlegen") : authStr("Master-Passwort"))
            AuthLead(newAccount
                ? authStr("Angemeldet als \(email). Wähle jetzt dein Master-Passwort: Es öffnet deinen Tresor und verlässt dieses Gerät nie. Niemand kann es zurücksetzen – auch nicht der Server.")
                : authStr("Angemeldet als \(email). Dein Master-Passwort öffnet den Tresor."))

            AuthField(label: authStr("Master-Passwort"), hint: newAccount ? requirement : nil) {
                SecureField(authStr("Master-Passwort"), text: $password)
                    .textContentType(newAccount ? .newPassword : .password)
                    .focused($focused)
                    .onSubmit(submit)
                    .disabled(model.busy)
            }
            if newAccount {
                AuthField(
                    label: authStr("Master-Passwort wiederholen"),
                    hint: mismatch ? authStr("Die Passwörter sind verschieden.") : nil
                ) {
                    SecureField(authStr("Master-Passwort wiederholen"), text: $repeated)
                        .textContentType(.newPassword)
                        .onSubmit(submit)
                        .disabled(model.busy)
                }
            }

            AuthErrorText(text: model.failure?.authMessage)

            HStack(spacing: UwUSpacing.sm) {
                Button(authStr("Zurück")) { model.backToForm() }
                    .uwuGlassButton()
                    .disabled(model.busy)
                    .keyboardShortcut(.cancelAction)
                Button(action: submit) {
                    BusyLabel(title: authStr("Weiter"), busy: model.busy ? authStr("Nyu leitet deinen Schlüssel ab …") : nil)
                }
                .uwuProminentButton()
                .disabled(model.busy || password.isEmpty || (newAccount && (tooShort || repeated != password)))
                .keyboardShortcut(.defaultAction)
            }
            .controlSize(.large)
        }
        .onAppear { focused = true }
    }

    private var requirement: String {
        let count = String(minLength)
        guard minComplexity > 0 else { return authStr("Mindestens \(count) Zeichen.") }
        let names = [authStr("schwach"), authStr("mittel"), authStr("stark"), authStr("sehr stark")]
        let strength = names[minComplexity - 1]
        let level = String(minComplexity)
        return authStr("Mindestens \(count) Zeichen und nicht leicht zu erraten: Dein Server verlangt die Stärke „\(strength)“ (\(level) von 4).")
    }

    private func submit() {
        guard !model.busy, !password.isEmpty else { return }
        if newAccount, tooShort || repeated != password { return }
        model.submitSsoPassword(password)
    }
}
