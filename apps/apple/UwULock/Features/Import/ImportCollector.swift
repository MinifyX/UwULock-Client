import Foundation

// CSV (csv.ts) and the collector every importer shares (collect.ts): items and folders in the
// shape of Bitwarden's JSON export, and the rule that nothing is dropped — a value without a
// place of its own becomes a custom field, or goes into the notes when it is long or has
// several lines. After Bitwarden's base importer (GPL-3.0).

extension String {
    /// JavaScript's `trim()`.
    var importTrimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }

    /// Whether the regular expression `pattern` matches somewhere.
    func importMatches(_ pattern: String, caseInsensitive: Bool = false) -> Bool {
        var options: String.CompareOptions = [.regularExpression]
        if caseInsensitive { options.insert(.caseInsensitive) }
        return range(of: pattern, options: options) != nil
    }

    /// `replace(/pattern/g, with)`.
    func importReplacing(_ pattern: String, with replacement: String, caseInsensitive: Bool = false) -> String {
        var options: String.CompareOptions = [.regularExpression]
        if caseInsensitive { options.insert(.caseInsensitive) }
        return replacingOccurrences(of: pattern, with: replacement, options: options)
    }

    /// CRLF and CR as LF.
    var importLF: String { replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n") }
}

/// Whether the value is null or only whitespace.
func importBlank(_ value: String?) -> Bool {
    value == nil || value!.importTrimmed.isEmpty
}

/// `value`, or nil when there is nothing in it.
func importSome(_ value: String?) -> String? {
    importBlank(value) ? nil : value
}

// MARK: - CSV

enum ImportCSV {
    /// RFC 4180 and what password managers actually write: quoted cells with doubled quotes,
    /// line breaks inside quotes, CRLF or LF, a BOM in front, a stray quote taken as it is.
    static func parse(_ input: String, delimiter: Character = ",") -> [[String]] {
        var text = Substring(input)
        if text.hasPrefix("\u{FEFF}") { text.removeFirst() }
        let chars = Array(text.unicodeScalars)
        let delim = delimiter.unicodeScalars.first!
        var rows: [[String]] = []
        var row: [String] = []
        var cell = String.UnicodeScalarView()
        var quoted = false
        var wasQuoted = false
        var i = 0
        while i < chars.count {
            let c = chars[i]
            if quoted {
                if c == "\"" {
                    if i + 1 < chars.count, chars[i + 1] == "\"" {
                        cell.append("\"")
                        i += 1
                    } else {
                        quoted = false
                    }
                } else if c == "\r", i + 1 < chars.count, chars[i + 1] == "\n" {
                    cell.append("\n")
                    i += 1
                } else {
                    cell.append(c)
                }
                i += 1
                continue
            }
            if c == "\"" && cell.isEmpty && !wasQuoted {
                quoted = true
                wasQuoted = true
            } else if c == delim {
                row.append(String(cell))
                cell = String.UnicodeScalarView()
                wasQuoted = false
            } else if c == "\n" || c == "\r" {
                if c == "\r", i + 1 < chars.count, chars[i + 1] == "\n" { i += 1 }
                row.append(String(cell))
                rows.append(row)
                row = []
                cell = String.UnicodeScalarView()
                wasQuoted = false
            } else {
                cell.append(c)
            }
            i += 1
        }
        if !cell.isEmpty || !row.isEmpty || wasQuoted {
            row.append(String(cell))
            rows.append(row)
        }
        return rows.filter { $0.contains { !$0.importTrimmed.isEmpty } }
    }
}

/// A CSV file with a header row: cells by column name, names compared without case.
struct ImportCsvTable {
    let header: [String]
    let rows: [[String]]
    private var index: [String: Int] = [:]

    init(_ text: String) {
        let parsed = ImportCSV.parse(text)
        header = (parsed.first ?? []).map(\.importTrimmed)
        rows = Array(parsed.dropFirst())
        for (i, name) in header.enumerated() where index[name.lowercased()] == nil {
            index[name.lowercased()] = i
        }
    }

    func has(_ names: String...) -> Bool {
        names.allSatisfy { index[$0.lowercased()] != nil }
    }

