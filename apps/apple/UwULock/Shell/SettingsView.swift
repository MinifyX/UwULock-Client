import SwiftUI
import UwUDesign

/// Settings: the Mac's Settings window (tabs), the iPhone's settings tab (a list of pages).
/// Only Appearance is real so far; the other pages arrive with milestone 4.
struct SettingsView: View {
    var body: some View {
        #if os(macOS)
        TabView {
            Tab { AppearanceSettings() } label: { Label("Darstellung", uwuIcon: .appearance) }
            Tab { SettingsPlaceholder() } label: { Label("Sicherheit", uwuIcon: .locked) }
            Tab { SettingsPlaceholder() } label: { Label("Konto", uwuIcon: .account) }
        }
        .frame(width: 520, height: 380)
        #else
        List {
            NavigationLink { AppearanceSettings() } label: { Label("Darstellung", uwuIcon: .appearance) }
            NavigationLink { SettingsPlaceholder() } label: { Label("Sicherheit", uwuIcon: .locked) }
            NavigationLink { SettingsPlaceholder() } label: { Label("Konto", uwuIcon: .account) }
            NavigationLink { SettingsPlaceholder() } label: { Label("Über UwULock", uwuIcon: .info) }
        }
        .navigationTitle(Text("Einstellungen"))
        #endif
    }
}

/// Font (each choice shown in itself, as on the desktop), text size, less motion.
struct AppearanceSettings: View {
    @AppStorage(AppSettingsKey.font) private var font: UwUFontChoice = .uwu
    @AppStorage(AppSettingsKey.textSize) private var textSize: UwUTextSize = .system
    @AppStorage(AppSettingsKey.reduceMotion) private var reduceMotion = false

    var body: some View {
        Form {
            Section {
                Picker("Schrift", selection: $font) {
                    ForEach(UwUFontChoice.allCases) { choice in
                        Text(verbatim: choice.displayName)
                            .font(.uwu(.body, choice: choice))
                            .tag(choice)
                    }
                }
                .pickerStyle(.inline)
            } footer: {
                Text("Nur auf diesem Gerät. UwU Sans ist die Schrift aller UwU-Apps.")
            }
            Section {
                Picker("Textgröße", selection: $textSize) {
                    ForEach(UwUTextSize.allCases) { size in
                        Text(LocalizedStringKey(size.titleKey)).tag(size)
                    }
                }
                Toggle("Weniger Bewegung", isOn: $reduceMotion)
            }
        }
        .formStyle(.grouped)
        .navigationTitle(Text("Darstellung"))
    }
}

struct SettingsPlaceholder: View {
    var body: some View {
        VStack(spacing: UwUSpacing.md) {
            NyuView(mood: .sleepy).frame(width: 90)
            Text("Kommt bald.").uwuFont(.headline)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

#if os(macOS)
/// The menu bar item: lock, open, quit. Quick search and copy come with milestone 4.
struct MenuBarContent: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Button("Sperren") { model.lock() }
            .disabled(model.phase != .unlocked)
        Divider()
        SettingsLink { Text("Einstellungen …") }
        Button("UwULock beenden") { NSApplication.shared.terminate(nil) }
            .keyboardShortcut("q")
    }
}
#endif
