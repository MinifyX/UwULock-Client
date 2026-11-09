import Foundation

// Which app a file comes from (detect.ts), and reading it into Bitwarden's JSON export
// (index.ts): everything happens on this device, the core only sees the result.

enum ImportFileKind {
    case kdbx, zip, pgp, json, xml, csv
}

enum ImportDetect {
    static func kind(_ bytes: [UInt8]) -> ImportFileKind {
        if ImportKdbx.isKdbx(bytes) { return .kdbx }
        if ImportZip.isZip(bytes) { return .zip }
        let start = String(ImportBytes.text(bytes.prefix(64)).drop(while: \.isWhitespace))
        if start.hasPrefix("-----BEGIN PGP") { return .pgp }
        if start.hasPrefix("{") || start.hasPrefix("[") { return .json }
        if start.hasPrefix("<") { return .xml }
        return .csv
    }

    /// The CSV header's column names, lower case.
    static func csvColumns(_ content: String) -> Set<String> {
        let lines = content.replacingOccurrences(of: "\r\n", with: "\n")
            .split(separator: "\n", maxSplits: 5, omittingEmptySubsequences: false).prefix(5)
        let header = ImportCSV.parse(lines.joined(separator: "\n")).first ?? []
        return Set(header.map { $0.importTrimmed.lowercased() })
    }

    static func source(csv columns: Set<String>) -> ImportSource? {
        func has(_ names: String...) -> Bool { names.allSatisfy(columns.contains) }
        if has("login_uri") || has("folder", "favorite", "type", "name", "reprompt") { return .bitwarden }
        if has("grouping", "extra") || has("profilename") { return .lastpass }
        if has("httprealm") || has("formactionorigin") || has("guid", "timecreated") { return .firefox }
        if has("type", "name", "email", "totp", "vault") { return .protonpass }
        if has("title", "otpauth") && (has("favorite") || has("archived") || has("tags")) { return .onePassword }
        if has("title", "url", "username", "password", "otpauth") { return .apple }
        if has("group", "title", "username", "password") || has("account", "login name") { return .keepass }
        if has("name", "url", "username", "password") { return .chrome }
        if has("title") { return .onePassword }
        return nil
    }

    static func source(json data: ImportJSON) -> ImportSource? {
        guard data.isObject else { return nil }
        if data["accounts"] != nil { return .onePassword }
        if data["vaults"] != nil { return .protonpass }
        if data["items"] != nil || data["encrypted"] != nil { return .bitwarden }
        return nil
    }

