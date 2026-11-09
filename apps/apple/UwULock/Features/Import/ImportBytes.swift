import CommonCrypto
import Compression
import CryptoKit
import Foundation

// Small helpers on bytes for the importers (bytes.ts): reading numbers, hashing, AES-CBC,
// inflating (gzip for KeePass, raw deflate for zips), and a JSON reader that keeps key order.

enum ImportBytes {
    /// Text of a file: UTF-8, without a byte order mark.
    static func text<C: Collection>(_ bytes: C) -> String where C.Element == UInt8 {
        var string = String(decoding: bytes, as: UTF8.self)
        if string.hasPrefix("\u{FEFF}") { string.removeFirst() }
        return string
    }

    static func sha256(_ parts: [UInt8]...) -> [UInt8] {
        var hash = SHA256()
        for part in parts { hash.update(data: part) }
        return Array(hash.finalize())
    }

    static func sha512(_ parts: [UInt8]...) -> [UInt8] {
        var hash = SHA512()
        for part in parts { hash.update(data: part) }
        return Array(hash.finalize())
    }

    static func hmacSha256(key: [UInt8], _ parts: [UInt8]...) -> [UInt8] {
        var mac = HMAC<SHA256>(key: SymmetricKey(data: key))
        for part in parts { mac.update(data: part) }
        return Array(mac.finalize())
    }

    static func equal(_ a: [UInt8], _ b: [UInt8]) -> Bool {
        guard a.count == b.count else { return false }
        var diff: UInt8 = 0
        for i in a.indices { diff |= a[i] ^ b[i] }
        return diff == 0
    }

    /// AES-256-CBC with PKCS#7 padding; nil when it fails (a wrong key usually shows as bad padding).
    static func aesCbcDecrypt(key: [UInt8], iv: [UInt8], data: [UInt8]) -> [UInt8]? {
        guard key.count == kCCKeySizeAES256, iv.count == kCCBlockSizeAES128 else { return nil }
        var out = [UInt8](repeating: 0, count: data.count + kCCBlockSizeAES128)
        var moved = 0
        let status = out.withUnsafeMutableBytes { outBuffer in
            CCCrypt(
                CCOperation(kCCDecrypt), CCAlgorithm(kCCAlgorithmAES), CCOptions(kCCOptionPKCS7Padding),
                key, key.count, iv, data, data.count,
                outBuffer.baseAddress, outBuffer.count, &moved)
        }
        guard status == kCCSuccess else { return nil }
        return Array(out.prefix(moved))
    }

