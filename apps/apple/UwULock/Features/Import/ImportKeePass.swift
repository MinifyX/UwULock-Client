import Foundation

// KeePass and KeePassXC (keepass.ts): the XML inside a KDBX file (and KeePass's XML export),
// and the CSV exports of both. Groups become folders by path without the root group, the
// standard fields go where they belong, other fields become custom fields (protected ones
// hidden), plus additional URLs, TOTPs, older passwords, and no recycle bin.

/// Text encodings of `URLSearchParams` and `encodeURIComponent`.
enum ImportURLText {
    private static let componentSafe = CharacterSet(
        charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.!~*'()")
    private static let formSafe = CharacterSet(
        charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.*")

    static func component(_ text: String) -> String {
        text.addingPercentEncoding(withAllowedCharacters: componentSafe) ?? text
    }

    static func form(_ pairs: [(String, String)]) -> String {
        func encode(_ text: String) -> String {
            text.split(separator: " ", omittingEmptySubsequences: false)
                .map { String($0).addingPercentEncoding(withAllowedCharacters: formSafe) ?? String($0) }
                .joined(separator: "+")
        }
        return pairs.map { "\(encode($0.0))=\(encode($0.1))" }.joined(separator: "&")
    }

    /// The first value of each name, as `URLSearchParams.get` reads it.
    static func params(_ query: String) -> [String: String] {
        var out: [String: String] = [:]
        let text = query.hasPrefix("?") ? String(query.dropFirst()) : query
        for pair in text.split(separator: "&") {
            let parts = pair.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            func decode(_ part: Substring) -> String {
                let plus = part.replacingOccurrences(of: "+", with: " ")
                return plus.removingPercentEncoding ?? plus
            }
            let name = decode(parts[0])
            if out[name] == nil { out[name] = parts.count > 1 ? decode(parts[1]) : "" }
        }
        return out
    }
}

/// JavaScript's `toISOString`: UTC with milliseconds.
enum ImportISODate {
    static func string(_ date: Date) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let c = calendar.dateComponents([.year, .month, .day, .hour, .minute, .second, .nanosecond], from: date)
        let ms = (c.nanosecond ?? 0) / 1_000_000
        return String(format: "%04d-%02d-%02dT%02d:%02d:%02d.%03dZ",
                      c.year ?? 0, c.month ?? 0, c.day ?? 0, c.hour ?? 0, c.minute ?? 0, c.second ?? 0, ms)
    }

    static func now() -> String { string(Date()) }

    /// An ISO date or date-time, as `new Date(text)` reads the usual forms.
    static func parse(_ text: String) -> Date? {
        let full = ISO8601DateFormatter()
        if let date = full.date(from: text) { return date }
        full.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = full.date(from: text) { return date }
        let day = ISO8601DateFormatter()
        day.formatOptions = [.withFullDate]
        if text.count == 10, let date = day.date(from: text) { return date }
        // Local time without a zone.
        let local = DateFormatter()
        local.locale = Locale(identifier: "en_US_POSIX")
        for format in ["yyyy-MM-dd'T'HH:mm:ss", "yyyy-MM-dd'T'HH:mm:ss.SSS", "yyyy-MM-dd'T'HH:mm"] {
            local.dateFormat = format
            if let date = local.date(from: text) { return date }
        }
        return nil
    }
}

enum ImportKeePass {
    typealias Strings = [(name: String, value: String, hidden: Bool)]

    /// KDBX 4 keeps times as seconds since the year 1 (base64, 8 bytes); KDBX 3.1 as ISO text.
    static func time(_ value: String) -> String? {
        if value.isEmpty { return nil }
        if value.importMatches(#"^\d{4}-\d\d-\d\d"#) {
            return ImportISODate.parse(value).map(ImportISODate.string)
        }
        guard let bytes = ImportBytes.fromBase64(value), bytes.count == 8 else { return nil }
        var raw: UInt64 = 0
        for (i, byte) in bytes.enumerated() { raw |= UInt64(byte) << (8 * UInt64(i)) }
        let seconds = Int64(bitPattern: raw) - 62_135_596_800
        return ImportISODate.string(Date(timeIntervalSince1970: TimeInterval(seconds)))
    }

