import SwiftUI

// Liquid Glass with the suite's accent. Only the system's materials and controls: the UwU look
// comes from tint, font, icons and colour, never from re-drawn controls.

extension View {
    /// Liquid Glass behind custom content (floating bars, badges, the lock screen's card), tinted
    /// with the accent. Use system controls first: they're glass on their own.
    public func uwuGlass<S: Shape>(
        in shape: S = Capsule(),
        tinted: Bool = false,
        interactive: Bool = false
    ) -> some View {
        var glass = Glass.regular
        if tinted { glass = glass.tint(.uwuPinkSolid) }
        if interactive { glass = glass.interactive() }
        return glassEffect(glass, in: shape)
    }

    /// The main action: `.glassProminent` in the accent colour.
    public func uwuProminentButton() -> some View {
        buttonStyle(.glassProminent).tint(.uwuPinkSolid)
    }

    /// A secondary action: `.glass`.
    public func uwuGlassButton() -> some View {
        buttonStyle(.glass)
    }

    /// The suite's look for a whole scene: accent tint, the user's font choice and text size,
    /// the default font. Apply once at the root of every scene.
    public func uwuTheme(
        font: UwUFontChoice = .uwu,
        textSize: UwUTextSize = .system,
        reduceMotion: Bool = false
    ) -> some View {
        self
            .tint(.uwuPinkSolid)
            .environment(\.uwuFontChoice, font)
            .environment(\.uwuTextSize, textSize)
            .environment(\.uwuReduceMotion, reduceMotion)
            .uwuFont(.body)
    }
}

/// `GlassEffectContainer` with the suite's spacing, so neighbouring glass shapes merge.
public struct UwUGlassContainer<Content: View>: View {
    private let spacing: CGFloat
    private let content: Content

    public init(spacing: CGFloat = UwUSpacing.md, @ViewBuilder content: () -> Content) {
        self.spacing = spacing
        self.content = content()
    }

    public var body: some View {
        GlassEffectContainer(spacing: spacing) { content }
    }
}

/// Spacing scale of tokens.css (4-pt grid).
public enum UwUSpacing {
    public static let xxs: CGFloat = 4
    public static let xs: CGFloat = 8
    public static let sm: CGFloat = 12
    public static let md: CGFloat = 16
    public static let lg: CGFloat = 20
    public static let xl: CGFloat = 24
    public static let xxl: CGFloat = 32
    public static let xxxl: CGFloat = 40
}

/// Corner radii of tokens.css. Native controls keep their own; these are for custom surfaces.
public enum UwURadius {
    public static let small: CGFloat = 8
    public static let control: CGFloat = 10
    public static let card: CGFloat = 16
    public static let dialog: CGFloat = 22
}
