import SwiftUI

/// The icon sizes of the suite (package docs/icons.md), in points at the default Dynamic Type size.
public enum UwUIconSize: Sendable {
    case xs, sm, md, lg, xl

    public var points: CGFloat {
        switch self {
        case .xs: 14
        case .sm: 16
        case .md: 18
        case .lg: 20
        case .xl: 24
        }
    }
}

extension UwUIconName {
    /// The icon as a template image (it takes the foreground style / tint).
    public var image: Image { Image(assetName, bundle: .module).renderingMode(.template) }
}

/// One icon of the suite's vocabulary, sized for its role and scaled with Dynamic Type.
/// Decorative by default; pass a label when the icon stands alone (e.g. in a toolbar).
public struct UwUIcon: View {
    private let name: UwUIconName
    private let label: Text?
    @ScaledMetric private var side: CGFloat

    public init(_ name: UwUIconName, size: UwUIconSize = .md, label: Text? = nil) {
        self.name = name
        self.label = label
        _side = ScaledMetric(wrappedValue: size.points, relativeTo: .body)
    }

    public var body: some View {
        let image = name.image.resizable().scaledToFit().frame(width: side, height: side)
        if let label {
            image.accessibilityLabel(label)
        } else {
            image.accessibilityHidden(true)
        }
    }
}

extension Label where Title == Text, Icon == UwUIcon {
    /// A native `Label` with a suite icon: `Label("Favoriten", uwuIcon: .favorite)`.
    @MainActor public init(_ titleKey: LocalizedStringKey, uwuIcon: UwUIconName, size: UwUIconSize = .md) {
        self.init { Text(titleKey) } icon: { UwUIcon(uwuIcon, size: size) }
    }

    /// A native `Label` with a suite icon and a title that's already localised or user content.
    @_disfavoredOverload
    @MainActor public init<S: StringProtocol>(_ title: S, uwuIcon: UwUIconName, size: UwUIconSize = .md) {
        self.init { Text(title) } icon: { UwUIcon(uwuIcon, size: size) }
    }
}
