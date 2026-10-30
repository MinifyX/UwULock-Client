// UwULock's AutoFill extension for passwords and passkeys (iOS and macOS), moved from
// apps/desktop/src-tauri/apple/PasskeyProvider with the same behaviour and the same shared store
// (PasskeyVault.swift); the sheet is PasskeySheet.swift, its text Localizable.xcstrings.
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
// requests, the older password-only calls, one-time codes and text — ends in a list, a message,
// a button to unlock with a tap, or an answer.
//
// Concurrency: everything here runs on the main actor except the Keychain read and opening the
// list (`opened`), which block; the system's callbacks hop back with `Task { @MainActor in }`.

import AuthenticationServices
import CryptoKit
import LocalAuthentication
import SwiftUI
import UwUDesign

#if os(iOS)
import UIKit
typealias HostingController<V: View> = UIHostingController<V>
#else
import AppKit
typealias HostingController<V: View> = NSHostingController<V>
#endif

/// A value handed to the background read and back that isn't Sendable (LAContext) but is only
/// used by one side at a time.
struct Handed<Value>: @unchecked Sendable {
    let value: Value
}

/// What runs once the list is open, on the main actor.
typealias Unlocked = @MainActor (_ vault: PasskeyVault, _ key: SymmetricKey, _ list: PasskeyList) throws -> Void

class CredentialProviderViewController: ASCredentialProviderViewController {
    private let model = PasskeyModel()
    private var installed = false
    /// The sheet is on screen: only then may Face ID, Touch ID or the passcode be asked for. Asked
    /// earlier (the system calls prepareInterfaceToProvideCredential before it presents the sheet),
    /// iOS refuses with errSecInteractionNotAllowed (-25308) — the direct pick's "broken(Keychain
    /// -25308)".
    private var appeared = false
    private var waiting: [@MainActor () -> Void] = []

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
        UwUFonts.register()
        model.cancel = { [weak self] in
            logStep("cancel tapped")
            self?.cancel(.userCanceled)
        }
        model.unlock = { [weak self] in
            logStep("unlock tapped")
            self?.retryUnlock?()
        }
        let host = HostingController(rootView: PasskeySheet(model: model))
        addChild(host)
        host.view.translatesAutoresizingMaskIntoConstraints = false
        #if os(iOS)
        view.backgroundColor = UIColor(Color.uwuCanvas)
        host.view.backgroundColor = .clear
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

    /// Runs `work` once the sheet is on screen. Should the system never say so, it runs after a
    /// moment anyway (and a refusal is tried again).
    private func whenOnScreen(_ work: @escaping @MainActor () -> Void) {
        if appeared {
            work()
            return
        }
        waiting.append(work)
        after(1.5) { [weak self] in
            guard let self, !self.appeared, !self.waiting.isEmpty else { return }
            logStep("sheet not reported on screen; going on")
            self.sheetAppeared()
        }
    }

