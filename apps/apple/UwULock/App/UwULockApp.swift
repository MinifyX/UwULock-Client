import SwiftUI
import UwUDesign
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
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .themed()
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
                .environment(model)
                .themed()
        }

        MenuBarExtra {
            MenuBarContent()
                .environment(model)
        } label: {
            UwUIcon(model.phase == .unlocked ? .unlocked : .locked, size: .md)
        }
        .menuBarExtraStyle(.menu)
        #endif
    }
}

/// App-wide keyboard commands (menu bar on the Mac, the shortcut overlay on the iPad).
struct AppCommands: Commands {
    let model: AppModel

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Button("Sperren") { model.lock() }
                .keyboardShortcut("l", modifiers: [.command, .shift])
                .disabled(model.phase != .unlocked)
        }
        SidebarCommands()
    }
}

/// The suite's theme from the user's appearance settings (Settings → Appearance).
private struct Themed: ViewModifier {
    @AppStorage(AppSettingsKey.font) private var font: UwUFontChoice = .uwu
    @AppStorage(AppSettingsKey.textSize) private var textSize: UwUTextSize = .system
    @AppStorage(AppSettingsKey.reduceMotion) private var reduceMotion = false

    func body(content: Content) -> some View {
        content.uwuTheme(font: font, textSize: textSize, reduceMotion: reduceMotion)
    }
}

extension View {
    func themed() -> some View { modifier(Themed()) }
}

#if DIRECT && canImport(Sparkle)
/// Sparkle, only in the disk image's build. It starts once Info.plist has a feed (SUFeedURL and
/// SUPublicEDKey arrive with the release pipeline, milestone 7).
@MainActor
final class Updater {
    private let controller: SPUStandardUpdaterController

    init() {
        let hasFeed = Bundle.main.object(forInfoDictionaryKey: "SUFeedURL") != nil
        controller = SPUStandardUpdaterController(
            startingUpdater: hasFeed, updaterDelegate: nil, userDriverDelegate: nil
        )
    }

    var menuItem: some View {
        Button("Nach Updates suchen") { [controller] in controller.checkForUpdates(nil) }
            .disabled(Bundle.main.object(forInfoDictionaryKey: "SUFeedURL") == nil)
    }
}
#endif
