import SwiftUI
import UwUDesign

/// Settings → Sicherheit: auto-lock, clearing the clipboard, locking with the system,
/// Face ID / Touch ID, "Jetzt sperren".
struct SecuritySettings: View {
    @Environment(AppModel.self) private var model
    @AppStorage(AppSettingsKey.autoLock) private var autoLock = 15
    @AppStorage(AppSettingsKey.clipboardClear) private var clipboardClear = 30
    @AppStorage(AppSettingsKey.lockWithSystem) private var lockWithSystem = true
    @State private var helloBusy = false
    @State private var helloProblem: String?

    var body: some View {
        Form {
            Section {
                Picker(prefStr("Automatisch sperren"), selection: $autoLock) {
                    ForEach(SecurityPrefs.autoLockChoices, id: \.self) { Text(Self.autoLockLabel($0)).tag($0) }
                }
            } footer: {
                Text(prefStr("Nach so langer Zeit ohne Eingabe sperrt UwULock den Tresor und vergisst alles Entschlüsselte. Beim Beenden ist er immer gesperrt."))
            }

            Section {
                #if os(macOS)
                Toggle(prefStr("Mit dem Computer sperren"), isOn: $lockWithSystem)
                #else
                Toggle(prefStr("Im Hintergrund sperren"), isOn: $lockWithSystem)
                #endif
            } footer: {
                #if os(macOS)
                Text(prefStr("Sperrt UwULock, sobald der Bildschirm gesperrt wird oder der Computer in den Ruhezustand geht."))
                #else
                Text(prefStr("Sperrt UwULock, wenn es eine Minute oder länger im Hintergrund war."))
                #endif
            }

            Section {
                Picker(prefStr("Zwischenablage leeren"), selection: $clipboardClear) {
                    ForEach(SecurityPrefs.clipboardChoices, id: \.self) { Text(Self.clipboardLabel($0)).tag($0) }
                }
            } footer: {
                Text(prefStr("Kopierte Werte verschwinden danach wieder – aber nur, wenn inzwischen nichts anderes kopiert wurde."))
            }

            if let hello = model.status?.hello {
                Section {
                    Toggle(isOn: Binding(get: { hello }, set: setHello)) {
                        Label(Biometrics.unlockLabel(model.status?.helloKind), uwuIcon: Biometrics.icon(model.status?.helloKind))
                    }
                    .disabled(helloBusy || model.phase != .unlocked)
                } footer: {
                    VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                        Text(Biometrics.description)
                        if let helloProblem {
                            Text(helloProblem).foregroundStyle(.uwuDangerInk)
                        }
                    }
                }
            }

            Section {
                Button {
                    model.lock()
                } label: {
                    Label(prefStr("Jetzt sperren"), uwuIcon: .locked)
                }
                .disabled(model.phase != .unlocked)
            } footer: {
                #if os(macOS)
                Text(prefStr("Auch mit \("⌘L"), von überall in UwULock."))
                #endif
            }
        }
        .formStyle(.grouped)
        .navigationTitle(Text(prefStr("Sicherheit")))
        .onChange(of: autoLock) { model.applySecurity() }
        .onChange(of: clipboardClear) { model.applySecurity() }
        .onChange(of: lockWithSystem) { model.applySecurity() }
    }

    private func setHello(_ enabled: Bool) {
        helloBusy = true
        helloProblem = nil
        Task {
            defer { helloBusy = false }
            do {
                try await model.setBiometrics(enabled)
            } catch {
                let failure = Core.failure(error)
                if failure.kind != "biometric-cancelled" { helloProblem = failure.authMessage }
            }
        }
    }

    static func autoLockLabel(_ minutes: Int) -> String {
        switch minutes {
        case 0: prefStr("Nie (nur beim Beenden)")
        case 1: prefStr("1 Minute")
        case 60: prefStr("1 Stunde")
        case ..<60: prefStr("\(String(minutes)) Minuten")
        default: prefStr("\(String(minutes / 60)) Stunden")
        }
    }

    static func clipboardLabel(_ seconds: Int) -> String {
        seconds == 0 ? prefStr("Nie") : prefStr("nach \(String(seconds)) Sekunden")
    }
}
