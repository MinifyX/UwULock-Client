#if os(macOS)
import SwiftUI
import UwUDesign

/// The menu bar item: the vault's state, lock, open the window, settings, quit.
struct MenuBarContent: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Text(stateText)
        if let email = model.status?.email {
            Text(verbatim: email)
        }
        Divider()
        Button(prefStr("Sperren")) { model.lock() }
            .disabled(model.phase != .unlocked)
        Button(prefStr("UwULock öffnen")) { open() }
        Divider()
        SettingsLink { Text(prefStr("Einstellungen …")) }
        Button(prefStr("UwULock beenden")) { NSApplication.shared.terminate(nil) }
            .keyboardShortcut("q")
    }

    private var stateText: String {
        switch model.phase {
        case .unlocked: prefStr("Tresor entsperrt")
        case .locked: prefStr("Tresor gesperrt")
        default: prefStr("Nicht angemeldet")
        }
    }

    /// Brings the main window back, or opens a new one when it was closed.
    private func open() {
        NSApp.activate()
        if let window = NSApp.windows.first(where: { $0.identifier?.rawValue.hasPrefix("main") == true && $0.canBecomeMain }) {
            window.makeKeyAndOrderFront(nil)
        } else {
            openWindow(id: "main")
        }
    }
}
#endif
