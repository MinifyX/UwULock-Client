import SwiftUI
import UwUDesign
#if os(iOS)
import AuthenticationServices
#endif

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
        // As the Tauri app's Suspended / Resumed: the auto-lock's clock, live updates, a sync.
        .onChange(of: scenePhase) { old, new in
            if new == .background {
                model.core.suspended()
            } else if old == .background, new != .background {
                model.core.resumed()
            }
        }
        // Apple Passwords hands credentials over (iOS 26): kept until they're taken in.
        .onContinueUserActivity(ASCredentialExchangeActivity) { activity in
            #if RUST_CORE && canImport(UwuLockFFI)
            CredentialExchange.receive(activity)
            #endif
        }
        #endif
    }
}

/// iPhone: tabs. iPad and Mac: three columns. Owns the vault on screen while it's open.
struct VaultShell: View {
    @Environment(AppModel.self) private var model
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    #endif
    @State private var vault: VaultModel?

    var body: some View {
        Group {
            if let vault {
                layout.environment(vault)
            } else {
                Color.clear
            }
        }
        .task {
            let vault = VaultModel(backend: Self.backend(model.core))
            self.vault = vault
            await vault.run(status: model.status)
        }
    }

    @ViewBuilder
    private var layout: some View {
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

    private static func backend(_ core: Core) -> VaultBackend {
        #if DEBUG
        if FixtureVaultBackend.requested { return FixtureVaultBackend() }
        #endif
        return core
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