    static func hex(_ bytes: [UInt8]) -> String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }

    static func fromHex(_ text: String) -> [UInt8]? {
        let clean = text.filter { !$0.isWhitespace }
        guard clean.count % 2 == 0 else { return nil }
        var out: [UInt8] = []
        out.reserveCapacity(clean.count / 2)
        var index = clean.startIndex
        while index < clean.endIndex {
            let next = clean.index(index, offsetBy: 2)
            guard let byte = UInt8(clean[index..<next], radix: 16) else { return nil }
            out.append(byte)
            index = next
        }
        return out
    }

    /// Standard base64 (whitespace ignored); nil when it isn't.
    static func fromBase64(_ text: String) -> [UInt8]? {
        let clean = text.filter { !$0.isWhitespace }
        if let data = Data(base64Encoded: clean) { return Array(data) }
        // atob also takes base64 without its padding.
        let padded = clean + String(repeating: "=", count: (4 - clean.count % 4) % 4)
        return Data(base64Encoded: padded).map(Array.init)
    }

    /// RFC 4648 base32 without padding: how authenticator secrets are written.
    static func base32(_ bytes: [UInt8]) -> String {
        let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
        var out = ""
        var bits = 0
        var value: UInt32 = 0
        for byte in bytes {
            value = (value << 8) | UInt32(byte)
            bits += 8
            while bits >= 5 {
                out.append(alphabet[Int((value >> UInt32(bits - 5)) & 31)])
                bits -= 5
            }
        }
        if bits > 0 { out.append(alphabet[Int((value << UInt32(5 - bits)) & 31)]) }
        return out
    }

    static func u32le(_ value: UInt32) -> [UInt8] {
        withUnsafeBytes(of: value.littleEndian, Array.init)
    }

    static func u64le(_ value: UInt64) -> [UInt8] {
        withUnsafeBytes(of: value.littleEndian, Array.init)
    }

    /// JSON, or an error without the parser's message (which could quote the file).
    static func parseJson<C: Collection>(_ bytes: C) throws -> ImportJSON where C.Element == UInt8 {
        guard let value = ImportJSONParser.parse(Array(bytes)) else {
            throw ImportProblem(featureString("Die Datei ist kein gültiges JSON."))
        }
        return value
    }

    // MARK: Inflate

    /// Raw deflate (zip entries). Stops with `tooMuch` once the output passes `limit` bytes.
    static func inflateRaw(_ data: ArraySlice<UInt8>, limit: Int, tooMuch: () -> Error) throws -> [UInt8] {
        let stream = UnsafeMutablePointer<compression_stream>.allocate(capacity: 1)
        defer { stream.deallocate() }
        guard compression_stream_init(stream, COMPRESSION_STREAM_DECODE, COMPRESSION_ZLIB)
            == COMPRESSION_STATUS_OK
        else { throw ImportBytesError.damaged }
        defer { compression_stream_destroy(stream) }
        let chunk = 64 * 1024
        var out: [UInt8] = []
        var buffer = [UInt8](repeating: 0, count: chunk)
        return try data.withUnsafeBufferPointer { input in
            stream.pointee.src_ptr = input.baseAddress ?? UnsafePointer(bitPattern: 1)!
            stream.pointee.src_size = input.count
            while true {
                let status: compression_status = buffer.withUnsafeMutableBufferPointer { output in
                    stream.pointee.dst_ptr = output.baseAddress!
                    stream.pointee.dst_size = chunk
                    return compression_stream_process(stream, Int32(COMPRESSION_STREAM_FINALIZE.rawValue))
                }
                let produced = chunk - stream.pointee.dst_size
                if out.count + produced > limit { throw tooMuch() }
                out.append(contentsOf: buffer[0..<produced])
                switch status {
                case COMPRESSION_STATUS_END: return out
                case COMPRESSION_STATUS_OK:
                    if produced == 0 && stream.pointee.src_size == 0 { throw ImportBytesError.damaged }
                default: throw ImportBytesError.damaged
                }
            }
        }
    }

    /// gzip (KeePass's payload): the header by hand, then raw deflate.
    static func gunzip(_ data: [UInt8], limit: Int, tooMuch: () -> Error) throws -> [UInt8] {
        guard data.count >= 18, data[0] == 0x1f, data[1] == 0x8b, data[2] == 8 else {
            throw ImportBytesError.damaged
        }
        let flags = data[3]
        var at = 10
        if flags & 0x04 != 0 {
            guard at + 2 <= data.count else { throw ImportBytesError.damaged }
            at += 2 + Int(data[at]) + Int(data[at + 1]) << 8
        }
        if flags & 0x08 != 0 {
            while at < data.count && data[at] != 0 { at += 1 }
            at += 1
        }
        if flags & 0x10 != 0 {
            while at < data.count && data[at] != 0 { at += 1 }
            at += 1
        }
        if flags & 0x02 != 0 { at += 2 }
        guard at < data.count else { throw ImportBytesError.damaged }
        return try inflateRaw(data[at...], limit: limit, tooMuch: tooMuch)
    }
}

/// A failure inside the byte helpers: the caller words it for the user.
enum ImportBytesError: Error {
    case damaged
}

/// Little-endian reading, with a clear error when the file ends early.
struct ImportReader {
    let bytes: [UInt8]
    var at = 0
    let truncated: @Sendable () -> Error

    init(_ bytes: [UInt8], truncated: @escaping @Sendable () -> Error) {
        self.bytes = bytes
        self.truncated = truncated
    }

    var left: Int { bytes.count - at }

    private func need(_ count: Int) throws {
        if count < 0 || at + count > bytes.count { throw truncated() }
    }

    mutating func u8() throws -> UInt8 {
        try need(1)
        defer { at += 1 }
        return bytes[at]
    }

    mutating func u16() throws -> Int {
        try need(2)
        defer { at += 2 }
        return Int(bytes[at]) | Int(bytes[at + 1]) << 8
    }

