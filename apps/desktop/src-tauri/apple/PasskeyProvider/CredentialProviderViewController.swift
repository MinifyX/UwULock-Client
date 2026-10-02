// UwULock's AutoFill extension for passkeys (iOS 17+, macOS 14+).
//
// The system starts it when a site or app asks for a passkey and the person picks UwULock. It
// reads the sealed list (PasskeyVault) after Face ID / Touch ID / the passcode, signs with the
// passkey picked, or makes a new one and leaves it in the outbox for the app.
//
// Its own process, without the app's open vault: it never sees the master password or the user
// key, only the provider key and the passkeys in the list.

import AuthenticationServices
import CryptoKit
import SwiftUI

#if os(iOS)
  typealias HostingController<V: View> = UIHostingController<V>
#else
  typealias HostingController<V: View> = NSHostingController<V>
#endif

/// German when the system prefers it, English otherwise.
func tr(_ german: String, _ english: String) -> String {
  Locale.preferredLanguages.first?.hasPrefix("de") == true ? german : english
}

final class PasskeyModel: ObservableObject {
  struct Choice: Identifiable {
    let id: String
    let title: String
    let subtitle: String
  }

  @Published var title = "UwULock"
  @Published var message: String?
  @Published var choices: [Choice] = []
  @Published var busy = true
  var pick: (Choice) -> Void = { _ in }
  var cancel: () -> Void = {}
}

struct PasskeyView: View {
  @ObservedObject var model: PasskeyModel

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text(model.title).font(.headline)
      if let message = model.message {
        Text(message).font(.subheadline).foregroundColor(.secondary)
      }
      if model.busy {
        ProgressView()
      }
      ForEach(model.choices) { choice in
        Button {
          model.pick(choice)
        } label: {
          VStack(alignment: .leading) {
            Text(choice.title)
            Text(choice.subtitle).font(.caption).foregroundColor(.secondary)
          }
          .frame(maxWidth: .infinity, alignment: .leading)
        }
      }
      Spacer()
      Button(tr("Abbrechen", "Cancel"), role: .cancel) { model.cancel() }
    }
    .padding()
    .frame(minWidth: 320, minHeight: 240)
  }
}

class CredentialProviderViewController: ASCredentialProviderViewController {
  private let model = PasskeyModel()
  private var shown = false

  #if os(macOS)
    // No nib: the view is made here.
    override func loadView() {
      view = NSView(frame: NSRect(x: 0, y: 0, width: 420, height: 320))
    }
  #endif

  private func show() {
    guard !shown else { return }
    shown = true
    model.cancel = { [weak self] in self?.cancel(.userCanceled) }
    let host = HostingController(rootView: PasskeyView(model: model))
    addChild(host)
    host.view.frame = view.bounds
    #if os(iOS)
      host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    #else
      host.view.autoresizingMask = [.width, .height]
    #endif
    view.addSubview(host.view)
  }

  private func cancel(_ code: ASExtensionError.Code) {
    extensionContext.cancelRequest(
      withError: NSError(domain: ASExtensionErrorDomain, code: code.rawValue))
  }

  /// Shows why it stopped, then gives up when the person closes it.
  private func fail(_ error: Error) {
    DispatchQueue.main.async {
      if case PasskeyVaultError.cancelled = error {
        self.cancel(.userCanceled)
        return
      }
      self.show()
      self.model.busy = false
      self.model.choices = []
      switch error {
      case PasskeyVaultError.notSetUp:
        self.model.message = tr(
          "Diese UwULock-Version ist nicht mit einem Apple-Entwicklerkonto signiert; die Passkey-Erweiterung kann den Tresor nicht erreichen.",
          "This UwULock build isn't signed with an Apple developer account; the passkey extension can't reach the vault.")
      case PasskeyVaultError.noList:
        self.model.message = tr(
          "Öffne UwULock einmal und entsperre den Tresor, mit eingeschalteten Passkeys für andere Apps.",
          "Open UwULock once and unlock the vault, with passkeys for other apps switched on.")
      default:
        self.model.message = tr("Das ging nicht: ", "That didn't work: ") + "\(error)"
      }
    }
  }

