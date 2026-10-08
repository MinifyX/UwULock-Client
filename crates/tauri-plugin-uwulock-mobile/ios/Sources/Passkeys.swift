import AuthenticationServices
import CryptoKit
import Foundation
import LocalAuthentication
import Security
import Tauri

/// One entry of the system's list, as passkeys/apple.rs `Identity`: a passkey (`kind` "passkey")
/// or a login's address (`kind` "password", `rpId` the service, `serviceType` domain or url).
struct PasskeyIdentityArgs: Decodable {
  let kind: String?
  let rpId: String
  let userName: String
  let credentialId: String?
  let userHandle: String?
  let recordIdentifier: String?
  let serviceType: String?
}

struct PasskeysStoreArgs: Decodable {
  let list: String
  let key: String?
  let identities: [PasskeyIdentityArgs]
}

struct PasskeysNamesArgs: Decodable {
  let names: [String]
}

/// The app's half of the AutoFill extension (apps/desktop/src-tauri/apple/PasskeyProvider): the
/// sealed passkey list and the outbox in the App Group folder, the provider key in the shared
/// Keychain, and the system's list of passkeys. Rust decides what goes in (passkeys/apple.rs);
/// nothing here sees a private key unsealed.
///
/// The groups come from the app's Info.plist (UwULockAppGroup, UwULockKeychainGroup) and only
/// exist in a build signed with an Apple developer team: until then the status says why not.
///
/// One Keychain slot holds the provider key of the account whose list is there. Rust hands the key
/// over with every list; the item is (re)written only when the one there isn't this key — told by
/// its label (`key_id` in crates/uwulock-authenticator/src/apple.rs), which takes no Face ID to
/// read — or is gone.
extension UwuLockMobilePlugin {
  private static let passkeyService = "app.uwulock.passkeys"
  private static let passkeyAccount = "provider-key"

  private func passkeyGroups() -> (folder: URL, keychain: String)? {
    guard
      let group = Bundle.main.object(forInfoDictionaryKey: "UwULockAppGroup") as? String,
      let keychain = Bundle.main.object(forInfoDictionaryKey: "UwULockKeychainGroup") as? String,
      !group.isEmpty, !keychain.isEmpty,
      let container = FileManager.default.containerURL(
        forSecurityApplicationGroupIdentifier: group)
    else { return nil }
    return (container.appendingPathComponent("Passkeys", isDirectory: true), keychain)
  }

  private static func fromBase64URL(_ text: String) -> Data? {
    var base = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
    while base.count % 4 != 0 { base += "=" }
    return Data(base64Encoded: base)
  }

  private static func toBase64URL(_ data: Data) -> String {
    data.base64EncodedString()
      .replacingOccurrences(of: "+", with: "-")
      .replacingOccurrences(of: "/", with: "_")
      .replacingOccurrences(of: "=", with: "")
  }

  /// "UwULock passkeys " + hex of the first 8 bytes of SHA-256("uwulock-provider-key-id-v1" ‖ key),
  /// as `key_id` in apple.rs.
  private static func keyLabel(_ key: Data) -> String {
    var input = Data("uwulock-provider-key-id-v1".utf8)
    input.append(key)
    defer { input.resetBytes(in: 0..<input.count) }
    let hex = SHA256.hash(data: input).prefix(8).map { (byte: UInt8) -> String in
      String(format: "%02x", byte)
    }
    return "UwULock passkeys " + hex.joined()
  }

