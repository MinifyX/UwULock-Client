// The Safari web extension's native side, which Safari wants to exist (moved from
// apps/desktop/src-tauri/apple/SafariExtension). UwULock's web extension is the same as in Chrome
// and Firefox — its own login, unlocking and sync (apps/extension) — and sends nothing to the
// app: no native messaging. A message that arrives anyway gets an empty answer.
//
// The web extension's files (manifest.json, scripts, icons) are copied into the bundle by the
// target's "Web extension files" build phase (project.yml) from apps/extension's Safari build.
import Foundation
import SafariServices

final class SafariWebExtensionHandler: NSObject, NSExtensionRequestHandling {
    func beginRequest(with context: NSExtensionContext) {
        context.completeRequest(returningItems: [], completionHandler: nil)
    }
}
