import Foundation

/// The password generator's settings. The minimums raise the length when they need more.
struct GeneratorOptions: Codable, Sendable, Equatable, Hashable {
    var length: Int
    var lowercase: Bool
    var uppercase: Bool
    var digits: Bool
    var symbols: Bool
    var avoidAmbiguous: Bool
    /// At least this many of a set that is on (0 or 1: at least one).
    var minLowercase: Int?
    var minUppercase: Int?
    var minNumber: Int?
    var minSpecial: Int?

    init(
        length: Int = 20, lowercase: Bool = true, uppercase: Bool = true, digits: Bool = true,
        symbols: Bool = true, avoidAmbiguous: Bool = false, minLowercase: Int? = nil,
        minUppercase: Int? = nil, minNumber: Int? = nil, minSpecial: Int? = nil
    ) {
        self.length = length
        self.lowercase = lowercase
        self.uppercase = uppercase
        self.digits = digits
        self.symbols = symbols
        self.avoidAmbiguous = avoidAmbiguous
        self.minLowercase = minLowercase
        self.minUppercase = minUppercase
        self.minNumber = minNumber
        self.minSpecial = minSpecial
    }
}

/// A generated password; `length` is raised to `required` (the minimums) when that is more.
struct Generated: Codable, Sendable, Equatable, Hashable {
    var password: String
    var bits: Int
    var length: Int
    var required: Int
}