    /// `work` on the main actor after `seconds`.
    private func after(_ seconds: Double, _ work: @escaping @MainActor () -> Void) {
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(seconds))
            work()
        }
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
        // A new request: one answered before on this controller (userInteractionRequired from
        // provideCredentialWithoutUserInteraction, should the system keep the instance for the
        // sheet) must not leave this one with a dead "Abbrechen" and no Face ID.
        finished = false
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
        after(1) { [weak self] in
            guard let self, round == self.watching, !self.finished, !self.progressed,
                let entered = self.entered
            else { return }
            let now = Date()
            if let asking = self.asking {
                if now.timeIntervalSince(asking) >= 15 {
                    logStep("watchdog: no answer to the verification after 15 s", error: true)
                    self.offerUnlock(
                        String(
                            localized:
                                "Face ID, Touch ID oder der Code kam nicht. Tippe, um es noch einmal zu versuchen."
                        ))
                    return
                }
            } else if now.timeIntervalSince(entered) >= 4 {
                logStep(
                    "watchdog: nothing asked after 4 s (on screen \(self.appeared), unlock \(self.retryUnlock != nil))",
                    error: true)
                if self.retryUnlock != nil {
                    self.offerUnlock(
                        String(
                            localized: "Das System hat Face ID, Touch ID oder den Code nicht von selbst gestartet."))
                } else {
                    self.progressed = true
                    self.model.busy = false
                    self.model.message = String(
                        localized: "Das hat zu lange gedauert. Brich ab und versuche es noch einmal.")
                }
                return
            }
            self.watch(round)
        }
    }

    /// "Mit Face ID entsperren" (or Touch ID, Optic ID, the passcode) under `why`.
    private func offerUnlock(_ why: String) {
        guard !finished else { return }
        progressed = true
        model.busy = false
        model.message = why
        model.unlockButton = Self.unlockLabel()
    }

    static func unlockLabel() -> String {
        let context = LAContext()
        _ = context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil)
        switch context.biometryType {
        case .faceID: return String(localized: "Mit Face ID entsperren")
        case .touchID: return String(localized: "Mit Touch ID entsperren")
        case .opticID: return String(localized: "Mit Optic ID entsperren")
        default: return String(localized: "Mit Code entsperren")
        }
    }

    // MARK: Ending

    private func cancel(_ code: ASExtensionError.Code) {
        logStep("cancel: code \(code.rawValue)")
        guard !finished else { return }
        finished = true
        context?.invalidate()
        AutoFillLog.shared.flush()
        extensionContext.cancelRequest(
            withError: NSError(domain: ASExtensionErrorDomain, code: code.rawValue))
    }

    /// Marks the request answered, right before the answer goes.
    private func finishing(_ what: String) {
        finished = true
        logStep(what)
        AutoFillLog.shared.flush()
    }

    /// Shows why it stopped, then gives up when the person closes it. Not being allowed to ask for
    /// Face ID offers the button instead of an error.
    private func fail(_ error: Error) {
        logStep("failed: \(String(describing: error))", error: true)
        if case PasskeyVaultError.cancelled = error {
            cancel(.userCanceled)
            return
        }
        if case PasskeyVaultError.notInteractive = error {
            offerUnlock(
                String(
                    localized:
                        "Das System hat Face ID, Touch ID oder den Code gerade nicht von selbst zugelassen."))
            return
        }
        ready()
        progressed = true
        model.busy = false
        model.choices = []
        model.passkeys = []
        model.searchable = false
        model.unlockButton = nil
        let retry = String(
            localized: "Das ging nicht. Öffne UwULock und entsperre den Tresor, dann versuche es noch einmal.")
        switch error {
        case PasskeyVaultError.notSetUp:
            model.message = String(
                localized:
                    "Diese UwULock-Version ist nicht mit einem Apple-Entwicklerkonto signiert; die AutoFill-Erweiterung kann den Tresor nicht erreichen."
            )
        case PasskeyVaultError.noList:
            // Switched off or logged out: the system's list goes too.
            ASCredentialIdentityStore.shared.removeAllCredentialIdentities { @Sendable _, _ in }
            Self.forgetHandedOver()
            model.message = String(
                localized:
                    "Öffne UwULock einmal und entsperre den Tresor, mit eingeschaltetem AutoFill für andere Apps.")
        case PasskeyVaultError.stale:
            model.message = String(
                localized:
                    "Die Liste ist älter als eine, die schon hier war. Öffne UwULock und entsperre den Tresor.")
        case PasskeyVaultError.noPasscode:
            model.message = String(
                localized:
                    "UwULock braucht einen Gerätecode (und Face ID oder Touch ID, falls gesperrt: einmal mit dem Code entsperren)."
            )
        case PasskeyVaultError.broken(let why):
            model.message = retry + " (\(why))"
        default:
            model.message = retry
        }
    }

    /// A message of its own, with "Abbrechen" (a way in UwULock doesn't serve).
    private func tell(_ message: String) {
        ready()
        progressed = true
        model.busy = false
        model.unlockButton = nil
        model.message = message
    }

    /// `work`, its error shown.
    private func trying(_ work: () throws -> Void) {
        do { try work() } catch { fail(error) }
    }

    // MARK: Unlocking

    /// Pauses between attempts while the system says it may not ask yet: about 3 seconds in all,
    /// then the button.
    private static let backoff: [Double] = [0.2, 0.4, 0.6, 0.8, 1.0]

    /// The provider key and the list, after Face ID, Touch ID or the passcode — asked for
    /// explicitly, from the sheet once it is on screen — then `then`. A refusal for asking at the
    /// wrong moment is tried again a few times, then the person gets the button, which asks again
    /// on their tap.
    private func unlock(reason: String, then: @escaping Unlocked) {
        let vault: PasskeyVault
        do {
            vault = try PasskeyVault()
        } catch {
            fail(error)
            return
        }
        retryUnlock = { [weak self] in
            self?.verify(vault: vault, reason: reason, round: 0, tapped: true, then: then)
        }
        whenOnScreen { [weak self] in
            self?.verify(vault: vault, reason: reason, round: 0, tapped: false, then: then)
        }
    }

    /// One attempt.
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
        vault: PasskeyVault, reason: String, round: Int, tapped: Bool, then: @escaping Unlocked
    ) {
        guard !finished else { return }
        attempt += 1
        let mine = attempt
        context?.invalidate()
        let context = LAContext()
        context.localizedCancelTitle = String(localized: "Abbrechen")
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
        let handed = Handed(value: context)
        context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) {
            @Sendable [weak self] ok, error in
            Task { @MainActor in
                self?.verified(
                    ok: ok, error: error, context: handed, mine: mine, vault: vault, reason: reason,
                    round: round, tapped: tapped, then: then)
            }
        }
    }

    /// The answer to one attempt's verification.
    private func verified(
        ok: Bool, error: Error?, context: Handed<LAContext>, mine: Int, vault: PasskeyVault,
        reason: String, round: Int, tapped: Bool, then: @escaping Unlocked
    ) {
        guard mine == attempt, !finished else {
            logStep("verification: answer of an older attempt ignored")
            return
        }
        if !ok {
            let code = (error as? LAError)?.code
            // Still asking while it is tried again: the watchdog leaves the retries alone.
            if code != .notInteractive { asking = nil }
            let raw = (error as NSError?).map { "\($0.domain) \($0.code)" } ?? "-"
            logStep("verification: LAError \(code.map { String($0.rawValue) } ?? "-") (\(raw))")
            switch code {
            case .userCancel?, .appCancel?, .userFallback?:
                fail(PasskeyVaultError.cancelled)
            case .notInteractive?:
                guard round < Self.backoff.count else {
                    logStep("verification: still not allowed after \(round + 1) rounds", error: true)
                    fail(PasskeyVaultError.notInteractive)
                    return
                }
                after(Self.backoff[round]) { [weak self] in
                    guard let self, mine == self.attempt, !self.finished else { return }
                    self.verify(vault: vault, reason: reason, round: round + 1, tapped: tapped, then: then)
                }
            case .systemCancel?:
                // The system took the screen away for a moment (or presented the sheet just then):
                // not the person saying no. Their tap tries again; "Abbrechen" ends it.
                offerUnlock(String(localized: "Face ID, Touch ID oder der Code wurde vom System unterbrochen."))
            case .passcodeNotSet?, .biometryLockout?:
                fail(PasskeyVaultError.noPasscode)
            default:
                fail(PasskeyVaultError.broken("verification \(code.map { String($0.rawValue) } ?? "-")"))
            }
            return
        }
        logStep("verification: ok")
        // Answered: the watchdog's rounds end (a button now would start a second attempt, and a
        // second passkey on registration, beside this one).
        watching += 1
        model.unlockButton = nil
        // The Keychain read blocks: never on the main thread.
        Task { @MainActor [weak self] in
            let opened = await Task.detached(priority: .userInitiated) {
                () -> Result<Handed<(SymmetricKey, PasskeyList)>, Error> in
                defer { context.value.invalidate() }
                do {
                    let key = try vault.key(context: context.value)
                    return .success(Handed(value: (key, try vault.list(key: key))))
                } catch {
                    return .failure(error)
                }
            }.value
            self?.opened(opened, mine: mine, vault: vault, then: then)
        }
    }

    /// The provider key and the list, read after a verification.
    private func opened(
        _ opened: Result<Handed<(SymmetricKey, PasskeyList)>, Error>, mine: Int, vault: PasskeyVault,
        then: Unlocked
    ) {
        switch opened {
        case .failure(PasskeyVaultError.notInteractive):
            // The person already passed Face ID, Touch ID or the passcode: trying again by itself
            // would ask once more, maybe several times. Their tap asks again, once.
            logStep("verification: the Keychain didn't take it", error: true)
            guard mine == attempt, !finished else { return }
            offerUnlock(
                String(
                    localized:
                        "Der Schlüssel ließ sich nach der Bestätigung nicht öffnen. Tippe, um es noch einmal zu versuchen."
                ))
        case .failure(let error):
            fail(error)
        case .success(let handed):
            let (key, list) = handed.value
            if mine == attempt { asking = nil }
            logStep(
                "list opened: \(list.snapshot.entries.count) passkeys, \(list.snapshot.logins.map { "\($0.count) logins" } ?? "no logins field (a list from before password AutoFill)"), generation \(list.snapshot.generation)"
            )
            trying { try then(vault, key, list) }
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

    /// Which list the system's entries were last made from (account, generation, count), in the
    /// extension's own container: the same list isn't handed over again.
    nonisolated private static let handedOverKey = "uwulock.identities.handedOver"

    private static func handedOver(_ list: PasskeyList, count: Int) -> String {
        "\(list.account):\(list.snapshot.generation):\(count)"
    }

    /// The system's entries are gone (or to be made again): the next list is handed over.
    nonisolated static func forgetHandedOver() {
        UserDefaults.standard.removeObject(forKey: handedOverKey)
    }

    /// Keeps the system's list in step with the list, then `done` (always, also when the system
    /// has UwULock switched off). Never while the system answers a pick from that list
    /// (prepareInterfaceToProvideCredential): replacing the entries under a request made from one
    /// of them is asking for trouble. On iOS the app keeps it current anyway; on macOS this is the
    /// only place it is filled — once per list the app wrote, not on every request: each
    /// replacement has Safari read all of UwULock's entries again, and the page it fills into
    /// hangs meanwhile. `force` hands it over regardless (the system asked to be set up).
    private func updateIdentities(
        _ list: PasskeyList, force: Bool = false, done: (@MainActor () -> Void)? = nil
    ) {
        let identities = Handed(value: Self.identities(of: list.snapshot))
        let count = identities.value.count
        let mark = Self.handedOver(list, count: count)
        Task { @MainActor in
            let store = ASCredentialIdentityStore.shared
            guard await store.state().isEnabled else {
                logStep("identities: UwULock isn't switched on in the system")
                Self.forgetHandedOver()
                done?()
                return
            }
            if !force && UserDefaults.standard.string(forKey: Self.handedOverKey) == mark {
                logStep("identities: the system already has this list (\(count))")
                done?()
                return
            }
            do {
                try await store.replaceCredentialIdentities(identities.value)
                logStep("identities replaced: \(count), ok true")
                UserDefaults.standard.set(mark, forKey: Self.handedOverKey)
            } catch {
                logStep("identities replaced: \(count), ok false \(String(describing: error))")
                Self.forgetHandedOver()
            }
            done?()
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
        let signature = try PasskeyVault.sign(signingKey, authData: authData, clientDataHash: clientDataHash)
        // The signature has to verify under the key the site keeps: checked here, so a broken key
        // shows up in the log rather than as the site's refusal.
        let verifies =
            (try? P256.Signing.ECDSASignature(derRepresentation: signature)).map {
                signingKey.publicKey.isValidSignature($0, for: authData + clientDataHash)
            } ?? false
        logStep(
            "assertion (\(path)): rp \(rpId), entry rp \(entry.rpId), credential \(short(id)), user handle \(userHandle.count) bytes, authData \(authData.count) bytes flags 0x\(String(flags(), radix: 16)), signature \(signature.count) bytes, verifies \(verifies)"
        )
        let credential = ASPasskeyAssertionCredential(
            userHandle: userHandle, relyingParty: rpId, signature: signature,
            clientDataHash: clientDataHash, authenticatorData: authData, credentialID: id)
        guard !finished else { return }
        finishing("assertion handing over (\(path))")
        extensionContext.completeAssertionRequest(using: credential) { @Sendable done in
            logStep("assertion handed over (\(path)): \(done)")
        }
    }

    /// UwULock's own list of passkeys for `rpId`: one signs straight away, several are shown.
    /// With `services` (the system asked for passkeys and passwords), the logins come along: the
    /// site's first and a search over all, and one passkey signs straight away only when no login
    /// fits the site.
    private func passkeyList(
        rpId: String, allowed: [Data], clientDataHash: Data, key: SymmetricKey, list: PasskeyList,
        path: String, services: [ASCredentialServiceIdentifier]? = nil
    ) throws {
        let found = list.snapshot.entries.filter { entry in
            entry.rpId == rpId
                && (allowed.isEmpty
                    || Base64URL.decode(entry.credentialId).map { allowed.contains($0) } == true)
        }
        let forSite = list.snapshot.entries.filter { $0.rpId == rpId }.count
        // The site itself stands in for the system's services when it names none.
        let sites = services.map {
            $0.isEmpty ? [ASCredentialServiceIdentifier(identifier: rpId, type: .domain)] : $0
        }
        let logins = sites.map { _ in Self.sortedLogins(list) } ?? []
        let suggested =
            sites.map { sites in
                Set(logins.filter { PasskeyVault.matches($0, sites) }.map(\.itemId))
            } ?? []
        logStep(
            "passkey list (\(path)): rp \(rpId), \(found.count) found, \(allowed.count) allowed, \(forSite) for the rp, \(list.snapshot.entries.count) in the list, \(sites == nil ? "no logins" : "\(suggested.count) of \(logins.count) logins match")"
        )
        if found.count == 1 && suggested.isEmpty {
            try answer(found[0], list: list, key: key, rpId: rpId, clientDataHash: clientDataHash, path: path)
            return
        }
        ready()
        progressed = true
        model.busy = false
        model.unlockButton = nil
        model.title = String(localized: "Anmelden bei \(rpId)")
        model.message = found.isEmpty ? String(localized: "Im Tresor ist kein Passkey für diese Seite.") : nil
        model.passkeys = found.map {
            PasskeyModel.Choice(
                id: Self.passkeyPrefix + $0.credentialId,
                title: $0.userName ?? $0.userDisplayName ?? $0.rpId,
                subtitle: $0.userDisplayName ?? $0.rpId, suggested: true, passkey: true)
        }
        if sites != nil {
            showLogins(logins, suggested: suggested, key: key, list: list)
        }
        let pickLogin = model.pick
        model.pick = { [weak self] choice in
            guard let self else { return }
            guard choice.id.hasPrefix(Self.passkeyPrefix) else { return pickLogin(choice) }
            let id = String(choice.id.dropFirst(Self.passkeyPrefix.count))
            guard let entry = found.first(where: { $0.credentialId == id }) else { return }
            self.model.busy = true
            self.trying {
                try self.answer(entry, list: list, key: key, rpId: rpId, clientDataHash: clientDataHash, path: path)
            }
        }
    }

    /// Passkey choices' ids: apart from the logins' item ids in the same sheet.
    private static let passkeyPrefix = "passkey:"

    override func provideCredentialWithoutUserInteraction(for credentialRequest: any ASCredentialRequest) {
        // The provider key always wants Face ID, Touch ID or the passcode: the system shows
        // prepareInterfaceToProvideCredential next.
        logStep("entry provideCredentialWithoutUserInteraction(request): type \(credentialRequest.type.rawValue)")
        cancel(.userInteractionRequired)
    }

    /// The older, password-only call. The request one above replaces it; should a system still
    /// make this one, its default does nothing — a sheet that never comes — so it is answered alike.
    @available(iOS, deprecated: 17.0)
    @available(macOS, deprecated: 14.0)
    override func provideCredentialWithoutUserInteraction(for credentialIdentity: ASPasswordCredentialIdentity) {
        logStep("entry provideCredentialWithoutUserInteraction(identity)")
        cancel(.userInteractionRequired)
    }

    /// The person picked one of UwULock's passwords or passkeys in the system's list.
    override func prepareInterfaceToProvideCredential(for credentialRequest: any ASCredentialRequest) {
        enter("prepareInterfaceToProvideCredential(request): type \(credentialRequest.type.rawValue)")
        if let request = credentialRequest as? ASPasskeyCredentialRequest,
            let identity = request.credentialIdentity as? ASPasskeyCredentialIdentity
        {
            pickedPasskey(request, identity)
        } else if let identity = credentialRequest.credentialIdentity as? ASPasswordCredentialIdentity {
            pickedPassword(identity)
        } else {
            logStep(
                "picked: a request of type \(credentialRequest.type.rawValue) UwULock doesn't answer", error: true)
            tell(String(localized: "Diese Art Anmeldung kann UwULock nicht ausfüllen."))
        }
    }

    /// The older, password-only call: its default does nothing, which leaves an empty sheet.
    @available(iOS, deprecated: 17.0)
    @available(macOS, deprecated: 14.0)
    override func prepareInterfaceToProvideCredential(for credentialIdentity: ASPasswordCredentialIdentity) {
        enter("prepareInterfaceToProvideCredential(identity)")
        pickedPassword(credentialIdentity)
    }

    private func pickedPasskey(_ request: ASPasskeyCredentialRequest, _ identity: ASPasskeyCredentialIdentity) {
        let rpId = identity.relyingPartyIdentifier
        let clientDataHash = request.clientDataHash
        let credentialID = identity.credentialID
        let userHandle = identity.userHandle
        logStep(
            "picked passkey: rp \(rpId), credential \(short(credentialID)), record \(identity.recordIdentifier ?? "-"), user handle \(userHandle.count) bytes, client data hash \(clientDataHash.count) bytes"
        )
        model.message = String(localized: "Anmelden bei \(rpId) …")
        unlock(reason: String(localized: "Mit UwULock anmelden bei \(rpId)")) { [weak self] _, key, list in
            guard let self else { return }
            // The passkey the system listed — by its credential id, as the system knows it, and
            // only for the site that asks (another site's passkey could never verify there).
            let byId = list.snapshot.entries.filter { Base64URL.decode($0.credentialId) == credentialID }
            if byId.contains(where: { $0.rpId != rpId }) {
                logStep("picked passkey: the list has it for another rp than \(rpId)", error: true)
            }
            if let entry = byId.first(where: { $0.rpId == rpId }) {
                let handle = entry.userHandle.flatMap(Base64URL.decode) ?? Data()
                if handle != userHandle {
                    logStep(
                        "picked passkey: the system's entry has another user handle (\(userHandle.count) vs \(handle.count) bytes): it is out of date",
                        error: true)
                }
                try self.answer(
                    entry, list: list, key: key, rpId: rpId, clientDataHash: clientDataHash, path: "picked")
                return
            }
            // The system's entry is out of date (deleted or replaced since): UwULock's own list
            // for the site instead of giving up.
            logStep("picked passkey: not in the list any more; showing the site's passkeys", error: true)
            try self.passkeyList(
                rpId: rpId, allowed: [], clientDataHash: clientDataHash, key: key, list: list,
                path: "picked-fallback")
        }
    }

    /// The person picked UwULock without a passkey from the system's list: UwULock shows its own,
    /// and the logins beside them — the system asks for either (a password completes the request
    /// as well).
    override func prepareCredentialList(
        for serviceIdentifiers: [ASCredentialServiceIdentifier],
        requestParameters: ASPasskeyCredentialRequestParameters
    ) {
        let rpId = requestParameters.relyingPartyIdentifier
        let allowed = requestParameters.allowedCredentials
        let clientDataHash = requestParameters.clientDataHash
        enter("prepareCredentialList(passkey)")
        logStep(
            "passkey list asked: rp \(rpId), client data hash \(clientDataHash.count) bytes, \(serviceIdentifiers.count) services"
        )
        model.title = String(localized: "Passkey für \(rpId)")
        unlock(reason: String(localized: "Mit UwULock anmelden bei \(rpId)")) { [weak self] _, key, list in
            guard let self else { return }
            try self.passkeyList(
                rpId: rpId, allowed: allowed, clientDataHash: clientDataHash, key: key, list: list,
                path: "list", services: serviceIdentifiers)
            self.updateIdentities(list)
        }
    }

    // MARK: Filling in a password

    private func complete(_ login: PasskeyLoginEntry, key: SymmetricKey, list: PasskeyList) throws {
        let password = try PasskeyVault.password(login, key: key, account: list.account)
        let credential = ASPasswordCredential(user: login.userName ?? "", password: password)
        guard !finished else { return }
        finishing("password handed over: record \(login.itemId)")
        extensionContext.completeRequest(withSelectedCredential: credential, completionHandler: nil)
    }

    /// A service for the log: a URL identifier is a saved address and may carry tokens, so only
    /// its host.
    private static func site(_ service: ASCredentialServiceIdentifier) -> String {
        service.type == .URL ? (URL(string: service.identifier)?.host ?? "?") : service.identifier
    }

    private func pickedPassword(_ identity: ASPasswordCredentialIdentity) {
        let service = identity.serviceIdentifier
        let record = identity.recordIdentifier
        let site = Self.site(service)
        logStep("picked password: service \(site), record \(record ?? "-")")
        model.message = String(localized: "Anmelden bei \(site) …")
        unlock(reason: String(localized: "Mit UwULock anmelden bei \(site)")) { [weak self] _, key, list in
            guard let self else { return }
            let logins = list.snapshot.logins ?? []
            // The login the system listed, and only while it still belongs to that site: an entry
            // left in the system's list from before the login's addresses changed fills nothing.
            if let login = logins.first(where: { $0.itemId == record && PasskeyVault.matches($0, [service]) }) {
                try self.complete(login, key: key, list: list)
                return
            }
            logStep("picked password: not in the list any more; showing the logins", error: true)
            self.passwordList([service], key: key, list: list)
        }
    }

    private func passwordList(_ services: [ASCredentialServiceIdentifier], key: SymmetricKey, list: PasskeyList) {
        let logins = Self.sortedLogins(list)
        let suggested = Set(logins.filter { PasskeyVault.matches($0, services) }.map(\.itemId))
        logStep("password list: \(logins.count) logins, \(suggested.count) match")
        ready()
        progressed = true
        model.busy = false
        model.unlockButton = nil
        model.message = nil
        showLogins(logins, suggested: suggested, key: key, list: list)
    }

    private static func sortedLogins(_ list: PasskeyList) -> [PasskeyLoginEntry] {
        (list.snapshot.logins ?? []).sorted {
            $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
    }

    /// The logins to pick from, the `suggested` ones first, with a search over all.
    private func showLogins(
        _ logins: [PasskeyLoginEntry], suggested: Set<String>, key: SymmetricKey, list: PasskeyList
    ) {
        model.searchable = true
        model.choices = logins.map {
            PasskeyModel.Choice(
                id: $0.itemId, title: $0.name,
                subtitle: [$0.userName, $0.subtitle].compactMap { $0 }.joined(separator: " · "),
                suggested: suggested.contains($0.itemId),
                keywords: ($0.hosts ?? [])
                    + ($0.uris ?? []).map { hint in
                        hint.kind == "domain" || hint.kind == "host"
                            ? hint.value : (URL(string: hint.value)?.host ?? hint.value)
                    })
        }
        model.pick = { [weak self] choice in
            guard let self, let login = logins.first(where: { $0.itemId == choice.id }) else { return }
            self.model.busy = true
            self.trying { try self.complete(login, key: key, list: list) }
        }
    }

    /// The person picked UwULock for a password: its logins for the site, and a search over all.
    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        let site = serviceIdentifiers.first.map(Self.site) ?? ""
        enter("prepareCredentialList(password)")
        logStep("password list asked: \(serviceIdentifiers.count) services, first \(site.isEmpty ? "-" : site)")
        let shown = site.isEmpty ? "…" : site
        model.title = String(localized: "Passwort für \(shown)")
        unlock(reason: String(localized: "Mit UwULock anmelden bei \(site)")) { [weak self] _, key, list in
            guard let self else { return }
            self.passwordList(serviceIdentifiers, key: key, list: list)
            self.updateIdentities(list)
        }
    }

    // MARK: What UwULock doesn't fill

    /// One-time codes: UwULock doesn't say it provides them (Info.plist), so the system shouldn't
    /// ask; should it, a clear word instead of an empty sheet.
    override func prepareOneTimeCodeCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        enter("prepareOneTimeCodeCredentialList: \(serviceIdentifiers.count) services")
        tell(String(localized: "UwULock füllt keine Einmalcodes aus. Öffne UwULock und kopiere den Code dort."))
    }

    /// Text to insert (the edit menu's AutoFill → Passwords on any field; iOS only): not offered
    /// either.
    #if os(iOS)
    override func prepareInterfaceForUserChoosingTextToInsert() {
        enter("prepareInterfaceForUserChoosingTextToInsert")
        tell(
            String(
                localized: "UwULock fügt hier keinen Text ein. Tippe in ein Anmeldefeld und wähle dort UwULock."))
    }
    #endif

    // MARK: Making a passkey

    override func prepareInterface(forPasskeyRegistration registrationRequest: any ASCredentialRequest) {
        enter("prepareInterface(forPasskeyRegistration:): type \(registrationRequest.type.rawValue)")
        guard let request = registrationRequest as? ASPasskeyCredentialRequest,
            let identity = request.credentialIdentity as? ASPasskeyCredentialIdentity
        else {
            fail(PasskeyVaultError.broken("not a passkey registration"))
            return
        }
        let rpId = identity.relyingPartyIdentifier
        let userName = identity.userName
        let userHandle = identity.userHandle
        let clientDataHash = request.clientDataHash
        logStep("registration: rp \(rpId)")
        model.title = String(localized: "Passkey sichern")
        model.message = rpId + " · " + userName
        guard PasskeyVault.validRpId(rpId) else {
            fail(PasskeyVaultError.broken("UwULock keeps no passkeys for this site name"))
            return
        }
        guard request.supportedAlgorithms.isEmpty || request.supportedAlgorithms.contains(.ES256) else {
            fail(PasskeyVaultError.broken("the site takes no ES256 passkeys"))
            return
        }
        // The list first: it names the account the passkey is for, and a list that doesn't open is
        // never replaced by one with only the new passkey.
        unlock(reason: String(localized: "Passkey für \(rpId) in UwULock sichern")) { [weak self] vault, key, list in
            guard let self else { return }
            let made = PasskeyVault.make(rpId: rpId, userName: userName, userHandle: userHandle)
            let kept = try vault.keep(made.entry, privateKey: made.key, list: list, key: key)
            let authData = PasskeyVault.authenticatorData(
                rpId: rpId, flags: self.flags(), credentialId: made.id, publicKey: made.key.publicKey)
            let credential = ASPasskeyRegistrationCredential(
                relyingParty: rpId, clientDataHash: clientDataHash, credentialID: made.id,
                attestationObject: PasskeyVault.attestationObject(authData))
            logStep("registration: made \(short(made.id))")
            self.updateIdentities(kept)
            guard !self.finished else { return }
            self.finishing("registration handing over")
            self.extensionContext.completeRegistrationRequest(using: credential, completionHandler: nil)
        }
    }

    // MARK: Switched on in the settings

    /// Shown once the person picks UwULock for passwords and passkeys: fills the system's list,
    /// and says done once the system took it (or after a few seconds, should it not answer).
    override func prepareInterfaceForExtensionConfiguration() {
        enter("prepareInterfaceForExtensionConfiguration")
        model.title = String(localized: "UwULock-AutoFill")
        unlock(reason: String(localized: "UwULocks Passwörter und Passkeys für das System freigeben")) {
            [weak self] _, _, list in
            guard let self else { return }
            let done: @MainActor () -> Void = { [weak self] in
                guard let self, !self.finished else { return }
                self.finishing("configuration done")
                self.extensionContext.completeExtensionConfigurationRequest()
            }
            self.updateIdentities(list, force: true, done: done)
            self.after(3, done)
        }
    }
}
