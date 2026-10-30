import Foundation

// Bitwarden (bitwarden.ts), the browsers' CSV exports (browsers.ts) and LastPass (lastpass.ts).

// MARK: - Bitwarden

enum ImportBitwarden {
    /// Bitwarden's JSON export with a password of its own ("Passwortgeschützt").
    static func isProtected(_ data: ImportJSON) -> Bool {
        data["encrypted"]?.bool == true && data["passwordProtected"]?.bool == true
    }

    static func accountBound() -> ImportProblem {
        ImportProblem(featureString(
            "Dieser Export ist mit dem Konto verschlüsselt, das ihn erstellt hat. Exportiere noch einmal als JSON – unverschlüsselt oder „Passwortgeschützt“ – und importiere diese Datei."))
    }

    /// Bitwarden's JSON export, read for the preview only: the core imports the text itself.
    static func readJson(_ data: ImportJSON) throws -> ImportExportData {
        if data["encrypted"]?.truthy == true { throw accountBound() }
        guard let items = data["items"]?.array else {
            throw ImportProblem(featureString("Das sieht nicht nach einem JSON-Export von Bitwarden aus."))
        }
        let folders = (data["folders"]?.array ?? []).compactMap { folder -> ImportExportFolder? in
            guard let id = folder["id"]?.scalarText else { return nil }
            return ImportExportFolder(id: id, name: folder["name"]?.string ?? "")
        }
        // Types 1–5 only: the preview lists nothing else.
        return ImportExportData(folders: folders, items: items.compactMap(previewItem))
    }

    /// An item of the file as far as the preview reads it.
    private static func previewItem(_ json: ImportJSON) -> ImportExportItem? {
        guard let raw = json["type"]?.number, raw.rounded() == raw, abs(raw) < 1e9,
              let type = ImportItemType(rawValue: Int(raw))
        else { return nil }
        let item = ImportExportItem(type: type, name: json["name"]?.string ?? "")
        item.folderId = json["folderId"]?.scalarText
        item.fields = (json["fields"]?.array ?? []).compactMap { field in
            guard let name = field["name"]?.string, let value = field["value"]?.string else { return nil }
            return ImportExportField(name: name, value: value, type: .text)
        }
        if let login = json["login"], login.isObject {
            item.login = ImportExportLogin(
                username: login["username"]?.string, password: nil, totp: login["totp"]?.string)
        }
        if let card = json["card"], card.isObject {
            item.card = ImportExportCard(cardholderName: card["cardholderName"]?.string)
        }
        if let identity = json["identity"], identity.isObject {
            item.identity = ImportExportIdentity(
                firstName: identity["firstName"]?.string, lastName: identity["lastName"]?.string)
        }
        return item
    }

    /// Bitwarden's CSV, for the preview only: the vault imports the file itself.
    static func readCsv(_ table: ImportCsvTable, _ collector: ImportCollector) throws {
        guard table.has("name") else {
            throw ImportProblem(featureString("Das sieht nicht nach einem CSV-Export von Bitwarden aus."))
        }
        for row in table.rows {
            let item = collector.login(table.get(row, "name"))
            if table.get(row, "type") == "note" { collector.retype(item, .note) }
            item.favorite = table.get(row, "favorite") == "1"
            item.login?.username = importSome(table.get(row, "login_username"))
            item.login?.password = importSome(table.get(row, "login_password"))
            item.login?.totp = importSome(table.get(row, "login_totp"))
            collector.uris(item, table.get(row, "login_uri").components(separatedBy: ","))
            collector.appendNote(item, table.get(row, "notes"))
            for line in table.get(row, "fields").components(separatedBy: "\n") {
                if let colon = line.range(of: ": "), colon.lowerBound > line.startIndex {
                    collector.field(item, String(line[..<colon.lowerBound]),
                                    String(line[colon.upperBound...]), .text)
                }
            }
            let folder = table.get(row, "folder")
            collector.add(item, folder: folder.isEmpty ? nil : folder)
        }
    }
}

