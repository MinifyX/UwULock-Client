import Foundation

// Opens KeePass files (KDBX 4.0, 4.1 and 3.1) and gives their XML (kdbx.ts), with the stream
// ciphers of protected values and payloads (stream.ts) and a small XML tree (DOMParser's part).

// MARK: - Stream ciphers

/// ChaCha20 (RFC 8439) and Salsa20; both keep their place in the key stream between calls.
final class ImportStreamCipher {
    private enum Kind { case chacha, salsa }
    private let kind: Kind
    private var state: [UInt32]
    private var block = [UInt8](repeating: 0, count: 64)
    private var used = 64

    private static let sigma: [UInt32] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]

    private static func words(_ bytes: [UInt8], _ count: Int) -> [UInt32] {
        (0..<count).map { i in
            UInt32(bytes[i * 4]) | UInt32(bytes[i * 4 + 1]) << 8 | UInt32(bytes[i * 4 + 2]) << 16
                | UInt32(bytes[i * 4 + 3]) << 24
        }
    }

    /// A 32-byte key and a 12-byte nonce; the block counter starts at `counter`.
    static func chacha20(key: [UInt8], nonce: [UInt8], counter: UInt32 = 0) -> ImportStreamCipher? {
        guard key.count == 32, nonce.count == 12 else { return nil }
        return ImportStreamCipher(kind: .chacha, state: sigma + words(key, 8) + [counter] + words(nonce, 3))
    }

    /// A 32-byte key and an 8-byte nonce; the block counter starts at 0.
    static func salsa20(key: [UInt8], nonce: [UInt8]) -> ImportStreamCipher? {
        guard key.count == 32, nonce.count == 8 else { return nil }
        let k = words(key, 8)
        let n = words(nonce, 2)
        let state = [sigma[0]] + k[0..<4] + [sigma[1]] + n + [0, 0, sigma[2]] + k[4...] + [sigma[3]]
        return ImportStreamCipher(kind: .salsa, state: state)
    }

    private init(kind: Kind, state: [UInt32]) {
        self.kind = kind
        self.state = state
    }

    @inline(__always) private static func rotl(_ x: UInt32, _ n: UInt32) -> UInt32 {
        (x << n) | (x >> (32 - n))
    }

    private func next() {
        var x = state
        switch kind {
        case .chacha:
            func quarter(_ a: Int, _ b: Int, _ c: Int, _ d: Int) {
                x[a] &+= x[b]; x[d] = Self.rotl(x[d] ^ x[a], 16)
                x[c] &+= x[d]; x[b] = Self.rotl(x[b] ^ x[c], 12)
                x[a] &+= x[b]; x[d] = Self.rotl(x[d] ^ x[a], 8)
                x[c] &+= x[d]; x[b] = Self.rotl(x[b] ^ x[c], 7)
            }
            for _ in 0..<10 {
                quarter(0, 4, 8, 12); quarter(1, 5, 9, 13); quarter(2, 6, 10, 14); quarter(3, 7, 11, 15)
                quarter(0, 5, 10, 15); quarter(1, 6, 11, 12); quarter(2, 7, 8, 13); quarter(3, 4, 9, 14)
            }
        case .salsa:
            func quarter(_ a: Int, _ b: Int, _ c: Int, _ d: Int) {
                x[b] ^= Self.rotl(x[a] &+ x[d], 7)
                x[c] ^= Self.rotl(x[b] &+ x[a], 9)
                x[d] ^= Self.rotl(x[c] &+ x[b], 13)
                x[a] ^= Self.rotl(x[d] &+ x[c], 18)
            }
            for _ in 0..<10 {
                quarter(0, 4, 8, 12); quarter(5, 9, 13, 1); quarter(10, 14, 2, 6); quarter(15, 3, 7, 11)
                quarter(0, 1, 2, 3); quarter(5, 6, 7, 4); quarter(10, 11, 8, 9); quarter(15, 12, 13, 14)
            }
        }
        for i in 0..<16 {
            let word = x[i] &+ state[i]
            block[i * 4] = UInt8(truncatingIfNeeded: word)
            block[i * 4 + 1] = UInt8(truncatingIfNeeded: word >> 8)
            block[i * 4 + 2] = UInt8(truncatingIfNeeded: word >> 16)
            block[i * 4 + 3] = UInt8(truncatingIfNeeded: word >> 24)
        }
        switch kind {
        case .chacha: state[12] &+= 1
        case .salsa:
            state[8] &+= 1
            if state[8] == 0 { state[9] &+= 1 }
        }
        used = 0
    }

    /// `data` XOR the next bytes of the key stream.
    func process(_ data: [UInt8]) -> [UInt8] {
        var out = data
        for i in out.indices {
            if used == 64 { next() }
            out[i] ^= block[used]
            used += 1
        }
        return out
    }
}

