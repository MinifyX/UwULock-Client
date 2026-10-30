// Placeholder for UwULock's AutoFill extension (passwords and passkeys). Milestone 6 brings the
// real one over from apps/desktop/src-tauri/apple/PasskeyProvider and onto the Rust core.
import AuthenticationServices

final class CredentialProviderViewController: ASCredentialProviderViewController {
    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        extensionContext.cancelRequest(
            withError: NSError(domain: ASExtensionErrorDomain, code: ASExtensionError.Code.failed.rawValue)
        )
    }
}
