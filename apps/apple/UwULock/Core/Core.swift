// The app's logic: crates/uwulock-app through crates/uwulock-ffi (UniFFI). Every command of the
// Tauri app under the same name, with the same camelCase JSON arguments and answers, and the
// same events (crates/uwulock-app/README.md). The typed commands: CoreCommands.swift; the types
// they take and give: Models/.
//
// A build without the Rust core (scripts/apple-build.sh without the XCFramework) has the same
// API: every command fails with `unsupported`, and nothing happens.
import Foundation
import Observation

/// A command's failure: the page's `{kind, message}` (errors.ts). `kind` is what to branch on
/// (`locked`, `wrong-password`, `biometric-cancelled`, …), `message` what to show.
struct CoreFailure: Error, Codable, Sendable, Equatable, LocalizedError {
    var kind: String
    var message: String

    var errorDescription: String? { message }

    static func unsupported(_ what: String) -> CoreFailure {
        CoreFailure(kind: "unsupported", message: what)
    }
}

/// What the core says by itself: the Tauri app's events, decoded.
enum CoreEvent: Sendable {
    /// Logged in, locked, unlocked, synced: the whole status anew.
    case vaultStatus(Status)
    /// Items, folders or settings changed: read again what is shown.
    case vaultChanged
    /// Another device asks to log in with this one (`auth_requests`).
    case authRequest(id: String)
    /// The UwULock suite's data changed (sends, file requests, masked addresses).
    case suiteChanged
    /// A notice from UwULock's server, such as a file request's new submission.
    case uwuNotice(kind: String, id: String?)
    case uwuChanged(UwuStatus)
    case healthProgress(CheckProgress)
    case importProgress(ImportProgress)
    case moveProgress(MoveProgress)
    case moveFinished(MoveFinished)
    case moveFailed(CoreFailure)
    /// A browser asks for a passkey: `id` for `passkey_request`; `0` when it went away.
    case passkeyRequest(id: UInt64)
    /// Another program stands in for UwULock; `nil` when it no longer does.
    case passkeyProviderWarning(PasskeyProviderWarning?)
    /// An event this app doesn't know yet, with its JSON.
    case other(name: String, json: String)

    /// The event `name` with its JSON payload; `.other` when the payload doesn't read.
    init(name: String, json: String) {
        let data = Data(json.utf8)
        func read<T: Decodable>(_ type: T.Type) -> T? {
            try? JSONDecoder().decode(T.self, from: data)
        }
        struct Notice: Decodable {
            var kind: String
            var id: String?
        }
        struct Request: Decodable {
            var id: String
        }
        let event: CoreEvent? =
            switch name {
            case "vault-status": read(Status.self).map(CoreEvent.vaultStatus)
            case "vault-changed": .vaultChanged
            case "auth-request": read(Request.self).map { .authRequest(id: $0.id) }
            case "suite-changed": .suiteChanged
            case "uwu-notice": read(Notice.self).map { .uwuNotice(kind: $0.kind, id: $0.id) }
            case "uwu-changed": read(UwuStatus.self).map(CoreEvent.uwuChanged)
            case "health-progress": read(CheckProgress.self).map(CoreEvent.healthProgress)
            case "import-progress": read(ImportProgress.self).map(CoreEvent.importProgress)
            case "move-progress": read(MoveProgress.self).map(CoreEvent.moveProgress)
            case "move-finished": read(MoveFinished.self).map(CoreEvent.moveFinished)
            case "move-failed": read(CoreFailure.self).map(CoreEvent.moveFailed)
            case "passkey-request": read(UInt64.self).map { .passkeyRequest(id: $0) }
            case "passkey-provider-warning":
                json == "null" ? .passkeyProviderWarning(nil)
                    : read(PasskeyProviderWarning.self).map(CoreEvent.passkeyProviderWarning)
            default: nil
            }
        self = event ?? .other(name: name, json: json)
    }
}

/// A command's arguments: names as the page sends them (camelCase), values as JSON. `nil`
/// goes as `null` (an `Option` the command takes as none).
struct CoreArgs: Encodable, Sendable, ExpressibleByDictionaryLiteral {
    typealias Key = String
    typealias Value = (any Encodable & Sendable)?

    private var values: [(String, Value)]

    init(dictionaryLiteral elements: (Key, Value)...) {
        values = elements
    }

    private struct ArgKey: CodingKey {
        var stringValue: String
        var intValue: Int? { nil }
        init(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { nil }
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: ArgKey.self)
        for (name, value) in values {
            if let value {
                try container.encode(value, forKey: ArgKey(stringValue: name))
            } else {
                try container.encodeNil(forKey: ArgKey(stringValue: name))
            }
        }
    }
}

/// The app's one core: made at launch (`AppModel`), alive as long as the app.
@MainActor
@Observable
final class Core {
    /// The last status the core told (`vault_status` or the `vault-status` event).
    private(set) var status: Status?
    /// The folder with the accounts: as the Tauri app's (`UWULOCK_DATA_DIR` points elsewhere).
    let dataDirectory: URL
    /// Why the core isn't there (no Rust core in this build, or the data folder isn't usable).
    private(set) var problem: CoreFailure?

    @ObservationIgnored private var listeners: [UUID: AsyncStream<CoreEvent>.Continuation] = [:]
    #if RUST_CORE && canImport(UwuLockFFI)
    @ObservationIgnored private var ffi: UwuLockCore?
    @ObservationIgnored let host: AppleHost
    #endif