    /// The cell of `row` in the first of `names` the file has, trimmed; "" when there is none.
    func get(_ row: [String], _ names: String...) -> String {
        for name in names {
            if let i = index[name.lowercased()] {
                return (i < row.count ? row[i] : "").importTrimmed
            }
        }
        return ""
    }

    /// The row's cells with their column names, for the columns not in `known`.
    func rest(_ row: [String], known: [String]) -> [(String, String)] {
        let skip = Set(known.map { $0.lowercased() })
        return header.enumerated().compactMap { i, name in
            let value = (i < row.count ? row[i] : "").importTrimmed
            guard !name.isEmpty, !value.isEmpty, !skip.contains(name.lowercased()) else { return nil }
            return (name, value)
        }
    }
}

// MARK: - Helpers (collect.ts, wifi.ts)

enum ImportWifi {
    static let marker = "uwulock:type"
    static let type = "wifi"
    static let ssid = "SSID"
    static let password = "Password"
    static let security = "Security"
    static let hidden = "Hidden network"
    static let eap = "EAP method"
    static let phase2 = "Phase 2"
    static let identity = "Identity"
    static let anonymous = "Anonymous identity"
    static let ca = "CA certificate"

    static let securities = [
        "WPA3", "WPA2/WPA3", "WPA2", "WPA", "WEP", "None", "WPA2-Enterprise", "WPA3-Enterprise",
    ]

    /// One of the contract's securities for whatever another app calls it, or nil.
    static func securityOf(_ text: String?) -> String? {
        let raw = (text ?? "").importTrimmed
        if let exact = securities.first(where: { $0.lowercased() == raw.lowercased() }) { return exact }
        let plain = raw.lowercased().importReplacing("[\\s_-]+", with: "")
        if plain.isEmpty { return nil }
        if ["none", "open", "nopass", "nosecurity", "unsecured", "unencrypted", "off"].contains(plain) {
            return "None"
        }
        let enterprise = plain.importMatches("enterprise|eap|802\\.?1x|^wpa[23]?e$")
        let wpa3 = plain.contains("wpa3")
        let wpa2 = plain.contains("wpa2")
        if enterprise { return wpa3 && !wpa2 ? "WPA3-Enterprise" : "WPA2-Enterprise" }
        if wpa2 && wpa3 { return "WPA2/WPA3" }
        if wpa3 { return "WPA3" }
        if wpa2 { return "WPA2" }
        if plain.contains("wpa") { return "WPA" }
        if plain.contains("wep") { return "WEP" }
        return nil
    }

    /// Whether the item carries UwULock's Wi-Fi marker (docs/wifi.md).
    static func hasMarker(_ item: ImportExportItem) -> Bool {
        item.fields.contains { $0.name == marker && $0.value.importTrimmed.lowercased() == type }
    }
}

/// A Wi-Fi network as another app has it; `security` in that app's words.
struct ImportWifiEntry {
    var ssid: String?
    var password: String?
    var security: String?
    var hidden = false
    var eap: String?
    var phase2: String?
    var identity: String?
    var anonymous: String?
    var ca: String?
}

enum ImportText {
    private static let secret =
        "\\b(pin|cvv|cvc|otp|totp|puk)\\b|pass|pwd|secret|geheim|kennwort|token|key|schlüssel|seed|recovery|private|backup.?code|security.?code|sicherheitscode|wiederherstellung"

    static func secretLooking(_ name: String) -> Bool {
        name.importMatches(secret, caseInsensitive: true)
    }

    private static let unsafeSchemes = ["javascript:", "vbscript:", "data:", "file:", "blob:"]

    /// Whether `uri` has a scheme that runs or shows something in place of a page.
    static func unsafeUri(_ uri: String) -> Bool {
        let plain = String(String.UnicodeScalarView(uri.unicodeScalars.filter {
            $0.value > 0x20 && $0.value != 0x7f
        })).lowercased()
        return unsafeSchemes.contains { plain.hasPrefix($0) }
    }