// MARK: - XML

/// An element of a parsed XML document.
final class ImportXMLElement {
    let name: String
    let attributes: [String: String]
    private(set) var children: [ImportXMLElement] = []
    weak var parent: ImportXMLElement?
    /// The element's text and its children's, in order (`textContent`).
    private var parts: [Part] = []

    private enum Part {
        case text(String)
        case child(ImportXMLElement)
    }

    init(name: String, attributes: [String: String]) {
        self.name = name
        self.attributes = attributes
    }

    fileprivate func append(_ child: ImportXMLElement) {
        child.parent = self
        children.append(child)
        parts.append(.child(child))
    }

    fileprivate func append(text: String) {
        if case .text(let last) = parts.last {
            parts[parts.count - 1] = .text(last + text)
        } else {
            parts.append(.text(text))
        }
    }

    var textContent: String {
        get {
            parts.map { part in
                switch part {
                case .text(let text): text
                case .child(let child): child.textContent
                }
            }.joined()
        }
        set {
            children = []
            parts = [.text(newValue)]
        }
    }

    func attribute(_ name: String) -> String? { attributes[name] }

    func elements(_ name: String) -> [ImportXMLElement] { children.filter { $0.name == name } }
    func element(_ name: String) -> ImportXMLElement? { children.first { $0.name == name } }
    func text(_ name: String) -> String { element(name)?.textContent ?? "" }

    /// Every element below, in document order.
    func descendants() -> [ImportXMLElement] {
        var out: [ImportXMLElement] = []
        func walk(_ element: ImportXMLElement) {
            for child in element.children {
                out.append(child)
                walk(child)
            }
        }
        walk(self)
        return out
    }
}

enum ImportXML {
    /// XML without a DOCTYPE, parsed; nil when it doesn't parse. The prolog is checked before
    /// the parser sees the text, so no entity is ever expanded.
    static func parse(_ xml: String) -> ImportXMLElement? {
        var at = xml.startIndex
        while true {
            guard let open = xml[at...].firstIndex(of: "<") else { return nil }
            let rest = xml[open...]
            let close: String? = rest.hasPrefix("<?") ? "?>" : rest.hasPrefix("<!--") ? "-->" : nil
            guard let close else {
                if rest.hasPrefix("<!") { return nil }
                break
            }
            let from = xml.index(open, offsetBy: 2, limitedBy: xml.endIndex) ?? xml.endIndex
            guard let end = xml[from...].range(of: close) else { return nil }
            at = end.upperBound
        }
        let builder = ImportXMLBuilder()
        let parser = XMLParser(data: Data(xml.utf8))
        parser.shouldResolveExternalEntities = false
        parser.shouldProcessNamespaces = false
        parser.delegate = builder
        guard parser.parse(), !builder.failed, let root = builder.root else { return nil }
        return root
    }
}

private final class ImportXMLBuilder: NSObject, XMLParserDelegate {
    var root: ImportXMLElement?
    var stack: [ImportXMLElement] = []
    var failed = false

    func parser(_ parser: XMLParser, didStartElement elementName: String, namespaceURI: String?,
                qualifiedName qName: String?, attributes attributeDict: [String: String] = [:]) {
        let element = ImportXMLElement(name: elementName, attributes: attributeDict)
        if let parent = stack.last {
            parent.append(element)
        } else if root == nil {
            root = element
        } else {
            failed = true
        }
        stack.append(element)
    }