    /// The core with the accounts in `dataDirectory` (default: `Core.defaultDataDirectory`).
    /// Nothing runs yet: `start()`.
    init(dataDirectory: URL = Core.defaultDataDirectory()) {
        self.dataDirectory = dataDirectory
        #if RUST_CORE && canImport(UwuLockFFI)
        host = AppleHost()
        host.core = self
        do {
            try FileManager.default.createDirectory(
                at: dataDirectory, withIntermediateDirectories: true)
            AppleHost.keepOutOfBackups(dataDirectory)
            ffi = try UwuLockCore(dataDir: dataDirectory.path, host: host)
        } catch {
            problem = Core.failure(error)
        }
        #else
        problem = .unsupported("This build has no Rust core (scripts/apple-xcframework.sh).")
        #endif
    }

    /// Where the Tauri app keeps its accounts: Application Support/<bundle id> (in the sandbox
    /// container on iOS and in the Mac App Store build), or `UWULOCK_DATA_DIR`.
    nonisolated static func defaultDataDirectory() -> URL {
        if let dir = ProcessInfo.processInfo.environment["UWULOCK_DATA_DIR"], !dir.isEmpty {
            return URL(fileURLWithPath: dir, isDirectory: true)
        }
        let support =
            FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(
                "Library/Application Support", isDirectory: true)
        return support.appendingPathComponent(
            Bundle.main.bundleIdentifier ?? "app.uwulock", isDirectory: true)
    }

    /// Whether this build carries the Rust core and it came up.
    var isAvailable: Bool { problem == nil }

    /// Starts syncing, live changes, the extras and the passkey providers. Once.
    func start() {
        #if RUST_CORE && canImport(UwuLockFFI)
        ffi?.start()
        #endif
    }

    // MARK: Commands

    /// Runs `command` and decodes its answer. Throws `CoreFailure`.
    func call<T: Decodable & Sendable>(
        _ command: String, _ args: CoreArgs = [:], as type: T.Type = T.self
    ) async throws -> T {
        let json = try await raw(command, args)
        do {
            return try JSONDecoder().decode(T.self, from: Data(json.utf8))
        } catch {
            throw CoreFailure(kind: "decode", message: "\(command): \(error)")
        }
    }

    /// Runs `command` whose answer is nothing (or isn't needed). Throws `CoreFailure`.
    func run(_ command: String, _ args: CoreArgs = [:]) async throws {
        _ = try await raw(command, args)
    }

    /// Runs `command` and hands back its answer as JSON. Throws `CoreFailure`.
    func raw(_ command: String, _ args: CoreArgs = [:]) async throws -> String {
        #if RUST_CORE && canImport(UwuLockFFI)
        guard let ffi else { throw problem ?? .unsupported("No core.") }
        let encoded: String
        do {
            encoded = String(decoding: try JSONEncoder().encode(args), as: UTF8.self)
        } catch {
            throw CoreFailure(kind: "args", message: "\(command): \(error)")
        }
        do {
            return try await ffi.call(command: command, argsJson: encoded)
        } catch {
            throw Core.failure(error)
        }
        #else
        throw problem ?? .unsupported("No core.")
        #endif
    }

    /// `stage_send_file` with the file's bytes; `nil` drops what was staged.
    func stageSendFile(_ bytes: Data?) async throws {
        #if RUST_CORE && canImport(UwuLockFFI)
        guard let ffi else { throw problem ?? .unsupported("No core.") }
        do {
            try await ffi.stageSendFile(bytes: bytes)
        } catch {
            throw Core.failure(error)
        }
        #else
        throw problem ?? .unsupported("No core.")
        #endif
    }

    /// Every command the core knows (for the debug view).
    var commands: [String] {
        #if RUST_CORE && canImport(UwuLockFFI)
        ffi?.commands() ?? []
        #else
        []
        #endif
    }

    // MARK: Lifecycle

    /// The app went to the background: the auto-lock's clock keeps going, live updates pause.
    func suspended() {
        #if RUST_CORE && canImport(UwuLockFFI)
        ffi?.suspended()
        #endif
    }

    /// The app is in front again: what was missed is synced.
    func resumed() {
        #if RUST_CORE && canImport(UwuLockFFI)
        ffi?.resumed()
        #endif
    }

    /// The system locked, slept or started the screen saver (`why` for the log): locks the
    /// vault when the settings say so.
    func systemLocked(_ why: String) {
        #if RUST_CORE && canImport(UwuLockFFI)
        ffi?.systemLocked(why: why)
        #endif
    }

    // MARK: Events

    /// The core's events from now on, for as long as the stream is read.
    func events() -> AsyncStream<CoreEvent> {
        let id = UUID()
        let (stream, continuation) = AsyncStream<CoreEvent>.makeStream(
            bufferingPolicy: .bufferingNewest(64))
        continuation.onTermination = { [weak self] _ in
            Task { @MainActor in self?.listeners[id] = nil }
        }
        listeners[id] = continuation
        return stream
    }

    /// An event from the host (`AppleHost.emit`), on the main actor.
    func receive(_ event: CoreEvent) {
        if case .vaultStatus(let status) = event {
            self.status = status
        }
        for listener in listeners.values {
            listener.yield(event)
        }
    }

    /// Keeps the status a command answered with (`vault_status`, `unlock`, …).
    func remember(_ status: Status) {
        self.status = status
    }

    // MARK: Errors

    /// A thrown error as the page's failure.
    nonisolated static func failure(_ error: any Error) -> CoreFailure {
        if let failure = error as? CoreFailure {
            return failure
        }
        #if RUST_CORE && canImport(UwuLockFFI)
        if case .Failure(let json) = error as? CoreError {
            return (try? JSONDecoder().decode(CoreFailure.self, from: Data(json.utf8)))
                ?? CoreFailure(kind: "failed", message: json)
        }
        #endif
        return CoreFailure(kind: "failed", message: "\(error)")
    }
}
