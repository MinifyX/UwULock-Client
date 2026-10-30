import Foundation

/// Proton Pass (protonpass.ts): its unencrypted JSON export (data.json, alone or in its zip) and
/// its CSV export. Vaults become folders only when there are several.
enum ImportProton {
    private static let trashed = 2

    static func encrypted() -> ImportProblem {
        ImportProblem(featureString(
            "Dieser Export von Proton Pass ist mit PGP verschlüsselt. Exportiere noch einmal ohne Verschlüsselung (JSON oder CSV) und importiere diese Datei."))
    }

    private static func notProton() -> ImportProblem {
        ImportProblem(featureString("Das sieht nicht nach einem Export von Proton Pass aus."))
    }

    /// Proton Pass's Wi-Fi security: a number (1 WPA, 2 WPA2, 3 WPA3, 4 WEP) or the name.
    private static func wifiSecurity(_ value: ImportJSON?) -> String? {
        switch value {
        case .string(let text):
            if text.importMatches(#"^\d+$"#) { return wifiSecurity(.number(Double(text) ?? 0)) }
            return text
        case .number(let number):
            return [1: "WPA", 2: "WPA2", 3: "WPA3", 4: "WEP"][Int(exactly: number) ?? 0]
        default:
            return nil
        }
    }

    private static let identityKeys: Set<String> = [
        "fullName", "firstName", "middleName", "lastName", "email", "phoneNumber", "company",
        "socialSecurityNumber", "passportNumber", "licenseNumber", "organization", "streetAddress",
        "floor", "county", "city", "stateOrProvince", "zipOrPostalCode", "countryOrRegion",
    ]
    private static let identityLists: Set<String> = [
        "extraPersonalDetails", "extraAddressDetails", "extraContactDetails", "extraWorkDetails",
    ]

    private static func extraFields(_ collector: ImportCollector, _ item: ImportExportItem, _ fields: ImportJSON?) {
        for field in fields?.array ?? [] {
            let data = field["data"] ?? .object([])
            let type = field["type"]?.string
            let value: String?
            switch type {
            case "totp": value = data["totpUri"]?.string
            case "timestamp":
                let stamp = data["timestamp"]
                value = stamp == nil || stamp?.isNull == true ? "" : stamp?.scalarText ?? ""
            default: value = data["content"]?.string
            }
            let hidden = type == "totp" || type == "hidden"
            collector.field(item, field["fieldName"]?.string ?? "", value, hidden ? .hidden : .text)
        }
    }

    private static func sections(_ collector: ImportCollector, _ item: ImportExportItem, _ list: ImportJSON?) {
        for section in list?.array ?? [] { extraFields(collector, item, section["sectionFields"]) }
    }

    private static func readItem(_ collector: ImportCollector, _ entry: ImportJSON) -> ImportExportItem? {
        let data = entry["data"] ?? .object([])
        let content = data["content"] ?? .object([])
        func str(_ key: String) -> String { content[key]?.string ?? "" }
        let item = collector.login(data["metadata"]?["name"]?.string)
        item.favorite = entry["pinned"]?.bool == true
        collector.appendNote(item, data["metadata"]?["note"]?.string)

        switch data["type"]?.string {
        case "login":
            collector.uris(item, (content["urls"]?.array ?? []).map(\.string))
            let email = content["itemEmail"]?.string
            item.login?.username = importSome(content["itemUsername"]?.string) ?? importSome(email)
            if let username = item.login?.username, !username.isEmpty, username != email {
                collector.field(item, "email", email)
            }
            item.login?.password = importSome(content["password"]?.string)
            item.login?.totp = importSome(content["totpUri"]?.string)
            if let passkeys = content["passkeys"]?.array, !passkeys.isEmpty {
                collector.warn(featureString(
                    "Passkeys aus Proton Pass kommen nicht mit; melde dich damit neu an, wo du sie nutzt."))
            }
        case "alias":
            // An address that forwards mail: kept as a login with the address as its user name.
            item.login?.username = importSome(entry["aliasEmail"]?.string)
        case "note":
            collector.retype(item, .note)
        case "creditCard":
            collector.retype(item, .card)
            item.card?.cardholderName = importSome(str("cardholderName"))
            item.card?.number = importSome(str("number"))
            let number = item.card?.number
            item.card?.brand = ImportText.cardBrand(number)
            item.card?.code = importSome(str("verificationNumber"))
            // "2027-03"
            let expiry = Array(str("expirationDate"))
            if !expiry.isEmpty {
                item.card?.expYear = String(expiry.prefix(4))
                let month = expiry.count > 5 ? String(expiry[5..<min(7, expiry.count)]) : ""
                item.card?.expMonth = month.isEmpty ? "0" : Int(month).map(String.init) ?? "NaN"
            }
            collector.field(item, "PIN", str("pin"), .hidden)
        case "identity":
            collector.retype(item, .identity)
            let (first, middle, last) = ImportText.splitName(str("fullName"))
            let named = !(str("firstName") + str("middleName") + str("lastName")).isEmpty
            var identity = item.identity ?? ImportExportIdentity()
            identity.firstName = named ? importSome(str("firstName")) : first
            identity.middleName = named ? importSome(str("middleName")) : middle
            identity.lastName = named ? importSome(str("lastName")) : last
            identity.email = importSome(str("email"))
            identity.phone = importSome(str("phoneNumber"))
            identity.company = importSome(str("company"))
            identity.ssn = importSome(str("socialSecurityNumber"))
            identity.passportNumber = importSome(str("passportNumber"))
            identity.licenseNumber = importSome(str("licenseNumber"))
            identity.address1 = importSome(str("organization"))
            identity.address2 = importSome(str("streetAddress"))
            identity.address3 = importSome("\(str("floor")) \(str("county"))".importTrimmed)
            identity.city = importSome(str("city"))
            identity.state = importSome(str("stateOrProvince"))
            identity.postalCode = importSome(str("zipOrPostalCode"))
            identity.country = importSome(str("countryOrRegion"))
            item.identity = identity
            for (key, value) in content.object ?? [] {
                if identityKeys.contains(key) { continue }
                if identityLists.contains(key) {
                    extraFields(collector, item, value)
                } else if key == "extraSections" {
                    sections(collector, item, value)
                } else if let text = value.string {
                    collector.extra(item, key, text)
                }
            }
        case "sshKey":
            let privateKey = str("privateKey"), publicKey = str("publicKey"), fingerprint = str("fingerprint")
            if !privateKey.isEmpty && !publicKey.isEmpty && !fingerprint.isEmpty {
                collector.retype(item, .sshKey)
                item.sshKey = ImportExportSshKey(privateKey: privateKey, publicKey: publicKey, keyFingerprint: fingerprint)
            } else {
                // The vault needs all three; what there is stays, in a note.
                collector.retype(item, .note)
                collector.field(item, featureString("Privater Schlüssel"), privateKey, .hidden)
                collector.field(item, featureString("Öffentlicher Schlüssel"), publicKey, .text)
                collector.field(item, featureString("Fingerabdruck"), fingerprint, .text)
            }
            sections(collector, item, content["sections"])
        case "custom":
            collector.retype(item, .note)
            sections(collector, item, content["sections"])
        case "wifi":
            collector.wifi(item, ImportWifiEntry(
                ssid: str("ssid"), password: str("password"), security: wifiSecurity(content["security"])))
            sections(collector, item, content["sections"])
        default:
            return nil
        }
        extraFields(collector, item, data["extraFields"])
        return item
    }

    static func readJson(_ data: ImportJSON, _ collector: ImportCollector) throws {
        if data["encrypted"]?.truthy == true { throw encrypted() }
        guard let vaults = data["vaults"]?.object else { throw notProton() }
        var trashedCount = 0
        var unknown = 0
        for (_, vault) in vaults {
            let folder = vaults.count > 1 ? vault["name"]?.string : nil
            for entry in vault["items"]?.array ?? [] {
                collector.entry(entry["data"]?["metadata"]?["name"]?.string) {
                    if entry["state"]?.number == Double(trashed) {
                        trashedCount += 1
                        return
                    }
                    if let item = readItem(collector, entry) {
                        collector.add(item, folder: folder)
                    } else {
                        unknown += 1
                    }
                }
            }
        }
        if trashedCount > 0 {
            collector.warn(featureString("\(trashedCount) Einträge aus dem Papierkorb bleiben weg."))
        }
        if unknown > 0 {
            collector.warn(featureString("\(unknown) Einträge einer Art, die UwULock nicht kennt, bleiben weg."))
        }
    }

    /// Proton Pass's CSV: "type,name,url,email,username,password,note,totp,createTime,modifyTime,vault".
    static func readCsv(_ table: ImportCsvTable, _ collector: ImportCollector) throws {
        if !table.has("type", "name", "url", "username", "password", "note") { throw notProton() }
        let vaults = Set(table.rows.map { table.get($0, "vault") })
        let known = ["type", "name", "url", "email", "username", "password", "note", "totp", "vault", "createTime", "modifyTime"]
        for row in table.rows {
            let item = collector.login(table.get(row, "name"))
            let type = table.get(row, "type")
            collector.appendNote(item, table.get(row, "note"))
            if type == "note" { collector.retype(item, .note) }
            let vault = table.get(row, "vault")
            let folder = vaults.count > 1 && !vault.isEmpty ? vault : nil
            if type == "wifi" {
                // The CSV has the network's name only as the item's name.
                collector.wifi(item, ImportWifiEntry(ssid: table.get(row, "name"), password: table.get(row, "password")))
                for (name, value) in table.rest(row, known: known) { collector.extra(item, name, value) }
                collector.add(item, folder: folder)
                continue
            }
            let email = table.get(row, "email")
            item.login?.username = importSome(table.get(row, "username")) ?? importSome(email)
            if item.login?.username != email && !importBlank(email) { collector.field(item, "email", email) }
            item.login?.password = importSome(table.get(row, "password"))
            item.login?.totp = importSome(table.get(row, "totp"))
            let urls = table.get(row, "url").components(separatedBy: ",")
                .enumerated().map { $0.offset == 0 ? $0.element : String($0.element.drop(while: \.isWhitespace)) }
            collector.uris(item, urls.map { $0 as String? })
            for (name, value) in table.rest(row, known: known) { collector.extra(item, name, value) }
            collector.add(item, folder: folder)
        }
    }
}
