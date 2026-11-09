import SwiftUI
import UwUDesign

/// Settings → Darstellung (SettingsDialog.tsx's Appearance, the phone's Darstellung page):
/// colour scheme, contrast, motion, font, text size, the trash and website icons.
struct AppearanceSettings: View {
    @AppStorage(AppSettingsKey.theme) private var theme: AppTheme = .system
    @AppStorage(AppSettingsKey.contrast) private var contrast: AppContrast = .system
    @AppStorage(AppSettingsKey.motion) private var motion: AppMotion = .system
    @AppStorage(AppSettingsKey.font) private var font: UwUFontChoice = .uwu
    @AppStorage(AppSettingsKey.textSize) private var textSize: UwUTextSize = .system
    @AppStorage(AppSettingsKey.showTrash) private var showTrash = true
    @AppStorage(AppSettingsKey.siteIcons) private var siteIcons = true
    #if os(iOS)
    @AppStorage(AppSettingsKey.haptics) private var haptics = true
    #endif

    private var followsSystem: String { prefStr("„System“ folgt der Einstellung von \(systemName).") }

    var body: some View {
        Form {
            Section {
                Picker(prefStr("Farbschema"), selection: $theme) {
                    ForEach(AppTheme.allCases) { Text($0.title).tag($0) }
                }
                Picker(prefStr("Kontrast"), selection: $contrast) {
                    ForEach(AppContrast.allCases) { Text($0.title).tag($0) }
                }
                Picker(prefStr("Animationen"), selection: $motion) {
                    ForEach(AppMotion.allCases) { Text($0.title).tag($0) }
                }
            } footer: {
                Text(followsSystem)
            }

            Section {
                Picker(prefStr("Schrift"), selection: $font) {
                    ForEach(UwUFontChoice.allCases) { choice in
                        Text(choice == .system ? prefStr("System") : choice.displayName)
                            .font(.uwu(.body, choice: choice))
                            .tag(choice)
                    }
                }
                Text(verbatim: "Tresor 0123 Il1 O0")
                    .font(.uwu(.title3, choice: font, textSize: textSize))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityHidden(true)
            } footer: {
                Text(prefStr("Nur auf diesem Gerät. UwU Sans ist die Schrift aller UwU-Apps."))
            }

            Section {
                Picker(prefStr("Textgröße"), selection: $textSize) {
                    ForEach(UwUTextSize.allCases) { Text($0.title).tag($0) }
                }
            } footer: {
                Text(prefStr("„System“ folgt der Textgröße von \(systemName)."))
            }

            Section {
                Toggle(prefStr("Papierkorb zeigen"), isOn: $showTrash)
            } footer: {
                Text(prefStr("Gelöschte Einträge in einem eigenen Bereich der Seitenleiste."))
            }

            Section {
                Toggle(prefStr("Website-Icons laden"), isOn: $siteIcons)
            } footer: {
                Text(prefStr("Nur mit UwULock Server: Er holt die Icons der Websites und erfährt dabei, welche Seiten in deinem Tresor sind. Eigene Icons bleiben verschlüsselt und erscheinen immer."))
            }

            #if os(iOS)
            Section {
                Toggle(prefStr("Haptisches Feedback"), isOn: $haptics)
            }
            #endif
        }
        .formStyle(.grouped)
        .navigationTitle(Text(prefStr("Darstellung")))
    }
}
