import Foundation

#if canImport(SafariServices)
  import SafariServices
#endif

/// The Safari extension's native side, which Safari wants to exist. UwULock's extension is the
/// same as in Chrome and Firefox — its own login, unlocking and sync (apps/extension) — and
/// sends nothing to the app: no native messaging. A message that arrives anyway gets an empty
/// answer.
final class SafariWebExtensionHandler: NSObject, NSExtensionRequestHandling {
  func beginRequest(with context: NSExtensionContext) {
    context.completeRequest(returningItems: [], completionHandler: nil)
  }
}
