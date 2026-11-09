import Foundation

/// What another app hands over through Apple's Credential Exchange (Apple Passwords → "Export
/// data to another app" → UwULock): the FIDO Credential Exchange Format (CXF 1.0) as JSON
/// (lib/import/cxf.ts). Turned into Bitwarden's JSON export like every other import.
///
/// Read leniently: the spec's names and Swift's (`userName`, `relyingPartyIdentifier`,
/// `credentialID`, `created`); binary values in base64url or base64; editable fields as
/// `{ fieldType, value }` or plain strings; dates in seconds or as ISO text.
enum ImportCxf {
    /// The first of `keys` that `object` has (and that isn't null).
    private static func pick(_ object: ImportJSON, _ keys: String...) -> ImportJSON? {
        pick(object, keys)
    }

    private static func pick(_ object: ImportJSON, _ keys: [String]) -> ImportJSON? {
        for key in keys {
            if let value = object[key], !value.isNull { return value }
        }
        return nil
    }

    /// An editable field's value (`{ fieldType, value }`), or a plain string.
    private static func value(_ raw: ImportJSON?) -> String? {
        guard let raw else { return nil }
        switch raw {
        case .string, .number, .bool: return raw.scalarText
        case .object: return value(raw["value"])
        default: return nil
        }
    }

    private static func str(_ object: ImportJSON, _ keys: String...) -> String? {
        importSome(value(pick(object, keys)))
    }

    /// base64url or base64 → bytes.
    private static func bytes(_ raw: ImportJSON?) -> [UInt8]? {
        guard let text = raw?.string?.importTrimmed, !text.isEmpty else { return nil }
        var plain = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while plain.hasSuffix("=") { plain.removeLast() }
        plain += String(repeating: "=", count: (4 - plain.count % 4) % 4)
        return ImportBytes.fromBase64(plain)
    }

    private static func base64(_ data: [UInt8]) -> String {
        Data(data).base64EncodedString()
    }

    /// URL-safe base64 without padding, as Bitwarden keeps a passkey's values.
    private static func b64url(_ data: [UInt8]) -> String {
        var text = base64(data).replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
        while text.hasSuffix("=") { text.removeLast() }
        return text
    }

    /// Seconds (or milliseconds, or ISO text) → ISO text.
    private static func date(_ raw: ImportJSON?) -> String? {
        if let number = raw?.number, number.isFinite {
            let ms = number > 1e12 ? number : number * 1000
            return ImportISODate.string(Date(timeIntervalSince1970: ms / 1000))
        }
        if let text = raw?.string, let date = ImportISODate.parse(text) {
            return ImportISODate.string(date)
        }
        return nil
    }

    /// JavaScript's `Number(value)` for a whole number in range, else `fallback`.
    private static func whole(_ raw: ImportJSON?, _ min: Int, _ max: Int, _ fallback: Int) -> Int {
        let number: Double?
        switch raw {
        case nil: number = Double(fallback)
        case .number(let n): number = n
        case .string(let text): number = text.importTrimmed.isEmpty ? 0 : Double(text.importTrimmed)
        case .bool(let flag): number = flag ? 1 : 0
        default: number = nil
        }
        guard let number, number.rounded() == number, number >= Double(min), number <= Double(max) else {
            return fallback
        }
        return Int(number)
    }

