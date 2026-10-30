// Placeholder for the Safari web extension's native handler. Milestone 6 brings the real one
// (apps/desktop/src-tauri/apple/SafariExtension) and the web extension's files.
import Foundation
import SafariServices

final class SafariWebExtensionHandler: NSObject, NSExtensionRequestHandling {
    func beginRequest(with context: NSExtensionContext) {
        context.completeRequest(returningItems: [], completionHandler: nil)
    }
}
