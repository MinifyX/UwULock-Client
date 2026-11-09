import Foundation

// What every importer makes: Bitwarden's unencrypted JSON export, which the core reads, seals
// and sends (`import_vault`). Port of apps/desktop/src/lib/import/types.ts.

/// The app an import comes from.
enum ImportSource: String, CaseIterable, Identifiable, Sendable {
    case bitwarden
    case keepass
    case onePassword = "1password"
    case chrome
    case firefox
    case apple
    case protonpass
    case lastpass
    /// Handed over by the system (Apple's Credential Exchange), not read from a file.
    case credentialExchange = "credential-exchange"

    var id: String { rawValue }

    /// The apps to choose from, in the order the menu lists them (index.ts, SOURCES).
    static let menu: [ImportSource] = [
        .bitwarden, .keepass, .onePassword, .chrome, .firefox, .apple, .protonpass, .lastpass,
    ]

    /// What a source is called, for the menu and the preview (index.ts, sourceLabel).
    var label: String {
        switch self {
        case .bitwarden: "Bitwarden, Vaultwarden, UwULock"
        case .keepass: "KeePass, KeePassXC"
        case .onePassword: "1Password"
        case .chrome: featureString("Chrome, Edge und andere Chromium-Browser")
        case .firefox: "Firefox"
        case .apple: featureString("Apple Passwörter (Safari)")
        case .protonpass: "Proton Pass"
        case .lastpass: "LastPass"
        case .credentialExchange: featureString("Andere App (Credential Exchange)")
        }
    }
}

enum ImportItemType: Int, Sendable {
    case login = 1, note = 2, card = 3, identity = 4, sshKey = 5
}

enum ImportFieldType: Int, Sendable {
    case text = 0, hidden = 1, boolean = 2
}

struct ImportExportField: Sendable, Equatable {
    var name: String
    var value: String
    var type: ImportFieldType
}

struct ImportExportUri: Sendable, Equatable {
    var uri: String
    var match: Int?
}

/// A passkey as Bitwarden's JSON export has it, every value plain.
struct ImportExportPasskey: Sendable, Equatable {
    var credentialId: String
    var keyValue: String
    var rpId: String
    var rpName: String?
    var userHandle: String?
    var userName: String?
    var userDisplayName: String?
    var counter: String = "0"
    var discoverable: String = "true"
    var creationDate: String
}

struct ImportExportLogin: Sendable {
    var username: String?
    var password: String?
    var totp: String?
    var uris: [ImportExportUri] = []
    var fido2Credentials: [ImportExportPasskey]?
}

struct ImportExportCard: Sendable {
    var cardholderName: String?
    var brand: String?
    var number: String?
    var expMonth: String?
    var expYear: String?
    var code: String?
}

struct ImportExportIdentity: Sendable {
    var title: String?
    var firstName: String?
    var middleName: String?
    var lastName: String?
    var address1: String?
    var address2: String?
    var address3: String?
    var city: String?
    var state: String?
    var postalCode: String?
    var country: String?
    var company: String?
    var email: String?
    var phone: String?
    var ssn: String?
    var username: String?
    var passportNumber: String?
    var licenseNumber: String?
}

struct ImportExportSshKey: Sendable {
    var privateKey: String
    var publicKey: String
    var keyFingerprint: String
}

struct ImportPasswordHistory: Sendable {
    var password: String
    var lastUsedDate: String
}

/// One item of the export. A class: the collector keeps a reference and changes it later.
final class ImportExportItem: @unchecked Sendable, Identifiable {
    var type: ImportItemType
    var name: String
    var notes: String?
    var favorite = false
    var reprompt = 0
    var folderId: String?
    var fields: [ImportExportField] = []
    var login: ImportExportLogin?
    var secureNote = false
    var card: ImportExportCard?
    var identity: ImportExportIdentity?
    var sshKey: ImportExportSshKey?
    var passwordHistory: [ImportPasswordHistory]?

    init(type: ImportItemType, name: String) {
        self.type = type
        self.name = name
    }

    var id: ObjectIdentifier { ObjectIdentifier(self) }
}

struct ImportExportFolder: Sendable, Equatable {
    var id: String
    var name: String
}

struct ImportExportData: Sendable {
    var folders: [ImportExportFolder]
    var items: [ImportExportItem]
}

