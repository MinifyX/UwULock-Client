// What the AutoFill extension knows of the vault: the sealed passkey list the app leaves in the
// App Group folder, and the provider key in the shared Keychain behind Face ID, Touch ID or the
// device passcode. The same format as crates/uwulock-authenticator/src/apple.rs — change both.
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
  /// UwULock never left a list here (switched off, or never unlocked since).
  case noList
  case cancelled
  case broken(String)
}

/// One passkey, as apple.rs `Entry`: binary values URL-safe base64, the private key as the raw
/// P-256 scalar.
struct PasskeyEntry: Codable {
  var credentialId: String
  var rpId: String
  var rpName: String?
  var userName: String?
  var userDisplayName: String?
  var userHandle: String?
  var itemId: String?
  var privateKey: String
  var created: String

  var title: String { userName ?? userDisplayName ?? rpName ?? rpId }
}

struct PasskeySnapshot: Codable {
  var version: Int
  var entries: [PasskeyEntry]
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
  static let aad = Data("uwulock-passkeys-v1".utf8)
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
      guard let data = found as? Data, data.count == 32 else {
        throw PasskeyVaultError.broken("the provider key isn't 32 bytes")
      }
      return SymmetricKey(data: data)
    case errSecUserCanceled, errSecAuthFailed:
      throw PasskeyVaultError.cancelled
    case errSecItemNotFound:
      throw PasskeyVaultError.noList
    default:
      throw PasskeyVaultError.broken("Keychain \(status)")
    }
  }

  // MARK: The seal: 0x01 ‖ nonce ‖ ciphertext ‖ tag, AES-256-GCM.

  static func seal(_ plain: Data, key: SymmetricKey) throws -> Data {
    let box = try AES.GCM.seal(plain, using: key, nonce: AES.GCM.Nonce(), authenticating: aad)
    guard let combined = box.combined else { throw PasskeyVaultError.broken("AES-GCM") }
    return Data([1]) + combined
  }

  static func open(_ sealed: Data, key: SymmetricKey) throws -> Data {
    guard sealed.count >= 1 + 12 + 16, sealed.first == 1 else {
      throw PasskeyVaultError.broken("not a sealed passkey list")
    }
    let box = try AES.GCM.SealedBox(combined: sealed.dropFirst())
    return try AES.GCM.open(box, using: key, authenticating: aad)
  }

  func snapshot(key: SymmetricKey) throws -> PasskeySnapshot {
    guard let sealed = try? Data(contentsOf: listURL) else { throw PasskeyVaultError.noList }
    return try JSONDecoder().decode(PasskeySnapshot.self, from: Self.open(sealed, key: key))
  }

  /// A passkey made here: into the outbox for the app (which takes it into the vault at its next
  /// unlock), and into the list right away, so it signs in before that.
  func keep(_ entry: PasskeyEntry, key: SymmetricKey) throws {
    try FileManager.default.createDirectory(at: outboxURL, withIntermediateDirectories: true)
    let sealed = try Self.seal(JSONEncoder().encode(entry), key: key)
    let file = outboxURL.appendingPathComponent(UUID().uuidString + ".sealed")
    try sealed.write(to: file, options: Self.writing)
    var list = (try? snapshot(key: key)) ?? PasskeySnapshot(version: 1, entries: [])
    list.entries.append(entry)
    try Self.seal(JSONEncoder().encode(list), key: key)
      .write(to: listURL, options: Self.writing)
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

  static func signingKey(_ entry: PasskeyEntry) throws -> P256.Signing.PrivateKey {
    guard let raw = Base64URL.decode(entry.privateKey) else {
      throw PasskeyVaultError.broken("a private key doesn't decode")
    }
    return try P256.Signing.PrivateKey(rawRepresentation: raw)
  }

  /// The signature over the authenticator data and the client data hash, DER.
  static func sign(_ entry: PasskeyEntry, authData: Data, clientDataHash: Data) throws -> Data {
    try signingKey(entry).signature(for: authData + clientDataHash).derRepresentation
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
      userDisplayName: nil, userHandle: userHandle.map(Base64URL.encode), itemId: nil,
      privateKey: Base64URL.encode(key.rawRepresentation), created: formatter.string(from: Date()))
    return (entry, id, key)
  }
}