    /// A TOTP credential as the vault keeps it: the base32 secret, or otpauth:// when not the default.
    private static func totp(_ credential: ImportJSON, title: String) -> String? {
        guard let raw = pick(credential, "secret")?.string, !raw.importTrimmed.isEmpty else { return nil }
        let clean = raw.importTrimmed.importReplacing(#"\s+"#, with: "")
        // Apple's Data comes as base64; base32 only when it can't be anything else: upper case
        // only and a length base32 can have.
        let unpadded = clean.importReplacing("=+$", with: "")
        var secret: String? = clean.importMatches("^[A-Z2-7]+=*$") && [0, 2, 4, 5, 7].contains(unpadded.count % 8)
            ? unpadded : nil
        if secret == nil, let decoded = bytes(.string(clean)), !decoded.isEmpty {
            secret = ImportBytes.base32(decoded)
        }
        guard let secret else { return nil }
        let algorithm = (pick(credential, "algorithm")?.scalarText ?? "sha1").uppercased()
        let digits = whole(pick(credential, "digits"), 6, 10, 6)
        let period = whole(pick(credential, "period"), 1, 300, 30)
        let issuer = str(credential, "issuer")
        let user = str(credential, "username", "userName")
        if algorithm == "SHA1" && digits == 6 && period == 30 { return secret }
        let joined = [issuer ?? title, user ?? ""].filter { !$0.isEmpty }.joined(separator: ":")
        let label = ImportURLText.component(joined.isEmpty ? "UwULock" : joined)
        var params = [("secret", secret), ("algorithm", algorithm), ("digits", String(digits)),
                      ("period", String(period))]
        if let issuer { params.append(("issuer", issuer)) }
        return "otpauth://totp/\(label)?\(ImportURLText.form(params))"
    }

    private static func passkey(_ credential: ImportJSON, created: String) -> ImportExportPasskey? {
        guard let id = bytes(pick(credential, "credentialId", "credentialID")), !id.isEmpty,
              let key = bytes(pick(credential, "key")), !key.isEmpty,
              let rpId = str(credential, "rpId", "relyingPartyIdentifier")
        else { return nil }
        let handle = bytes(pick(credential, "userHandle"))
        return ImportExportPasskey(
            credentialId: b64url(id),
            // PKCS#8, as the spec has it and Bitwarden keeps it.
            keyValue: b64url(key),
            rpId: rpId.lowercased(),
            rpName: nil,
            userHandle: handle.flatMap { $0.isEmpty ? nil : b64url($0) },
            userName: str(credential, "username", "userName"),
            userDisplayName: str(credential, "userDisplayName"),
            // Importers start the counter at 0 (CXF 1.0).
            counter: "0",
            discoverable: "true",
            creationDate: created)
    }

    /// "2027-08" (CXF's year-month) → ("8", "2027").
    private static func yearMonth(_ raw: String?) -> (String?, String?) {
        guard let raw, let match = raw.firstMatch(of: /^(\d{4})-(\d{1,2})/) else { return (nil, nil) }
        return (Int(match.2).map(String.init), String(match.1))
    }

    /// Every editable field of a credential, as custom fields: for kinds the vault has no place for.
    private static func fieldsOf(_ collector: ImportCollector, _ item: ImportExportItem, _ credential: ImportJSON,
                                 prefix: String) {
        for (key, raw) in credential.object ?? [] {
            if key == "type" || key == "id" { continue }
            let text = value(raw)
            if importBlank(text) { continue }
            let concealed = raw["fieldType"]?.string == "concealed-string"
            collector.extra(item, "\(prefix): \(key)", text, concealed ? .hidden : nil)
        }
    }

    /// The collections' items as folders: item id → "Collection/Sub".
    private static func folders(_ account: ImportJSON) -> [String: String] {
        var out: [String: String] = [:]
        func walk(_ collection: ImportJSON, _ path: [String], _ depth: Int) {
            // Nested at most this deep.
            guard collection.isObject, depth <= 32 else { return }
            let here = str(collection, "title").map { path + [$0.replacingOccurrences(of: "/", with: "∕")] } ?? path
            for linked in collection["items"]?.array ?? [] {
                let id = linked.isObject ? pick(linked, "item")?.string : linked.string
                if let id, !here.isEmpty, out[id] == nil { out[id] = here.joined(separator: "/") }
            }
            for sub in pick(collection, "subCollections", "subcollections")?.array ?? [] {
                walk(sub, here, depth + 1)
            }
        }
        for collection in account["collections"]?.array ?? [] { walk(collection, [], 0) }
        return out
    }

    /// Reads what the system handed over. Throws an ImportProblem when it isn't CXF.
    static func read(_ json: String) throws -> ImportParsed {
        guard let root = try? ImportBytes.parseJson(Array(json.utf8)), root.isObject,
              let accounts = root["accounts"]?.array
        else {
            throw ImportProblem(featureString("Die übergebenen Daten ließen sich nicht lesen."))
        }
        let collector = ImportCollector()
        let exporter = str(root, "exporterDisplayName", "exporterRpId", "exporterRelyingPartyIdentifier")
        for account in accounts where account.isObject {
            let inFolder = folders(account)
            for raw in account["items"]?.array ?? [] where raw.isObject {
                let title = str(raw, "title") ?? ""
                collector.entry(title) { readItem(collector, raw, title: title, inFolder: inFolder) }
            }
        }
        let (data, warnings) = collector.result()
        return ImportParsed(
            source: .credentialExchange, format: exporter.map { "\($0) (CXF)" } ?? "CXF", data: data,
            warnings: warnings, submitFormat: .json, submitText: data.jsonText())
    }

    private static func readItem(_ collector: ImportCollector, _ raw: ImportJSON, title: String,
                                 inFolder: [String: String]) {
        let folder = raw["id"]?.string.flatMap { inFolder[$0] }
        let created = date(pick(raw, "creationAt", "created")) ?? ImportISODate.now()
        let favorite = raw["favorite"]?.bool == true
        let credentials = (raw["credentials"]?.array ?? []).filter(\.isObject)
        let scope = raw["scope"].flatMap { $0.isObject ? $0 : nil } ?? .object([])
        let urls = (scope["urls"]?.array ?? []).compactMap(\.string)
        let apps = (pick(scope, "androidApps")?.array ?? []).filter(\.isObject)
            .compactMap { str($0, "bundleId", "bundleID") }
            .map { "androidapp://\($0)" }

        var logins: [ImportExportItem] = []
        func login() -> ImportExportItem {
            let made = collector.login(title)
            made.favorite = favorite
            collector.uris(made, urls.map { $0 as String? })
            for app in apps { made.login?.uris.append(ImportExportUri(uri: app, match: nil)) }
            logins.append(made)
            return made
        }
        /// The first login that still has room for `what`.
        func roomFor(_ what: String) -> ImportExportItem {
            logins.first { item in
                switch what {
                case "passkey": return item.login?.fido2Credentials?.isEmpty != false
                case "totp": return item.login?.totp == nil
                // A passkey of the same item may have come first and set the user name.
                default: return item.login?.password == nil
                }
            } ?? login()
        }

        var notes: [String] = []
        var others: [ImportJSON] = []
        /// Cards, networks, identities: added after the logins, in the item's order.
        var made: [ImportExportItem] = []
        for credential in credentials {
            switch credential["type"]?.string {
            case "basic-auth":
                let target = roomFor("password")
                if let username = str(credential, "username", "userName") { target.login?.username = username }
                target.login?.password = value(pick(credential, "password"))
            case "passkey":
                guard let passkey = passkey(credential, created: created) else {
                    collector.warn(featureString("Ein Passkey ließ sich nicht übernehmen."))
                    continue
                }
                let target = roomFor("passkey")
                target.login?.fido2Credentials = [passkey]
                if target.login?.username == nil { target.login?.username = passkey.userName }
                if target.login?.uris.isEmpty != false { collector.uris(target, passkey.rpId) }
            case "totp":
                if let code = totp(credential, title: title) { roomFor("totp").login?.totp = code }
            case "note":
                if let text = str(credential, "content") { notes.append(text) }
            default:
                others.append(credential)
            }
        }

        for credential in others {
            let kind = credential["type"]?.scalarText ?? ""
            switch kind {
            case "credit-card":
                let card = collector.item(.card, title)
                card.favorite = favorite
                let number = str(credential, "number")
                let (month, year) = yearMonth(str(credential, "expiryDate"))
                card.card = ImportExportCard(
                    cardholderName: str(credential, "fullName"),
                    brand: str(credential, "cardType") ?? ImportText.cardBrand(number),
                    number: number, expMonth: month, expYear: year,
                    code: str(credential, "verificationNumber"))
                collector.field(card, "PIN", str(credential, "pin"), .hidden)
                collector.field(card, featureString("Gültig ab"), str(credential, "validFrom"))
                made.append(card)
            case "wifi":
                let wifi = collector.item(.note, title)
                wifi.favorite = favorite
                collector.wifi(wifi, ImportWifiEntry(
                    ssid: str(credential, "ssid"), password: value(pick(credential, "passphrase")),
                    security: str(credential, "networkSecurityType"),
                    hidden: str(credential, "hidden")?.lowercased() == "true"))
                made.append(wifi)
            case "address", "person-name":
                let person = collector.item(.identity, title)
                person.favorite = favorite
                var identity = person.identity ?? ImportExportIdentity()
                if kind == "address" {
                    identity.address1 = str(credential, "streetAddress")
                    identity.postalCode = str(credential, "postalCode")
                    identity.city = str(credential, "city")
                    identity.state = str(credential, "territory")
                    identity.country = str(credential, "country")
                    identity.phone = str(credential, "tel", "telephone")
                } else {
                    identity.title = str(credential, "title")
                    identity.firstName = str(credential, "given")
                    identity.middleName = str(credential, "given2")
                    let last = [str(credential, "surnamePrefix"), str(credential, "surname")]
                        .compactMap { $0 }.joined(separator: " ")
                    identity.lastName = last.isEmpty ? nil : last
                    if identity.firstName == nil && identity.lastName == nil {
                        let (first, middle, lastName) = ImportText.splitName(title)
                        identity.firstName = first
                        identity.middleName = middle
                        identity.lastName = lastName
                    }
                }
                person.identity = identity
                made.append(person)
            case "generated-password":
                let target = logins.first ?? login()
                if target.login?.password == nil {
                    target.login?.password = str(credential, "password")
                } else {
                    collector.extra(target, featureString("Erzeugtes Passwort"), str(credential, "password"), .hidden)
                }
            case "custom-fields":
                let target = logins.first ?? login()
                let label = str(credential, "label")
                for field in (credential["fields"]?.array ?? []).filter(\.isObject) {
                    let joined = [label, str(field, "label")].compactMap { $0 }.joined(separator: ": ")
                    let name = joined.isEmpty ? featureString("Feld") : joined
                    let concealed = field["fieldType"]?.string == "concealed-string"
                    collector.field(target, name, value(field["value"]), concealed ? .hidden : nil)
                }
            default:
                // api-key, ssh-key, passport, drivers-license, identity-document, …: kept as fields.
                let target = logins.first ?? login()
                if kind == "ssh-key" {
                    if let key = bytes(pick(credential, "privateKey")) {
                        collector.extra(target, featureString("Privater SSH-Schlüssel (PKCS#8, base64)"),
                                        base64(key), .hidden)
                    }
                    collector.extra(target, featureString("SSH-Schlüssel-Kommentar"), str(credential, "keyComment"))
                } else {
                    fieldsOf(collector, target, credential, prefix: kind.isEmpty ? featureString("Feld") : kind)
                }
            }
        }

        // Only a note (or nothing at all): a secure note. Otherwise the notes go with the first item.
        var all = logins + made
        if all.isEmpty {
            let note = collector.item(.note, title)
            note.favorite = favorite
            all.append(note)
        }
        collector.appendNote(all[0], notes.joined(separator: "\n\n"))
        for item in all { collector.add(item, folder: folder) }
    }
}
