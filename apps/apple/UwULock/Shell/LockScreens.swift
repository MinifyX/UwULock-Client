import SwiftUI
import UwUDesign

/// The locked vault: wordmark, a sleepy Nyu, master password or biometrics. Placeholder until
/// the Rust core checks the password (milestone 4).
struct LockView: View {
    @Environment(AppModel.self) private var model
    @State private var password = ""

    var body: some View {
        AuthScaffold(mood: .sleepy) {
            Text("Dein Tresor ist gesperrt")
                .uwuFont(.title2, weight: .bold)
                .multilineTextAlignment(.center)

            SecureField("Master-Passwort", text: $password)
                .textContentType(.password)
                .onSubmit { model.unlock() }
                .authField()

            VStack(spacing: UwUSpacing.sm) {
                Button { model.unlock() } label: {
                    Text("Entsperren").frame(maxWidth: .infinity)
                }
                .uwuProminentButton()
                .keyboardShortcut(.defaultAction)

                Button { model.unlock() } label: {
                    Label(biometricTitle, uwuIcon: biometricIcon)
                        .frame(maxWidth: .infinity)
                }
                .uwuGlassButton()
            }
            .controlSize(.large)

            Button("Abmelden") { model.logOut() }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
        }
    }

    private var biometricTitle: LocalizedStringKey {
        #if os(iOS)
        "Mit Face ID entsperren"
        #else
        "Mit Touch ID entsperren"
        #endif
    }

    private var biometricIcon: UwUIconName { .fingerprint }
}

/// Logging in: server, email, master password. Placeholder for milestone 4 (2FA, SSO).
struct LoginView: View {
    @Environment(AppModel.self) private var model
    @State private var selfHosted = false
    @State private var server = ""
    @State private var email = ""
    @State private var password = ""

    var body: some View {
        AuthScaffold(mood: .happy) {
            Text("Anmelden")
                .uwuFont(.title2, weight: .bold)

            VStack(spacing: UwUSpacing.sm) {
                Toggle("Selbst gehostet", isOn: $selfHosted.animation(.uwuOut()))
                    .toggleStyle(.switch)
                if selfHosted {
                    TextField("Server-Adresse", text: $server, prompt: Text(verbatim: "https://vault.example.com"))
                        .textContentType(.URL)
                        .authField()
                }
                TextField("E-Mail-Adresse", text: $email, prompt: Text(verbatim: "name@example.com"))
                    .textContentType(.username)
                    .authField()
                SecureField("Master-Passwort", text: $password)
                    .textContentType(.password)
                    .onSubmit { model.logIn() }
                    .authField()
            }

            Button { model.logIn() } label: {
                Text("Anmelden").frame(maxWidth: .infinity)
            }
            .uwuProminentButton()
            .controlSize(.large)
            .keyboardShortcut(.defaultAction)
        }
    }
}

/// Shared layout of the lock and login screens: wordmark and Nyu above a glass card.
private struct AuthScaffold<Content: View>: View {
    let mood: NyuMood
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(spacing: UwUSpacing.xl) {
                NyuView(mood: mood).frame(width: 128)
                UwUWordmark(style: .title, mood: nil)
                VStack(spacing: UwUSpacing.lg) { content }
                    .padding(UwUSpacing.xl)
                    .frame(maxWidth: 400)
                    .uwuGlass(in: RoundedRectangle(cornerRadius: UwURadius.dialog, style: .continuous))
            }
            .padding(UwUSpacing.xl)
            .frame(maxWidth: .infinity)
        }
        .scrollBounceBehavior(.basedOnSize)
        .defaultScrollAnchor(.center)
        .background(.uwuCanvas)
    }
}

private extension View {
    func authField() -> some View {
        #if os(iOS)
        self
            .padding(.horizontal, UwUSpacing.md)
            .frame(minHeight: 48)
            .background(.uwuSurface, in: RoundedRectangle(cornerRadius: UwURadius.control, style: .continuous))
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
        #else
        self.textFieldStyle(.roundedBorder).controlSize(.large)
        #endif
    }
}
