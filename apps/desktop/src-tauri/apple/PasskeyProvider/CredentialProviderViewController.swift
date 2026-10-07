// UwULock's AutoFill extension for passwords and passkeys (iOS 17+, macOS 14+).
//
// The system starts it when a site or app asks for a password or a passkey and the person picks
// UwULock. It reads the sealed list (PasskeyVault) after Face ID / Touch ID / the passcode, fills
// the login or signs with the passkey picked, or makes a new passkey and leaves it in the outbox
// for the app.
//
// Its own process, without the app's open vault: it never sees the master password or the user
// key, only the provider key and what is in the list.
//
// Both ways to a passkey end in the same `answer(...)`: picked directly in the system's sheet or
// the QuickType bar (`prepareInterfaceToProvideCredential`), or from UwULock's own list
// (`prepareCredentialList`). Each step is logged (subsystem app.uwulock.passkeys, no secrets),
// so a device's log says where a sign-in went wrong:
//   log stream --predicate 'subsystem == "app.uwulock.passkeys"' --info

import AuthenticationServices
import CryptoKit
import LocalAuthentication
import SwiftUI
import os

#if os(iOS)
  typealias HostingController<V: View> = UIHostingController<V>
#else
  typealias HostingController<V: View> = NSHostingController<V>
#endif

/// German when the system prefers it, English otherwise.
func tr(_ german: String, _ english: String) -> String {
  Locale.preferredLanguages.first?.hasPrefix("de") == true ? german : english
}

let providerLog = Logger(subsystem: "app.uwulock.passkeys", category: "provider")

/// The first characters of an id, for the log: enough to tell two apart, nothing to use.
func short(_ data: Data?) -> String {
  guard let data, !data.isEmpty else { return "-" }
  return String(Base64URL.encode(data).prefix(6)) + "…(\(data.count))"
}

final class PasskeyModel: ObservableObject {
  struct Choice: Identifiable {
    let id: String
    let title: String
    let subtitle: String
    /// Matches the page or app: listed first, and all that is shown until the person searches.
    let suggested: Bool
  }

  @Published var title = "UwULock"
  @Published var message: String?
  @Published var choices: [Choice] = []
  @Published var busy = true
  @Published var query = ""
  /// Passwords: a search over all logins, not only the suggested ones.
  @Published var searchable = false
  var pick: (Choice) -> Void = { _ in }
  var cancel: () -> Void = {}

  var shown: [Choice] {
    let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
    if needle.isEmpty { return searchable ? choices.filter(\.suggested) : choices }
    return choices.filter {
      $0.title.lowercased().contains(needle) || $0.subtitle.lowercased().contains(needle)
    }
  }
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
      if model.searchable && !model.busy {
        TextField(tr("Suchen", "Search"), text: $model.query)
          .textFieldStyle(.roundedBorder)
          .disableAutocorrection(true)
      }
      ScrollView {
        VStack(alignment: .leading, spacing: 10) {
          ForEach(model.shown) { choice in
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
          if model.searchable && !model.busy && model.shown.isEmpty {
            Text(
              model.query.isEmpty
                ? tr(
                  "Kein Login passt zu dieser Seite. Suche nach einem anderen.",
                  "No login fits this site. Search for another one.")
                : tr("Nichts gefunden.", "Nothing found.")
            )
            .font(.subheadline).foregroundColor(.secondary)
          }
        }
      }
      Button(tr("Abbrechen", "Cancel"), role: .cancel) { model.cancel() }
    }
    .padding()
    .frame(minWidth: 320, minHeight: 240)
  }
}

class CredentialProviderViewController: ASCredentialProviderViewController {
  private let model = PasskeyModel()
  private var shown = false
  /// The sheet is on screen: only then may Face ID, Touch ID or the passcode be asked for. Asked
  /// earlier (the system calls prepareInterfaceToProvideCredential before it presents the sheet),
  /// iOS refuses with errSecInteractionNotAllowed (-25308) — the direct pick's "broken(Keychain
  /// -25308)".
  private var appeared = false
  private var waiting: [() -> Void] = []