    static func source(zip: ImportZip) -> ImportSource? {
        if zip.names.contains("export.data") { return .onePassword }
        if zip.names.contains(where: { $0.importMatches(#"(^|/)data\.(json|pgp)$"#) || $0.hasSuffix(".pgp") }) {
            return .protonpass
        }
        return nil
    }

    /// The app a file most likely comes from; nil when nothing fits.
    static func detect(_ bytes: [UInt8]) -> ImportSource? {
        switch kind(bytes) {
        case .kdbx: return .keepass
        case .pgp: return .protonpass
        case .xml: return ImportBytes.text(bytes.prefix(4096)).importMatches(#"<KeePassFile[\s>]"#) ? .keepass : nil
        case .zip: return (try? ImportZip(bytes)).flatMap { source(zip: $0) }
        case .json: return (try? ImportBytes.parseJson(bytes)).flatMap { source(json: $0) }
        case .csv: return source(csv: csvColumns(ImportBytes.text(bytes)))
        }
    }
}

/// A file the user picked: its name and what is in it.
struct ImportFile: Sendable {
    var name: String
    var bytes: [UInt8]
}

enum ImportReading {
    /// Whether the file is a KeePass database, which needs its password (and key file).
    static func needsPassword(_ bytes: [UInt8]) -> Bool {
        ImportDetect.kind(bytes) == .kdbx
    }

    /// Whether the file is Bitwarden's JSON export with a password of its own.
    static func isProtectedExport(_ bytes: [UInt8]) -> Bool {
        guard ImportDetect.kind(bytes) == .json, let data = try? ImportBytes.parseJson(bytes) else { return false }
        return ImportBitwarden.isProtected(data)
    }

    /// Reads `file` as an export of `source` (nil: whatever it looks like). The error is always
    /// one for the user; never the raw error of something that broke on the file's content.
    static func read(_ file: ImportFile, source: ImportSource?, credentials: ImportCredentials? = nil,
                     kdf: ImportKdf? = nil) async throws -> ImportParsed {
        do {
            return try await readFile(file, source: source, credentials: credentials, kdf: kdf)
        } catch let problem as ImportProblem {
            throw problem
        } catch let error where error is CancellationError {
            throw error
        } catch let error where isCoreError(error) {
            throw error
        } catch {
            throw ImportProblem(featureString(
                "Die Datei ließ sich nicht lesen: Sie ist beschädigt oder anders aufgebaut als erwartet."))
        }
    }

    /// The core's failures (from a key derivation) are worded by `importErrorText`.
    private static func isCoreError(_ error: Error) -> Bool {
        error is CoreFailure || Core.failure(error).kind != "failed"
    }

    private static func readFile(_ file: ImportFile, source: ImportSource?, credentials: ImportCredentials?,
                                 kdf: ImportKdf?) async throws -> ImportParsed {
        try ImportLimits.checkFileSize(file.bytes.count)
        guard let chosen = source ?? ImportDetect.detect(file.bytes) else {
            throw ImportProblem(featureString(
                "UwULock erkennt nicht, aus welcher App diese Datei kommt. Wähl die App bitte aus."))
        }
        let kind = ImportDetect.kind(file.bytes)
        let collector = ImportCollector()
        func wrong() -> ImportProblem {
            let app = chosen.label
            return ImportProblem(featureString("Diese Datei ist kein Export, den UwULock von \(app) kennt."))
        }
        func done(_ format: String, submit: (ImportFormat, String)? = nil) -> ImportParsed {
            let (data, warnings) = collector.result()
            let (submitFormat, submitText) = submit ?? (.json, data.jsonText())
            return ImportParsed(source: chosen, format: format, data: data, warnings: warnings,
                                submitFormat: submitFormat, submitText: submitText)
        }
        func csv(_ read: (ImportCsvTable, ImportCollector) throws -> Void) throws -> ImportParsed {
            guard kind == .csv else { throw wrong() }
            try read(ImportCsvTable(ImportBytes.text(file.bytes)), collector)
            return done("CSV")
        }

        switch chosen {
        case .bitwarden:
            if kind == .json {
                let data = try ImportBitwarden.readJson(try ImportBytes.parseJson(file.bytes))
                return ImportParsed(source: chosen, format: "JSON", data: data, warnings: [],
                                    submitFormat: .json, submitText: ImportBytes.text(file.bytes))
            }
            guard kind == .csv else { throw wrong() }
            let text = ImportBytes.text(file.bytes)
            try ImportBitwarden.readCsv(ImportCsvTable(text), collector)
            return done("CSV", submit: (.csv, text))
        case .keepass:
            if kind == .kdbx {
                guard let credentials, let kdf else {
                    throw ImportProblem(featureString("Für diese Datei braucht es ihr Passwort."))
                }
                let (doc, version) = try await ImportKdbx.open(file.bytes, credentials: credentials, kdf: kdf)
                try ImportKeePass.readXml(doc, collector)
                return done(version)
            }
            if kind == .xml {
                guard let doc = ImportXML.parse(ImportBytes.text(file.bytes)) else { throw wrong() }
                try ImportKeePass.readXml(doc, collector)
                return done("XML")
            }
            return try csv(ImportKeePass.readCsv)
        case .onePassword:
            if kind == .zip {
                try ImportOnePassword.read1pux(try ImportZip(file.bytes), collector)
                return done("1PUX")
            }
            if kind == .json {
                // export.data, taken out of the .1pux by hand.
                try ImportOnePassword.read1pux(ImportSingleFile(name: "export.data", bytes: file.bytes), collector)
                return done("1PUX")
            }
            return try csv(ImportOnePassword.readCsv)
        case .protonpass:
            if kind == .pgp { throw ImportProton.encrypted() }
            if kind == .zip {
                let zip = try ImportZip(file.bytes)
                if ImportDetect.source(zip: zip) != .protonpass { throw wrong() }
                if zip.names.contains(where: { $0.hasSuffix(".pgp") }) { throw ImportProton.encrypted() }
                guard let name = zip.names.first(where: { $0.importMatches(#"(^|/)data\.json$"#) }) else { throw wrong() }
                try ImportProton.readJson(try ImportBytes.parseJson(try zip.read(name)), collector)
                let files = zip.names.filter { $0.importMatches(#"(^|/)files/."#) }.count
                if files > 0 { collector.attachment(files) }
                return done("ZIP")
            }
            if kind == .json {
                try ImportProton.readJson(try ImportBytes.parseJson(file.bytes), collector)
                return done("JSON")
            }
            return try csv(ImportProton.readCsv)
        case .chrome: return try csv(ImportBrowsers.readChrome)
        case .firefox: return try csv(ImportBrowsers.readFirefox)
        case .apple: return try csv(ImportBrowsers.readApple)
        case .lastpass: return try csv(ImportLastPass.read)
        case .credentialExchange:
            // Not a file: the system hands these over.
            throw wrong()
        }
    }
}

// MARK: - Preview summary (index.ts `summarize`)

struct ImportPreviewItem: Identifiable, Sendable {
    let id: Int
    let name: String
    let type: ImportItemType
    /// The user name, the card holder, the person: whatever tells two items apart.
    let detail: String
    let folder: String?
    let totp: Bool
    let wifi: Bool
}

struct ImportSummary: Sendable {
    var logins = 0, notes = 0, cards = 0, identities = 0, sshKeys = 0, wifi = 0
    var passkeys = 0
    var folders: [String] = []
    var items: [ImportPreviewItem] = []

    init(_ parsed: ImportParsed) {
        let names = Dictionary(parsed.data.folders.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
        items = parsed.data.items.enumerated().map { index, item in
            let person = [item.identity?.firstName, item.identity?.lastName].compactMap { $0 }
                .filter { !$0.isEmpty }.joined(separator: " ")
            let detail = item.login?.username ?? item.card?.cardholderName ?? person
            return ImportPreviewItem(
                id: index, name: item.name, type: item.type, detail: detail,
                folder: item.folderId.flatMap { names[$0] }.flatMap { $0.isEmpty ? nil : $0 },
                totp: !(item.login?.totp ?? "").isEmpty,
                wifi: item.type == .note && ImportWifi.hasMarker(item))
        }
        for item in items {
            if item.wifi { wifi += 1; continue }
            switch item.type {
            case .login: logins += 1
            case .note: notes += 1
            case .card: cards += 1
            case .identity: identities += 1
            case .sshKey: sshKeys += 1
            }
        }
        passkeys = parsed.data.items.reduce(0) { $0 + ($1.login?.fido2Credentials?.count ?? 0) }
        folders = parsed.data.folders.map(\.name)
    }
}