    /// An address as the vault keeps it: "example.com" becomes https://example.com.
    static func fixUri(_ uri: String) -> String? {
        let trimmed = uri.importTrimmed
        if trimmed.isEmpty { return nil }
        if !trimmed.contains("://") && trimmed.contains(".") && !trimmed.importMatches("\\s") {
            return String("https://\(trimmed)".utf16.prefix(1000)) ?? "https://\(trimmed)"
        }
        return String(trimmed.utf16.prefix(1000)) ?? trimmed
    }

    /// The host of an address without "www.", for items that have no name of their own.
    static func nameFromUrl(_ url: String) -> String? {
        guard let fixed = fixUri(url), let components = URLComponents(string: fixed),
              let scheme = components.scheme, !scheme.isEmpty,
              let host = components.host, !host.isEmpty
        else { return nil }
        let lower = host.lowercased()
        return lower.hasPrefix("www.") ? String(lower.dropFirst(4)) : lower
    }

    /// "Mika Maria Muster" → first, middle, last; two parts are first and last.
    static func splitName(_ full: String) -> (String?, String?, String?) {
        let parts = full.importTrimmed.split(whereSeparator: \.isWhitespace).map(String.init)
        switch parts.count {
        case 0: return (nil, nil, nil)
        case 1: return (parts[0], nil, nil)
        case 2: return (parts[0], nil, parts[1])
        default: return (parts[0], parts[1], parts[2...].joined(separator: " "))
        }
    }

    /// A card's brand from its number, as Bitwarden's apps name brands.
    static func cardBrand(_ number: String?) -> String? {
        let digits = (number ?? "").filter(\.isASCII).filter(\.isNumber)
        if digits.isEmpty { return nil }
        let brands: [(String, String)] = [
            ("^4", "Visa"),
            ("^(5[1-5]|2(2[2-9]|[3-6]\\d|7[01]|720))", "Mastercard"),
            ("^3[47]", "Amex"),
            ("^(6011|65|64[4-9]|622)", "Discover"),
            ("^3(0[0-5]|[68])", "Diners Club"),
            ("^35", "JCB"),
            ("^(5018|5020|5038|6304|6759|676[1-3])", "Maestro"),
            ("^62", "UnionPay"),
            ("^220[0-4]", "Mir"),
        ]
        return brands.first { digits.importMatches($0.0) }?.1
    }
}

// MARK: - Collector

final class ImportCollector {
    private var folders: [ImportExportFolder] = []
    private var folderIds: [String: String] = [:]
    private(set) var items: [ImportExportItem] = []
    private var extras: [ObjectIdentifier] = []
    private var extraSet = Set<ObjectIdentifier>()
    private var attachments = 0
    private var warnings: [String] = []
    private var entries = 0
    private var skipped: [String] = []

    /// Reads one entry of the file with `read`. When that breaks on a value, the entry is left
    /// out and the preview names it by its title (or its place in the file), never the value.
    func entry(_ title: String?, _ read: () throws -> Void) {
        entries += 1
        let index = entries
        let extrasBefore = extras.count
        let attachmentsBefore = attachments
        do {
            try read()
        } catch {
            for id in extras[extrasBefore...] { extraSet.remove(id) }
            extras.removeSubrange(extrasBefore...)
            attachments = attachmentsBefore
            let name = String((title ?? "").importTrimmed.prefix(80))
            skipped.append(name.isEmpty ? "#\(index)" : name)
        }
    }

    func item(_ type: ImportItemType, _ name: String? = nil) -> ImportExportItem {
        let item = ImportExportItem(type: type, name: name?.importTrimmed ?? "")
        retype(item, type)
        return item
    }

    /// Makes `item` a `type`, with that type's empty part.
    func retype(_ item: ImportExportItem, _ type: ImportItemType) {
        item.type = type
        switch type {
        case .login: if item.login == nil { item.login = ImportExportLogin() }
        case .note: item.secureNote = true
        case .card: if item.card == nil { item.card = ImportExportCard() }
        case .identity: if item.identity == nil { item.identity = ImportExportIdentity() }
        case .sshKey: break
        }
    }

    func login(_ name: String? = nil) -> ImportExportItem {
        item(.login, name)
    }

