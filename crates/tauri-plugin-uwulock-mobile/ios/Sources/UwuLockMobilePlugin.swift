import Foundation
import LocalAuthentication
import Security
import SwiftRs
import Tauri
import UIKit
import UniformTypeIdentifiers
import WebKit

struct NameArgs: Decodable {
  let name: String
}

struct PromptArgs: Decodable {
  let name: String
  let title: String
  let subtitle: String?
  let cancel: String
}

struct AppearanceArgs: Decodable {
  let dark: Bool
  let background: String
}

struct CopyArgs: Decodable {
  let text: String
  let expiresInSeconds: Double?
}

/// UwULock's own iOS code, called from Rust (tauri-plugin-uwulock-mobile).
///
/// Unlocking with Face ID or Touch ID: 32 random bytes per account in a
/// Keychain item that only opens after Face ID or Touch ID with the fingers or
/// face enrolled right now (`biometryCurrentSet`), on this device only, and
/// only while the iPhone has a passcode. Not in backups, not in iCloud
/// Keychain. Rust stretches the bytes into the key that seals the account's
/// user key.
///
/// Copying: the copy stays on this device (no Universal Clipboard) and
/// expires by itself.
///
/// New phone features (the Wi-Fi "connect" button) get an `@objc` function
/// here and a method in src/lib.rs — see docs/mobile.md.
class UwuLockMobilePlugin: Plugin {
  private let service = "app.uwulock.unlock"
  /// The pasteboard's change count right after UwULock's last copy.
  private var lastCopy: Int?
  /// What covers the vault while UwULock isn't in front (the app switcher's picture).
  private var covers: [UIView] = []
  /// The page's theme, for the cover's colour.
  private var dark: Bool?

  @objc public override func load(webview: WKWebView) {
    super.load(webview: webview)
    let center = NotificationCenter.default
    center.addObserver(
      self, selector: #selector(coverScreen), name: UIApplication.willResignActiveNotification,
      object: nil)
    center.addObserver(
      self, selector: #selector(uncoverScreen), name: UIApplication.didBecomeActiveNotification,
      object: nil)
    excludeDataFromBackup()
  }

  // MARK: Privacy

  /// iOS keeps a picture of the last screen for the app switcher (and shows it while the app
  /// starts again): an empty screen instead of the open vault. Android's FLAG_SECURE does the
  /// same there (MainActivity).
  @objc private func coverScreen() {
    DispatchQueue.main.async {
      guard self.covers.isEmpty else { return }
      for scene in UIApplication.shared.connectedScenes {
        for window in (scene as? UIWindowScene)?.windows ?? [] {
          let cover = UIView(frame: window.bounds)
          cover.autoresizingMask = [.flexibleWidth, .flexibleHeight]
          if let dark = self.dark {
            cover.backgroundColor =
              dark
              ? UIColor(red: 0x14 / 255, green: 0x10 / 255, blue: 0x16 / 255, alpha: 1)
              : UIColor(red: 0xF8 / 255, green: 0xF4 / 255, blue: 0xF6 / 255, alpha: 1)
          } else {
            cover.backgroundColor = .systemBackground
          }
          window.addSubview(cover)
          self.covers.append(cover)
        }
      }
    }
  }

  @objc private func uncoverScreen() {
    DispatchQueue.main.async {
      for cover in self.covers { cover.removeFromSuperview() }
      self.covers.removeAll()
    }
  }

  /// UwULock's data folder (Library/Application Support/<bundle id>, Tauri's app data dir) stays
  /// out of iCloud and computer backups, like Android's (allowBackup="false"): the vault comes
  /// back from the server, and the copy of the user key for Face ID only opens with a Keychain
  /// item that never leaves this iPhone anyway.
  private func excludeDataFromBackup() {
    let files = FileManager.default
    guard
      let support = files.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
    else { return }
    var folder = support.appendingPathComponent(
      Bundle.main.bundleIdentifier ?? "app.uwulock", isDirectory: true)
    do {
      try files.createDirectory(at: folder, withIntermediateDirectories: true)
      var values = URLResourceValues()
      values.isExcludedFromBackup = true
      try folder.setResourceValues(values)
    } catch {
      NSLog("UwULock: couldn't keep the data folder out of backups: \(error)")
    }
  }

  // MARK: Unlocking with a biometric

  @objc public func unlockStatus(_ invoke: Invoke) {
    let context = LAContext()
    var error: NSError?
    let available = context.canEvaluatePolicy(
      .deviceOwnerAuthenticationWithBiometrics, error: &error)
    var result: JsonObject = ["available": available, "kind": kind(context)]
    if !available {
      switch (error as? LAError)?.code {
      case .biometryNotEnrolled?: result["reason"] = "none-enrolled"
      case .passcodeNotSet?: result["reason"] = "no-passcode"
      case .biometryNotAvailable?: result["reason"] = "no-hardware"
      case .biometryLockout?: result["reason"] = "lockout"
      default: result["reason"] = "unavailable"
      }
    }
    invoke.resolve(result)
  }