// MARK: - Browsers

enum ImportBrowsers {
    /// The app of an Android password's address, "android://…@com.example.app/".
    static func androidApp(_ url: String) -> String? {
        guard url.hasPrefix("android://"), let at = url.lastIndex(of: "@") else { return nil }
        guard url.distance(from: url.startIndex, to: at) >= "android://".count else { return nil }
        var app = String(url[url.index(after: at)...])
        if url.hasSuffix("/") && !app.isEmpty { app.removeLast() }
        return !app.isEmpty && !app.contains("/") ? app : nil
    }

    static func readChrome(_ table: ImportCsvTable, _ collector: ImportCollector) throws {
        guard table.has("url", "username", "password") else {
            throw ImportProblem(featureString("Das sieht nicht nach einem Passwort-Export von Chrome oder Edge aus."))
        }
        let known = ["name", "url", "username", "password", "note"]
        for row in table.rows {
            let url = table.get(row, "url")
            let app = androidApp(url)
            let name = table.get(row, "name")
            let item = collector.login(!name.isEmpty ? name : app ?? ImportText.nameFromUrl(url))
            item.login?.username = importSome(table.get(row, "username"))
            item.login?.password = importSome(table.get(row, "password"))
            collector.uris(item, app.map { "androidapp://\($0)" } ?? url)
            collector.appendNote(item, table.get(row, "note"))
            for (name, value) in table.rest(row, known: known) { collector.extra(item, name, value) }
            collector.add(item)
        }
    }

    static func readFirefox(_ table: ImportCsvTable, _ collector: ImportCollector) throws {
        let own = ["guid", "httpRealm", "formActionOrigin", "hostname"].contains { table.has($0) }
        guard own, table.has("username", "password") else {
            throw ImportProblem(featureString("Das sieht nicht nach einem Passwort-Export von Firefox aus."))
        }
        let known = [
            "url", "hostname", "username", "password", "formActionOrigin", "guid", "timeCreated",
            "timeLastUsed", "timePasswordChanged",
        ]
        for row in table.rows {
            let url = table.get(row, "url", "hostname")
            if url == "chrome://FirefoxAccounts" { continue }
            let item = collector.login(ImportText.nameFromUrl(url))
            item.login?.username = importSome(table.get(row, "username"))
            item.login?.password = importSome(table.get(row, "password"))
            collector.uris(item, url)
            for (name, value) in table.rest(row, known: known) { collector.extra(item, name, value) }
            collector.add(item)
        }
    }

    static func readApple(_ table: ImportCsvTable, _ collector: ImportCollector) throws {
        guard table.has("Title", "Username", "Password") else {
            throw ImportProblem(featureString("Das sieht nicht nach einem Export der Passwörter-App von Apple aus."))
        }
        let known = ["Title", "URL", "Username", "Password", "Notes", "OTPAuth"]
        for row in table.rows {
            let url = table.get(row, "URL")
            let title = table.get(row, "Title")
            let item = collector.login(!title.isEmpty ? title : ImportText.nameFromUrl(url))
            item.login?.username = importSome(table.get(row, "Username"))
            item.login?.password = importSome(table.get(row, "Password"))
            item.login?.totp = importSome(table.get(row, "OTPAuth"))
            collector.uris(item, url)
            collector.appendNote(item, table.get(row, "Notes"))
            for (name, value) in table.rest(row, known: known) { collector.extra(item, name, value) }
            collector.add(item)
        }
    }
}

// MARK: - LastPass

enum ImportLastPass {
    private static let months = [
        "january", "february", "march", "april", "may", "june", "july", "august", "september",
        "october", "november", "december",
    ]

    /// "June" → "6".
    private static func month(_ name: String?) -> String? {
        months.firstIndex(of: (name ?? "").importTrimmed.lowercased()).map { String($0 + 1) }
    }

    private typealias IdentityKey = WritableKeyPath<ImportExportIdentity, String?> & Sendable
    private typealias CardKey = WritableKeyPath<ImportExportCard, String?> & Sendable

