import Foundation

// MARK: - Zip (zip.ts)

/// Files by name: a zip, or a single file standing in for one.
protocol ImportArchive {
    var names: [String] { get }
    func read(_ name: String) throws -> [UInt8]
}

/// A single file standing in for an archive (export.data taken out of a .1pux by hand).
struct ImportSingleFile: ImportArchive {
    let name: String
    let bytes: [UInt8]
    var names: [String] { [name] }
    func read(_ name: String) throws -> [UInt8] { bytes }
}

/// Just enough of zip to read an export: 1Password's .1pux and Proton Pass's zip.
struct ImportZip: ImportArchive {
    private struct Entry {
        var method: Int
        var flags: Int
        var size: UInt32
        var offset: UInt32
    }

    private let bytes: [UInt8]
    private var entries: [String: Entry] = [:]
    private(set) var names: [String] = []

    private static func broken() -> ImportProblem {
        ImportProblem(featureString("Die ZIP-Datei ist beschädigt oder unvollständig."))
    }

    static func isZip(_ bytes: [UInt8]) -> Bool {
        bytes.count > 4 && bytes[0] == 0x50 && bytes[1] == 0x4B && bytes[2] == 3
    }

    init(_ bytes: [UInt8]) throws {
        self.bytes = bytes
        func u32(_ at: Int) -> UInt32 {
            UInt32(bytes[at]) | UInt32(bytes[at + 1]) << 8 | UInt32(bytes[at + 2]) << 16 | UInt32(bytes[at + 3]) << 24
        }
        // The end record sits in the last 22 bytes plus a comment of up to 64 KiB.
        var end = -1
        if bytes.count >= 22 {
            var at = bytes.count - 22
            let lowest = max(0, bytes.count - 22 - 0xFFFF)
            while at >= lowest {
                if u32(at) == 0x0605_4B50 { end = at; break }
                at -= 1
            }
        }
        if end < 0 { throw Self.broken() }
        let count = Int(bytes[end + 10]) | Int(bytes[end + 11]) << 8
        if count > ImportLimits.maxZipEntries {
            let limit = ImportLimits.maxZipEntries
            throw ImportProblem(featureString("Die ZIP-Datei hat mehr als \(limit) Einträge, das importiert UwULock nicht."))
        }
        guard end + 20 <= bytes.count else { throw Self.broken() }
        var reader = ImportReader(bytes, truncated: { Self.broken() })
        reader.at = Int(u32(end + 16))
        for _ in 0..<count {
            if try reader.u32() != 0x0201_4B50 { throw Self.broken() }
            _ = try reader.take(4)
            let flags = try reader.u16()
            let method = try reader.u16()
            _ = try reader.take(8)
            let size = try reader.u32()
            _ = try reader.take(4)
            let nameLength = try reader.u16()
            let extraLength = try reader.u16()
            let commentLength = try reader.u16()
            _ = try reader.take(8)
            let offset = try reader.u32()
            let name = String(decoding: try reader.take(nameLength), as: UTF8.self)
            _ = try reader.take(extraLength + commentLength)
            if entries[name] == nil { names.append(name) }
            entries[name] = Entry(method: method, flags: flags, size: size, offset: offset)
        }
    }

    func read(_ name: String) throws -> [UInt8] {
        guard let entry = entries[name] else {
            throw ImportProblem(featureString("In der ZIP-Datei fehlt \(name)."))
        }
        if entry.flags & 1 != 0 { throw ImportProblem(featureString("Die ZIP-Datei ist verschlüsselt.")) }
        if entry.size == 0xFFFF_FFFF { throw ImportProblem(featureString("Die ZIP-Datei ist zu groß.")) }
        var reader = ImportReader(bytes, truncated: { Self.broken() })
        reader.at = Int(entry.offset)
        if try reader.u32() != 0x0403_4B50 { throw Self.broken() }
        _ = try reader.take(22)
        let skip = try reader.u16() + reader.u16()
        _ = try reader.take(skip)
        let data = try reader.take(Int(entry.size))
        switch entry.method {
        case 0: return data
        case 8:
            do {
                return try ImportBytes.inflateRaw(data[...], limit: ImportLimits.maxUnpackedBytes,
                                                  tooMuch: ImportLimits.unpacksTooMuch)
            } catch let problem as ImportProblem {
                throw problem
            } catch {
                throw Self.broken()
            }
        default:
            throw ImportProblem(featureString("Die ZIP-Datei ist in einer Art komprimiert, die UwULock nicht kennt."))
        }
    }
}

