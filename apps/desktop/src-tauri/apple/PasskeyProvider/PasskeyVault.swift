// What the AutoFill extension knows of the vault: the sealed passkey list of one account the app
// leaves in the App Group folder, and that account's provider key in the shared Keychain behind
// Face ID, Touch ID or the device passcode. The same format as
// crates/uwulock-authenticator/src/apple.rs — change both.
//
// iOS 17+ and macOS 14+. Nothing here works before the app and the extension are signed with an
// Apple developer team (App Group, Keychain group): docs/passkeys.md.

import CryptoKit
import Foundation
import LocalAuthentication
import Security

enum PasskeyVaultError: Error {
  /// The build isn't signed with a team: no App Group, no Keychain group.
  case notSetUp
  /// UwULock never left a list here (switched off, logged out, or never unlocked since).
  case noList
  /// The list is older than one this extension already saw: put back, not UwULock's.
  case stale
  case cancelled
  case broken(String)
}

/// One passkey in the list, as apple.rs `ListEntry`: binary values URL-safe base64; the private key
/// (the raw P-256 scalar) sealed on its own in `sealedKey`, opened only to sign with it.
struct PasskeyListEntry: Codable {
  var credentialId: String
  var rpId: String
  var rpName: String?
  var userName: String?
  var userDisplayName: String?
  var userHandle: String?
  var itemId: String?
  var sealedKey: String
  var created: String?

  var title: String { userName ?? userDisplayName ?? rpName ?? rpId }
}

/// A passkey made here, for the app's outbox, as apple.rs `Entry`: the private key as the raw P-256
/// scalar, and the generation of the list written with it.
struct PasskeyEntry: Codable {
  var credentialId: String
  var rpId: String
  var rpName: String?
  var userName: String?
  var userDisplayName: String?
  var userHandle: String?
  var privateKey: String
  var created: String
  var generation: UInt64
}

/// As apple.rs `Snapshot`; the account comes from the file's header.
struct PasskeySnapshot: Codable {
  var version: Int
  var generation: UInt64
  var entries: [PasskeyListEntry]
}

/// The list as opened: whose it is, and what is in it.
struct PasskeyList {
  let account: String
  var snapshot: PasskeySnapshot
}

enum Base64URL {
  static func encode(_ data: Data) -> String {
    data.base64EncodedString()
      .replacingOccurrences(of: "+", with: "-")
      .replacingOccurrences(of: "/", with: "_")
      .replacingOccurrences(of: "=", with: "")
  }

  static func decode(_ text: String) -> Data? {
    var base = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
    while base.count % 4 != 0 { base += "=" }
    return Data(base64Encoded: base)
  }
}

final class PasskeyVault {
  static let domain = "uwulock-passkeys-v2"
  static let version: UInt8 = 2
  static let service = "app.uwulock.passkeys"
  static let account = "provider-key"
  /// UwULock's AAGUID (uwulock-core passkey.rs).
  static let aaguid = Data([
    0x4d, 0x0c, 0x2e, 0x23, 0x4c, 0x15, 0xc4, 0x11, 0x9b, 0xd1, 0xf2, 0x65, 0xe4, 0x26, 0x6a, 0xd6,
  ])
  static let up: UInt8 = 0x01
  static let uv: UInt8 = 0x04
  static let be: UInt8 = 0x08
  static let bs: UInt8 = 0x10
  static let at: UInt8 = 0x40

  let appGroup: String
  let keychainGroup: String
  let folder: URL

  /// The groups come from the extension's Info.plist (UwULockAppGroup, UwULockKeychainGroup),
  /// filled in when it is signed.
  init() throws {
    guard
      let group = Bundle.main.object(forInfoDictionaryKey: "UwULockAppGroup") as? String,
      let keychain = Bundle.main.object(forInfoDictionaryKey: "UwULockKeychainGroup") as? String,
      !group.isEmpty, !keychain.isEmpty, !keychain.hasPrefix("."),
      let container = FileManager.default.containerURL(
        forSecurityApplicationGroupIdentifier: group)
    else { throw PasskeyVaultError.notSetUp }
    appGroup = group
    keychainGroup = keychain
    folder = container.appendingPathComponent("Passkeys", isDirectory: true)
  }