    private static let card: [String: CardKey] = [
        "Number": \.number, "Name on Card": \.cardholderName, "Security Code": \.code,
    ]
    private static let address: [String: IdentityKey] = [
        "Title": \.title, "First Name": \.firstName, "Middle Name": \.middleName,
        "Last Name": \.lastName, "Company": \.company, "Address 1": \.address1,
        "Address 2": \.address2, "Address 3": \.address3, "City / Town": \.city, "State": \.state,
        "Zip / Postal Code": \.postalCode, "Country": \.country, "Email Address": \.email,
        "Username": \.username, "Phone": \.phone,
    ]
    private static let passport: [String: IdentityKey] = ["Number": \.passportNumber, "Country": \.country]
    private static let license: [String: IdentityKey] = [
        "Number": \.licenseNumber, "State": \.state, "Country": \.country, "Address": \.address1,
        "City / Town": \.city, "Zip / Postal Code": \.postalCode,
    ]
    private static let ssn: [String: IdentityKey] = ["Number": \.ssn]

    private static func lines(_ text: String) -> [String] {
        text.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
            .components(separatedBy: "\n")
    }

    /// The "Key:Value" lines of a LastPass form; everything after "Notes:" is the note.
    private static func formLines(_ extra: String) -> (pairs: [(String, String)], notes: String) {
        var pairs: [(String, String)] = []
        let all = lines(extra)
        for (i, line) in all.enumerated() {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let key = String(line[..<colon])
            let value = String(line[line.index(after: colon)...])
            if key == "Notes" {
                return (pairs, ([value] + all[(i + 1)...]).joined(separator: "\n"))
            }
            if key != "NoteType" { pairs.append((key, value)) }
        }
        return (pairs, "")
    }

    private static func readForm(_ collector: ImportCollector, _ item: ImportExportItem, _ extra: String) {
        let type = String((lines(extra).first ?? "").dropFirst("NoteType:".count)).importTrimmed
        let (pairs, notes) = formLines(extra)
        if type == "Wi-Fi Password" {
            readWifiForm(collector, item, pairs, notes)
            return
        }
        var identityMap: [String: IdentityKey]?
        var fullName: String?
        if type == "Credit Card" {
            collector.retype(item, .card)
        } else if ["Address", "Passport", "Driver's License", "Social Security"].contains(type) {
            collector.retype(item, .identity)
            identityMap = ["Address": address, "Passport": passport, "Driver's License": license][type] ?? ssn
        } else {
            collector.retype(item, .note)
            collector.extra(item, "NoteType", type)
        }
        for (key, value) in pairs {
            guard importSome(value) != nil else { continue }
            if item.card != nil && key == "Expiration Date" {
                let parts = value.components(separatedBy: ",")
                item.card?.expMonth = month(parts.first)
                item.card?.expYear = parts.count > 1 ? importSome(parts[1].importTrimmed) : nil
                continue
            }
            if item.identity != nil && key == "Name" && type != "Address" {
                fullName = value
                continue
            }
            if item.card != nil, let target = card[key] {
                item.card![keyPath: target] = value
            } else if item.identity != nil, let target = identityMap?[key] {
                item.identity![keyPath: target] = value
            } else {
                collector.extra(item, key, value)
            }
        }
        if item.card != nil { let number = item.card?.number; item.card?.brand = ImportText.cardBrand(number) }
        if item.identity != nil, let fullName, !fullName.isEmpty {
            let (first, middle, last) = ImportText.splitName(fullName)
            item.identity?.firstName = first
            item.identity?.middleName = middle
            item.identity?.lastName = last
        }
        collector.appendNote(item, notes)
    }

