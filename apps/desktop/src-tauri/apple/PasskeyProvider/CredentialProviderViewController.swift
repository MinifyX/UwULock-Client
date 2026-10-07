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
// and kept in the AutoFill protocol the app shows (AutoFillLog.swift), so a device says where a
// sign-in went wrong:
//   log stream --predicate 'subsystem == "app.uwulock.passkeys"' --info
//
// The sheet is never empty: its view (title, a spinner, "Abbrechen") is built as soon as the
// view loads, before the system says what it wants, and every way in the system may take — the
// iOS 17 requests, the older password-only calls, iOS 18's one-time codes and text — ends in a
// list, a message, a button to unlock with a tap, or an answer.

import AuthenticationServices
import CryptoKit
import LocalAuthentication
import SwiftUI

#if os(iOS)
  import UIKit
  typealias HostingController<V: View> = UIHostingController<V>
  let sheetBackground = Color(UIColor.systemBackground)
#else
  import AppKit
  typealias HostingController<V: View> = NSHostingController<V>
  let sheetBackground = Color(NSColor.windowBackgroundColor)
#endif

/// German when the system prefers it, English otherwise.
func tr(_ german: String, _ english: String) -> String {
  Locale.preferredLanguages.first?.hasPrefix("de") == true ? german : english
}

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
  @Published var message: String? = tr("Wird geladen …", "Loading …")
  @Published var choices: [Choice] = []
  @Published var busy = true
  @Published var query = ""
  /// Passwords: a search over all logins, not only the suggested ones.
  @Published var searchable = false
  /// The button that asks for Face ID, Touch ID or the passcode with a tap, when the system
  /// didn't let the sheet ask by itself. Its label; nil while it isn't offered.
  @Published var unlockButton: String?
  var pick: (Choice) -> Void = { _ in }
  var cancel: () -> Void = {}
  var unlock: () -> Void = {}

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
          .fixedSize(horizontal: false, vertical: true)
      }
      if model.busy {
        ProgressView()
      }
      if let label = model.unlockButton {
        Button {
          model.unlock()
        } label: {
          Text(label).frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
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
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .background(sheetBackground)
  }
}

class CredentialProviderViewController: ASCredentialProviderViewController {
  private let model = PasskeyModel()
  private var installed = false
  /// The sheet is on screen: only then may Face ID, Touch ID or the passcode be asked for. Asked
  /// earlier (the system calls prepareInterfaceToProvideCredential before it presents the sheet),
  /// iOS refuses with errSecInteractionNotAllowed (-25308) — the direct pick's "broken(Keychain
  /// -25308)".
  private var appeared = false
  private var waiting: [() -> Void] = []

  /// When the system called in, for the watchdog.
  private var entered: Date?
  /// Something the person can see or act on happened: a list, a message, the button, or the
  /// request answered. The watchdog stops looking then.
  private var progressed = false
  /// The request was answered or cancelled: nothing more happens.
  private var finished = false
  /// Face ID, Touch ID or the passcode is being asked for (or the provider key read after it),
  /// since then.
  private var asking: Date?
  /// Each watchdog round belongs to one start; a newer start ends the older rounds.
  private var watching = 0
  /// The verification in flight; a newer attempt (the button) invalidates it, and the answers
  /// of older attempts are ignored by their number.
  private var context: LAContext?
  private var attempt = 0
  /// Runs the current unlock again, started by the person: the button.
  private var retryUnlock: (() -> Void)?

  // MARK: The sheet

  override func viewDidLoad() {
    super.viewDidLoad()
    install()
  }

  #if os(iOS)
    override func viewDidAppear(_ animated: Bool) {
      super.viewDidAppear(animated)
      sheetAppeared()
    }
  #else
    // No nib: the view is made here.
    override func loadView() {
      view = NSView(frame: NSRect(x: 0, y: 0, width: 420, height: 420))
    }

    override func viewDidAppear() {
      super.viewDidAppear()
      sheetAppeared()
    }
  #endif

