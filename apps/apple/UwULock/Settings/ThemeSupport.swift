import SwiftUI
import UwUDesign
#if os(iOS)
import UIKit
#else
import AppKit
#endif

/// A string from the settings' catalog (Resources/Settings.xcstrings). German is the source.
func prefStr(_ key: String.LocalizationValue) -> String {
    String(localized: key, table: "Settings")
}

/// Settings → Appearance → Farbschema.
enum AppTheme: String, CaseIterable, Identifiable {
    case system, light, dark
    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: prefStr("System")
        case .light: prefStr("Hell")
        case .dark: prefStr("Dunkel")
        }
    }
}

/// Settings → Appearance → Kontrast.
enum AppContrast: String, CaseIterable, Identifiable {
    case system, normal, high
    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: prefStr("System")
        case .normal: prefStr("Normal")
        case .high: prefStr("Hoch")
        }
    }
}

/// Settings → Appearance → Animationen. "An" still gives way to the system's Reduce Motion.
enum AppMotion: String, CaseIterable, Identifiable {
    case system, on, off
    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: prefStr("System")
        case .on: prefStr("An")
        case .off: prefStr("Aus")
        }
    }
}

extension UwUTextSize {
    var title: String {
        switch self {
        case .smaller: prefStr("Kleiner")
        case .system: prefStr("System")
        case .larger: prefStr("Größer")
        case .largest: prefStr("Sehr groß")
        }
    }
}

/// "macOS" / "iOS", for "„System“ folgt der Einstellung von …".
@MainActor let systemName: String = {
    #if os(macOS)
    "macOS"
    #else
    UIDevice.current.userInterfaceIdiom == .pad ? "iPadOS" : "iOS"
    #endif
}()

/// The suite's theme from the user's appearance settings (Settings → Appearance), plus the
/// colour scheme and contrast for the whole app.
private struct Themed: ViewModifier {
    @AppStorage(AppSettingsKey.font) private var font: UwUFontChoice = .uwu
    @AppStorage(AppSettingsKey.textSize) private var textSize: UwUTextSize = .system
    @AppStorage(AppSettingsKey.motion) private var motion: AppMotion = .system
    @AppStorage(AppSettingsKey.theme) private var theme: AppTheme = .system
    @AppStorage(AppSettingsKey.contrast) private var contrast: AppContrast = .system

    func body(content: Content) -> some View {
        content
            .uwuTheme(font: font, textSize: textSize, reduceMotion: motion == .off)
            .background(AppearanceApplier(theme: theme, contrast: contrast))
    }
}

extension View {
    func themed() -> some View { modifier(Themed()) }
}

/// Puts the colour scheme and contrast on every window (iOS) or the app (macOS), so sheets,
/// alerts and the Settings window follow too.
private struct AppearanceApplier: View {
    let theme: AppTheme
    let contrast: AppContrast

    var body: some View {
        Color.clear
            .frame(width: 0, height: 0)
            .accessibilityHidden(true)
            .onAppear(perform: apply)
            .onChange(of: theme) { apply() }
            .onChange(of: contrast) { apply() }
    }

    @MainActor
    private func apply() {
        #if os(iOS)
        let style: UIUserInterfaceStyle = switch theme {
        case .system: .unspecified
        case .light: .light
        case .dark: .dark
        }
        for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
            for window in scene.windows {
                window.overrideUserInterfaceStyle = style
                switch contrast {
                case .system: window.traitOverrides.remove(UITraitAccessibilityContrast.self)
                case .normal: window.traitOverrides.accessibilityContrast = .normal
                case .high: window.traitOverrides.accessibilityContrast = .high
                }
            }
        }
        #else
        let high: Bool? = switch contrast {
        case .system: nil
        case .normal: false
        case .high: true
        }
        let name: NSAppearance.Name? = switch (theme, high) {
        case (.system, nil): nil
        case (.system, .some(let high)):
            // The system's own mode: NSApp's appearance may still be the one set before.
            UserDefaults.standard.string(forKey: "AppleInterfaceStyle") == "Dark"
                ? (high ? .accessibilityHighContrastDarkAqua : .darkAqua)
                : (high ? .accessibilityHighContrastAqua : .aqua)
        case (.light, let high): high == true ? .accessibilityHighContrastAqua : .aqua
        case (.dark, let high): high == true ? .accessibilityHighContrastDarkAqua : .darkAqua
        }
        NSApp.appearance = name.flatMap(NSAppearance.init(named:))
        #endif
    }
}

/// Input anywhere in the app restarts the auto-lock's clock (App.tsx's `touch`).
struct ActivityTracker: View {
    let onActivity: @MainActor () -> Void

    var body: some View {
        Representable(onActivity: onActivity)
            .frame(width: 0, height: 0)
            .accessibilityHidden(true)
    }

    #if os(iOS)
    private struct Representable: UIViewRepresentable {
        let onActivity: @MainActor () -> Void

        func makeUIView(context: Context) -> Probe { Probe(onActivity: onActivity) }
        func updateUIView(_ view: Probe, context: Context) { view.onActivity = onActivity }
    }

    /// Hangs a recognizer on its window that only watches: it never recognizes, never cancels.
    final class Probe: UIView {
        var onActivity: @MainActor () -> Void
        private let watcher = Watcher()

        init(onActivity: @escaping @MainActor () -> Void) {
            self.onActivity = onActivity
            super.init(frame: .zero)
            isUserInteractionEnabled = false
            watcher.onTouch = { [weak self] in self?.onActivity() }
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            watcher.view?.removeGestureRecognizer(watcher)
            window?.addGestureRecognizer(watcher)
        }
    }

    final class Watcher: UIGestureRecognizer {
        var onTouch: (@MainActor () -> Void)?

        override init(target: Any?, action: Selector?) {
            super.init(target: target, action: action)
            cancelsTouchesInView = false
            delaysTouchesBegan = false
            delaysTouchesEnded = false
        }

        convenience init() { self.init(target: nil, action: nil) }

        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
            onTouch?()
            state = .failed
        }

        override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
        override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    }
    #else
    private struct Representable: NSViewRepresentable {
        let onActivity: @MainActor () -> Void

        func makeNSView(context: Context) -> Probe { Probe(onActivity: onActivity) }
        func updateNSView(_ view: Probe, context: Context) { view.onActivity = onActivity }
        static func dismantleNSView(_ view: Probe, coordinator: ()) { view.stop() }
    }

    /// A local event monitor: keys, clicks and scrolling in any of UwULock's windows.
    final class Probe: NSView {
        var onActivity: @MainActor () -> Void
        private var monitor: Any?

        init(onActivity: @escaping @MainActor () -> Void) {
            self.onActivity = onActivity
            super.init(frame: .zero)
            monitor = NSEvent.addLocalMonitorForEvents(
                matching: [.keyDown, .leftMouseDown, .rightMouseDown, .scrollWheel]
            ) { [weak self] event in
                MainActor.assumeIsolated { self?.onActivity() }
                return event
            }
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        func stop() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }
    }
    #endif
}