  private func background(_ work: @escaping () throws -> Void) {
    DispatchQueue.global(qos: .userInitiated).async {
      do { try work() } catch { self.fail(error) }
    }
  }

  /// Keeps the system's list of passkeys (QuickType bar, the sheet) in step with the list.
  private func updateIdentities(_ entries: [PasskeyEntry]) {
    let identities: [ASCredentialIdentity] = entries.compactMap { entry in
      guard let id = Base64URL.decode(entry.credentialId) else { return nil }
      return ASPasskeyCredentialIdentity(
        relyingPartyIdentifier: entry.rpId, userName: entry.title, credentialID: id,
        userHandle: entry.userHandle.flatMap(Base64URL.decode) ?? Data(),
        recordIdentifier: entry.itemId)
    }
    ASCredentialIdentityStore.shared.replaceCredentialIdentities(identities) { _, _ in }
  }

  private func flags(_ verification: ASAuthorizationPublicKeyCredentialUserVerificationPreference)
    -> UInt8
  {
    // Opening the provider key took Face ID, Touch ID or the passcode: verified either way.
    PasskeyVault.up | PasskeyVault.uv | PasskeyVault.be | PasskeyVault.bs
  }

  private func answer(
    _ entry: PasskeyEntry, rpId: String, clientDataHash: Data,
    verification: ASAuthorizationPublicKeyCredentialUserVerificationPreference
  ) throws {
    guard let id = Base64URL.decode(entry.credentialId) else {
      throw PasskeyVaultError.broken("a credential id doesn't decode")
    }
    let authData = PasskeyVault.authenticatorData(rpId: rpId, flags: flags(verification))
    let signature = try PasskeyVault.sign(entry, authData: authData, clientDataHash: clientDataHash)
    let credential = ASPasskeyAssertionCredential(
      userHandle: entry.userHandle.flatMap(Base64URL.decode) ?? Data(), relyingParty: rpId,
      signature: signature, clientDataHash: clientDataHash, authenticatorData: authData,
      credentialID: id)
    DispatchQueue.main.async {
      self.extensionContext.completeAssertionRequest(using: credential, completionHandler: nil)
    }
  }

  // MARK: Signing in

  override func provideCredentialWithoutUserInteraction(for credentialRequest: ASCredentialRequest) {
    // The provider key always wants Face ID, Touch ID or the passcode.
    cancel(.userInteractionRequired)
  }

  /// The person picked one of UwULock's passkeys in the system's list.
  override func prepareInterfaceToProvideCredential(for credentialRequest: ASCredentialRequest) {
    guard let request = credentialRequest as? ASPasskeyCredentialRequest,
      let identity = request.credentialIdentity as? ASPasskeyCredentialIdentity
    else {
      cancel(.credentialIdentityNotFound)
      return
    }
    show()
    model.message = tr("Anmelden bei ", "Signing in to ") + identity.relyingPartyIdentifier
    background {
      let vault = try PasskeyVault()
      let key = try vault.key(
        reason: tr("Mit UwULock anmelden bei ", "Sign in with UwULock to ")
          + identity.relyingPartyIdentifier)
      let list = try vault.snapshot(key: key)
      self.updateIdentities(list.entries)
      guard
        let entry = list.entries.first(where: {
          Base64URL.decode($0.credentialId) == identity.credentialID
            && $0.rpId == identity.relyingPartyIdentifier
        })
      else {
        DispatchQueue.main.async { self.cancel(.credentialIdentityNotFound) }
        return
      }
      try self.answer(
        entry, rpId: identity.relyingPartyIdentifier, clientDataHash: request.clientDataHash,
        verification: request.userVerificationPreference)
    }
  }

