import SwiftUI
import UwUDesign

/// Login, lock screen or vault, and the privacy cover over all of them.
struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.uwuReduceMotion) private var appReduceMotion
    @Environment(\.accessibilityReduceMotion) private var systemReduceMotion

    var body: some View {
        ZStack {
            switch model.phase {
            case .login: LoginView().transition(.opacity)
            case .locked: LockView().transition(.opacity)
            case .unlocked: VaultShell().transition(.opacity)
            }
        }
        .uwuAnimation(value: model.phase)
        #if os(iOS)
        // The app switcher's snapshot never shows the vault.
        .overlay {
            if scenePhase != .active {
                PrivacyCover().transition(.opacity)
            }
        }
        #endif
    }
}

/// iPhone: tabs. iPad and Mac: three columns.
struct VaultShell: View {
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    var body: some View {
        #if os(iOS)
        if sizeClass == .compact {
            PhoneTabs()
        } else {
            VaultSplitView()
        }
        #else
        VaultSplitView()
        #endif
    }
}

/// What the app switcher (and a screen recording of it) sees while UwULock isn't active.
struct PrivacyCover: View {
    var body: some View {
        ZStack {
            Rectangle().fill(.uwuCanvas).ignoresSafeArea()
            VStack(spacing: UwUSpacing.lg) {
                NyuView(mood: .sleepy).frame(width: 120)
                UwUWordmark(style: .title, mood: nil)
            }
        }
        .accessibilityHidden(true)
    }
}
