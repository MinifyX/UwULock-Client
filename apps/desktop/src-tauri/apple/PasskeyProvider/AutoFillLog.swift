// The AutoFill protocol: each step of the extension, as in the unified log (providerLog), also
// into a small file in the App Group folder (Passkeys/autofill.log), so UwULock can show it in
// its settings (Einstellungen → AutoFill → AutoFill-Protokoll) without a Mac and Console.app.
//
// No secrets, ever: which way in, the system's version, hosts and rpIds, counts, short id
// prefixes, error and Keychain codes, step names. Never a password, a user name, a full address,
// a key or client data. The file keeps only the last lines (about 48 KB, at most 300 lines) and
// is written on a queue of its own, so logging never holds up the sheet.

import Foundation
import os

let providerLog = Logger(subsystem: "app.uwulock.passkeys", category: "provider")

final class AutoFillLog {
  static let shared = AutoFillLog()

  /// Past this size the file is cut back to its last lines.
  static let maxBytes = 64 * 1024
  /// What is left after a cut: room for the next lines before the next one.
  static let keepBytes = 48 * 1024
  static let keepLines = 300

  /// Tells this process's lines from another's (each request may start a new one).
  let run = String(format: "%04x", UInt16.random(in: 0...UInt16.max))

  private let queue = DispatchQueue(label: "app.uwulock.passkeys.log", qos: .utility)
  private let url: URL?
  private let stamp: ISO8601DateFormatter = {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter
  }()

  /// The App Group folder only exists in a signed build; without it, only the unified log.
  private init() {
    if let group = Bundle.main.object(forInfoDictionaryKey: "UwULockAppGroup") as? String,
      !group.isEmpty,
      let container = FileManager.default.containerURL(
        forSecurityApplicationGroupIdentifier: group)
    {
      url = container.appendingPathComponent("Passkeys", isDirectory: true)
        .appendingPathComponent("autofill.log")
    } else {
      url = nil
    }
  }

  func write(_ text: String) {
    let now = Date()
    queue.async {
      guard let url = self.url else { return }
      let line =
        self.stamp.string(from: now) + " [" + self.run + "] "
        + text.replacingOccurrences(of: "\n", with: " ") + "\n"
      var data = (try? Data(contentsOf: url)) ?? Data()
      data.append(Data(line.utf8))
      if data.count > Self.maxBytes { data = Self.tail(data) }
      try? FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
      try? data.write(to: url, options: PasskeyVault.writing)
    }
  }

  /// Waits for the lines written so far: the system may end the process right after the
  /// request is answered, and the last lines are the ones that matter.
  func flush() {
    queue.sync {}
  }

  /// The last whole lines, at most `keepLines` and `keepBytes`.
  static func tail(_ data: Data) -> Data {
    let lines = data.split(separator: 0x0a, omittingEmptySubsequences: true)
    var start = lines.endIndex
    var size = 0
    while start > lines.startIndex, lines.endIndex - start < keepLines {
      let next = size + lines[start - 1].count + 1
      if next > keepBytes { break }
      size = next
      start -= 1
    }
    var out = Data(capacity: size)
    for line in lines[start...] {
      out.append(contentsOf: line)
      out.append(0x0a)
    }
    return out
  }
}

/// One step: into the unified log (public: nothing secret goes in) and the protocol.
func logStep(_ text: String, error: Bool = false) {
  if error {
    providerLog.error("\(text, privacy: .public)")
  } else {
    providerLog.info("\(text, privacy: .public)")
  }
  AutoFillLog.shared.write(error ? "ERROR " + text : text)
}