    func add(_ item: ImportExportItem, folder: String? = nil) {
        item.folderId = folder.flatMap { $0.isEmpty ? nil : self.folder($0) }
        items.append(item)
    }

    /// The id of the folder `name`; "A/B" (or "A\B") is B inside A, and A is made too.
    func folder(_ name: String) -> String? {
        let parts = name.replacingOccurrences(of: "\\", with: "/").split(separator: "/", omittingEmptySubsequences: false)
            .map { String($0).importTrimmed }.filter { !$0.isEmpty }
        if parts.isEmpty { return nil }
        for i in 1...parts.count {
            let path = parts[0..<i].joined(separator: "/")
            if folderIds[path] == nil {
                let id = "folder-\(folders.count + 1)"
                folderIds[path] = id
                folders.append(ImportExportFolder(id: id, name: path))
            }
        }
        return folderIds[parts.joined(separator: "/")]
    }

    func uris(_ item: ImportExportItem, _ uris: [String?]) {
        guard item.login != nil else { return }
        for uri in uris {
            if let uri, !uri.isEmpty, ImportText.unsafeUri(uri) {
                extra(item, "URL", uri.importTrimmed, .text)
                continue
            }
            if let uri, let fixed = ImportText.fixUri(uri),
               !(item.login?.uris.contains { $0.uri == fixed } ?? true) {
                item.login?.uris.append(ImportExportUri(uri: fixed, match: nil))
            }
        }
    }

    func uris(_ item: ImportExportItem, _ uris: String?...) {
        self.uris(item, uris)
    }

    func appendNote(_ item: ImportExportItem, _ text: String?) {
        guard let text, !importBlank(text) else { return }
        var clean = text.importLF
        while let last = clean.last, last.isWhitespace { clean.removeLast() }
        item.notes = item.notes.map { $0.isEmpty ? clean : "\($0)\n\(clean)" } ?? clean
    }

    /// A custom field the source had as one. Long values and values with several lines go
    /// into the notes, as "name: value".
    func field(_ item: ImportExportItem, _ name: String, _ value: String?, _ type: ImportFieldType? = nil) {
        guard let value, !importBlank(value) else { return }
        let kind = type ?? (ImportText.secretLooking(name) ? .hidden : .text)
        if kind != .hidden && (value.utf16.count > 200 || value.importTrimmed.contains(where: { $0 == "\n" || $0 == "\r" || $0 == "\r\n" })) {
            appendNote(item, "\(name): \(value.importLF)")
            return
        }
        item.fields.append(ImportExportField(name: name, value: value, type: kind))
    }

    /// A value the vault has no place for: kept as a custom field, and the preview says so.
    func extra(_ item: ImportExportItem, _ name: String, _ value: String?, _ type: ImportFieldType? = nil) {
        guard !importBlank(value) else { return }
        if extraSet.insert(item.id).inserted { extras.append(item.id) }
        field(item, name, value, type)
    }

    /// Makes `item` a Wi-Fi network: a secure note with the marker and the network's fields in
    /// front of the fields it has (docs/wifi.md).
    func wifi(_ item: ImportExportItem, _ entry: ImportWifiEntry) {
        retype(item, .note)
        let known = ImportWifi.securityOf(entry.security)
        let security = known ?? (importBlank(entry.password) ? "None" : "WPA2")
        if !importBlank(entry.security) && known == nil {
            extra(item, "\(ImportWifi.security) (\(featureString("Original")))", entry.security)
        }
        let ssid = importSome(entry.ssid) ?? item.name
        var own: [ImportExportField] = [
            ImportExportField(name: ImportWifi.marker, value: ImportWifi.type, type: .text),
            ImportExportField(name: ImportWifi.ssid, value: ssid, type: .text),
            ImportExportField(name: ImportWifi.password, value: entry.password ?? "", type: .hidden),
            ImportExportField(name: ImportWifi.security, value: security, type: .text),
            ImportExportField(name: ImportWifi.hidden, value: entry.hidden ? "true" : "false", type: .boolean),
        ]
        if security.hasSuffix("-Enterprise") {
            let enterprise: [(String, String?)] = [
                (ImportWifi.eap, entry.eap), (ImportWifi.phase2, entry.phase2),
                (ImportWifi.identity, entry.identity), (ImportWifi.anonymous, entry.anonymous),
                (ImportWifi.ca, entry.ca),
            ]
            for (name, value) in enterprise where !importBlank(value) {
                own.append(ImportExportField(name: name, value: value!.importTrimmed, type: .text))
            }
        }
        let names = Set(own.map(\.name))
        item.fields = own + item.fields.filter { !names.contains($0.name) }
        if item.name.importTrimmed.isEmpty { item.name = ssid }
    }

