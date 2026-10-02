import AuthenticationServices
import Foundation
import Security
import Tauri

struct PasskeyIdentityArgs: Decodable {
  let rpId: String
  let userName: String
  let credentialId: String
  let userHandle: String
  let recordIdentifier: String?
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
        item[kSecValueData as String] = key
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess else {
          invoke.reject("The Keychain said no (\(status)).", code: "failed")
          return
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
      guard let id = Self.fromBase64URL(identity.credentialId) else { return nil }
      return ASPasskeyCredentialIdentity(
        relyingPartyIdentifier: identity.rpId, userName: identity.userName, credentialID: id,
        userHandle: Self.fromBase64URL(identity.userHandle) ?? Data(),
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

  @objc public func passkeysOutbox(_ invoke: Invoke) {
    guard let groups = passkeyGroups() else {
      invoke.resolve(["entries": [JsonObject]()])
      return
    }
    let outbox = groups.folder.appendingPathComponent("outbox", isDirectory: true)
    let names = (try? FileManager.default.contentsOfDirectory(atPath: outbox.path)) ?? []
    var entries: [JsonObject] = []
    for name in names where name.hasSuffix(".sealed") {
      if let data = try? Data(contentsOf: outbox.appendingPathComponent(name)) {
        entries.append(["name": name, "sealed": Self.toBase64URL(data)])
      }
    }
    invoke.resolve(["entries": entries])
  }

  @objc public func passkeysClearOutbox(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PasskeysNamesArgs.self)
    if let groups = passkeyGroups() {
      let outbox = groups.folder.appendingPathComponent("outbox", isDirectory: true)
      for name in args.names where !name.contains("/") && !name.hasPrefix(".") {
        try? FileManager.default.removeItem(at: outbox.appendingPathComponent(name))
      }
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
      ]
      SecItemDelete(query as CFDictionary)
    }
    ASCredentialIdentityStore.shared.removeAllCredentialIdentities { _, _ in
      invoke.resolve()
    }
  }
}