  /// The SwiftUI sheet, pinned to the view's edges with constraints — not a frame copied from
  /// bounds that may still be zero when the system calls in before presenting — on an opaque
  /// background, and showing "Wird geladen …" and "Abbrechen" from the start. Everything later
  /// only changes the model.
  private func install() {
    guard !installed else { return }
    installed = true
    model.cancel = { [weak self] in
      logStep("cancel tapped")
      self?.cancel(.userCanceled)
    }
    model.unlock = { [weak self] in
      logStep("unlock tapped")
      self?.retryUnlock?()
    }
    let host = HostingController(rootView: PasskeyView(model: model))
    addChild(host)
    host.view.translatesAutoresizingMaskIntoConstraints = false
    #if os(iOS)
      view.backgroundColor = .systemBackground
      host.view.backgroundColor = .systemBackground
    #endif
    view.addSubview(host.view)
    NSLayoutConstraint.activate([
      host.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      host.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      host.view.topAnchor.constraint(equalTo: view.topAnchor),
      host.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
    ])
    #if os(iOS)
      host.didMove(toParent: self)
    #endif
    preferredContentSize = CGSize(width: 420, height: 480)
    logStep("sheet built")
  }

  /// Makes sure the view (and with it the sheet) exists; every way in calls it first.
  private func ready() {
    _ = view
    install()
  }

  private func sheetAppeared() {
    guard !appeared else { return }
    appeared = true
    let size = view.bounds.size
    logStep("sheet on screen: \(Int(size.width))×\(Int(size.height))")
    let work = waiting
    waiting = []
    work.forEach { $0() }
  }