  #if os(iOS)
    static let writing: Data.WritingOptions = [.atomic, .completeFileProtection]
  #else
    static let writing: Data.WritingOptions = [.atomic]
  #endif

  var listURL: URL { folder.appendingPathComponent("passkeys.sealed") }
  var outboxURL: URL { folder.appendingPathComponent("outbox", isDirectory: true) }

  /// The provider key: asks for Face ID, Touch ID or the passcode. Blocks — never on the main
  /// thread.
  func key(reason: String) throws -> SymmetricKey {
    let context = LAContext()
    context.localizedReason = reason
    var query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: Self.service,
      kSecAttrAccount as String: Self.account,
      kSecAttrAccessGroup as String: keychainGroup,
      kSecAttrSynchronizable as String: false,
      kSecReturnData as String: true,
      kSecMatchLimit as String: kSecMatchLimitOne,
      kSecUseAuthenticationContext as String: context,
    ]
    #if os(macOS)
      query[kSecUseDataProtectionKeychain as String] = true
    #endif
    var found: AnyObject?
    let status = SecItemCopyMatching(query as CFDictionary, &found)
    switch status {
    case errSecSuccess:
      guard var data = found as? Data, data.count == 32 else {
        throw PasskeyVaultError.broken("the provider key isn't 32 bytes")
      }
      defer { data.resetBytes(in: 0..<data.count) }
      return SymmetricKey(data: data)
    case errSecUserCanceled, errSecAuthFailed:
      throw PasskeyVaultError.cancelled
    case errSecItemNotFound:
      throw PasskeyVaultError.noList
    default:
      throw PasskeyVaultError.broken("Keychain \(status)")
    }
  }

  // MARK: The seal, as apple.rs:
  // 0x02 ‖ n ‖ account id (n bytes) ‖ nonce ‖ ciphertext ‖ tag, AES-256-GCM, authenticating
  // "uwulock-passkeys-v2:<list|outbox>:<account id>".

  enum Kind: String {
    case list
    case outbox
  }

  static func validAccount(_ account: String) -> Bool {
    let bytes = Array(account.utf8)
    return !bytes.isEmpty && bytes.count <= 64
      && bytes.allSatisfy { (b: UInt8) -> Bool in
        (b >= 0x30 && b <= 0x39) || (b >= 0x41 && b <= 0x5a) || (b >= 0x61 && b <= 0x7a)
          || b == 0x2d || b == 0x5f
      }
  }

  static func aad(_ kind: Kind, account: String) -> Data {
    Data("\(domain):\(kind.rawValue):\(account)".utf8)
  }

  static func keyAAD(account: String, credentialId: String) -> Data {
    Data("\(domain):key:\(account):\(credentialId)".utf8)
  }

  static func seal(_ plain: Data, key: SymmetricKey, kind: Kind, account: String) throws -> Data {
    guard validAccount(account) else { throw PasskeyVaultError.broken("not an account id") }
    let box = try AES.GCM.seal(
      plain, using: key, nonce: AES.GCM.Nonce(), authenticating: aad(kind, account: account))
    guard let combined = box.combined else { throw PasskeyVaultError.broken("AES-GCM") }
    let name = Data(account.utf8)
    return Data([version, UInt8(name.count)]) + name + combined
  }

  /// The account a sealed file names; only `open` proves it.
  static func accountOf(_ sealed: Data) -> String? {
    let bytes = [UInt8](sealed)
    guard bytes.count >= 2, bytes[0] == version, bytes.count >= 2 + Int(bytes[1]) else {
      return nil
    }
    guard let account = String(bytes: bytes[2..<(2 + Int(bytes[1]))], encoding: .utf8),
      validAccount(account)
    else { return nil }
    return account
  }

  static func open(_ sealed: Data, key: SymmetricKey, kind: Kind) throws -> (
    account: String, plain: Data
  ) {
    guard let account = Self.accountOf(sealed) else {
      throw PasskeyVaultError.broken("not a sealed passkey list")
    }
    let body = Data([UInt8](sealed).dropFirst(2 + account.utf8.count))
    guard body.count >= 12 + 16 else { throw PasskeyVaultError.broken("a cut-off passkey list") }
    let box = try AES.GCM.SealedBox(combined: body)
    let plain = try AES.GCM.open(box, using: key, authenticating: aad(kind, account: account))
    return (account, plain)
  }

  // MARK: The list

  /// The highest generation seen of an account's list, kept in the extension's own container (not
  /// the App Group, which the list comes from).
  private static func seenKey(_ account: String) -> String { "uwulock.generation.\(account)" }

  static func seen(_ account: String) -> UInt64 {
    UInt64(UserDefaults.standard.string(forKey: seenKey(account)) ?? "") ?? 0
  }

  static func see(_ generation: UInt64, account: String) {
    if generation > seen(account) {
      UserDefaults.standard.set(String(generation), forKey: seenKey(account))
    }
  }

  static func nowMs() -> UInt64 { UInt64(max(0, Date().timeIntervalSince1970 * 1000)) }

  /// The list, if it opens and isn't older than one seen before.
  func list(key: SymmetricKey) throws -> PasskeyList {
    guard let sealed = try? Data(contentsOf: listURL) else { throw PasskeyVaultError.noList }
    let opened = try Self.open(sealed, key: key, kind: .list)
    let snapshot = try JSONDecoder().decode(PasskeySnapshot.self, from: opened.plain)
    if snapshot.generation < Self.seen(opened.account) {
      throw PasskeyVaultError.stale
    }
    Self.see(snapshot.generation, account: opened.account)
    return PasskeyList(account: opened.account, snapshot: snapshot)
  }

  /// The private key of one list entry, opened just to sign with it.
  static func signingKey(_ entry: PasskeyListEntry, key: SymmetricKey, account: String) throws
    -> P256.Signing.PrivateKey
  {
    guard let sealed = Base64URL.decode(entry.sealedKey), sealed.count == 12 + 32 + 16 else {
      throw PasskeyVaultError.broken("a private key doesn't decode")
    }
    let box = try AES.GCM.SealedBox(combined: sealed)
    var raw = try AES.GCM.open(
      box, using: key, authenticating: keyAAD(account: account, credentialId: entry.credentialId))
    defer { raw.resetBytes(in: 0..<raw.count) }
    return try P256.Signing.PrivateKey(rawRepresentation: raw)
  }

  /// A passkey made here: into the outbox for the app (which takes it into the vault at its next
  /// unlock), and into the list right away, so it signs in before that. `list` is the one just
  /// opened: nothing replaces a list that didn't open.
  func keep(_ made: PasskeyEntry, privateKey: P256.Signing.PrivateKey, list: PasskeyList,
    key: SymmetricKey
  ) throws -> PasskeyList {
    var entry = made
    let generation = max(list.snapshot.generation + 1, Self.nowMs())
    entry.generation = generation
    try FileManager.default.createDirectory(at: outboxURL, withIntermediateDirectories: true)
    var plain = try JSONEncoder().encode(entry)
    defer { plain.resetBytes(in: 0..<plain.count) }
    let sealed = try Self.seal(plain, key: key, kind: .outbox, account: list.account)
    let file = outboxURL.appendingPathComponent(UUID().uuidString + ".sealed")
    try sealed.write(to: file, options: Self.writing)

    var raw = privateKey.rawRepresentation
    defer { raw.resetBytes(in: 0..<raw.count) }
    let box = try AES.GCM.seal(
      raw, using: key, nonce: AES.GCM.Nonce(),
      authenticating: Self.keyAAD(account: list.account, credentialId: entry.credentialId))
    guard let sealedKey = box.combined else { throw PasskeyVaultError.broken("AES-GCM") }
    var next = list
    next.snapshot.generation = generation
    next.snapshot.entries.append(
      PasskeyListEntry(
        credentialId: entry.credentialId, rpId: entry.rpId, rpName: entry.rpName,
        userName: entry.userName, userDisplayName: entry.userDisplayName,
        userHandle: entry.userHandle, itemId: nil, sealedKey: Base64URL.encode(sealedKey),
        created: entry.created))
    try Self.seal(JSONEncoder().encode(next.snapshot), key: key, kind: .list, account: list.account)
      .write(to: listURL, options: Self.writing)
    Self.see(generation, account: list.account)
    return next
  }

  // MARK: WebAuthn

  static func sha256(_ data: Data) -> Data { Data(SHA256.hash(data: data)) }

  static func cborHead(_ major: UInt8, _ length: Int) -> Data {
    let major = major << 5
    switch length {
    case 0..<24: return Data([major | UInt8(length)])
    case 24..<256: return Data([major | 24, UInt8(length)])
    default: return Data([major | 25, UInt8(length >> 8), UInt8(length & 0xff)])
    }
  }

  static func cborBytes(_ data: Data) -> Data { cborHead(2, data.count) + data }
  static func cborText(_ text: String) -> Data {
    let bytes = Data(text.utf8)
    return cborHead(3, bytes.count) + bytes
  }

  /// {1: 2, 3: -7, -1: 1, -2: x, -3: y}
  static func coseKey(_ key: P256.Signing.PublicKey) -> Data {
    let raw = key.rawRepresentation
    return Data([0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21]) + cborBytes(raw.prefix(32))
      + Data([0x22]) + cborBytes(raw.suffix(32))
  }

  static func authenticatorData(
    rpId: String, flags: UInt8, credentialId: Data? = nil, publicKey: P256.Signing.PublicKey? = nil
  ) -> Data {
    var out = sha256(Data(rpId.utf8))
    let attested = credentialId != nil && publicKey != nil
    out.append(attested ? flags | at : flags & ~at)
    out.append(contentsOf: [0, 0, 0, 0])
    if let id = credentialId, let key = publicKey {
      out.append(aaguid)
      out.append(contentsOf: [UInt8(id.count >> 8), UInt8(id.count & 0xff)])
      out.append(id)
      out.append(coseKey(key))
    }
    return out
  }

  static func attestationObject(_ authData: Data) -> Data {
    Data([0xa3]) + cborText("fmt") + cborText("none") + cborText("attStmt") + Data([0xa0])
      + cborText("authData") + cborBytes(authData)
  }

  /// The signature over the authenticator data and the client data hash, DER.
  static func sign(_ key: P256.Signing.PrivateKey, authData: Data, clientDataHash: Data) throws
    -> Data
  {
    try key.signature(for: authData + clientDataHash).derRepresentation
  }

  /// Whether UwULock takes `rpId`: the host-name half of uwulock_authenticator's `rpid::valid`
  /// (lower-case letters, digits and hyphens, at most 253 characters, no IP address, a dot unless
  /// it is `localhost`). The app refuses an outbox entry whose rpId fails it, so the extension
  /// doesn't make such a passkey either. Public suffixes aren't looked up here: the system only
  /// hands over an rpId the site or app may speak for, and the app sets such an entry aside.
  static func validRpId(_ rpId: String) -> Bool {
    guard !rpId.isEmpty, rpId.utf8.count <= 253 else { return false }
    let labels = rpId.split(separator: ".", omittingEmptySubsequences: false)
    let ldh = labels.allSatisfy { label in
      !label.isEmpty && label.utf8.count <= 63 && !label.hasPrefix("-") && !label.hasSuffix("-")
        && label.utf8.allSatisfy { b in
          (b >= 0x61 && b <= 0x7a) || (b >= 0x30 && b <= 0x39) || b == 0x2d
        }
    }
    guard ldh, let last = labels.last, !last.utf8.allSatisfy({ $0 >= 0x30 && $0 <= 0x39 })
    else { return false }
    return labels.count > 1 || rpId == "localhost"
  }

  /// A new passkey: a 16-byte id (a GUID as Bitwarden keeps it) and a P-256 key.
  static func make(
    rpId: String, userName: String?, userHandle: Data?
  ) -> (entry: PasskeyEntry, id: Data, key: P256.Signing.PrivateKey) {
    let key = P256.Signing.PrivateKey()
    var id = Data(count: 16)
    _ = id.withUnsafeMutableBytes { SecRandomCopyBytes(kSecRandomDefault, 16, $0.baseAddress!) }
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let entry = PasskeyEntry(
      credentialId: Base64URL.encode(id), rpId: rpId, rpName: nil, userName: userName,
      userDisplayName: nil, userHandle: userHandle.map(Base64URL.encode),
      privateKey: Base64URL.encode(key.rawRepresentation), created: formatter.string(from: Date()),
      generation: 0)
    return (entry, id, key)
  }
}