    mutating func u32() throws -> UInt32 {
        try need(4)
        defer { at += 4 }
        return UInt32(bytes[at]) | UInt32(bytes[at + 1]) << 8 | UInt32(bytes[at + 2]) << 16
            | UInt32(bytes[at + 3]) << 24
    }

    mutating func u64() throws -> UInt64 {
        let low = UInt64(try u32())
        let high = UInt64(try u32())
        return high << 32 | low
    }

    mutating func take(_ count: Int) throws -> [UInt8] {
        try need(count)
        defer { at += count }
        return Array(bytes[at..<(at + count)])
    }
}

// MARK: - JSON

/// A JSON value with the order of an object's keys kept.
indirect enum ImportJSON: Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([ImportJSON])
    case object([(String, ImportJSON)])

    subscript(key: String) -> ImportJSON? {
        if case .object(let pairs) = self { return pairs.first { $0.0 == key }?.1 }
        return nil
    }

    var string: String? {
        if case .string(let value) = self { return value }
        return nil
    }

    var array: [ImportJSON]? {
        if case .array(let value) = self { return value }
        return nil
    }

    var object: [(String, ImportJSON)]? {
        if case .object(let value) = self { return value }
        return nil
    }

    var bool: Bool? {
        if case .bool(let value) = self { return value }
        return nil
    }

    var number: Double? {
        if case .number(let value) = self { return value }
        return nil
    }

    var isObject: Bool { object != nil }
    var isNull: Bool { if case .null = self { return true } else { return false } }

    /// How JavaScript's `String(value)` writes a string, number or boolean; nil for the rest.
    var scalarText: String? {
        switch self {
        case .string(let value): value
        case .number(let value): ImportJSON.numberText(value)
        case .bool(let value): value ? "true" : "false"
        default: nil
        }
    }

    /// Truthiness as JavaScript has it.
    var truthy: Bool {
        switch self {
        case .null: false
        case .bool(let value): value
        case .number(let value): value != 0 && !value.isNaN
        case .string(let value): !value.isEmpty
        case .array, .object: true
        }
    }

    static func numberText(_ value: Double) -> String {
        if value.rounded() == value && abs(value) < 1e21 { return String(Int64(value)) }
        return String(value)
    }
}

/// A strict JSON reader (RFC 8259) on UTF-8 bytes; nil when the text isn't JSON.
private struct ImportJSONParser {
    let bytes: [UInt8]
    var at = 0
    var depth = 0

    static func parse(_ input: [UInt8]) -> ImportJSON? {
        var bytes = input
        if bytes.starts(with: [0xEF, 0xBB, 0xBF]) { bytes.removeFirst(3) }
        var parser = ImportJSONParser(bytes: bytes)
        guard let value = parser.value() else { return nil }
        parser.space()
        return parser.at == bytes.count ? value : nil
    }

    mutating func space() {
        while at < bytes.count, [0x20, 0x09, 0x0A, 0x0D].contains(bytes[at]) { at += 1 }
    }

    mutating func value() -> ImportJSON? {
        space()
        guard at < bytes.count else { return nil }
        switch bytes[at] {
        case UInt8(ascii: "{"): return object()
        case UInt8(ascii: "["): return array()
        case UInt8(ascii: "\""): return string().map(ImportJSON.string)
        case UInt8(ascii: "t"): return literal("true", .bool(true))
        case UInt8(ascii: "f"): return literal("false", .bool(false))
        case UInt8(ascii: "n"): return literal("null", .null)
        default: return number()
        }
    }

    mutating func literal(_ word: String, _ value: ImportJSON) -> ImportJSON? {
        let utf8 = Array(word.utf8)
        guard at + utf8.count <= bytes.count, Array(bytes[at..<(at + utf8.count)]) == utf8 else {
            return nil
        }
        at += utf8.count
        return value
    }

