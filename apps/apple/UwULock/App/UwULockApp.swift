import SwiftUI
import UwUDesign
#if os(macOS)
import AppKit
#endif
#if DIRECT && canImport(Sparkle)
import Sparkle
#endif

@main
struct UwULockApp: App {
    @State private var model = AppModel()
    #if DIRECT && canImport(Sparkle)
    private let updater = Updater()
    #endif

    init() {
        UwUFonts.register()
        #if os(macOS)
        // One vault window: no tabs (and ⌘T copies a code, not a new tab).
        NSWindow.allowsAutomaticWindowTabbing = false
        #endif
    }

    var body: some Scene {
        WindowGroup(id: "main") {
            RootView()
                .modifier(AuthRequestAlert())
                .background(ActivityTracker { model.noteActivity() })
                .themed()
                .environment(model)
        }
        .commands {
            AppCommands(model: model)
            #if DIRECT && canImport(Sparkle)
            CommandGroup(after: .appInfo) { updater.menuItem }
            #endif
        }
        #if os(macOS)
        .defaultSize(width: 1100, height: 720)
        .windowToolbarStyle(.unified)
        #endif

        #if os(macOS)
        Settings {
            SettingsView()
                .themed()
                .environment(model)
        }

        MenuBarExtra {
            MenuBarContent()
                .themed()
                .environment(model)
        } label: {
            UwUIcon(model.phase == .unlocked ? .unlocked : .locked, size: .md)
        }
        .menuBarExtraStyle(.menu)
        #endif
    }
}

/// App-wide keyboard commands (menu bar on the Mac, the shortcut overlay on the iPad), as App.tsx's
/// menu: "Tresor" with the generator, import and lock; ⌘U, ⌘P, ⌘T copy the selected login's
/// username, password and code (VaultScreen.tsx, as in Bitwarden's desktop app).
struct AppCommands: Commands {
    let model: AppModel
    @FocusedValue(\.vaultModel) private var vault

    var body: some Commands {
        // ⌘P copies the password: there's nothing to print.
        CommandGroup(replacing: .printItem) {}
        CommandGroup(after: .pasteboard) {
            Section {
                Button(vt("Benutzername kopieren")) { copy("username") }
                    .keyboardShortcut("u", modifiers: .command)
                Button(vt("Passwort kopieren")) { copy("password") }
                    .keyboardShortcut("p", modifiers: .command)
                Button(vt("Code kopieren")) { copy("totp") }
                    .keyboardShortcut("t", modifiers: .command)
            }
            .disabled(selectedLogin == nil)
        }
        CommandMenu(vt("Tresor")) {
            Button(vt("Passwort-Generator …")) { vault?.showGenerator = true }
                .keyboardShortcut("g", modifiers: .command)
                .disabled(vault == nil)
            Button(vt("Importieren …")) { vault?.showImport = true }
                .disabled(vault == nil)
            Divider()
            Button(prefStr("Sperren")) { model.lock() }
                .keyboardShortcut("l", modifiers: .command)
                .disabled(model.phase != .unlocked)
        }
        SidebarCommands()
    }

    private var selectedLogin: ItemSummary? {
        guard let vault, let item = vault.item(vault.selectedItemId), item.kind == .login, !item.deleted
        else { return nil }
        return item
    }

    private func copy(_ field: String) {
        guard let vault, let item = selectedLogin else { return }
        vault.copy(item.id, field: field)
    }
}

#if DIRECT && canImport(Sparkle)
/// Sparkle, only in the disk image's build. It starts once Info.plist has a feed (SUFeedURL and
/// SUPublicEDKey, Config/Direct.xcconfig); the other builds leave the key empty.
@MainActor
final class Updater {
    private let controller: SPUStandardUpdaterController
    private let channel = ChannelFeed()

    /// Info.plist's feed: Stable's appcast, or nil when the build has none.
    nonisolated static var stableFeed: String? {
        (Bundle.main.object(forInfoDictionaryKey: "SUFeedURL") as? String).flatMap { $0.isEmpty ? nil : $0 }
    }

    init() {
        controller = SPUStandardUpdaterController(
            startingUpdater: Self.stableFeed != nil, updaterDelegate: channel, userDriverDelegate: nil
        )
    }

    var menuItem: some View {
        Button("Nach Updates suchen") { [controller] in controller.checkForUpdates(nil) }
            .disabled(Self.stableFeed == nil)
    }
}

/// The update channel of the settings (AboutSettings): Beta reads appcast-beta.xml next to
/// Stable's, as the Tauri builds read beta.json.
private final class ChannelFeed: NSObject, SPUUpdaterDelegate {
    func feedURLString(for updater: SPUUpdater) -> String? {
        guard UserDefaults.standard.string(forKey: AppSettingsKey.updateChannel) == "beta",
              let stable = Updater.stableFeed
        else { return nil }
        return stable.replacingOccurrences(of: "appcast-stable.xml", with: "appcast-beta.xml")
    }
}
#endif