  /// Runs `work` on the main thread once the sheet is on screen. Should the system never say so,
  /// it runs after a moment anyway (and a refusal is tried again).
  private func whenOnScreen(_ work: @escaping () -> Void) {
    onMain {
      if self.appeared {
        work()
        return
      }
      self.waiting.append(work)
      DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
        guard !self.appeared, !self.waiting.isEmpty else { return }
        logStep("sheet not reported on screen; going on")
        self.sheetAppeared()
      }
    }
  }

  /// Every answer to the system goes from the main thread.
  private func onMain(_ work: @escaping () -> Void) {
    if Thread.isMainThread { work() } else { DispatchQueue.main.async(execute: work) }
  }

  // MARK: Ways in, and the watchdog

  /// The system called in, at `point`: logged with the system's version, and the watchdog starts.
  private func enter(_ point: String) {
    #if os(iOS)
      let system = "iOS"
    #else
      let system = "macOS"
    #endif
    let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    logStep(
      "entry \(point) · \(system) \(ProcessInfo.processInfo.operatingSystemVersionString) · UwULock \(version ?? "?")"
    )
    ready()
    startWatching()
  }

  private func startWatching() {
    entered = Date()
    progressed = false
    watching += 1
    watch(watching)
  }

  /// Looks every second whether the sheet got anywhere. Nothing asked for after 4 seconds (the
  /// sheet never reported on screen, or the system never started Face ID), or Face ID asked for
  /// 15 seconds without getting anywhere: the button to unlock with a tap, instead of a sheet
  /// that hangs. A tap is always allowed to ask.
  private func watch(_ round: Int) {
    DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
      guard let self, round == self.watching, !self.finished, !self.progressed,
        let entered = self.entered
      else { return }
      let now = Date()
      if let asking = self.asking {
        if now.timeIntervalSince(asking) >= 15 {
          logStep("watchdog: no answer to the verification after 15 s", error: true)
          self.offerUnlock(
            tr(
              "Face ID, Touch ID oder der Code kam nicht. Tippe, um es noch einmal zu versuchen.",
              "Face ID, Touch ID or the passcode didn't come up. Tap to try again."))
          return
        }
      } else if now.timeIntervalSince(entered) >= 4 {
        logStep(
          "watchdog: nothing asked after 4 s (on screen \(self.appeared), unlock \(self.retryUnlock != nil))",
          error: true)
        if self.retryUnlock != nil {
          self.offerUnlock(
            tr(
              "Das System hat Face ID, Touch ID oder den Code nicht von selbst gestartet.",
              "The system didn't start Face ID, Touch ID or the passcode by itself."))
        } else {
          self.progressed = true
          self.model.busy = false
          self.model.message = tr(
            "Das hat zu lange gedauert. Brich ab und versuche es noch einmal.",
            "That took too long. Cancel and try again.")
        }
        return
      }
      self.watch(round)
    }
  }

  /// "Mit Face ID entsperren" (or Touch ID, Optic ID, the passcode) under `why`.
  private func offerUnlock(_ why: String) {
    onMain {
      guard !self.finished else { return }
      self.progressed = true
      self.model.busy = false
      self.model.message = why
      self.model.unlockButton = Self.unlockLabel()
    }
  }

  static func unlockLabel() -> String {
    let context = LAContext()
    _ = context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil)
    switch context.biometryType {
    case .faceID: return tr("Mit Face ID entsperren", "Unlock with Face ID")
    case .touchID: return tr("Mit Touch ID entsperren", "Unlock with Touch ID")
    case .opticID: return tr("Mit Optic ID entsperren", "Unlock with Optic ID")
    default: return tr("Mit Code entsperren", "Unlock with passcode")
    }
  }

  // MARK: Ending

  private func cancel(_ code: ASExtensionError.Code) {
    logStep("cancel: code \(code.rawValue)")
    onMain {
      guard !self.finished else { return }
      self.finished = true
      self.context?.invalidate()
      AutoFillLog.shared.flush()
      self.extensionContext.cancelRequest(
        withError: NSError(domain: ASExtensionErrorDomain, code: code.rawValue))
    }
  }

  /// Marks the request answered (on the main thread), right before the answer goes.
  private func finishing(_ what: String) {
    finished = true
    logStep(what)
    AutoFillLog.shared.flush()
  }

  /// Shows why it stopped, then gives up when the person closes it. Not being allowed to ask for
  /// Face ID offers the button instead of an error.
  private func fail(_ error: Error) {
    logStep("failed: \(String(describing: error))", error: true)
    onMain {
      if case PasskeyVaultError.cancelled = error {
        self.cancel(.userCanceled)
        return
      }
      if case PasskeyVaultError.notInteractive = error {
        self.offerUnlock(
          tr(
            "Das System hat Face ID, Touch ID oder den Code gerade nicht von selbst zugelassen.",
            "The system didn't allow asking for Face ID, Touch ID or the passcode by itself just now."
          ))
        return
      }
      self.ready()
      self.progressed = true
      self.model.busy = false
      self.model.choices = []
      self.model.searchable = false
      self.model.unlockButton = nil
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

  /// A message of its own, with "Abbrechen" (a way in UwULock doesn't serve).
  private func tell(_ message: String) {
    onMain {
      self.ready()
      self.progressed = true
      self.model.busy = false
      self.model.unlockButton = nil
      self.model.message = message
    }
  }

  private func background(_ work: @escaping () throws -> Void) {
    DispatchQueue.global(qos: .userInitiated).async {
      do { try work() } catch { self.fail(error) }
    }
  }

  // MARK: Unlocking

  /// Pauses between attempts while the system says it may not ask yet: about 3 seconds in all,
  /// then the button.
  private static let backoff: [Double] = [0.2, 0.4, 0.6, 0.8, 1.0]

  /// The provider key and the list, after Face ID, Touch ID or the passcode — asked for
  /// explicitly, from the sheet once it is on screen — then `then` on a background queue.
  /// A refusal for asking at the wrong moment is tried again a few times, then the person gets
  /// the button, which asks again on their tap.
  private func unlock(
    reason: String,
    then: @escaping (_ vault: PasskeyVault, _ key: SymmetricKey, _ list: PasskeyList) throws -> Void
  ) {
    let vault: PasskeyVault
    do {
      vault = try PasskeyVault()
    } catch {
      fail(error)
      return
    }
    onMain {
      self.retryUnlock = { [weak self] in
        self?.verify(vault: vault, reason: reason, round: 0, tapped: true, then: then)
      }
    }
    whenOnScreen {
      self.verify(vault: vault, reason: reason, round: 0, tapped: false, then: then)
    }
  }

  /// One attempt, on the main thread.
  ///
  /// Why `evaluatePolicy(.deviceOwnerAuthentication)` and then the Keychain read with that same
  /// context (and `interactionNotAllowed`): the provider key's item is `.userPresence` —
  /// biometry or the passcode — which is exactly what that policy checks, so the evaluated
  /// context covers the read and the read never prompts on its own (a prompt from inside
  /// SecItemCopyMatching is what iOS refuses with -25308 while the sheet isn't up). A fresh
  /// context per attempt: one that answered notInteractive is spent. Not
  /// `evaluateAccessControl`: it would need the item's exact access control rebuilt here and
  /// checks nothing more for `.userPresence`. Not `touchIDAuthenticationAllowableReuseDuration`:
  /// that reuses the device unlock, which an AutoFill sheet must not count as consent.
  private func verify(
    vault: PasskeyVault, reason: String, round: Int, tapped: Bool,
    then: @escaping (PasskeyVault, SymmetricKey, PasskeyList) throws -> Void
  ) {
    guard !finished else { return }
    attempt += 1
    let mine = attempt
    self.context?.invalidate()
    let context = LAContext()
    context.localizedCancelTitle = tr("Abbrechen", "Cancel")
    self.context = context
    asking = Date()
    model.busy = true
    model.unlockButton = nil
    if tapped && round == 0 {
      // The person asked: the watchdog looks after this attempt too.
      model.message = nil
      startWatching()
    }
    logStep("verification: asking (attempt \(mine), round \(round + 1)\(tapped ? ", tapped" : ""))")
    let again: () -> Void = { [weak self] in
      guard let self else { return }
      guard round < Self.backoff.count else {
        logStep("verification: still not allowed after \(round + 1) rounds", error: true)
        self.fail(PasskeyVaultError.notInteractive)
        return
      }
      DispatchQueue.main.asyncAfter(deadline: .now() + Self.backoff[round]) {
        guard mine == self.attempt, !self.finished else { return }
        self.verify(vault: vault, reason: reason, round: round + 1, tapped: tapped, then: then)
      }
    }
    context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { ok, error in
      self.onMain {
        guard mine == self.attempt, !self.finished else {
          logStep("verification: answer of an older attempt ignored")
          return
        }
        if !ok {
          let code = (error as? LAError)?.code
          // Still asking while it is tried again: the watchdog leaves the retries alone.
          if code != .notInteractive { self.asking = nil }
          let raw = (error as NSError?).map { "\($0.domain) \($0.code)" } ?? "-"
          logStep("verification: LAError \(code.map { String($0.rawValue) } ?? "-") (\(raw))")
          switch code {
          case .userCancel?, .appCancel?, .userFallback?:
            self.fail(PasskeyVaultError.cancelled)
          case .notInteractive?:
            again()
          case .systemCancel?:
            // The system took the screen away for a moment (or presented the sheet just then):
            // not the person saying no. Their tap tries again; "Abbrechen" ends it.
            self.offerUnlock(
              tr(
                "Face ID, Touch ID oder der Code wurde vom System unterbrochen.",
                "The system interrupted Face ID, Touch ID or the passcode."))
          case .passcodeNotSet?, .biometryLockout?:
            self.fail(PasskeyVaultError.noPasscode)
          default:
            self.fail(
              PasskeyVaultError.broken(
                "verification \(code.map { String($0.rawValue) } ?? "-")"))
          }
          return
        }
        logStep("verification: ok")
        DispatchQueue.global(qos: .userInitiated).async {
          defer { context.invalidate() }
          do {
            let key: SymmetricKey
            do {
              key = try vault.key(context: context)
            } catch PasskeyVaultError.notInteractive {
              self.onMain {
                guard mine == self.attempt, !self.finished else { return }
                again()
              }
              return
            }
            let list = try vault.list(key: key)
            self.onMain { if mine == self.attempt { self.asking = nil } }
            logStep(
              "list opened: \(list.snapshot.entries.count) passkeys, \(list.snapshot.logins?.count ?? 0) logins, generation \(list.snapshot.generation)"
            )
            try then(vault, key, list)
          } catch {
            self.fail(error)
          }
        }
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

  /// Keeps the system's list in step with the list, then `done` (always, also when the system
  /// has UwULock switched off). Never while the system answers a pick from that list
  /// (prepareInterfaceToProvideCredential): replacing the entries under a request made from one
  /// of them is asking for trouble. On iOS the app keeps it current anyway; on macOS this is the
  /// only place it is filled.
  private func updateIdentities(_ snapshot: PasskeySnapshot, done: (() -> Void)? = nil) {
    let identities = Self.identities(of: snapshot)
    ASCredentialIdentityStore.shared.getState { state in
      guard state.isEnabled else {
        logStep("identities: UwULock isn't switched on in the system")
        done?()
        return
      }
      ASCredentialIdentityStore.shared.replaceCredentialIdentities(identities) { ok, error in
        logStep(
          "identities replaced: \(identities.count), ok \(ok) \(error.map { String(describing: $0) } ?? "")"
        )
        done?()
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
    logStep(
      "assertion (\(path)): rp \(rpId), entry rp \(entry.rpId), credential \(short(id)), user handle \(userHandle.count) bytes, authData \(authData.count) bytes flags 0x\(String(self.flags(), radix: 16)), signature \(signature.count) bytes, verifies \(verifies)"
    )
    let credential = ASPasskeyAssertionCredential(
      userHandle: userHandle, relyingParty: rpId, signature: signature,
      clientDataHash: clientDataHash, authenticatorData: authData, credentialID: id)
    onMain {
      guard !self.finished else { return }
      self.finishing("assertion handing over (\(path))")
      self.extensionContext.completeAssertionRequest(using: credential) { done in
        logStep("assertion handed over (\(path)): \(done)")
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
    logStep(
      "passkey list (\(path)): rp \(rpId), \(found.count) found, \(allowed.count) allowed")
    if found.count == 1 {
      try answer(
        found[0], list: list, key: key, rpId: rpId, clientDataHash: clientDataHash, path: path)
      return
    }
    onMain {
      self.ready()
      self.progressed = true
      self.model.busy = false
      self.model.unlockButton = nil
      self.model.title = tr("Passkey für ", "Passkey for ") + rpId
      if found.isEmpty {
        self.model.message = tr(
          "Im Tresor ist kein Passkey für diese Seite.", "There's no passkey for this site in the vault.")
        return
      }
      self.model.message = nil
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
    logStep("entry provideCredentialWithoutUserInteraction(request): type \(credentialRequest.type.rawValue)")
    cancel(.userInteractionRequired)
  }

  /// The older, password-only call. iOS 17+ makes the request one above; should a system still
  /// make this one, its default does nothing — a sheet that never comes — so it is answered alike.
  @available(iOS, deprecated: 17.0)
  @available(macOS, deprecated: 14.0)
  override func provideCredentialWithoutUserInteraction(
    for credentialIdentity: ASPasswordCredentialIdentity
  ) {
    logStep("entry provideCredentialWithoutUserInteraction(identity)")
    cancel(.userInteractionRequired)
  }

  /// The person picked one of UwULock's passwords or passkeys in the system's list.
  override func prepareInterfaceToProvideCredential(for credentialRequest: ASCredentialRequest) {
    enter("prepareInterfaceToProvideCredential(request): type \(credentialRequest.type.rawValue)")
    if let request = credentialRequest as? ASPasskeyCredentialRequest,
      let identity = request.credentialIdentity as? ASPasskeyCredentialIdentity
    {
      pickedPasskey(request, identity)
    } else if let identity = credentialRequest.credentialIdentity as? ASPasswordCredentialIdentity {
      pickedPassword(identity)
    } else {
      logStep(
        "picked: a request of type \(credentialRequest.type.rawValue) UwULock doesn't answer",
        error: true)
      tell(
        tr(
          "Diese Art Anmeldung kann UwULock nicht ausfüllen.",
          "UwULock can't fill in this kind of sign-in."))
    }
  }

  /// The older, password-only call: its default does nothing, which leaves an empty sheet.
  @available(iOS, deprecated: 17.0)
  @available(macOS, deprecated: 14.0)
  override func prepareInterfaceToProvideCredential(
    for credentialIdentity: ASPasswordCredentialIdentity
  ) {
    enter("prepareInterfaceToProvideCredential(identity)")
    pickedPassword(credentialIdentity)
  }

  private func pickedPasskey(_ request: ASPasskeyCredentialRequest, _ identity: ASPasskeyCredentialIdentity) {
    let rpId = identity.relyingPartyIdentifier
    logStep(
      "picked passkey: rp \(rpId), credential \(short(identity.credentialID)), record \(identity.recordIdentifier ?? "-"), user handle \(identity.userHandle.count) bytes, client data hash \(request.clientDataHash.count) bytes"
    )
    model.message = tr("Anmelden bei ", "Signing in to ") + rpId
    unlock(reason: tr("Mit UwULock anmelden bei ", "Sign in with UwULock to ") + rpId) {
      _, key, list in
      // The passkey the system listed — by its credential id, as the system knows it, and only
      // for the site that asks (another site's passkey could never verify there).
      let byId = list.snapshot.entries.filter {
        Base64URL.decode($0.credentialId) == identity.credentialID
      }
      if byId.contains(where: { $0.rpId != rpId }) {
        logStep("picked passkey: the list has it for another rp than \(rpId)", error: true)
      }
      if let entry = byId.first(where: { $0.rpId == rpId }) {
        let handle = entry.userHandle.flatMap(Base64URL.decode) ?? Data()
        if handle != identity.userHandle {
          logStep(
            "picked passkey: the system's entry has another user handle (\(identity.userHandle.count) vs \(handle.count) bytes): it is out of date",
            error: true)
        }
        try self.answer(
          entry, list: list, key: key, rpId: rpId, clientDataHash: request.clientDataHash,
          path: "picked")
        return
      }
      // The system's entry is out of date (deleted or replaced since): UwULock's own list
      // for the site instead of giving up.
      logStep("picked passkey: not in the list any more; showing the site's passkeys", error: true)
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
    enter("prepareCredentialList(passkey)")
    logStep(
      "passkey list asked: rp \(rpId), client data hash \(requestParameters.clientDataHash.count) bytes, \(serviceIdentifiers.count) services"
    )
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
    let credential = ASPasswordCredential(user: login.userName ?? "", password: password)
    onMain {
      guard !self.finished else { return }
      self.finishing("password handed over: record \(login.itemId)")
      self.extensionContext.completeRequest(withSelectedCredential: credential, completionHandler: nil)
    }
  }

  /// A service for the log: a URL identifier is a saved address and may carry tokens, so only
  /// its host.
  private static func site(_ service: ASCredentialServiceIdentifier) -> String {
    service.type == .URL ? (URL(string: service.identifier)?.host ?? "?") : service.identifier
  }

  private func pickedPassword(_ identity: ASPasswordCredentialIdentity) {
    let site = Self.site(identity.serviceIdentifier)
    logStep("picked password: service \(site), record \(identity.recordIdentifier ?? "-")")
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
      logStep("picked password: not in the list any more; showing the logins", error: true)
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
    logStep("password list: \(logins.count) logins, \(suggested.count) match")
    onMain {
      self.ready()
      self.progressed = true
      self.model.busy = false
      self.model.unlockButton = nil
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
    let site = serviceIdentifiers.first.map(Self.site) ?? ""
    enter("prepareCredentialList(password)")
    logStep("password list asked: \(serviceIdentifiers.count) services, first \(site.isEmpty ? "-" : site)")
    model.title = tr("Passwort für ", "Password for ") + (site.isEmpty ? "…" : site)
    unlock(reason: tr("Mit UwULock anmelden bei ", "Sign in with UwULock to ") + site) {
      _, key, list in
      self.passwordList(serviceIdentifiers, key: key, list: list)
      self.updateIdentities(list.snapshot)
    }
  }

  // MARK: What UwULock doesn't fill (iOS 18, macOS 15)

  /// One-time codes: UwULock doesn't say it provides them (Info.plist), so the system shouldn't
  /// ask; should it, a clear word instead of an empty sheet.
  @available(iOS 18.0, macOS 15.0, *)
  override func prepareOneTimeCodeCredentialList(
    for serviceIdentifiers: [ASCredentialServiceIdentifier]
  ) {
    enter("prepareOneTimeCodeCredentialList: \(serviceIdentifiers.count) services")
    tell(
      tr(
        "UwULock füllt keine Einmalcodes aus. Öffne UwULock und kopiere den Code dort.",
        "UwULock doesn't fill in one-time codes. Open UwULock and copy the code there."))
  }

  /// Text to insert (the edit menu's AutoFill → Passwords on any field; iOS only): not offered
  /// either.
  #if os(iOS)
    @available(iOS 18.0, *)
    override func prepareInterfaceForUserChoosingTextToInsert() {
      enter("prepareInterfaceForUserChoosingTextToInsert")
      tell(
        tr(
          "UwULock fügt hier keinen Text ein. Tippe in ein Anmeldefeld und wähle dort UwULock.",
          "UwULock doesn't insert text here. Tap a sign-in field and pick UwULock there."))
    }
  #endif

  // MARK: Making a passkey

  override func prepareInterface(forPasskeyRegistration registrationRequest: ASCredentialRequest) {
    enter("prepareInterface(forPasskeyRegistration:): type \(registrationRequest.type.rawValue)")
    guard let request = registrationRequest as? ASPasskeyCredentialRequest,
      let identity = request.credentialIdentity as? ASPasskeyCredentialIdentity
    else {
      fail(PasskeyVaultError.broken("not a passkey registration"))
      return
    }
    let rpId = identity.relyingPartyIdentifier
    logStep("registration: rp \(rpId)")
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
      logStep("registration: made \(short(made.id))")
      self.updateIdentities(kept.snapshot)
      self.onMain {
        guard !self.finished else { return }
        self.finishing("registration handing over")
        self.extensionContext.completeRegistrationRequest(using: credential, completionHandler: nil)
      }
    }
  }

  // MARK: Switched on in the settings

  /// Shown once the person picks UwULock for passwords and passkeys: fills the system's list,
  /// and says done once the system took it (or after a few seconds, should it not answer).
  override func prepareInterfaceForExtensionConfiguration() {
    enter("prepareInterfaceForExtensionConfiguration")
    model.title = tr("UwULock-AutoFill", "UwULock AutoFill")
    unlock(
      reason: tr(
        "UwULocks Passwörter und Passkeys für das System freigeben",
        "Let the system list UwULock's passwords and passkeys")
    ) { _, _, list in
      let done = {
        self.onMain {
          guard !self.finished else { return }
          self.finishing("configuration done")
          self.extensionContext.completeExtensionConfigurationRequest()
        }
      }
      self.updateIdentities(list.snapshot, done: done)
      DispatchQueue.main.asyncAfter(deadline: .now() + 3, execute: done)
    }
  }
}