    mutating func number() -> ImportJSON? {
        let start = at
        if at < bytes.count, bytes[at] == UInt8(ascii: "-") { at += 1 }
        let digits = { (b: UInt8) in b >= 0x30 && b <= 0x39 }
        guard at < bytes.count, digits(bytes[at]) else { return nil }
        if bytes[at] == 0x30 { at += 1 } else { while at < bytes.count, digits(bytes[at]) { at += 1 } }
        if at < bytes.count, bytes[at] == UInt8(ascii: ".") {
            at += 1
            guard at < bytes.count, digits(bytes[at]) else { return nil }
            while at < bytes.count, digits(bytes[at]) { at += 1 }
        }
        if at < bytes.count, bytes[at] == UInt8(ascii: "e") || bytes[at] == UInt8(ascii: "E") {
            at += 1
            if at < bytes.count, bytes[at] == UInt8(ascii: "+") || bytes[at] == UInt8(ascii: "-") { at += 1 }
            guard at < bytes.count, digits(bytes[at]) else { return nil }
            while at < bytes.count, digits(bytes[at]) { at += 1 }
        }
        let text = String(decoding: bytes[start..<at], as: UTF8.self)
        return Double(text).map(ImportJSON.number)
    }

    mutating func hex4() -> UInt32? {
        guard at + 4 <= bytes.count,
              let value = UInt32(String(decoding: bytes[at..<(at + 4)], as: UTF8.self), radix: 16)
        else { return nil }
        at += 4
        return value
    }

    mutating func string() -> String? {
        at += 1
        var out: [UInt8] = []
        while at < bytes.count {
            let byte = bytes[at]
            if byte == UInt8(ascii: "\"") {
                at += 1
                return String(decoding: out, as: UTF8.self)
            }
            if byte < 0x20 { return nil }
            if byte != UInt8(ascii: "\\") {
                out.append(byte)
                at += 1
                continue
            }
            at += 1
            guard at < bytes.count else { return nil }
            let escape = bytes[at]
            at += 1
            switch escape {
            case UInt8(ascii: "\""): out.append(0x22)
            case UInt8(ascii: "\\"): out.append(0x5C)
            case UInt8(ascii: "/"): out.append(0x2F)
            case UInt8(ascii: "b"): out.append(0x08)
            case UInt8(ascii: "f"): out.append(0x0C)
            case UInt8(ascii: "n"): out.append(0x0A)
            case UInt8(ascii: "r"): out.append(0x0D)
            case UInt8(ascii: "t"): out.append(0x09)
            case UInt8(ascii: "u"):
                guard var code = hex4() else { return nil }
                if (0xD800...0xDBFF).contains(code), at + 6 <= bytes.count,
                   bytes[at] == UInt8(ascii: "\\"), bytes[at + 1] == UInt8(ascii: "u") {
                    let save = at
                    at += 2
                    if let low = hex4(), (0xDC00...0xDFFF).contains(low) {
                        code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00)
                    } else {
                        at = save
                    }
                }
                let scalar = Unicode.Scalar(code) ?? "\u{FFFD}"
                out.append(contentsOf: Array(String(Character(scalar)).utf8))
            default: return nil
            }
        }
        return nil
    }

    mutating func array() -> ImportJSON? {
        depth += 1
        defer { depth -= 1 }
        guard depth < 512 else { return nil }
        at += 1
        var values: [ImportJSON] = []
        space()
        if at < bytes.count, bytes[at] == UInt8(ascii: "]") { at += 1; return .array(values) }
        while true {
            guard let value = value() else { return nil }
            values.append(value)
            space()
            guard at < bytes.count else { return nil }
            if bytes[at] == UInt8(ascii: ",") { at += 1; continue }
            if bytes[at] == UInt8(ascii: "]") { at += 1; return .array(values) }
            return nil
        }
    }

    mutating func object() -> ImportJSON? {
        depth += 1
        defer { depth -= 1 }
        guard depth < 512 else { return nil }
        at += 1
        var pairs: [(String, ImportJSON)] = []
        space()
        if at < bytes.count, bytes[at] == UInt8(ascii: "}") { at += 1; return .object(pairs) }
        while true {
            space()
            guard at < bytes.count, bytes[at] == UInt8(ascii: "\""), let key = string() else { return nil }
            space()
            guard at < bytes.count, bytes[at] == UInt8(ascii: ":") else { return nil }
            at += 1
            guard let value = value() else { return nil }
            // A later key wins, as in JSON.parse; it keeps the first one's place.
            if let index = pairs.firstIndex(where: { $0.0 == key }) {
                pairs[index].1 = value
            } else {
                pairs.append((key, value))
            }
            space()
            guard at < bytes.count else { return nil }
            if bytes[at] == UInt8(ascii: ",") { at += 1; continue }
            if bytes[at] == UInt8(ascii: "}") { at += 1; return .object(pairs) }
            return nil
        }
    }
}