    /// An item that came with UwULock's Wi-Fi marker: a proper network.
    private func wifiFromMarker(_ item: ImportExportItem) {
        func get(_ name: String) -> String? { item.fields.first { $0.name == name }?.value }
        let password = get(ImportWifi.password) ?? item.login?.password
        if item.login != nil && password == item.login?.password { item.login?.password = nil }
        wifi(item, ImportWifiEntry(
            ssid: get(ImportWifi.ssid), password: password, security: get(ImportWifi.security),
            hidden: get(ImportWifi.hidden)?.importTrimmed.lowercased() == "true",
            eap: get(ImportWifi.eap), phase2: get(ImportWifi.phase2),
            identity: get(ImportWifi.identity), anonymous: get(ImportWifi.anonymous),
            ca: get(ImportWifi.ca)))
    }

    func attachment(_ count: Int = 1) {
        attachments += count
    }

    func warn(_ text: String) {
        if !warnings.contains(text) { warnings.append(text) }
    }

    func result() -> (data: ImportExportData, warnings: [String]) {
        for item in items {
            if ImportWifi.hasMarker(item) { wifiFromMarker(item) }
            if item.name.importTrimmed.isEmpty { item.name = "--" }
            // A card or a note with a password or an address from the source: kept, as fields.
            if item.type != .login, let login = item.login {
                if login.username != item.identity?.username {
                    extra(item, featureString("Benutzername"), login.username)
                }
                extra(item, featureString("Passwort"), login.password, .hidden)
                extra(item, "TOTP", login.totp, .hidden)
                for uri in login.uris { extra(item, "URL", uri.uri, .text) }
            }
            if item.type == .login, let login = item.login, importBlank(login.username),
               importBlank(login.password), importBlank(login.totp), login.uris.isEmpty {
                retype(item, .note)
            }
            if item.type != .login { item.login = nil }
            if item.type != .note { item.secureNote = false }
            if item.type != .card { item.card = nil }
            if item.type != .identity { item.identity = nil }
            if importBlank(item.notes) { item.notes = nil }
            if item.passwordHistory?.isEmpty == true { item.passwordHistory = nil }
        }
        var warnings = self.warnings
        if !extras.isEmpty {
            warnings.append(featureString(
                "\(extras.count) Einträge mit Feldern ohne eigenen Platz: als eigene Felder oder Notizen übernommen."))
        }
        if !skipped.isEmpty {
            let shown = skipped.prefix(10).joined(separator: ", ")
            let more = skipped.count - 10
            let names = more > 0 ? "\(shown) \(featureString("… und \(more) weitere"))" : shown
            warnings.append(featureString(
                "\(skipped.count) Einträge ließen sich nicht lesen und bleiben weg: \(names)"))
        }
        if attachments > 0 {
            warnings.append(featureString(
                "\(attachments) Anhänge kommen nicht mit. Speichere sie aus der alten App und hänge sie danach wieder an."))
        }
        // Only folders something is in, and every folder on their paths.
        let used = Set(items.compactMap(\.folderId))
        var keep = Set<String>()
        for folder in folders where used.contains(folder.id) {
            let parts = folder.name.split(separator: "/", omittingEmptySubsequences: false)
            for i in 1...parts.count { keep.insert(parts[0..<i].joined(separator: "/")) }
        }
        let kept = folders.filter { keep.contains($0.name) }
        return (ImportExportData(folders: kept, items: items), warnings)
    }
}