    func parser(_ parser: XMLParser, didEndElement elementName: String, namespaceURI: String?,
                qualifiedName qName: String?) {
        _ = stack.popLast()
    }

    func parser(_ parser: XMLParser, foundCharacters string: String) {
        stack.last?.append(text: string)
    }

    func parser(_ parser: XMLParser, foundCDATA CDATABlock: Data) {
        stack.last?.append(text: String(decoding: CDATABlock, as: UTF8.self))
    }

    func parser(_ parser: XMLParser, foundInternalSubsetWithName name: String, publicID: String?, systemID: String?) {
        failed = true
        parser.abortParsing()
    }

    func parser(_ parser: XMLParser, parseErrorOccurred parseError: Error) {
        failed = true
    }
}

// MARK: - KDBX

enum ImportKdbx {
    private static let signature1: UInt32 = 0x9AA2_D903
    private static let signature2: UInt32 = 0xB54B_FB67
    private static let oldSignatures: [UInt32] = [0xB54B_FB65, 0xB54B_FB66]

    private static let cipherAes = "31c1f2e6bf714350be5805216afc5aff"
    private static let cipherChaCha = "d6038a2b8b6f4cb5a524339a31dbb59a"
    private static let cipherTwofish = "ad68f29f576f4bb9a36ad47af965346c"
    private static let kdfAes = ["c9d9f39a628a4460bf740d08c18a4fea", "7c02bb8279a74ac0927d114a00648238"]
    private static let kdfArgon2d = "ef636ddf8c29444b91f7a9a403e30a0c"
    private static let kdfArgon2id = "9e298b1956db4773b23dfc3ec6f0a1e6"
    private static let salsaNonce: [UInt8] = [0xE8, 0x30, 0x09, 0x4B, 0x97, 0x20, 0x5D, 0x2A]

    static func damaged() -> ImportProblem {
        ImportProblem(featureString("Die KeePass-Datei ist beschädigt oder unvollständig."))
    }

    static func wrongKey() -> ImportProblem {
        ImportProblem(featureString("Das Passwort oder die Schlüsseldatei stimmt nicht."))
    }

    static func tooCostly() -> ImportProblem {
        ImportProblem(featureString(
            "Die Schlüsselableitung dieser KeePass-Datei ist aufwendiger, als UwULock beim Import zulässt. Stell sie in KeePassXC oder KeePass bei den Datenbank-Einstellungen niedriger, speichere und importiere die Datei noch einmal."))
    }

    private static func unknownCipher() -> ImportProblem {
        ImportProblem(featureString("Diese KeePass-Datei nutzt eine Verschlüsselung, die UwULock nicht kennt."))
    }

    static func isKdbx(_ bytes: [UInt8]) -> Bool {
        guard bytes.count >= 12 else { return false }
        let value = UInt32(bytes[0]) | UInt32(bytes[1]) << 8 | UInt32(bytes[2]) << 16 | UInt32(bytes[3]) << 24
        return value == signature1
    }

    /// A key file's 32 bytes: KeePass's XML key file (1.0 base64, 2.0 hex with a check hash),
    /// 32 bytes as they are, 64 hex digits, or else the SHA-256 of the file.
    static func keyFileKey(_ bytes: [UInt8]) throws -> [UInt8] {
        var text: String? = String(validating: bytes, as: UTF8.self)
        if let value = text, value.hasPrefix("\u{FEFF}") { text = String(value.dropFirst()) }
        if let text, text.drop(while: \.isWhitespace).hasPrefix("<") {
            let doc = ImportXML.parse(text)
            let file = doc?.name == "KeyFile" ? doc : nil
            let version = file?.element("Meta")?.element("Version")?.textContent.importTrimmed ?? ""
            let broken = ImportProblem(featureString("Die Schlüsseldatei ist beschädigt."))
            if let data = file?.element("Key")?.element("Data") {
                if version.hasPrefix("1.") {
                    guard let key = ImportBytes.fromBase64(data.textContent), key.count == 32 else { throw broken }
                    return key
                }
                if version.hasPrefix("2.") {
                    guard let key = ImportBytes.fromHex(data.textContent), key.count == 32 else { throw broken }
                    if let hash = data.attribute("Hash") {
                        guard let check = ImportBytes.fromHex(hash),
                              ImportBytes.equal(Array(ImportBytes.sha256(key).prefix(4)), check)
                        else { throw broken }
                    }
                    return key
                }
            }
        }
        if bytes.count == 32 { return bytes }
        if bytes.count == 64, let text, let key = ImportBytes.fromHex(text) { return key }
        return ImportBytes.sha256(bytes)
    }