    /// LastPass's "Wi-Fi Password" form: SSID, Password and Authentication make the network.
    private static func readWifiForm(
        _ collector: ImportCollector, _ item: ImportExportItem, _ pairs: [(String, String)], _ notes: String
    ) {
        var network = ImportWifiEntry()
        var seen = Set<String>()
        for (key, value) in pairs {
            guard importSome(value) != nil else { continue }
            if key == "SSID" && seen.insert("ssid").inserted {
                network.ssid = value
            } else if key == "Password" && seen.insert("password").inserted {
                network.password = value
            } else if key == "Authentication" && seen.insert("security").inserted {
                network.security = value
            } else if key != "Language" {
                collector.field(item, key, value)
            }
        }
        collector.wifi(item, network)
        collector.appendNote(item, notes)
    }

    private static func profileCard(_ table: ImportCsvTable, _ row: [String], _ item: ImportExportItem) {
        var card = item.card ?? ImportExportCard()
        card.cardholderName = importSome(table.get(row, "ccname"))
        card.number = importSome(table.get(row, "ccnum"))
        card.code = importSome(table.get(row, "cccsc"))
        card.brand = ImportText.cardBrand(card.number)
        let parts = table.get(row, "ccexp").components(separatedBy: "-")
        if parts.count > 1, !parts[0].isEmpty, !parts[1].isEmpty {
            card.expYear = parts[0]
            card.expMonth = Double(parts[1]).map { ImportJSON.numberText($0) } ?? "NaN"
        }
        item.card = card
    }

    static func read(_ table: ImportCsvTable, _ collector: ImportCollector) throws {
        let profiles = table.has("profilename")
        guard profiles || table.has("url", "username", "password", "extra", "name") else {
            throw ImportProblem(featureString("Das sieht nicht nach einem CSV-Export von LastPass aus."))
        }
        for row in table.rows {
            let grouping = String(String.UnicodeScalarView(table.get(row, "grouping").unicodeScalars.filter {
                !($0.value <= 0x1f || (0x7f...0x9f).contains($0.value))
            }))
            let folder = !grouping.isEmpty && grouping != "(none)" ? grouping : nil

            if profiles {
                let name = table.get(row, "profilename")
                let person = ["title", "firstname", "lastname", "address1", "phone", "username", "email"]
                let hasPerson = person.contains { !table.get(row, $0).isEmpty }
                if hasPerson {
                    let item = collector.item(.identity, name)
                    let map: [(IdentityKey, String)] = [
                        (\.title, "title"), (\.firstName, "firstname"), (\.middleName, "middlename"),
                        (\.lastName, "lastname"), (\.username, "username"), (\.company, "company"),
                        (\.ssn, "ssn"), (\.address1, "address1"), (\.address2, "address2"),
                        (\.address3, "address3"), (\.city, "city"), (\.state, "state"),
                        (\.postalCode, "zip"), (\.country, "country"), (\.email, "email"),
                        (\.phone, "phone"),
                    ]
                    for (target, column) in map { item.identity![keyPath: target] = importSome(table.get(row, column)) }
                    collector.appendNote(item, table.get(row, "notes"))
                    collector.add(item, folder: folder)
                }
                if !hasPerson || !table.get(row, "ccnum").isEmpty {
                    let card = collector.item(.card, name)
                    profileCard(table, row, card)
                    if !hasPerson { collector.appendNote(card, table.get(row, "notes")) }
                    collector.add(card, folder: folder)
                }
                continue
            }

            let url = table.get(row, "url")
            let item = collector.login(table.get(row, "name"))
            item.favorite = table.get(row, "fav") == "1"
            let extra = table.get(row, "extra")
            if url == "http://sn" {
                if extra.hasPrefix("NoteType:") {
                    readForm(collector, item, extra)
                } else {
                    collector.retype(item, .note)
                    collector.appendNote(item, extra)
                }
            } else {
                item.login?.username = importSome(table.get(row, "username"))
                item.login?.password = importSome(table.get(row, "password"))
                item.login?.totp = importSome(table.get(row, "totp"))
                collector.uris(item, url)
                collector.appendNote(item, extra)
            }
            collector.add(item, folder: folder)
        }
    }
}