  /// Whether the Keychain has the provider key with this label. Attributes only, and never any UI:
  /// when the Keychain wants one anyway, the item counts as missing and is written again.
  private func keyThere(label: String, keychain: String) -> Bool {
    let context = LAContext()
    context.interactionNotAllowed = true
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: Self.passkeyService,
      kSecAttrAccount as String: Self.passkeyAccount,
      kSecAttrAccessGroup as String: keychain,
      kSecAttrSynchronizable as String: false,
      kSecAttrLabel as String: label,
      kSecReturnAttributes as String: true,
      kSecMatchLimit as String: kSecMatchLimitOne,
      kSecUseAuthenticationContext as String: context,
    ]
    var found: AnyObject?
    return SecItemCopyMatching(query as CFDictionary, &found) == errSecSuccess
  }

  /// A file name in the outbox: nothing that leaves it.
  private static func outboxFile(_ name: String) -> Bool {
    name.hasSuffix(".sealed") && name != ".sealed" && !name.hasPrefix(".")
      && !name.contains("/") && !name.contains("\\")
  }

  @objc public func passkeysStatus(_ invoke: Invoke) {
    if passkeyGroups() == nil {
      invoke.resolve([
        "ready": false,
        "reason":
          "This build isn't signed with an Apple developer team: the AutoFill extension can't reach the vault (docs/passkeys.md).",
      ])
      return
    }
    invoke.resolve(["ready": true])
  }

  @objc public func passkeysStore(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PasskeysStoreArgs.self)
    guard let groups = passkeyGroups() else {
      invoke.reject("No App Group in this build.", code: "unsupported")
      return
    }
    guard let list = Self.fromBase64URL(args.list) else {
      invoke.reject("The list isn't base64.", code: "failed")
      return
    }
    do {
      try FileManager.default.createDirectory(
        at: groups.folder.appendingPathComponent("outbox", isDirectory: true),
        withIntermediateDirectories: true)
      if let encoded = args.key {
        guard var key = Self.fromBase64URL(encoded), key.count == 32 else {
          invoke.reject("The key isn't 32 bytes.", code: "failed")
          return
        }
        defer { key.resetBytes(in: 0..<key.count) }
        let label = Self.keyLabel(key)
        if !keyThere(label: label, keychain: groups.keychain) {
          var accessError: Unmanaged<CFError>?
          guard
            let access = SecAccessControlCreateWithFlags(
              nil, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, .userPresence, &accessError)
          else {
            invoke.reject("The iPhone couldn't protect the key.", code: "unavailable")
            return
          }
          let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.passkeyService,
            kSecAttrAccount as String: Self.passkeyAccount,
            kSecAttrAccessGroup as String: groups.keychain,
            kSecAttrSynchronizable as String: false,
          ]
          SecItemDelete(query as CFDictionary)
          var item = query
          item[kSecAttrAccessControl as String] = access
          item[kSecAttrLabel as String] = label
          item[kSecValueData as String] = key
          let status = SecItemAdd(item as CFDictionary, nil)
          guard status == errSecSuccess else {
            invoke.reject("The Keychain said no (\(status)).", code: "failed")
            return
          }
        }
      }
      try list.write(
        to: groups.folder.appendingPathComponent("passkeys.sealed"),
        options: [.atomic, .completeFileProtection])
    } catch {
      invoke.reject("\(error)", code: "failed")
      return
    }

    // The system's list of passkeys (QuickType bar, the sheet): iOS 17 and later.
    guard #available(iOS 17.0, *) else {
      invoke.resolve()
      return
    }
    let identities: [ASCredentialIdentity] = args.identities.compactMap { identity in
      if identity.kind == "password" {
        return ASPasswordCredentialIdentity(
          serviceIdentifier: ASCredentialServiceIdentifier(
            identifier: identity.rpId, type: identity.serviceType == "url" ? .URL : .domain),
          user: identity.userName, recordIdentifier: identity.recordIdentifier)
      }
      guard let id = identity.credentialId.flatMap(Self.fromBase64URL), !id.isEmpty else {
        return nil
      }
      return ASPasskeyCredentialIdentity(
        relyingPartyIdentifier: identity.rpId, userName: identity.userName, credentialID: id,
        userHandle: identity.userHandle.flatMap(Self.fromBase64URL) ?? Data(),
        recordIdentifier: identity.recordIdentifier)
    }
    ASCredentialIdentityStore.shared.getState { state in
      guard state.isEnabled else {
        invoke.resolve()
        return
      }
      ASCredentialIdentityStore.shared.replaceCredentialIdentities(identities) { _, _ in
        invoke.resolve()
      }
    }
  }

  // MARK: UwULock as the AutoFill provider

  private func providerState(enabled: Bool?) -> JsonObject {
    var state: JsonObject = [
      "supported": passkeyGroups() != nil,
      "direct": false,
    ]
    if #available(iOS 18.0, *) { state["direct"] = true }
    if let enabled { state["enabled"] = enabled }
    return state
  }

  /// Whether UwULock is switched on in Settings → General → AutoFill & Passwords.
  @objc public func providerStatus(_ invoke: Invoke) {
    ASCredentialIdentityStore.shared.getState { state in
      invoke.resolve(self.providerState(enabled: state.isEnabled))
    }
  }

  /// iOS 18+: the system asks the person in a sheet of its own; iOS 17: the AutoFill settings
  /// open (the person comes back by themselves). The state afterwards either way.
  @objc public func providerRequest(_ invoke: Invoke) {
    guard passkeyGroups() != nil else {
      invoke.resolve(providerState(enabled: false))
      return
    }
    let answer = {
      ASCredentialIdentityStore.shared.getState { state in
        invoke.resolve(self.providerState(enabled: state.isEnabled))
      }
    }
    DispatchQueue.main.async {
      if #available(iOS 18.0, *) {
        ASSettingsHelper.requestToTurnOnCredentialProviderExtension { _ in answer() }
      } else if #available(iOS 17.0, *) {
        ASSettingsHelper.openCredentialProviderAppSettings { _ in answer() }
      } else {
        answer()
      }
    }
  }

  /// The outbox's files, and those set aside in `outbox/unreadable/` as "unreadable/<name>": Rust
  /// tries them again whenever their account is open.
  @objc public func passkeysOutbox(_ invoke: Invoke) {
    guard let groups = passkeyGroups() else {
      invoke.resolve(["entries": [JsonObject]()])
      return
    }
    let outbox = groups.folder.appendingPathComponent("outbox", isDirectory: true)
    var entries: [JsonObject] = []
    for (folder, prefix) in [(outbox, ""), (outbox.appendingPathComponent("unreadable"), "unreadable/")] {
      let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
      for name in names where Self.outboxFile(name) {
        if let data = try? Data(contentsOf: folder.appendingPathComponent(name)) {
          entries.append(["name": prefix + name, "sealed": Self.toBase64URL(data)])
        }
      }
    }
    invoke.resolve(["entries": entries])
  }

  /// Removes outbox files the app took in: "<name>" from the outbox, "unreadable/<name>" from the
  /// set-aside ones. "aside:<name>" moves one into `outbox/unreadable/` instead — it didn't open,
  /// and is never deleted for that.
  @objc public func passkeysClearOutbox(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PasskeysNamesArgs.self)
    if let groups = passkeyGroups() {
      let outbox = groups.folder.appendingPathComponent("outbox", isDirectory: true)
      let unreadable = outbox.appendingPathComponent("unreadable", isDirectory: true)
      for name in args.names {
        if name.hasPrefix("aside:") {
          let file = String(name.dropFirst("aside:".count))
          guard Self.outboxFile(file) else { continue }
          try? FileManager.default.createDirectory(at: unreadable, withIntermediateDirectories: true)
          try? FileManager.default.moveItem(
            at: outbox.appendingPathComponent(file), to: unreadable.appendingPathComponent(file))
        } else if name.hasPrefix("unreadable/") {
          let file = String(name.dropFirst("unreadable/".count))
          guard Self.outboxFile(file) else { continue }
          try? FileManager.default.removeItem(at: unreadable.appendingPathComponent(file))
        } else if Self.outboxFile(name) {
          try? FileManager.default.removeItem(at: outbox.appendingPathComponent(name))
        }
      }
    }
    invoke.resolve()
  }

  // MARK: The AutoFill protocol

  /// What the AutoFill extension logged (AutoFillLog.swift: Passkeys/autofill.log in the App
  /// Group folder): step names, hosts, counts and error codes, nothing secret. Empty when the
  /// extension never wrote one; `supported` false without an App Group (unsigned build).
  @objc public func autofillLog(_ invoke: Invoke) {
    guard let groups = passkeyGroups() else {
      invoke.resolve(["supported": false, "text": ""])
      return
    }
    let file = groups.folder.appendingPathComponent("autofill.log")
    let data = (try? Data(contentsOf: file)) ?? Data()
    // The extension keeps it under 64 KB; a cut-off first line is Rust's to drop.
    let text = String(decoding: data.suffix(64 * 1024), as: UTF8.self)
    invoke.resolve(["supported": true, "text": text])
  }

  @objc public func autofillLogClear(_ invoke: Invoke) {
    if let groups = passkeyGroups() {
      try? FileManager.default.removeItem(at: groups.folder.appendingPathComponent("autofill.log"))
    }
    invoke.resolve()
  }

  @objc public func passkeysClear(_ invoke: Invoke) {
    if let groups = passkeyGroups() {
      try? FileManager.default.removeItem(at: groups.folder.appendingPathComponent("passkeys.sealed"))
      let query: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: Self.passkeyService,
        kSecAttrAccount as String: Self.passkeyAccount,
        kSecAttrAccessGroup as String: groups.keychain,
        kSecAttrSynchronizable as String: false,
      ]
      SecItemDelete(query as CFDictionary)
    }
    ASCredentialIdentityStore.shared.removeAllCredentialIdentities { _, _ in
      invoke.resolve()
    }
  }
}
