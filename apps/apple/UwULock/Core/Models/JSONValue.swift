import Foundation

/// Free-form JSON (TS `unknown` / `Record<string, unknown>`): a suite record's payload, a patch.
enum JSONValue: Codable, Sendable, Hashable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([JSONValue].self) {
            self = .array(value)
        } else {
            self = .object(try container.decode([String: JSONValue].self))
        }
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case .bool(let value): try container.encode(value)
        case .number(let value): try container.encode(value)
        case .string(let value): try container.encode(value)
        case .array(let value): try container.encode(value)
        case .object(let value): try container.encode(value)
        }
    }

    /// The string, when this is one.
    var string: String? {
        if case .string(let value) = self { value } else { nil }
    }

    /// The number, when this is one.
    var number: Double? {
        if case .number(let value) = self { value } else { nil }
    }

    /// The whole number, when this is a number without a fraction.
    var int: Int? {
        if case .number(let value) = self, value == value.rounded(), abs(value) < 9e15 {
            Int(value)
        } else {
            nil
        }
    }

    /// The boolean, when this is one.
    var bool: Bool? {
        if case .bool(let value) = self { value } else { nil }
    }

    /// The members, when this is an object.
    var object: [String: JSONValue]? {
        if case .object(let value) = self { value } else { nil }
    }

    /// The elements, when this is an array.
    var array: [JSONValue]? {
        if case .array(let value) = self { value } else { nil }
    }
}

extension JSONValue: ExpressibleByNilLiteral, ExpressibleByBooleanLiteral,
    ExpressibleByIntegerLiteral, ExpressibleByFloatLiteral, ExpressibleByStringLiteral,
    ExpressibleByArrayLiteral, ExpressibleByDictionaryLiteral
{
    init(nilLiteral: ()) { self = .null }
    init(booleanLiteral value: Bool) { self = .bool(value) }
    init(integerLiteral value: Int) { self = .number(Double(value)) }
    init(floatLiteral value: Double) { self = .number(value) }
    init(stringLiteral value: String) { self = .string(value) }
    init(arrayLiteral elements: JSONValue...) { self = .array(elements) }
    init(dictionaryLiteral elements: (String, JSONValue)...) {
        self = .object(Dictionary(elements, uniquingKeysWith: { _, last in last }))
    }
}
