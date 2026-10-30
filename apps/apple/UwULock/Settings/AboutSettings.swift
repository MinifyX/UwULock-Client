import SwiftUI
import UwUDesign

/// The app's version from Info.plist.
let appVersion: String = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"

/// Settings → Über UwULock: Nyu, the version, the licence, the project's pages.
struct AboutSettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Form {
            Section {
                VStack(spacing: UwUSpacing.sm) {
                    NyuView(mood: .happy).frame(width: 96)
                    UwUWordmark(style: .title, mood: nil)
                    Text(prefStr("Version \(appVersion)"))
                        .uwuFont(.subheadline)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, UwUSpacing.sm)
            } footer: {
                VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                    Text(prefStr("Freie Software unter der GNU GPL v3.0. Nutzen, ändern, weitergeben – nur geänderte Versionen müssen offen bleiben. Kein Tracking, kein Konto."))
                    Text(prefStr("Spricht das Protokoll von Bitwarden und Vaultwarden, mit derselben Verschlüsselung wie die offiziellen Apps. Nicht verbunden mit Bitwarden Inc."))
                }
            }

            Section {
                link(prefStr("Quellcode auf GitHub"), .source, icon: .terminal)
                link(prefStr("Versionen"), .releases, icon: .history)
                link(prefStr("Fehler melden"), .issues, icon: .warning)
                link(prefStr("Lizenz"), .license, icon: .file)
            }

            #if os(iOS)
            Section {
                UpdatesSettingsSection()
            }
            #endif
        }
        .formStyle(.grouped)
        .navigationTitle(Text(prefStr("Über UwULock")))
    }

    private func link(_ title: String, _ page: ProjectPage, icon: UwUIconName) -> some View {
        Button {
            Task { try? await model.core.openProjectPage(page: page) }
        } label: {
            HStack {
                Label(title, uwuIcon: icon)
                Spacer()
                UwUIcon(.openExternal, size: .xs).foregroundStyle(.secondary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// "Neue Versionen": the disk image updates itself (Sparkle, `DIRECT`); the App Store's and the
/// iPhone's builds get theirs from the store.
struct UpdatesSettingsSection: View {
    #if DIRECT
    @AppStorage(AppSettingsKey.updateChannel) private var channel = "stable"
    #endif

    var body: some View {
        #if DIRECT
        Picker(prefStr("Update-Kanal"), selection: $channel) {
            Text(prefStr("Stabil")).tag("stable")
            Text(prefStr("Beta")).tag("beta")
        }
        Text(channel == "beta"
            ? prefStr("Beta bekommt neue Versionen früher. Es kann mal etwas wackeln.")
            : prefStr("Stabil bekommt nur fertige Versionen."))
            .uwuFont(.footnote)
            .foregroundStyle(.secondary)
        LabeledContent(prefStr("Version \(appVersion)")) { EmptyView() }
        Text(prefStr("UwULock lädt neue Versionen still herunter und installiert sie beim nächsten Start. Jedes Update ist signiert und wird vor dem Start geprüft."))
            .uwuFont(.footnote)
            .foregroundStyle(.secondary)
        #else
        LabeledContent(prefStr("Neue Versionen")) { Text(prefStr("Version \(appVersion)")) }
        Text(prefStr("Diese Ausgabe kommt aus dem App Store und bekommt neue Versionen von dort."))
            .uwuFont(.footnote)
            .foregroundStyle(.secondary)
        #endif
    }
}