    /// SHA-256 over the hashes of what opens the file: the password, the key file, or both.
    static func compositeKey(_ credentials: ImportCredentials) throws -> [UInt8] {
        var parts: [UInt8] = []
        if !credentials.password.isEmpty || credentials.keyFile == nil {
            parts += ImportBytes.sha256(Array(credentials.password.utf8))
        }
        if let keyFile = credentials.keyFile { parts += try keyFileKey(keyFile) }
        return ImportBytes.sha256(parts)
    }

    private enum Variant {
        case number(UInt64)
        case bool(Bool)
        case string(String)
        case bytes([UInt8])
    }

    /// KDBX 4's typed dictionary, for the key derivation's parameters.
    private static func variantDictionary(_ bytes: [UInt8]) throws -> [String: Variant] {
        var reader = ImportReader(bytes, truncated: { damaged() })
        var out: [String: Variant] = [:]
        let version = try reader.u16()
        if version >> 8 > 1 { throw damaged() }
        while true {
            let type = try reader.u8()
            if type == 0 { break }
            let name = ImportBytes.text(try reader.take(Int(try reader.u32())))
            let raw = try reader.take(Int(try reader.u32()))
            var value = ImportReader(raw, truncated: { damaged() })
            switch type {
            case 0x04, 0x0C: out[name] = .number(UInt64(try value.u32()))
            case 0x05, 0x0D: out[name] = .number(try value.u64())
            case 0x08: out[name] = .bool(try value.u8() != 0)
            case 0x18: out[name] = .string(String(decoding: raw, as: UTF8.self))
            default: out[name] = .bytes(raw)
            }
        }
        return out
    }

    private static func derive(_ params: [String: Variant], _ composite: [UInt8], _ kdf: ImportKdf) async throws -> [UInt8] {
        var id = ""
        if case .bytes(let uuid) = params["$UUID"] { id = ImportBytes.hex(uuid) }
        func bytes(_ name: String) throws -> [UInt8] {
            guard case .bytes(let value) = params[name] else { throw damaged() }
            return value
        }
        func number(_ name: String) throws -> UInt64 {
            guard case .number(let value) = params[name] else { throw damaged() }
            return value
        }
        if kdfAes.contains(id) {
            let rounds = try number("R")
            if rounds > ImportLimits.maxAesKdfRounds { throw tooCostly() }
            return try await kdf.aesKdf(composite, try bytes("S"), rounds)
        }
        if id == kdfArgon2d || id == kdfArgon2id {
            for extra in ["K", "A"] {
                if case .bytes(let value) = params[extra], !value.isEmpty {
                    throw ImportProblem(featureString(
                        "Diese KeePass-Datei nutzt Argon2 mit Zusatzdaten, das kann UwULock nicht öffnen."))
                }
            }
            let memoryKiB = try number("M") / 1024
            let iterations = try number("I")
            let lanes = try number("P")
            let (cost, overflow) = memoryKiB.multipliedReportingOverflow(by: iterations)
            if memoryKiB > UInt64(ImportLimits.maxArgon2MemoryKiB) || overflow
                || cost > UInt64(ImportLimits.maxArgon2CostKiB) || lanes > UInt64(ImportLimits.maxArgon2Lanes) {
                throw tooCostly()
            }
            let version = try number("V")
            guard version <= UInt64(UInt32.max) else { throw damaged() }
            return try await kdf.argon2(
                id == kdfArgon2id, Int(version), composite, try bytes("S"), Int(memoryKiB),
                Int(iterations), Int(lanes))
        }
        throw ImportProblem(featureString("Diese KeePass-Datei nutzt eine Schlüsselableitung, die UwULock nicht kennt."))
    }

