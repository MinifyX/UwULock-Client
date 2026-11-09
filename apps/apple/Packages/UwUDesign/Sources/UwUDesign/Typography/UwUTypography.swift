import SwiftUI

// The suite's type (package docs/typography.md): UwU Sans in the platform's own type roles and
// sizes, made optically as large as the system font (x-height), following Dynamic Type. Weights:
// 400 text, 500 menus/labels, 600 titles/buttons/headline, 700 dialog/group titles/badges,
// 800 only the wordmark.

/// The font choice of Settings → Appearance (same choices and order as the desktop picker).
public enum UwUFontChoice: String, CaseIterable, Identifiable, Sendable {
    case uwu, manrope, rubik, dmsans, system

    public var id: String { rawValue }

    /// The font's own name, shown in itself in the picker. `system` is the app's label for it.
    public var displayName: String {
        switch self {
        case .uwu: "UwU Sans"
        case .manrope: "Manrope"
        case .rubik: "Rubik"
        case .dmsans: "DM Sans"
        case .system: "System"
        }
    }

    /// x-height ÷ em; the system font's is 0.526. Sizes are scaled so all choices look as large.
    var xHeight: CGFloat {
        switch self {
        case .uwu: 0.496
        case .manrope: 0.54
        case .rubik: 0.52
        case .dmsans: 0.504
        case .system: 0.526
        }
    }

    /// Letter spacing in em (tokens.css --uwu-font-tracking for each choice).
    public var tracking: CGFloat {
        switch self {
        case .uwu: -0.008
        case .manrope, .dmsans: -0.004
        case .rubik, .system: 0
        }
    }

    var opticalFactor: CGFloat { 0.526 / xHeight }

    /// PostScript name of the variable font's named instance for a weight (nil: system font).
    func postScriptName(_ weight: Font.Weight) -> String? {
        let suffix = UwUFontChoice.suffix(weight)
        switch self {
        case .uwu: return suffix.isEmpty ? "UwUSans-Regular" : "UwUSans-Regular_\(suffix)"
        case .manrope: return "Manrope-\(suffix.isEmpty ? "Regular" : suffix)"
        case .rubik: return "Rubik-Light_\(suffix.isEmpty ? "Regular" : suffix)"
        case .dmsans: return suffix.isEmpty ? "DMSans9pt-Regular" : "DMSans9pt-Regular_\(suffix)"
        case .system: return nil
        }
    }

    static func suffix(_ weight: Font.Weight) -> String {
        switch weight {
        case .ultraLight, .thin: "ExtraLight"
        case .light: "Light"
        case .medium: "Medium"
        case .semibold: "SemiBold"
        case .bold: "Bold"
        case .heavy, .black: "ExtraBold"
        default: ""
        }
    }
}

/// Text size of Settings → Appearance, on top of Dynamic Type.
public enum UwUTextSize: String, CaseIterable, Identifiable, Sendable {
    case smaller, system, larger, largest

    public var id: String { rawValue }

    public var scale: CGFloat {
        switch self {
        case .smaller: 0.9
        case .system: 1
        case .larger: 1.15
        case .largest: 1.3
        }
    }

    /// German key into the app's String Catalog (en: Smaller, System, Larger, Largest).
    public var titleKey: String {
        switch self {
        case .smaller: "Kleiner"
        case .system: "System"
        case .larger: "Größer"
        case .largest: "Sehr groß"
        }
    }
}

extension EnvironmentValues {
    /// The user's font choice; set it once at the root with `.uwuTheme(font:textSize:)`.
    @Entry public var uwuFontChoice: UwUFontChoice = .uwu
    /// The user's text size; set it once at the root with `.uwuTheme(font:textSize:)`.
    @Entry public var uwuTextSize: UwUTextSize = .system
}

/// The platform's size for a type role at the default Dynamic Type size, in points.
func uwuBaseSize(_ style: Font.TextStyle) -> CGFloat {
    #if os(macOS)
    switch style {
    case .largeTitle: 26
    case .title: 22
    case .title2: 17
    case .title3: 15
    case .headline, .body: 13
    case .callout: 12
    case .subheadline: 11
    case .footnote, .caption, .caption2: 10
    default: 13
    }
    #else
    switch style {
    case .largeTitle: 34
    case .title: 28
    case .title2: 22
    case .title3: 20
    case .headline, .body: 17
    case .callout: 16
    case .subheadline: 15
    case .footnote: 13
    case .caption: 12
    case .caption2: 11
    default: 17
    }
    #endif
}

/// The role's default weight (headline is semibold, as on the platform).
func uwuDefaultWeight(_ style: Font.TextStyle) -> Font.Weight {
    style == .headline ? .semibold : .regular
}

extension Font {
    /// UwU Sans (or another choice) in a type role, scaling with Dynamic Type.
    /// Views should prefer `.uwuFont(_:)`, which also follows the user's choice, text size and
    /// tracking from the environment.
    public static func uwu(
        _ style: Font.TextStyle = .body,
        weight: Font.Weight? = nil,
        choice: UwUFontChoice = .uwu,
        textSize: UwUTextSize = .system
    ) -> Font {
        let weight = weight ?? uwuDefaultWeight(style)
        guard let name = choice.postScriptName(weight) else {
            return textSize == .system
                ? .system(style, weight: weight)
                : .system(size: uwuBaseSize(style) * textSize.scale, weight: weight)
        }
        let size = uwuBaseSize(style) * choice.opticalFactor * textSize.scale
        return .custom(name, size: size, relativeTo: style)
    }

    /// JetBrains Mono, for passwords, keys and codes, scaling with Dynamic Type.
    public static func uwuMono(_ style: Font.TextStyle = .body, weight: Font.Weight = .regular) -> Font {
        let name = switch weight {
        case .medium, .semibold: "JetBrainsMonoRoman-Medium"
        case .bold, .heavy, .black: "JetBrainsMonoRoman-Bold"
        default: "JetBrainsMono-Regular"
        }
        // JetBrains Mono's x-height (0.55) is close enough to the system font's.
        return .custom(name, size: uwuBaseSize(style), relativeTo: style)
    }
}

struct UwUFontModifier: ViewModifier {
    let style: Font.TextStyle
    let weight: Font.Weight?
    @Environment(\.uwuFontChoice) private var choice
    @Environment(\.uwuTextSize) private var textSize
    @ScaledMetric private var dynamicSize: CGFloat

    init(style: Font.TextStyle, weight: Font.Weight?) {
        self.style = style
        self.weight = weight
        _dynamicSize = ScaledMetric(wrappedValue: uwuBaseSize(style), relativeTo: style)
    }

    func body(content: Content) -> some View {
        // The system font has no relativeTo: with a custom text size, scale it here instead.
        let font: Font = choice == .system && textSize != .system
            ? .system(size: dynamicSize * textSize.scale, weight: weight ?? uwuDefaultWeight(style))
            : .uwu(style, weight: weight, choice: choice, textSize: textSize)
        content
            .font(font)
            .tracking(choice.tracking * dynamicSize * textSize.scale)
    }
}

extension View {
    /// Sets the suite's font for a type role: the user's choice (UwU Sans by default), their text
    /// size, Dynamic Type and the font's tracking.
    public func uwuFont(_ style: Font.TextStyle = .body, weight: Font.Weight? = nil) -> some View {
        modifier(UwUFontModifier(style: style, weight: weight))
    }
}