/// An import, read and ready: what the preview shows, and what goes to the vault.
struct ImportParsed: Sendable {
    var source: ImportSource
    /// The file's format, for the preview: "KDBX 4.1", "CSV", …
    var format: String
    var data: ImportExportData
    /// Things the user should know before importing, in their language.
    var warnings: [String]
    /// What `import_vault` gets.
    var submitFormat: ImportFormat
    var submitText: String
}

/// Something the user can act on, in their language: shown as it is.
struct ImportProblem: Error, Sendable, LocalizedError {
    var message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}

/// How a KeePass file is opened.
struct ImportCredentials: Sendable {
    var password: String
    var keyFile: [UInt8]?
}

/// KeePass's slow key derivations, done by the core.
struct ImportKdf: Sendable {
    var argon2: @Sendable (_ id: Bool, _ version: Int, _ key: [UInt8], _ salt: [UInt8],
                           _ memoryKiB: Int, _ iterations: Int, _ lanes: Int) async throws -> [UInt8]
    var aesKdf: @Sendable (_ key: [UInt8], _ seed: [UInt8], _ rounds: UInt64) async throws -> [UInt8]

    @MainActor static func core(_ core: Core) -> ImportKdf {
        ImportKdf(
            argon2: { id, version, key, salt, memory, iterations, lanes in
                try await core.importKdbxArgon2(
                    id: id, version: version, key: key, salt: salt, memoryKib: memory,
                    iterations: iterations, lanes: lanes)
            },
            aesKdf: { key, seed, rounds in
                try await core.importKdbxAesKdf(key: key, seed: seed, rounds: rounds)
            })
    }
}

// MARK: - Limits (limits.ts)

enum ImportLimits {
    static let maxFileBytes = 256 * 1024 * 1024
    static let maxUnpackedBytes = 512 * 1024 * 1024
    static let maxZipEntries = 50_000
    static let maxArgon2MemoryKiB = 1024 * 1024
    static let maxArgon2CostKiB = maxArgon2MemoryKiB * 10
    static let maxArgon2Lanes = 256
    static let maxAesKdfRounds: UInt64 = 100_000_000

    static func tooLarge() -> ImportProblem {
        ImportProblem(featureString(
            "Die Datei ist zu groß für den Import (mehr als \(maxFileBytes / 1024 / 1024) MiB)."))
    }

    static func unpacksTooMuch() -> ImportProblem {
        ImportProblem(featureString(
            "Die Datei entpackt sich zu mehr als \(maxUnpackedBytes / 1024 / 1024) MiB, das importiert UwULock nicht."))
    }

    /// Throws when a file of `size` bytes is more than an import reads.
    static func checkFileSize(_ size: Int) throws {
        if size > maxFileBytes { throw tooLarge() }
    }
}

// MARK: - Export JSON

extension ImportExportData {
    /// Bitwarden's unencrypted JSON export, as `import_vault` reads it.
    func jsonText() -> String {
        let out = ImportJSONWriter()
        out.object {
            out.key("encrypted"); out.bool(false)
            out.key("folders")
            out.array(folders) { folder in
                out.object {
                    out.key("id"); out.string(folder.id)
                    out.key("name"); out.string(folder.name)
                }
            }
            out.key("items")
            out.array(items) { item in item.write(out) }
        }
        return out.text
    }
}

