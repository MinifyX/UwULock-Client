import AuthenticationServices
import Foundation
import ObjectiveC
import Tauri
import UIKit

struct CredentialExchangeArgs: Decodable {
  let discard: Bool?
}

/// Taking in what another app hands over (iOS 26+): Apple Passwords' "Export data to another
/// app" → UwULock. The system starts UwULock with an `NSUserActivity` of type
/// `ASCredentialExchangeActivity` that carries a token; `ASCredentialImportManager` turns the
/// token into the data (`ASExportedCredentialData`, the FIDO Credential Exchange Format).
///
/// The activity reaches the app delegate, which Tauri's window layer (tao) owns. So UwULock
/// hooks `application(_:continue:restorationHandler:)` (and `scene(_:continue:)`, where scenes
/// are used) once at start: an exchange activity's token is kept here, everything else goes on
/// to tao as before. The page asks whether one waits (`credentialExchangePending`) once the vault
/// is open and takes it in (`credentialExchangeImport`) after the person said yes: the data comes
/// from the system only then, and only once.
enum CredentialExchange {
  private static let lock = NSLock()
  private static var token: UUID?
  private static var hooked = false

  static var pending: Bool {
    lock.lock()
    defer { lock.unlock() }
    return token != nil
  }

  static func take() -> UUID? {
    lock.lock()
    defer { lock.unlock() }
    let taken = token
    token = nil
    return taken
  }

  /// Keeps the token of an exchange activity; `false` for any other activity.
  @discardableResult
  static func receive(_ activity: NSUserActivity) -> Bool {
    guard #available(iOS 26.0, *), activity.activityType == ASCredentialExchangeActivity else {
      return false
    }
    guard let received = activity.userInfo?[ASCredentialImportToken] as? UUID else {
      NSLog("UwULock: a credential exchange without a token")
      return true
    }
    lock.lock()
    token = received
    lock.unlock()
    NSLog("UwULock: credentials handed over by another app wait to be taken in")
    return true
  }

  /// Hooks the app delegate (and the scene delegates) once. Also looks at the launch options of
  /// a start by the activity, in case the delegate saw it before the hook.
  static func install() {
    guard #available(iOS 26.0, *) else { return }
    lock.lock()
    let first = !hooked
    hooked = true
    lock.unlock()
    guard first else { return }
    DispatchQueue.main.async {
      if let delegate = UIApplication.shared.delegate {
        hookApplication(type(of: delegate))
      }
      for scene in UIApplication.shared.connectedScenes {
        if let delegate = scene.delegate { hookScene(type(of: delegate)) }
      }
    }
    NotificationCenter.default.addObserver(
      forName: UIApplication.didFinishLaunchingNotification, object: nil, queue: .main
    ) { note in
      guard
        let options = note.userInfo?[UIApplication.LaunchOptionsKey.userActivityDictionary]
          as? [AnyHashable: Any]
      else { return }
      for value in options.values {
        if let activity = value as? NSUserActivity { receive(activity) }
      }
    }
    NotificationCenter.default.addObserver(
      forName: UIScene.willConnectNotification, object: nil, queue: .main
    ) { note in
      if let delegate = (note.object as? UIScene)?.delegate { hookScene(type(of: delegate)) }
    }
  }

  private static func hookApplication(_ cls: AnyClass) {
    let selector = #selector(
      UIApplicationDelegate.application(_:continue:restorationHandler:))
    typealias Original = @convention(c) (
      AnyObject, Selector, UIApplication, NSUserActivity, AnyObject?
    ) -> Bool
    let method = class_getInstanceMethod(cls, selector)
    let original = method.map { unsafeBitCast(method_getImplementation($0), to: Original.self) }
    let block:
      @convention(block) (AnyObject, UIApplication, NSUserActivity, AnyObject?) -> Bool = {
        this, application, activity, handler in
        if receive(activity) { return true }
        return original?(this, selector, application, activity, handler) ?? false
      }
    let implementation = imp_implementationWithBlock(block)
    if let method {
      method_setImplementation(method, implementation)
    } else {
      class_addMethod(cls, selector, implementation, "B@:@@@?")
    }
  }

  private static var hookedScenes = Set<ObjectIdentifier>()

  private static func hookScene(_ cls: AnyClass) {
    let key = ObjectIdentifier(cls)
    guard hookedScenes.insert(key).inserted else { return }
    let selector = #selector(UISceneDelegate.scene(_:continue:))
    typealias Original = @convention(c) (AnyObject, Selector, UIScene, NSUserActivity) -> Void
    let method = class_getInstanceMethod(cls, selector)
    let original = method.map { unsafeBitCast(method_getImplementation($0), to: Original.self) }
    let block: @convention(block) (AnyObject, UIScene, NSUserActivity) -> Void = {
      this, scene, activity in
      if receive(activity) { return }
      original?(this, selector, scene, activity)
    }
    let implementation = imp_implementationWithBlock(block)
    if let method {
      method_setImplementation(method, implementation)
    } else {
      class_addMethod(cls, selector, implementation, "v@:@@")
    }
  }

  /// The handed-over data as CXF JSON (Apple's own Codable form of it, dates in seconds).
  @available(iOS 26.0, *)
  static func load(_ token: UUID) async throws -> String {
    let data = try await ASCredentialImportManager().importCredentials(token: token)
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .secondsSince1970
    let json = try encoder.encode(data)
    return String(decoding: json, as: UTF8.self)
  }
}

extension UwuLockMobilePlugin {
  @objc public func credentialExchangePending(_ invoke: Invoke) {
    invoke.resolve(["pending": CredentialExchange.pending])
  }

  @objc public func credentialExchangeImport(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(CredentialExchangeArgs.self)
    guard let token = CredentialExchange.take() else {
      invoke.reject("Nothing was handed over.", code: "missing")
      return
    }
    if args.discard == true {
      invoke.resolve(["json": ""])
      return
    }
    guard #available(iOS 26.0, *) else {
      invoke.reject("Needs iOS 26.", code: "unsupported")
      return
    }
    Task {
      do {
        let json = try await CredentialExchange.load(token)
        invoke.resolve(["json": json])
      } catch {
        invoke.reject("\(error)", code: "failed")
      }
    }
  }
}