  /// The person picked UwULock without a passkey from the system's list: UwULock shows its own.
  override func prepareCredentialList(
    for serviceIdentifiers: [ASCredentialServiceIdentifier],
    requestParameters: ASPasskeyCredentialRequestParameters
  ) {
    let rpId = requestParameters.relyingPartyIdentifier
    show()
    model.title = tr("Passkey für ", "Passkey for ") + rpId
    background {
      let vault = try PasskeyVault()
      let key = try vault.key(
        reason: tr("Mit UwULock anmelden bei ", "Sign in with UwULock to ") + rpId)
      let list = try vault.snapshot(key: key)
      self.updateIdentities(list.entries)
      let allowed = requestParameters.allowedCredentials
      let found = list.entries.filter { entry in
        entry.rpId == rpId
          && (allowed.isEmpty
            || Base64URL.decode(entry.credentialId).map { allowed.contains($0) } == true)
      }
      if found.count == 1 {
        try self.answer(
          found[0], rpId: rpId, clientDataHash: requestParameters.clientDataHash,
          verification: requestParameters.userVerificationPreference)
        return
      }
      DispatchQueue.main.async {
        self.model.busy = false
        if found.isEmpty {
          self.model.message = tr(
            "Im Tresor ist kein Passkey für diese Seite.", "There's no passkey for this site in the vault.")
          return
        }
        self.model.choices = found.map {
          PasskeyModel.Choice(
            id: $0.credentialId, title: $0.title, subtitle: $0.userDisplayName ?? $0.rpId)
        }
        self.model.pick = { choice in
          guard let entry = found.first(where: { $0.credentialId == choice.id }) else { return }
          self.model.busy = true
          self.background {
            try self.answer(
              entry, rpId: rpId, clientDataHash: requestParameters.clientDataHash,
              verification: requestParameters.userVerificationPreference)
          }
        }
      }
    }
  }

  // MARK: Making a passkey

  override func prepareInterface(forPasskeyRegistration registrationRequest: ASCredentialRequest) {
    guard let request = registrationRequest as? ASPasskeyCredentialRequest,
      let identity = request.credentialIdentity as? ASPasskeyCredentialIdentity
    else {
      cancel(.failed)
      return
    }
    let rpId = identity.relyingPartyIdentifier
    show()
    model.title = tr("Passkey sichern", "Save passkey")
    model.message = rpId + " · " + identity.userName
    guard request.supportedAlgorithms.isEmpty || request.supportedAlgorithms.contains(.ES256)
    else {
      fail(PasskeyVaultError.broken("the site takes no ES256 passkeys"))
      return
    }
    background {
      let vault = try PasskeyVault()
      let key = try vault.key(
        reason: tr("Passkey für ", "Save a passkey for ") + rpId
          + tr(" in UwULock sichern", " in UwULock"))
      let made = PasskeyVault.make(
        rpId: rpId, userName: identity.userName, userHandle: identity.userHandle)
      try vault.keep(made.entry, key: key)
      if let list = try? vault.snapshot(key: key) { self.updateIdentities(list.entries) }
      let authData = PasskeyVault.authenticatorData(
        rpId: rpId, flags: self.flags(request.userVerificationPreference), credentialId: made.id,
        publicKey: made.key.publicKey)
      let credential = ASPasskeyRegistrationCredential(
        relyingParty: rpId, clientDataHash: request.clientDataHash, credentialID: made.id,
        attestationObject: PasskeyVault.attestationObject(authData))
      DispatchQueue.main.async {
        self.extensionContext.completeRegistrationRequest(using: credential, completionHandler: nil)
      }
    }
  }

  // MARK: Switched on in the settings

  /// Shown once the person picks UwULock for passwords and passkeys: fills the system's list.
  override func prepareInterfaceForExtensionConfiguration() {
    show()
    model.title = tr("UwULock-Passkeys", "UwULock passkeys")
    background {
      let vault = try PasskeyVault()
      let key = try vault.key(
        reason: tr("UwULocks Passkeys für das System freigeben", "Let the system list UwULock's passkeys"))
      let list = try vault.snapshot(key: key)
      self.updateIdentities(list.entries)
      DispatchQueue.main.async {
        self.extensionContext.completeExtensionConfigurationRequest()
      }
    }
  }
}