    private static func decrypt(_ cipher: String, key: [UInt8], iv: [UInt8], data: [UInt8]) throws -> [UInt8] {
        if cipher == cipherAes {
            // Bad padding: in KDBX 3.1 the only sign of a wrong key this early.
            guard let plain = ImportBytes.aesCbcDecrypt(key: key, iv: iv, data: data) else { throw wrongKey() }
            return plain
        }
        if cipher == cipherChaCha {
            guard let stream = ImportStreamCipher.chacha20(key: key, nonce: iv) else { throw damaged() }
            return stream.process(data)
        }
        if cipher == cipherTwofish {
            throw ImportProblem(featureString(
                "Diese KeePass-Datei ist mit Twofish verschlüsselt, das kann UwULock nicht öffnen. Stell in KeePassXC oder KeePass bei den Datenbank-Einstellungen AES-256 oder ChaCha20 ein, speichere und importiere die Datei noch einmal."))
        }
        throw unknownCipher()
    }

    /// The key stream that hides protected values inside the XML.
    private static func innerStream(_ id: UInt32, key: [UInt8]) throws -> ImportStreamCipher? {
        switch id {
        case 0: return nil
        case 2: return ImportStreamCipher.salsa20(key: ImportBytes.sha256(key), nonce: salsaNonce)
        case 3:
            let hash = ImportBytes.sha512(key)
            return ImportStreamCipher.chacha20(key: Array(hash[0..<32]), nonce: Array(hash[32..<44]))
        default: throw unknownCipher()
        }
    }

    private static func gunzip(_ data: [UInt8]) throws -> [UInt8] {
        do {
            return try ImportBytes.gunzip(data, limit: ImportLimits.maxUnpackedBytes, tooMuch: ImportLimits.unpacksTooMuch)
        } catch let problem as ImportProblem {
            throw problem
        } catch {
            throw damaged()
        }
    }