  #if os(iOS)
    override func viewDidAppear(_ animated: Bool) {
      super.viewDidAppear(animated)
      sheetAppeared()
    }
  #else
    override func viewDidAppear() {
      super.viewDidAppear()
      sheetAppeared()
    }
  #endif

  private func sheetAppeared() {
    guard !appeared else { return }
    appeared = true
    providerLog.info("sheet on screen")
    let work = waiting
    waiting = []
    work.forEach { $0() }
  }

  /// Runs `work` on the main thread once the sheet is on screen. Should the system never say so,
  /// it runs after a moment anyway (and a refusal is tried once more).
  private func whenOnScreen(_ work: @escaping () -> Void) {
    onMain {
      if self.appeared {
        work()
        return
      }
      self.waiting.append(work)
      DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
        guard !self.appeared, !self.waiting.isEmpty else { return }
        providerLog.info("sheet not reported on screen; going on")
        self.sheetAppeared()
      }
    }
  }

  #if os(macOS)
    // No nib: the view is made here.
    override func loadView() {
      view = NSView(frame: NSRect(x: 0, y: 0, width: 420, height: 420))
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

  /// Every answer to the system goes from the main thread.
  private func onMain(_ work: @escaping () -> Void) {
    if Thread.isMainThread { work() } else { DispatchQueue.main.async(execute: work) }
  }

  private func cancel(_ code: ASExtensionError.Code) {
    providerLog.info("cancel: code \(code.rawValue, privacy: .public)")
    onMain {
      self.extensionContext.cancelRequest(
        withError: NSError(domain: ASExtensionErrorDomain, code: code.rawValue))
    }
  }

  /// Shows why it stopped, then gives up when the person closes it.
  private func fail(_ error: Error) {
    providerLog.error("failed: \(String(describing: error), privacy: .public)")
    onMain {
      if case PasskeyVaultError.cancelled = error {
        self.cancel(.userCanceled)
        return
      }
      self.show()
      self.model.busy = false
      self.model.choices = []
      self.model.searchable = false
      switch error {
      case PasskeyVaultError.notSetUp:
        self.model.message = tr(
          "Diese UwULock-Version ist nicht mit einem Apple-Entwicklerkonto signiert; die AutoFill-Erweiterung kann den Tresor nicht erreichen.",
          "This UwULock build isn't signed with an Apple developer account; the AutoFill extension can't reach the vault.")
      case PasskeyVaultError.noList:
        // Switched off or logged out: the system's list goes too.
        ASCredentialIdentityStore.shared.removeAllCredentialIdentities { _, _ in }
        self.model.message = tr(
          "Öffne UwULock einmal und entsperre den Tresor, mit eingeschaltetem AutoFill für andere Apps.",
          "Open UwULock once and unlock the vault, with AutoFill for other apps switched on.")
      case PasskeyVaultError.stale:
        self.model.message = tr(
          "Die Liste ist älter als eine, die schon hier war. Öffne UwULock und entsperre den Tresor.",
          "The list is older than one seen here before. Open UwULock and unlock the vault.")
      case PasskeyVaultError.notInteractive:
        self.model.message = tr(
          "Das System hat die Abfrage von Face ID, Touch ID oder Code gerade nicht zugelassen. Bitte versuche es noch einmal.",
          "The system didn't allow asking for Face ID, Touch ID or the passcode just now. Please try again.")
      case PasskeyVaultError.noPasscode:
        self.model.message = tr(
          "UwULock braucht einen Gerätecode (und Face ID oder Touch ID, falls gesperrt: einmal mit dem Code entsperren).",
          "UwULock needs a device passcode (and if Face ID or Touch ID is locked out, unlock once with the passcode).")
      case PasskeyVaultError.broken(let why):
        self.model.message =
          tr(
            "Das ging nicht. Öffne UwULock und entsperre den Tresor, dann versuche es noch einmal.",
            "That didn't work. Open UwULock and unlock the vault, then try again.")
          + " (\(why))"
      default:
        self.model.message = tr(
          "Das ging nicht. Öffne UwULock und entsperre den Tresor, dann versuche es noch einmal.",
          "That didn't work. Open UwULock and unlock the vault, then try again.")
      }
    }
  }

  private func background(_ work: @escaping () throws -> Void) {
    DispatchQueue.global(qos: .userInitiated).async {
      do { try work() } catch { self.fail(error) }
    }
  }

  /// The provider key and the list, after Face ID, Touch ID or the passcode — asked for
  /// explicitly, from the sheet once it is on screen — then `then` on a background queue. A
  /// refusal for asking at the wrong moment is tried once more.
  private func unlock(
    reason: String, retried: Bool = false,
    then: @escaping (_ vault: PasskeyVault, _ key: SymmetricKey, _ list: PasskeyList) throws -> Void
  ) {
    let vault: PasskeyVault
    do {
      vault = try PasskeyVault()
    } catch {
      fail(error)
      return
    }
    whenOnScreen {
      let context = LAContext()
      context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { ok, error in
        if !ok {
          let code = (error as? LAError)?.code
          providerLog.info("verification: \(code.map { String($0.rawValue) } ?? "-", privacy: .public)")
          switch code {
          case .userCancel?, .appCancel?, .systemCancel?, .userFallback?:
            self.fail(PasskeyVaultError.cancelled)
          case .notInteractive?:
            self.retry(reason: reason, retried: retried, then: then)
          case .passcodeNotSet?, .biometryLockout?:
            self.fail(PasskeyVaultError.noPasscode)
          default:
            self.fail(PasskeyVaultError.broken("verification \(code.map { String($0.rawValue) } ?? "-")"))
          }
          return
        }
        self.background {
          defer { context.invalidate() }
          let key: SymmetricKey
          do {
            key = try vault.key(context: context)
          } catch PasskeyVaultError.notInteractive {
            self.retry(reason: reason, retried: retried, then: then)
            return
          }
          let list = try vault.list(key: key)
          providerLog.info(
            "list opened: \(list.snapshot.entries.count, privacy: .public) passkeys, \(list.snapshot.logins?.count ?? 0, privacy: .public) logins, generation \(list.snapshot.generation, privacy: .public)"
          )
          try then(vault, key, list)
        }
      }
    }
  }

  private func retry(
    reason: String, retried: Bool,
    then: @escaping (PasskeyVault, SymmetricKey, PasskeyList) throws -> Void
  ) {
    guard !retried else {
      fail(PasskeyVaultError.notInteractive)
      return
    }
    providerLog.info("not interactive yet: once more")
    onMain {
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
        self.unlock(reason: reason, retried: true, then: then)
      }
    }
  }

  // MARK: The system's list (QuickType bar, the sheet)

  /// What the system lists of a list: the same as the app hands over on iOS
  /// (passkeys/apple.rs `Identity`).
  static func identities(of snapshot: PasskeySnapshot) -> [ASCredentialIdentity] {
    var out: [ASCredentialIdentity] = snapshot.entries.compactMap { entry in
      guard let id = Base64URL.decode(entry.credentialId) else { return nil }
      return ASPasskeyCredentialIdentity(
        relyingPartyIdentifier: entry.rpId,
        userName: entry.userName ?? entry.userDisplayName ?? entry.rpId, credentialID: id,
        userHandle: entry.userHandle.flatMap(Base64URL.decode) ?? Data(),
        recordIdentifier: entry.itemId)
    }
    for login in snapshot.logins ?? [] {
      var seen = Set<String>()
      for hint in login.uris ?? [] where seen.insert(hint.value).inserted {
        let type: ASCredentialServiceIdentifier.IdentifierType
        switch hint.kind {
        case "domain", "host": type = .domain
        case "startsWith", "exact": type = .URL
        default: continue
        }
        out.append(
          ASPasswordCredentialIdentity(
            serviceIdentifier: ASCredentialServiceIdentifier(identifier: hint.value, type: type),
            user: login.userName ?? "", recordIdentifier: login.itemId))
      }
    }
    return out
  }

  /// Keeps the system's list in step with the list. Never while the system answers a pick from
  /// that list (prepareInterfaceToProvideCredential): replacing the entries under a request made
  /// from one of them is asking for trouble. On iOS the app keeps it current anyway; on macOS
  /// this is the only place it is filled.
  private func updateIdentities(_ snapshot: PasskeySnapshot) {
    let identities = Self.identities(of: snapshot)
    ASCredentialIdentityStore.shared.getState { state in
      guard state.isEnabled else { return }
      ASCredentialIdentityStore.shared.replaceCredentialIdentities(identities) { done, error in
        providerLog.info(
          "identities replaced: \(identities.count, privacy: .public), ok \(done, privacy: .public) \(error.map { String(describing: $0) } ?? "", privacy: .public)"
        )
      }
    }
  }

  // MARK: Signing in with a passkey

  private func flags() -> UInt8 {
    // Opening the provider key took Face ID, Touch ID or the passcode: verified either way.
    // Backup eligible and backed up: the passkey is synced through the vault.
    PasskeyVault.up | PasskeyVault.uv | PasskeyVault.be | PasskeyVault.bs
  }

  /// Signs with `entry` for `rpId` and hands the assertion to the system. Both ways to a passkey
  /// end here, so they answer alike.
  private func answer(
    _ entry: PasskeyListEntry, list: PasskeyList, key: SymmetricKey, rpId: String,
    clientDataHash: Data, path: String
  ) throws {
    guard let id = Base64URL.decode(entry.credentialId), !id.isEmpty else {
      throw PasskeyVaultError.broken("a credential id doesn't decode")
    }
    guard clientDataHash.count == 32 else {
      throw PasskeyVaultError.broken("the system's client data hash isn't a SHA-256")
    }
    let userHandle = entry.userHandle.flatMap(Base64URL.decode) ?? Data()
    let authData = PasskeyVault.authenticatorData(rpId: rpId, flags: flags())
    // Only this passkey's private key is opened.
    let signingKey = try PasskeyVault.signingKey(entry, key: key, account: list.account)
    let signature = try PasskeyVault.sign(
      signingKey, authData: authData, clientDataHash: clientDataHash)
    // The signature has to verify under the key the site keeps: checked here, so a broken key
    // shows up in the log rather than as the site's refusal.
    let verifies =
      (try? P256.Signing.ECDSASignature(derRepresentation: signature)).map {
        signingKey.publicKey.isValidSignature($0, for: authData + clientDataHash)
      } ?? false
    providerLog.info(
      "assertion (\(path, privacy: .public)): rp \(rpId, privacy: .public), entry rp \(entry.rpId, privacy: .public), credential \(short(id), privacy: .public), user handle \(userHandle.count, privacy: .public) bytes, authData \(authData.count, privacy: .public) bytes flags 0x\(String(self.flags(), radix: 16), privacy: .public), signature \(signature.count, privacy: .public) bytes, verifies \(verifies, privacy: .public)"
    )
    let credential = ASPasskeyAssertionCredential(
      userHandle: userHandle, relyingParty: rpId, signature: signature,
      clientDataHash: clientDataHash, authenticatorData: authData, credentialID: id)
    onMain {
      self.extensionContext.completeAssertionRequest(using: credential) { done in
        providerLog.info("assertion handed over (\(path, privacy: .public)): \(done, privacy: .public)")
      }
    }
  }

  /// UwULock's own list of passkeys for `rpId`: one signs straight away, several are shown.
  private func passkeyList(
    rpId: String, allowed: [Data], clientDataHash: Data, key: SymmetricKey, list: PasskeyList,
    path: String
  ) throws {
    let found = list.snapshot.entries.filter { entry in
      entry.rpId == rpId
        && (allowed.isEmpty
          || Base64URL.decode(entry.credentialId).map { allowed.contains($0) } == true)
    }
    providerLog.info(
      "passkey list (\(path, privacy: .public)): rp \(rpId, privacy: .public), \(found.count, privacy: .public) found, \(allowed.count, privacy: .public) allowed"
    )
    if found.count == 1 {
      try answer(
        found[0], list: list, key: key, rpId: rpId, clientDataHash: clientDataHash, path: path)
      return
    }
    onMain {
      self.show()
      self.model.busy = false
      self.model.title = tr("Passkey für ", "Passkey for ") + rpId
      if found.isEmpty {
        self.model.message = tr(
          "Im Tresor ist kein Passkey für diese Seite.", "There's no passkey for this site in the vault.")
        return
      }
      self.model.choices = found.map {
        PasskeyModel.Choice(
          id: $0.credentialId, title: $0.userName ?? $0.userDisplayName ?? $0.rpId,
          subtitle: $0.userDisplayName ?? $0.rpId, suggested: true)
      }
      self.model.pick = { choice in
        guard let entry = found.first(where: { $0.credentialId == choice.id }) else { return }
        self.model.busy = true
        self.background {
          try self.answer(
            entry, list: list, key: key, rpId: rpId, clientDataHash: clientDataHash, path: path)
        }
      }
    }
  }

  override func provideCredentialWithoutUserInteraction(for credentialRequest: ASCredentialRequest) {
    // The provider key always wants Face ID, Touch ID or the passcode: the system shows
    // prepareInterfaceToProvideCredential next.
    providerLog.info("without interaction: type \(credentialRequest.type.rawValue, privacy: .public)")
    cancel(.userInteractionRequired)
  }

  /// The person picked one of UwULock's passwords or passkeys in the system's list.
  override func prepareInterfaceToProvideCredential(for credentialRequest: ASCredentialRequest) {
    if let request = credentialRequest as? ASPasskeyCredentialRequest,
      let identity = request.credentialIdentity as? ASPasskeyCredentialIdentity
    {
      pickedPasskey(request, identity)
    } else if let identity = credentialRequest.credentialIdentity as? ASPasswordCredentialIdentity {
      pickedPassword(identity)
    } else {
      providerLog.error(
        "picked: a request of type \(credentialRequest.type.rawValue, privacy: .public) UwULock doesn't answer"
      )
      cancel(.credentialIdentityNotFound)
    }
  }

  private func pickedPasskey(_ request: ASPasskeyCredentialRequest, _ identity: ASPasskeyCredentialIdentity) {
    let rpId = identity.relyingPartyIdentifier
    providerLog.info(
      "picked passkey: rp \(rpId, privacy: .public), credential \(short(identity.credentialID), privacy: .public), record \(identity.recordIdentifier ?? "-", privacy: .public), user handle \(identity.userHandle.count, privacy: .public) bytes, client data hash \(request.clientDataHash.count, privacy: .public) bytes"
    )
    show()
    model.message = tr("Anmelden bei ", "Signing in to ") + rpId
    unlock(reason: tr("Mit UwULock anmelden bei ", "Sign in with UwULock to ") + rpId) {
      _, key, list in
      // The passkey the system listed — by its credential id, as the system knows it, and only
      // for the site that asks (another site's passkey could never verify there).
      let byId = list.snapshot.entries.filter {
        Base64URL.decode($0.credentialId) == identity.credentialID
      }
      if byId.contains(where: { $0.rpId != rpId }) {
        providerLog.error(
          "picked passkey: the list has it for another rp than \(rpId, privacy: .public)")
      }
      if let entry = byId.first(where: { $0.rpId == rpId }) {
        let handle = entry.userHandle.flatMap(Base64URL.decode) ?? Data()
        if handle != identity.userHandle {
          providerLog.error(
            "picked passkey: the system's entry has another user handle (\(identity.userHandle.count, privacy: .public) vs \(handle.count, privacy: .public) bytes): it is out of date"
          )
        }
        try self.answer(
          entry, list: list, key: key, rpId: rpId, clientDataHash: request.clientDataHash,
          path: "picked")
        return
      }
      // The system's entry is out of date (deleted or replaced since): UwULock's own list
      // for the site instead of giving up.
      providerLog.error("picked passkey: not in the list any more; showing the site's passkeys")
      try self.passkeyList(
        rpId: rpId, allowed: [], clientDataHash: request.clientDataHash, key: key, list: list,
        path: "picked-fallback")
    }
  }

  /// The person picked UwULock without a passkey from the system's list: UwULock shows its own.
  override func prepareCredentialList(
    for serviceIdentifiers: [ASCredentialServiceIdentifier],
    requestParameters: ASPasskeyCredentialRequestParameters
  ) {
    let rpId = requestParameters.relyingPartyIdentifier
    providerLog.info(
      "passkey list asked: rp \(rpId, privacy: .public), client data hash \(requestParameters.clientDataHash.count, privacy: .public) bytes, \(serviceIdentifiers.count, privacy: .public) services"
    )
    show()
    model.title = tr("Passkey für ", "Passkey for ") + rpId
    unlock(reason: tr("Mit UwULock anmelden bei ", "Sign in with UwULock to ") + rpId) {
      _, key, list in
      try self.passkeyList(
        rpId: rpId, allowed: requestParameters.allowedCredentials,
        clientDataHash: requestParameters.clientDataHash, key: key, list: list, path: "list")
      self.updateIdentities(list.snapshot)
    }
  }

  // MARK: Filling in a password

  private func complete(_ login: PasskeyLoginEntry, key: SymmetricKey, list: PasskeyList) throws {
    let password = try PasskeyVault.password(login, key: key, account: list.account)
    providerLog.info("password handed over: record \(login.itemId, privacy: .public)")
    let credential = ASPasswordCredential(user: login.userName ?? "", password: password)
    onMain {
      self.extensionContext.completeRequest(withSelectedCredential: credential, completionHandler: nil)
    }
  }

  private func pickedPassword(_ identity: ASPasswordCredentialIdentity) {
    let service = identity.serviceIdentifier.identifier
    // A URL identifier is a saved address and may carry tokens: only its host goes to the log.
    let site =
      identity.serviceIdentifier.type == .URL
      ? (URL(string: service)?.host ?? "?") : service
    providerLog.info(
      "picked password: service \(site, privacy: .public), record \(identity.recordIdentifier ?? "-", privacy: .public)"
    )
    show()
    model.message = tr("Anmelden bei ", "Signing in to ") + site
    unlock(reason: tr("Mit UwULock anmelden bei ", "Sign in with UwULock to ") + site) {
      _, key, list in
      let logins = list.snapshot.logins ?? []
      // The login the system listed, and only while it still belongs to that site: an entry left
      // in the system's list from before the login's addresses changed fills nothing.
      if let login = logins.first(where: {
        $0.itemId == identity.recordIdentifier
          && PasskeyVault.matches($0, [identity.serviceIdentifier])
      }) {
        try self.complete(login, key: key, list: list)
        return
      }
      providerLog.error("picked password: not in the list any more; showing the logins")
      self.passwordList([identity.serviceIdentifier], key: key, list: list)
    }
  }

  private func passwordList(
    _ services: [ASCredentialServiceIdentifier], key: SymmetricKey, list: PasskeyList
  ) {
    let logins = (list.snapshot.logins ?? []).sorted {
      $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
    }
    let suggested = Set(logins.filter { PasskeyVault.matches($0, services) }.map(\.itemId))
    providerLog.info(
      "password list: \(logins.count, privacy: .public) logins, \(suggested.count, privacy: .public) match"
    )
    onMain {
      self.show()
      self.model.busy = false
      self.model.searchable = true
      self.model.message = nil
      self.model.choices = logins.map {
        PasskeyModel.Choice(
          id: $0.itemId, title: $0.name,
          subtitle: [$0.userName, $0.subtitle].compactMap { $0 }.joined(separator: " · "),
          suggested: suggested.contains($0.itemId))
      }
      self.model.pick = { choice in
        guard let login = logins.first(where: { $0.itemId == choice.id }) else { return }
        self.model.busy = true
        self.background { try self.complete(login, key: key, list: list) }
      }
    }
  }

  /// The person picked UwULock for a password: its logins for the site, and a search over all.
  override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
    let site = serviceIdentifiers.first?.identifier ?? ""
    providerLog.info("password list asked: \(serviceIdentifiers.count, privacy: .public) services")
    show()
    model.title = tr("Passwort für ", "Password for ") + (site.isEmpty ? "…" : site)
    unlock(reason: tr("Mit UwULock anmelden bei ", "Sign in with UwULock to ") + site) {
      _, key, list in
      self.passwordList(serviceIdentifiers, key: key, list: list)
      self.updateIdentities(list.snapshot)
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
    providerLog.info("registration: rp \(rpId, privacy: .public)")
    show()
    model.title = tr("Passkey sichern", "Save passkey")
    model.message = rpId + " · " + identity.userName
    guard PasskeyVault.validRpId(rpId) else {
      fail(PasskeyVaultError.broken("UwULock keeps no passkeys for this site name"))
      return
    }
    guard request.supportedAlgorithms.isEmpty || request.supportedAlgorithms.contains(.ES256)
    else {
      fail(PasskeyVaultError.broken("the site takes no ES256 passkeys"))
      return
    }
    // The list first: it names the account the passkey is for, and a list that doesn't open is
    // never replaced by one with only the new passkey.
    unlock(
      reason: tr("Passkey für ", "Save a passkey for ") + rpId
        + tr(" in UwULock sichern", " in UwULock")
    ) { vault, key, list in
      let made = PasskeyVault.make(
        rpId: rpId, userName: identity.userName, userHandle: identity.userHandle)
      let kept = try vault.keep(made.entry, privateKey: made.key, list: list, key: key)
      let authData = PasskeyVault.authenticatorData(
        rpId: rpId, flags: self.flags(), credentialId: made.id, publicKey: made.key.publicKey)
      let credential = ASPasskeyRegistrationCredential(
        relyingParty: rpId, clientDataHash: request.clientDataHash, credentialID: made.id,
        attestationObject: PasskeyVault.attestationObject(authData))
      providerLog.info("registration: made \(short(made.id), privacy: .public)")
      self.updateIdentities(kept.snapshot)
      self.onMain {
        self.extensionContext.completeRegistrationRequest(using: credential, completionHandler: nil)
      }
    }
  }

  // MARK: Switched on in the settings

  /// Shown once the person picks UwULock for passwords and passkeys: fills the system's list.
  override func prepareInterfaceForExtensionConfiguration() {
    providerLog.info("configuration")
    show()
    model.title = tr("UwULock-AutoFill", "UwULock AutoFill")
    unlock(
      reason: tr(
        "UwULocks Passwörter und Passkeys für das System freigeben",
        "Let the system list UwULock's passwords and passkeys")
    ) { _, _, list in
      self.updateIdentities(list.snapshot)
      self.onMain {
        self.extensionContext.completeExtensionConfigurationRequest()
      }
    }
  }
}