// MARK: - 1Password (onepassword.ts)

/// 1Password: the .1pux export (export.data: accounts → vaults → items) and the CSV exports.
/// Categories become item types, section fields go to the card or identity by id, TOTP fields
/// to the login, the rest to custom fields; several vaults become folders, bank accounts notes.
enum ImportOnePassword {
    private enum Category {
        static let login = "001", creditCard = "002", secureNote = "003", identity = "004"
        static let password = "005", document = "006", softwareLicense = "100", bankAccount = "101"
        static let driversLicense = "103", outdoorLicense = "104", membership = "105", passport = "106"
        static let rewardsProgram = "107", socialSecurityNumber = "108", wirelessRouter = "109"
        static let server = "110", emailAccount = "111", apiCredential = "112", medicalRecord = "113"
        static let sshKey = "114"
    }

    private struct Problem: Error {}

    /// A section field's value as text.
    private static func valueText(_ value: ImportJSON) throws -> String {
        if let date = value["date"], !date.isNull {
            guard let seconds = date.number, seconds.isFinite else { throw Problem() }
            return String(ImportISODate.string(Date(timeIntervalSince1970: seconds)).prefix(10))
        }
        if let monthYear = value["monthYear"], !monthYear.isNull {
            let digits = Array(monthYear.scalarText ?? "")
            func slice(_ from: Int, _ to: Int) -> String {
                from < digits.count ? String(digits[from..<min(to, digits.count)]) : ""
            }
            return "\(slice(4, 6))/\(slice(0, 4))"
        }
        if let email = value["email"], email.truthy { return email["email_address"]?.string ?? "" }
        if let address = value["address"], address.truthy {
            func part(_ key: String) -> String? { address[key]?.string }
            let zipCity = [part("zip"), part("city")].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " ")
            return [part("street"), zipCity, part("state"), part("country")]
                .filter { !importBlank($0) }.compactMap { $0 }.joined(separator: ", ")
        }
        return value.object?.first { $0.1.string != nil }?.1.string ?? ""
    }

    private static func itemType(_ category: String?) -> ImportItemType {
        switch category {
        case Category.creditCard: .card
        case Category.identity, Category.driversLicense, Category.outdoorLicense, Category.membership,
             Category.passport, Category.rewardsProgram, Category.socialSecurityNumber: .identity
        case Category.secureNote, Category.softwareLicense, Category.bankAccount, Category.emailAccount,
             Category.medicalRecord, Category.document: .note
        case Category.sshKey: .sshKey
        default: .login
        }
    }

    private static func monthYearDigits(_ value: ImportJSON) -> String? {
        guard let monthYear = value["monthYear"], !monthYear.isNull else { return nil }
        return monthYear.scalarText
    }

    /// Puts a section field where it belongs; false when it has no place and becomes a field.
    private static func place(_ item: ImportExportItem, _ category: String?, _ field: ImportJSON, _ value: String) -> Bool {
        let id = field["id"]?.string ?? ""
        let title = field["title"]?.string ?? ""
        let v = field["value"] ?? .null
        if item.type == .login, var login = item.login {
            defer { item.login = login }
            let api = category == Category.apiCredential
            if login.username?.isEmpty != false && title == "username" {
                login.username = value
            } else if login.password?.isEmpty != false && (title == "password" || (api && title == "credential")) {
                login.password = value
            } else if login.totp?.isEmpty != false && (id.hasPrefix("TOTP_") || (v["totp"].map { !$0.isNull } ?? false)) {
                login.totp = value
            } else if login.uris.isEmpty && !ImportText.unsafeUri(value)
                        && ((category == Category.server && id == "url") || (api && title == "hostname")) {
                login.uris.append(ImportExportUri(uri: value, match: nil))
            } else {
                return false
            }
            return true
        }
        if item.type == .card, var card = item.card {
            defer { item.card = card }
            if card.number?.isEmpty != false && id == "ccnum" {
                card.number = value
                card.brand = ImportText.cardBrand(value)
            } else if card.code?.isEmpty != false && id == "cvv" {
                card.code = value
            } else if card.cardholderName?.isEmpty != false && id == "cardholder" {
                card.cardholderName = value
            } else if card.expMonth?.isEmpty != false && id == "expiry", let digits = monthYearDigits(v) {
                let chars = Array(digits)
                card.expYear = String(chars.prefix(4))
                let month = chars.count > 4 ? String(chars[4..<min(6, chars.count)]) : ""
                card.expMonth = Int(month).map(String.init) ?? (month.isEmpty ? "0" : "NaN")
            } else if id != "type" {
                return false // The type follows from the number.
            }
            return true
        }
        if item.type == .identity, var identity = item.identity {
            defer { item.identity = identity }
            func set(_ key: WritableKeyPath<ImportExportIdentity, String?>, _ candidates: [String]) -> Bool {
                if identity[keyPath: key]?.isEmpty == false || !candidates.contains(id) { return false }
                identity[keyPath: key] = value
                return true
            }
            func fullName(_ candidates: [String]) -> Bool {
                if identity.firstName?.isEmpty == false || !candidates.contains(id) { return false }
                (identity.firstName, identity.middleName, identity.lastName) = ImportText.splitName(value)
                return true
            }
            if let address = v["address"], address.truthy, identity.address1?.isEmpty != false {
                identity.address1 = importSome(address["street"]?.string)
                identity.city = importSome(address["city"]?.string)
                identity.postalCode = importSome(address["zip"]?.string)
                identity.state = importSome(address["state"]?.string)
                identity.country = importSome(address["country"]?.string?.uppercased())
                return true
            }
            if set(\.firstName, ["firstname"]) || set(\.lastName, ["lastname"]) { return true }
            if set(\.middleName, ["initial"]) || set(\.company, ["company"]) { return true }
            if set(\.phone, ["defphone"]) || set(\.username, ["username"]) { return true }
            if identity.email?.isEmpty != false && ((v["email"]?.truthy ?? false) || id == "email") {
                identity.email = value
                return true
            }
            switch category {
            case Category.driversLicense:
                return fullName(["fullname"]) || set(\.licenseNumber, ["number"]) || set(\.country, ["country"])
                    || set(\.state, ["state"]) || set(\.address1, ["address"])
            case Category.outdoorLicense:
                return fullName(["name"]) || set(\.country, ["country"]) || set(\.state, ["state"])
            case Category.membership:
                return fullName(["member_name"]) || set(\.company, ["org_name"]) || set(\.phone, ["phone"])
            case Category.passport:
                return fullName(["fullname"]) || set(\.passportNumber, ["number"]) || set(\.country, ["issuing_country"])
            case Category.rewardsProgram:
                return fullName(["member_name"]) || set(\.company, ["company_name"])
            case Category.socialSecurityNumber:
                return fullName(["name"]) || set(\.ssn, ["number"])
            default:
                return false
            }
        }
        return false
    }

    private static func isSet(_ value: ImportJSON?, _ key: String) -> Bool {
        guard let inner = value?[key] else { return false }
        return !inner.isNull
    }

    /// A "Wireless Router": the network becomes UwULock's Wi-Fi type; the base station's name,
    /// password, address and the rest stay as fields.
    private static func readWirelessRouter(_ collector: ImportCollector, _ entry: ImportJSON) throws -> ImportExportItem {
        let details = entry["details"] ?? .object([])
        let overview = entry["overview"] ?? .object([])
        let item = collector.item(.note, overview["title"]?.string)
        item.favorite = entry["favIndex"]?.number == 1
        var network = ImportWifiEntry()
        var seen = Set<String>()
        for section in details["sections"]?.array ?? [] {
            for field in section["fields"]?.array ?? [] {
                guard let v = field["value"], v.truthy else { continue }
                let value = try valueText(v)
                if importBlank(value) { continue }
                let id = field["id"]?.string
                if id == "network_name" && seen.insert("ssid").inserted {
                    network.ssid = value
                } else if id == "wireless_password" && seen.insert("password").inserted {
                    network.password = value
                } else if id == "wireless_security" && seen.insert("security").inserted {
                    network.security = value
                } else {
                    let name = importSome(field["title"]?.string) ?? section["title"]?.string ?? ""
                    collector.field(item, name, value, isSet(v, "concealed") ? .hidden : nil)
                }
            }
        }
        let urls = [overview["url"]?.string] + (overview["urls"]?.array ?? []).map { $0["url"]?.string }
        for url in urls where !importBlank(url) { collector.field(item, "URL", url, .text) }
        collector.wifi(item, network)
        let tags = (overview["tags"]?.array ?? []).compactMap(\.scalarText)
        if !tags.isEmpty { collector.extra(item, "Tags", tags.joined(separator: ", ")) }
        collector.appendNote(item, details["notesPlain"]?.string)
        return item
    }

    private static func readItem(_ collector: ImportCollector, _ entry: ImportJSON) throws -> ImportExportItem {
        guard entry.isObject else { throw Problem() }
        let category = entry["categoryUuid"]?.string
        if category == Category.wirelessRouter { return try readWirelessRouter(collector, entry) }
        let details = entry["details"] ?? .object([])
        let overview = entry["overview"] ?? .object([])
        let type = itemType(category)
        let item = collector.item(type, overview["title"]?.string)
        collector.retype(item, .login)
        item.type = type
        item.favorite = entry["favIndex"]?.number == 1
        let urls = (overview["urls"]?.array ?? []).map { $0["url"]?.string }
        collector.uris(item, urls.isEmpty ? [overview["url"]?.string] : urls)

        for field in details["loginFields"]?.array ?? [] {
            let value = field["value"]?.string ?? ""
            let designation = field["designation"]?.string
            let fieldType = field["fieldType"]?.string
            if designation == "username" && !value.isEmpty {
                collector.retype(item, .login)
                item.login?.username = value
            } else if designation == "password" && !value.isEmpty {
                collector.retype(item, .login)
                item.login?.password = value
            } else if fieldType == "C" {
                // A ticked box on the website's form; an empty one is nothing worth keeping.
                if !value.isEmpty { collector.field(item, field["name"]?.string ?? "", "true", .boolean) }
            } else {
                collector.field(item, field["name"]?.string ?? "", value, fieldType == "P" ? .hidden : nil)
            }
        }
        if category == Category.password, let password = details["password"]?.string, !password.isEmpty {
            item.login?.password = password
        }

        let history = (details["passwordHistory"]?.array ?? [])
            .compactMap { h -> (String, Double)? in
                guard let value = h["value"]?.string, !importBlank(value), let time = h["time"]?.number else { return nil }
                return (value, time)
            }
            .sorted { $0.1 > $1.1 }
            .prefix(5)
            .map { value, time in
                let ms = ImportJSON.numberText(time).count >= 13 ? time : time * 1000
                return ImportPasswordHistory(password: value, lastUsedDate: ImportISODate.string(Date(timeIntervalSince1970: ms / 1000)))
            }
        if !history.isEmpty { item.passwordHistory = Array(history) }

        for section in details["sections"]?.array ?? [] {
            for field in section["fields"]?.array ?? [] {
                guard let v = field["value"], v.truthy else { continue }
                if let ssh = v["sshKey"], ssh.truthy, item.type == .sshKey {
                    let meta = ssh["metadata"] ?? .object([])
                    if let privateKey = importSome(meta["privateKey"]?.string),
                       let publicKey = importSome(meta["publicKey"]?.string),
                       let fingerprint = importSome(meta["fingerprint"]?.string) {
                        item.sshKey = ImportExportSshKey(privateKey: privateKey, publicKey: publicKey, keyFingerprint: fingerprint)
                        continue
                    }
                }
                let value = try valueText(v)
                if importBlank(value) { continue }
                if place(item, category, field, value) { continue }
                let title = field["title"]?.string
                let name = importSome(title) ?? section["title"]?.string ?? ""
                if title == "password" && (item.passwordHistory?.contains { $0.password == value } ?? false) { continue }
                // An authenticator key the login has no room for stays hidden, like a password.
                let secret = isSet(v, "concealed") || isSet(v, "sshKey") || isSet(v, "totp")
                    || (field["id"]?.string?.hasPrefix("TOTP_") ?? false)
                collector.field(item, name, value, secret ? .hidden : nil)
                if let provider = v["email"]?["provider"]?.string, !provider.isEmpty {
                    collector.field(item, "provider", provider)
                }
            }
        }
        if item.type == .sshKey && item.sshKey == nil { collector.retype(item, .note) }
        if let document = details["documentAttributes"], document.truthy { collector.attachment() }

        let tags = (overview["tags"]?.array ?? []).compactMap(\.scalarText)
        if !tags.isEmpty { collector.extra(item, "Tags", tags.joined(separator: ", ")) }
        collector.appendNote(item, details["notesPlain"]?.string)
        return item
    }

    static func read1pux(_ zip: ImportArchive, _ collector: ImportCollector) throws {
        if !zip.names.contains("export.data") {
            throw ImportProblem(featureString("Das ist keine 1PUX-Datei von 1Password (export.data fehlt)."))
        }
        let data = try ImportBytes.parseJson(try zip.read("export.data"))
        let vaults = (data["accounts"]?.array ?? []).flatMap { $0["vaults"]?.array ?? [] }
        let several = vaults.count > 1
        var archived = 0
        var counted = 0
        for vault in vaults {
            let folder = several ? vault["attrs"]?["name"]?.string : nil
            for entry in vault["items"]?.array ?? [] {
                if entry["details"]?["documentAttributes"]?.truthy ?? false { counted += 1 }
                collector.entry(entry["overview"]?["title"]?.string) {
                    collector.add(try readItem(collector, entry), folder: folder)
                    if entry["state"]?.string == "archived" { archived += 1 }
                }
            }
        }
        // Documents and files attached to items, stored beside export.data.
        let files = zip.names.filter { $0.hasPrefix("files/") && !$0.hasSuffix("/") }.count
        if files > counted { collector.attachment(files - counted) }
        if archived > 0 {
            collector.warn(featureString("\(archived) archivierte Einträge kommen als normale Einträge mit."))
        }
    }

    /// Columns of 1Password's CSVs that are its own bookkeeping.
    private static let bookkeeping = ["uuid", "ainfo", "autosubmit", "ps", "scope", "created date", "modified date"]

    static func readCsv(_ table: ImportCsvTable, _ collector: ImportCollector) throws {
        if !table.has("title") {
            throw ImportProblem(featureString("Das sieht nicht nach einem CSV-Export von 1Password aus."))
        }
        let used = ["title", "url", "urls", "website", "username", "password", "otpauth", "one-time password",
                    "favorite", "archived", "tags", "notes", "notesplain", "type"] + bookkeeping
        let yes = ["true", "1", "yes"]
        var archived = 0
        for row in table.rows {
            let kind = table.get(row, "type").lowercased()
            let type: ImportItemType = kind == "credit card" ? .card : kind == "identity" ? .identity : .login
            let item = collector.item(type, table.get(row, "title"))
            collector.retype(item, .login)
            item.type = type
            item.login?.username = importSome(table.get(row, "username"))
            item.login?.password = importSome(table.get(row, "password"))
            item.login?.totp = importSome(table.get(row, "otpauth", "one-time password"))
            let urls = table.get(row, "urls", "url", "website").replacingOccurrences(of: "\r\n", with: "\n")
                .split(separator: "\n", omittingEmptySubsequences: false).map { String($0) as String? }
            collector.uris(item, urls)
            item.favorite = yes.contains(table.get(row, "favorite").lowercased())
            if yes.contains(table.get(row, "archived").lowercased()) { archived += 1 }
            collector.appendNote(item, table.get(row, "notesPlain"))
            collector.appendNote(item, table.get(row, "notes"))
            let tags = table.get(row, "tags")
            if !tags.isEmpty { collector.extra(item, "Tags", tags) }

            for (name, value) in table.rest(row, known: used) {
                let lower = name.lowercased()
                if item.card != nil, item.card?.number?.isEmpty != false, lower.contains("number"), !lower.contains("verification") {
                    item.card?.number = value
                    item.card?.brand = ImportText.cardBrand(value)
                } else if item.card != nil, item.card?.code?.isEmpty != false, lower.contains("verification number") {
                    item.card?.code = value
                } else if item.card != nil, item.card?.cardholderName?.isEmpty != false, lower.contains("cardholder") {
                    item.card?.cardholderName = value
                } else if item.card != nil, item.card?.expMonth?.isEmpty != false, lower.contains("expiry date") {
                    // "MM/YYYY", or "YYYYMM"
                    if let (month, year) = expiry(value) {
                        item.card?.expMonth = String(Int(month) ?? 0)
                        item.card?.expYear = year
                    } else {
                        collector.extra(item, name, value)
                    }
                } else if !identitySet(item, lower, value) {
                    collector.extra(item, name, value)
                }
            }
            if item.type == .identity, let username = item.login?.username, !username.isEmpty,
               item.identity?.username?.isEmpty != false {
                item.identity?.username = username
            }
            if kind == "wireless router" { wirelessRouterCsv(collector, item) }
            collector.add(item)
        }
        if archived > 0 {
            collector.warn(featureString("\(archived) archivierte Einträge kommen als normale Einträge mit."))
        }
    }

    private static func expiry(_ value: String) -> (String, String)? {
        let patterns = [#"^(\d{1,2})/(?:\d{1,2}/)?(\d{4})"#, #"^(\d{4})(\d{2})$"#]
        for pattern in patterns {
            guard let regex = try? NSRegularExpression(pattern: pattern),
                  let match = regex.firstMatch(in: value, range: NSRange(value.startIndex..., in: value)),
                  let first = Range(match.range(at: 1), in: value), let second = Range(match.range(at: 2), in: value)
            else { continue }
            let a = String(value[first]), b = String(value[second])
            return a.count == 4 ? (b, a) : (a, b)
        }
        return nil
    }

    private static func identitySet(_ item: ImportExportItem, _ lower: String, _ value: String) -> Bool {
        guard item.identity != nil else { return false }
        let rules: [(WritableKeyPath<ImportExportIdentity, String?>, String)] = [
            (\.firstName, "first name"), (\.middleName, "initial"), (\.lastName, "last name"),
            (\.email, "email"), (\.phone, "phone"), (\.company, "company"), (\.username, "username"),
        ]
        for (key, word) in rules where item.identity?[keyPath: key]?.isEmpty != false && lower.contains(word) {
            item.identity?[keyPath: key] = value
            return true
        }
        return false
    }

    /// A "Wireless Router" row of an older CSV export: its network columns become the Wi-Fi
    /// network; the row's own password is the base station's and stays a field.
    private static func wirelessRouterCsv(_ collector: ImportCollector, _ item: ImportExportItem) {
        func take(_ names: String...) -> String? {
            guard let index = item.fields.firstIndex(where: { names.contains($0.name.lowercased()) }) else { return nil }
            return item.fields.remove(at: index).value
        }
        let ssid = take("network name")
        let password = take("wireless network password", "wireless password")
        let security = take("wireless security")
        if let base = item.login?.password, !base.isEmpty {
            collector.field(item, featureString("Passwort der Basisstation"), base, .hidden)
            item.login?.password = nil
        }
        collector.wifi(item, ImportWifiEntry(ssid: ssid, password: password, security: security))
    }
}