    /// A TOTP as the vault reads it: an otpauth:// address, or just the secret when it is standard.
    static func totpFrom(_ secret: String, period: Int = 30, digits: Int = 6, algorithm: String = "SHA1",
                         label: String = "") -> String {
        let clean = secret.importReplacing(#"[\s=]"#, with: "").uppercased()
        if period == 30 && digits == 6 && algorithm == "SHA1" { return clean }
        var query = [("secret", clean), ("period", String(period)), ("digits", String(digits))]
        if algorithm != "SHA1" { query.append(("algorithm", algorithm)) }
        return "otpauth://totp/\(ImportURLText.component(label.isEmpty ? "TOTP" : label))?\(ImportURLText.form(query))"
    }

    /// `Number(text)` for the small numbers of a TOTP: nil when it isn't a number.
    private static func number(_ text: String) -> Int? {
        let trimmed = text.importTrimmed
        if trimmed.isEmpty { return 0 }
        guard let value = Double(trimmed), value.isFinite else { return nil }
        return Int(value)
    }

    /// The TOTP of an entry, and the fields it came from: KeePassXC's `otp`, KeePass 2.47+'s
    /// TimeOtp-* fields, or KeeTrayTOTP's "TOTP Seed" and "TOTP Settings".
    static func totp(_ strings: Strings, label: String) -> (totp: String, used: [String])? {
        func get(_ name: String) -> String {
            strings.first { $0.name == name }?.value.importTrimmed ?? ""
        }
        let otp = get("otp")
        if !otp.isEmpty {
            if otp.importMatches("^otpauth://", caseInsensitive: true) { return (otp, ["otp"]) }
            let params = ImportURLText.params(otp)
            if let key = params["key"], !key.isEmpty {
                let digits = params["size"]
                let steam = params["encoder"] == "steam" || digits == "S"
                if steam { return ("steam://\(key)", ["otp"]) }
                // Number("x") is NaN in the page; a NaN period never equals 30, so it goes into the address.
                let period = params["step"].map { number($0) ?? 0 } ?? 30
                let size = digits.map { number($0) ?? 0 } ?? 6
                return (totpFrom(key, period: period, digits: size, label: label), ["otp"])
            }
        }
        let seed = get("TOTP Seed")
        if !seed.isEmpty {
            let settings = get("TOTP Settings").split(separator: ";", omittingEmptySubsequences: false).map(String.init)
            let period = settings.count > 0 ? settings[0] : "30"
            let digits = settings.count > 1 ? settings[1] : "6"
            if digits == "S" { return ("steam://\(seed)", ["TOTP Seed", "TOTP Settings"]) }
            let p = number(period).flatMap { $0 == 0 ? nil : $0 } ?? 30
            let d = number(digits).flatMap { $0 == 0 ? nil : $0 } ?? 6
            return (totpFrom(seed, period: p, digits: d, label: label), ["TOTP Seed", "TOTP Settings"])
        }
        let encodings: [(String, (String) -> [UInt8]?)] = [
            ("TimeOtp-Secret-Base32", { _ in nil }),
            ("TimeOtp-Secret", { Array($0.utf8) }),
            ("TimeOtp-Secret-Hex", { ImportBytes.fromHex($0) }),
            ("TimeOtp-Secret-Base64", { ImportBytes.fromBase64($0) }),
        ]
        for (name, decode) in encodings {
            let value = get(name)
            if value.isEmpty { continue }
            let secret = name == "TimeOtp-Secret-Base32" ? value : decode(value).map(ImportBytes.base32)
            guard let secret, !secret.isEmpty else { continue }
            var algorithm = get("TimeOtp-Algorithm").importReplacing("^HMAC-", with: "", caseInsensitive: true)
            if let dash = algorithm.firstIndex(of: "-") { algorithm.remove(at: dash) }
            if algorithm.isEmpty { algorithm = "SHA1" }
            let period = number(get("TimeOtp-Period")).flatMap { $0 == 0 ? nil : $0 } ?? 30
            let digits = number(get("TimeOtp-Length")).flatMap { $0 == 0 ? nil : $0 } ?? 6
            return (totpFrom(secret, period: period, digits: digits, algorithm: algorithm.uppercased(), label: label),
                    [name, "TimeOtp-Length", "TimeOtp-Period", "TimeOtp-Algorithm"])
        }
        return nil
    }

    private static let standard: Set<String> = ["Title", "UserName", "Password", "URL", "Notes"]

    private static func readEntry(_ collector: ImportCollector, _ entry: ImportXMLElement) -> ImportExportItem {
        var strings: Strings = []
        for string in entry.elements("String") {
            let value = string.element("Value")
            let text = value?.textContent ?? ""
            if importBlank(text) { continue }
            let hidden = value?.attribute("Protected")?.lowercased() == "true"
                || value?.attribute("ProtectInMemory")?.lowercased() == "true"
            let key = string.text("Key")
            if let index = strings.firstIndex(where: { $0.name == key }) {
                strings[index] = (key, text, hidden)
            } else {
                strings.append((key, text, hidden))
            }
        }
        func get(_ name: String) -> String? { strings.first { $0.name == name }?.value }
        let item = collector.login(get("Title"))
        item.login?.username = importSome(get("UserName"))
        item.login?.password = importSome(get("Password"))
        collector.uris(item, get("URL"))
        collector.appendNote(item, get("Notes"))

        var used = standard
        for string in strings where string.name.importMatches(#"^KP2A_URL(_\d+)?$"#) {
            collector.uris(item, string.value)
            used.insert(string.name)
        }
        if let totp = totp(strings, label: item.name) {
            item.login?.totp = totp.totp
            used.formUnion(totp.used)
        }
        for string in strings where !used.contains(string.name) {
            collector.field(item, string.name, string.value, string.hidden ? .hidden : nil)
        }

        let tags = entry.text("Tags")
        if !importBlank(tags) {
            let parts = tags.components(separatedBy: CharacterSet(charactersIn: ";,"))
            let joined = parts.enumerated().map { $0.offset == 0 ? $0.element : String($0.element.drop(while: \.isWhitespace)) }
            collector.extra(item, "Tags", joined.joined(separator: ", "))
        }
        let times = entry.element("Times")
        if times?.text("Expires").lowercased() == "true", let expires = time(times?.text("ExpiryTime") ?? "") {
            collector.extra(item, featureString("Läuft ab"), String(expires.prefix(10)), .text)
        }
        let attachments = entry.elements("Binary").count
        if attachments > 0 { collector.attachment(attachments) }

        // Older passwords, newest first, from the entry's earlier versions.
        if let history = entry.element("History") {
            var old: [ImportPasswordHistory] = []
            for version in history.elements("Entry").reversed() {
                let password = version.elements("String").first { $0.text("Key") == "Password" }
                let value = password?.text("Value") ?? ""
                if importBlank(value) || value == item.login?.password || old.contains(where: { $0.password == value }) {
                    continue
                }
                let changed = time(version.element("Times")?.text("LastModificationTime") ?? "")
                old.append(ImportPasswordHistory(password: value, lastUsedDate: changed ?? ImportISODate.now()))
            }
            if !old.isEmpty { item.passwordHistory = Array(old.prefix(5)) }
        }
        return item
    }

    /// The entries of KeePass's XML (from a KDBX file or KeePass's XML export).
    static func readXml(_ doc: ImportXMLElement, _ collector: ImportCollector) throws {
        guard doc.name == "KeePassFile", let top = doc.element("Root")?.element("Group") else {
            throw ImportProblem(featureString("Die KeePass-Datei enthält keine Gruppen."))
        }
        let meta = doc.element("Meta")
        let empty = "AAAAAAAAAAAAAAAAAAAAAA=="
        let recycleBin = meta?.text("RecycleBinEnabled").lowercased() != "false" ? meta?.text("RecycleBinUUID") ?? "" : ""
        let templates = meta?.text("EntryTemplatesGroup") ?? ""
        var skipped = 0

        func walk(_ group: ImportXMLElement, _ path: String?) {
            let uuid = group.text("UUID")
            if path != nil, !uuid.isEmpty, uuid != empty, uuid == recycleBin || uuid == templates {
                skipped += group.descendants().filter { $0.name == "Entry" && $0.parent?.name != "History" }.count
                return
            }
            for entry in group.elements("Entry") {
                let title = entry.elements("String").first { $0.text("Key") == "Title" }
                collector.entry(title?.text("Value")) {
                    collector.add(readEntry(collector, entry), folder: path)
                }
            }
            for sub in group.elements("Group") {
                let trimmed = sub.text("Name").importTrimmed
                let name = trimmed.isEmpty ? "-" : trimmed
                walk(sub, path.map { $0.isEmpty ? name : "\($0)/\(name)" } ?? name)
            }
        }
        walk(top, nil)
        if skipped > 0 {
            collector.warn(featureString("\(skipped) Einträge aus dem Papierkorb oder den Vorlagen bleiben weg."))
        }
    }

    /// KeePassXC's CSV ("Group","Title","Username","Password","URL","Notes","TOTP",…) and
    /// KeePass 2's ("Account","Login Name","Password","Web Site","Comments").
    static func readCsv(_ table: ImportCsvTable, _ collector: ImportCollector) throws {
        let keepassXc = table.has("Title", "Username", "Password")
        let keepass2 = table.has("Account", "Login Name", "Password")
        if !keepassXc && !keepass2 {
            throw ImportProblem(featureString("Das sieht nicht nach einem CSV-Export von KeePass oder KeePassXC aus."))
        }
        let known = keepassXc
            ? ["Group", "Title", "Username", "Password", "URL", "Notes", "TOTP", "Icon", "Last Modified", "Created"]
            : ["Account", "Login Name", "Password", "Web Site", "Comments"]
        for row in table.rows {
            let item = collector.login(table.get(row, "Title", "Account"))
            item.login?.username = importSome(table.get(row, "Username", "Login Name"))
            item.login?.password = importSome(table.get(row, "Password"))
            collector.uris(item, table.get(row, "URL", "Web Site"))
            collector.appendNote(item, table.get(row, "Notes", "Comments"))
            let otp = table.get(row, "TOTP")
            if !otp.isEmpty {
                item.login?.totp = totp([("otp", otp, true)], label: item.name)?.totp ?? otp
            }
            for (name, value) in table.rest(row, known: known) { collector.extra(item, name, value) }
            // KeePassXC writes the root group's name in front of every path.
            let group = table.get(row, "Group").replacingOccurrences(of: "\\", with: "/")
                .split(separator: "/", omittingEmptySubsequences: false).dropFirst().joined(separator: "/")
            collector.add(item, folder: group.isEmpty ? nil : group)
        }
    }
}
