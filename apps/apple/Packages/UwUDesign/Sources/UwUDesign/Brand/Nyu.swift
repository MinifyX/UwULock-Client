import SwiftUI

/// Nyu, the suite's mascot, as UwULock's padlock cat (package docs/nyu.md), with the white
/// sticker edge. Decorative: it never carries information alone.
public struct NyuView: View {
    private let mood: NyuMood

    public init(mood: NyuMood = .uwu) {
        self.mood = mood
    }

    public var body: some View {
        Image("nyu-lock-\(mood.rawValue)", bundle: .module)
            .resizable()
            .scaledToFit()
            .accessibilityHidden(true)
    }
}

/// The wordmark (package Wordmark): Nyu, then "UwU" in pink and the product in ink, extrabold,
/// tracking −0.02 em. Always UwU Sans, whatever the font choice.
public struct UwUWordmark: View {
    private let product: String
    private let style: Font.TextStyle
    private let mood: NyuMood?
    @ScaledMetric private var size: CGFloat

    /// `mood: nil` leaves Nyu out, for places that show a large Nyu right above.
    public init(product: String = "Lock", style: Font.TextStyle = .largeTitle, mood: NyuMood? = .uwu) {
        self.product = product
        self.style = style
        self.mood = mood
        _size = ScaledMetric(wrappedValue: uwuBaseSize(style), relativeTo: style)
    }

    public var body: some View {
        HStack(alignment: .center, spacing: size * 0.3) {
            if let mood {
                NyuView(mood: mood).frame(height: size * 1.35)
            }
            HStack(spacing: 0) {
                Text(verbatim: "UwU").foregroundStyle(.uwuPinkSolid)
                Text(verbatim: product).foregroundStyle(.uwuInk)
            }
            .font(.uwu(style, weight: .heavy))
            .tracking(-0.02 * size)
            .fixedSize()
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: "UwU\(product)"))
        .accessibilityAddTraits(.isHeader)
    }
}