    /// Opens a KeePass file: its XML, with every protected value decrypted in place.
    static func open(_ file: [UInt8], credentials: ImportCredentials, kdf: ImportKdf) async throws
        -> (doc: ImportXMLElement, version: String)
    {
        var reader = ImportReader(file, truncated: { damaged() })
        let notKeePass = ImportProblem(featureString("Das ist keine KeePass-Datei."))
        if try reader.u32() != signature1 { throw notKeePass }
        let signature = try reader.u32()
        if oldSignatures.contains(signature) {
            throw ImportProblem(featureString(
                "Das ist eine Datei von KeePass 1 (.kdb). Öffne sie in KeePassXC oder KeePass 2, speichere sie als KDBX und importiere dann diese Datei."))
        }
        if signature != signature2 { throw notKeePass }
        let minor = try reader.u16()
        let major = try reader.u16()
        if major != 3 && major != 4 {
            let version = "\(major).\(minor)"
            throw ImportProblem(featureString(
                "Diese KeePass-Datei hat Version \(version), UwULock kennt 3.1 und 4.x. Speichere sie in KeePassXC oder KeePass noch einmal und importiere dann diese Datei."))
        }

        var fields: [UInt8: [UInt8]] = [:]
        while true {
            let id = try reader.u8()
            let size = major >= 4 ? Int(try reader.u32()) : try reader.u16()
            let data = try reader.take(size)
            if id == 0 { break }
            fields[id] = data
        }
        let header = Array(file[0..<reader.at])
        func field(_ id: UInt8) throws -> [UInt8] {
            guard let data = fields[id] else { throw damaged() }
            return data
        }
        let cipher = ImportBytes.hex(try field(2))
        var compression = ImportReader(try field(3), truncated: { damaged() })
        let compressed = try compression.u32() == 1
        let masterSeed = try field(4)
        let iv = try field(7)
        let composite = try compositeKey(credentials)

        var payload: [UInt8]
        let stream: ImportStreamCipher?
        if major == 4 {
            if !ImportBytes.equal(try reader.take(32), ImportBytes.sha256(header)) { throw damaged() }
            let headerHmac = try reader.take(32)
            let transformed = try await derive(try variantDictionary(try field(11)), composite, kdf)
            let hmacBase = ImportBytes.sha512(masterSeed, transformed, [1])
            func blockKey(_ index: UInt64) -> [UInt8] { ImportBytes.sha512(ImportBytes.u64le(index), hmacBase) }
            let expected = ImportBytes.hmacSha256(key: blockKey(UInt64.max), header)
            if !ImportBytes.equal(headerHmac, expected) { throw wrongKey() }

            var blocks: [UInt8] = []
            var index: UInt64 = 0
            while true {
                let hmac = try reader.take(32)
                let size = try reader.u32()
                let data = try reader.take(Int(size))
                let check = ImportBytes.hmacSha256(
                    key: blockKey(index), ImportBytes.u64le(index), ImportBytes.u32le(size), data)
                if !ImportBytes.equal(hmac, check) { throw damaged() }
                if size == 0 { break }
                blocks += data
                index += 1
            }
            let key = ImportBytes.sha256(masterSeed, transformed)
            var plain = try decrypt(cipher, key: key, iv: iv, data: blocks)
            if compressed { plain = try gunzip(plain) }

            var inner = ImportReader(plain, truncated: { damaged() })
            var streamId: UInt32 = 0
            var streamKey: [UInt8] = []
            while true {
                let id = try inner.u8()
                let data = try inner.take(Int(try inner.u32()))
                if id == 0 { break }
                if id == 1 {
                    var value = ImportReader(data, truncated: { damaged() })
                    streamId = try value.u32()
                }
                if id == 2 { streamKey = data }
            }
            stream = try innerStream(streamId, key: streamKey)
            payload = Array(plain[inner.at...])
        } else {
            var roundsReader = ImportReader(try field(6), truncated: { damaged() })
            let rounds = try roundsReader.u64()
            if rounds > ImportLimits.maxAesKdfRounds { throw tooCostly() }
            let transformed = try await kdf.aesKdf(composite, try field(5), rounds)
            let key = ImportBytes.sha256(masterSeed, transformed)
            let plain = try decrypt(cipher, key: key, iv: iv, data: Array(file[reader.at...]))
            let streamStart = try field(9)
            if plain.count < 32 || !ImportBytes.equal(Array(plain[0..<32]), streamStart) { throw wrongKey() }
            var blocks = ImportReader(Array(plain[32...]), truncated: { damaged() })
            var parts: [UInt8] = []
            while true {
                _ = try blocks.u32()
                let hash = try blocks.take(32)
                let data = try blocks.take(Int(try blocks.u32()))
                if data.isEmpty { break }
                if !ImportBytes.equal(hash, ImportBytes.sha256(data)) { throw damaged() }
                parts += data
            }
            payload = parts
            if compressed { payload = try gunzip(payload) }
            var idReader = ImportReader(try field(10), truncated: { damaged() })
            stream = try innerStream(try idReader.u32(), key: try field(8))
        }

        guard let doc = ImportXML.parse(ImportBytes.text(payload)), doc.name == "KeePassFile" else {
            throw damaged()
        }
        if major == 3, let stored = doc.element("Meta")?.element("HeaderHash")?.textContent.importTrimmed,
           !stored.isEmpty {
            guard let hash = ImportBytes.fromBase64(stored) else { throw damaged() }
            if !ImportBytes.equal(hash, ImportBytes.sha256(header)) { throw damaged() }
        }
        // One key stream for all protected values, in the order they stand in the file.
        for element in [doc] + doc.descendants() {
            guard element.attribute("Protected")?.lowercased() == "true" else { continue }
            guard let hidden = ImportBytes.fromBase64(element.textContent) else { throw ImportBytesError.damaged }
            let plain = stream?.process(hidden) ?? hidden
            if element.name == "Value" { element.textContent = String(decoding: plain, as: UTF8.self) }
        }
        return (doc, "KDBX \(major).\(minor)")
    }
}