  private func kind(_ context: LAContext) -> String {
    switch context.biometryType {
    case .faceID: return "faceId"
    case .touchID: return "touchId"
    default:
      if #available(iOS 17.0, *), context.biometryType == .opticID { return "opticId" }
      return "biometric"
    }
  }

  @objc public func unlockCreate(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PromptArgs.self)
    DispatchQueue.global(qos: .userInitiated).async {
      var secret = Data(count: 32)
      let random = secret.withUnsafeMutableBytes {
        SecRandomCopyBytes(kSecRandomDefault, 32, $0.baseAddress!)
      }
      guard random == errSecSuccess else {
        invoke.reject("The iPhone couldn't make random bytes.", code: "failed")
        return
      }
      var accessError: Unmanaged<CFError>?
      guard
        let access = SecAccessControlCreateWithFlags(
          nil, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, .biometryCurrentSet, &accessError)
      else {
        invoke.reject("The iPhone couldn't protect the key.", code: "unavailable")
        return
      }
      SecItemDelete(self.query(args.name) as CFDictionary)
      var item = self.query(args.name)
      item[kSecAttrAccessControl as String] = access
      item[kSecValueData as String] = secret
      let status = SecItemAdd(item as CFDictionary, nil)
      guard status == errSecSuccess else {
        let code = status == errSecNotAvailable ? "unavailable" : "failed"
        invoke.reject("The Keychain said no (\(status)).", code: code)
        return
      }
      let out: JsonObject = ["secret": secret.base64EncodedString()]
      invoke.resolve(out)
      secret.resetBytes(in: 0..<secret.count)
    }
  }

  @objc public func unlockOpen(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PromptArgs.self)
    // Reading waits for Face ID or Touch ID: never on Tauri's IPC queue.
    DispatchQueue.global(qos: .userInitiated).async {
      let context = LAContext()
      context.localizedCancelTitle = args.cancel
      var query = self.query(args.name)
      query[kSecReturnData as String] = true
      query[kSecMatchLimit as String] = kSecMatchLimitOne
      query[kSecUseAuthenticationContext as String] = context
      query[kSecUseOperationPrompt as String] = args.subtitle?.isEmpty == false
        ? args.subtitle! : args.title
      var found: AnyObject?
      let status = SecItemCopyMatching(query as CFDictionary, &found)
      switch status {
      case errSecSuccess:
        guard let data = found as? Data, data.count == 32 else {
          invoke.reject("The Keychain gave back something else.", code: "failed")
          return
        }
        let out: JsonObject = ["secret": data.base64EncodedString()]
        invoke.resolve(out)
      case errSecUserCanceled:
        invoke.reject("Cancelled.", code: "cancelled")
      case errSecItemNotFound:
        // With `biometryCurrentSet`, iOS deletes the item when Face ID or Touch ID changes.
        invoke.reject("Nothing is kept for unlocking on this iPhone.", code: "invalidated")
      case errSecAuthFailed:
        invoke.reject("Face ID or Touch ID didn't recognise you.", code: "failed")
      default:
        invoke.reject("The Keychain said no (\(status)).", code: "failed")
      }
    }
  }

  @objc public func unlockDelete(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(NameArgs.self)
    SecItemDelete(query(args.name) as CFDictionary)
    invoke.resolve()
  }

  private func query(_ name: String) -> [String: Any] {
    return [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: name,
      kSecAttrSynchronizable as String: false,
    ]
  }

  // MARK: Copying

  @objc public func copySecret(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(CopyArgs.self)
    DispatchQueue.main.async {
      var options: [UIPasteboard.OptionsKey: Any] = [.localOnly: true]
      if let seconds = args.expiresInSeconds, seconds > 0 {
        options[.expirationDate] = Date().addingTimeInterval(seconds)
      }
      UIPasteboard.general.setItems(
        [[UTType.utf8PlainText.identifier: args.text]], options: options)
      self.lastCopy = UIPasteboard.general.changeCount
      invoke.resolve()
    }
  }

  @objc public func clearClipboard(_ invoke: Invoke) {
    DispatchQueue.main.async {
      // The change count says whether anything was copied since, without reading the
      // pasteboard (which would show iOS's paste notice).
      if let ours = self.lastCopy, UIPasteboard.general.changeCount == ours {
        UIPasteboard.general.items = []
      }
      self.lastCopy = nil
      invoke.resolve()
    }
  }
}

extension UwuLockMobilePlugin {
  // MARK: The window

  /// The status bar's text follows the page's theme, not the iPhone's: the window
  /// takes the page's light or dark.
  @objc public func setAppearance(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(AppearanceArgs.self)
    DispatchQueue.main.async {
      self.dark = args.dark
      let style: UIUserInterfaceStyle = args.dark ? .dark : .light
      for scene in UIApplication.shared.connectedScenes {
        for window in (scene as? UIWindowScene)?.windows ?? [] {
          window.overrideUserInterfaceStyle = style
        }
      }
      invoke.resolve()
    }
  }
}

@_cdecl("init_plugin_uwulock_mobile")
func initPlugin() -> Plugin {
  return UwuLockMobilePlugin()
}