private extension ImportExportItem {
    func write(_ out: ImportJSONWriter) {
        out.object {
            out.key("type"); out.int(type.rawValue)
            out.key("name"); out.string(name)
            out.key("notes"); out.string(notes)
            out.key("favorite"); out.bool(favorite)
            out.key("reprompt"); out.int(reprompt)
            out.key("folderId"); out.string(folderId)
            out.key("fields")
            out.array(fields) { field in
                out.object {
                    out.key("name"); out.string(field.name)
                    out.key("value"); out.string(field.value)
                    out.key("type"); out.int(field.type.rawValue)
                }
            }
            if let login {
                out.key("login")
                out.object {
                    out.key("username"); out.string(login.username)
                    out.key("password"); out.string(login.password)
                    out.key("totp"); out.string(login.totp)
                    out.key("uris")
                    out.array(login.uris) { uri in
                        out.object {
                            out.key("uri"); out.string(uri.uri)
                            out.key("match"); out.int(uri.match)
                        }
                    }
                    if let passkeys = login.fido2Credentials {
                        out.key("fido2Credentials")
                        out.array(passkeys) { key in
                            out.object {
                                out.key("credentialId"); out.string(key.credentialId)
                                out.key("keyType"); out.string("public-key")
                                out.key("keyAlgorithm"); out.string("ECDSA")
                                out.key("keyCurve"); out.string("P-256")
                                out.key("keyValue"); out.string(key.keyValue)
                                out.key("rpId"); out.string(key.rpId)
                                out.key("rpName"); out.string(key.rpName)
                                out.key("userHandle"); out.string(key.userHandle)
                                out.key("userName"); out.string(key.userName)
                                out.key("userDisplayName"); out.string(key.userDisplayName)
                                out.key("counter"); out.string(key.counter)
                                out.key("discoverable"); out.string(key.discoverable)
                                out.key("creationDate"); out.string(key.creationDate)
                            }
                        }
                    }
                }
            }
            if secureNote {
                out.key("secureNote"); out.object { out.key("type"); out.int(0) }
            }
            if let card {
                out.key("card")
                out.object {
                    out.key("cardholderName"); out.string(card.cardholderName)
                    out.key("brand"); out.string(card.brand)
                    out.key("number"); out.string(card.number)
                    out.key("expMonth"); out.string(card.expMonth)
                    out.key("expYear"); out.string(card.expYear)
                    out.key("code"); out.string(card.code)
                }
            }
            if let identity {
                out.key("identity")
                out.object {
                    let values: [(String, String?)] = [
                        ("title", identity.title), ("firstName", identity.firstName),
                        ("middleName", identity.middleName), ("lastName", identity.lastName),
                        ("address1", identity.address1), ("address2", identity.address2),
                        ("address3", identity.address3), ("city", identity.city),
                        ("state", identity.state), ("postalCode", identity.postalCode),
                        ("country", identity.country), ("company", identity.company),
                        ("email", identity.email), ("phone", identity.phone), ("ssn", identity.ssn),
                        ("username", identity.username), ("passportNumber", identity.passportNumber),
                        ("licenseNumber", identity.licenseNumber),
                    ]
                    for (key, value) in values {
                        out.key(key); out.string(value)
                    }
                }
            }
            if let sshKey {
                out.key("sshKey")
                out.object {
                    out.key("privateKey"); out.string(sshKey.privateKey)
                    out.key("publicKey"); out.string(sshKey.publicKey)
                    out.key("keyFingerprint"); out.string(sshKey.keyFingerprint)
                }
            }
            if let passwordHistory {
                out.key("passwordHistory")
                out.array(passwordHistory) { entry in
                    out.object {
                        out.key("password"); out.string(entry.password)
                        out.key("lastUsedDate"); out.string(entry.lastUsedDate)
                    }
                }
            }
        }
    }
}

/// A small JSON writer that keeps the order of keys.
final class ImportJSONWriter {
    private(set) var text = ""
    private var needsComma = [false]
    private var pendingKey = false

    private func prefix() {
        if pendingKey {
            pendingKey = false
        } else if needsComma[needsComma.count - 1] {
            text += ","
        }
        needsComma[needsComma.count - 1] = true
    }

    func object(_ body: () -> Void) {
        prefix(); text += "{"; needsComma.append(false)
        body()
        needsComma.removeLast(); text += "}"
    }

    func array<T>(_ values: [T], _ each: (T) -> Void) {
        prefix(); text += "["; needsComma.append(false)
        for value in values { each(value) }
        needsComma.removeLast(); text += "]"
    }

    func key(_ name: String) {
        prefix(); text += Self.quote(name) + ":"
        pendingKey = true
    }

    private func value(_ raw: String) {
        prefix(); text += raw
    }

    func string(_ value: String?) {
        self.value(value.map(Self.quote) ?? "null")
    }

    func int(_ value: Int?) { self.value(value.map(String.init) ?? "null") }
    func bool(_ value: Bool) { self.value(value ? "true" : "false") }

    static func quote(_ string: String) -> String {
        var out = "\""
        out.reserveCapacity(string.utf8.count + 2)
        for scalar in string.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            default:
                if scalar.value < 0x20 || scalar.value == 0x2028 || scalar.value == 0x2029 {
                    out += String(format: "\\u%04x", scalar.value)
                } else {
                    out.unicodeScalars.append(scalar)
                }
            }
        }
        return out + "\""
    }
}
