#if os(iOS)
import AuthenticationServices
import UIKit

/// iOS: the SSO page in an `ASWebAuthenticationSession` instead of Safari. The core listens on
/// `127.0.0.1` for the provider's redirect (as on the desktop); in Safari the app would sleep and
/// miss it, the in-app session keeps it awake. The redirect lands in the session's page, so its
/// callback never fires: the session closes once the core's step comes back (`finish`).
@MainActor
final class SsoBrowser: NSObject, ASWebAuthenticationPresentationContextProviding {
    static let shared = SsoBrowser()

    /// Set while `AppModel.logInWithSso` waits; `AppleHost.openUrl` shows the page here then.
    private(set) var pending = false
    private var session: ASWebAuthenticationSession?
    private var onCancel: (() -> Void)?

    /// An SSO login starts; `cancelled` runs when the user closes the page.
    func begin(cancelled: @escaping () -> Void) {
        finish()
        pending = true
        onCancel = cancelled
    }

    /// Shows the provider's page. False when no login waits for it (the system browser then).
    func open(_ url: URL) -> Bool {
        guard pending else { return false }
        session?.cancel()
        let session = ASWebAuthenticationSession(
            url: url,
            // Never reached: the redirect goes to the core's loopback listener.
            callback: .customScheme("uwulock-sso")
        ) { [weak self] _, error in
            Task { @MainActor in
                guard let self, let error = error as? ASWebAuthenticationSessionError,
                    error.code == .canceledLogin, self.pending
                else { return }
                let cancelled = self.onCancel
                self.finish()
                cancelled?()
            }
        }
        session.presentationContextProvider = self
        session.prefersEphemeralWebBrowserSession = false
        self.session = session
        return session.start()
    }

    /// The step came back (or failed): the page goes.
    func finish() {
        pending = false
        onCancel = nil
        let session = self.session
        self.session = nil
        session?.cancel()
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            let windows = scenes.flatMap(\.windows)
            if let window = windows.first(where: \.isKeyWindow) ?? windows.first { return window }
            // No scene at all: a bare window, and the session fails instead of the app.
            return scenes.first.map { ASPresentationAnchor(windowScene: $0) } ?? ASPresentationAnchor()
        }
    }
}
#endif
