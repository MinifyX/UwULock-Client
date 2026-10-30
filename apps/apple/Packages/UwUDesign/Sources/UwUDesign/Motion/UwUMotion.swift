import SwiftUI

/// The suite's motion (package docs/motion.md): short, eased, never in the way. Everything
/// here turns into no animation when Reduce Motion is on (or the app's own setting says so).
public enum UwUMotion {
    /// 150 ms: hover, press, small state changes.
    public static let fast: TimeInterval = 0.15
    /// 200 ms: most transitions.
    public static let base: TimeInterval = 0.2
    /// 220 ms: something entering (sheets, toasts, rows).
    public static let enter: TimeInterval = 0.22
}

extension Animation {
    /// cubic-bezier(0.2, 0.9, 0.3, 1): the default ease.
    public static func uwuOut(_ duration: TimeInterval = UwUMotion.base) -> Animation {
        .timingCurve(0.2, 0.9, 0.3, 1, duration: duration)
    }

    /// cubic-bezier(0.2, 0.9, 0.3, 1.3): a slight overshoot, for things that appear.
    public static func uwuPop(_ duration: TimeInterval = UwUMotion.enter) -> Animation {
        .timingCurve(0.2, 0.9, 0.3, 1.3, duration: duration)
    }

    /// cubic-bezier(0.34, 1.56, 0.64, 1): playful, only for Nyu and celebrations.
    public static func uwuSpring(_ duration: TimeInterval = UwUMotion.enter) -> Animation {
        .timingCurve(0.34, 1.56, 0.64, 1, duration: duration)
    }
}

extension EnvironmentValues {
    /// The app's own "reduce motion" setting, on top of the system's.
    @Entry public var uwuReduceMotion: Bool = false
}

struct UwUAnimationModifier<V: Equatable>: ViewModifier {
    let animation: Animation
    let value: V
    @Environment(\.accessibilityReduceMotion) private var systemReduce
    @Environment(\.uwuReduceMotion) private var appReduce

    func body(content: Content) -> some View {
        content.animation(systemReduce || appReduce ? nil : animation, value: value)
    }
}

extension View {
    /// `.animation(_:value:)` that respects Reduce Motion (system and app setting).
    public func uwuAnimation<V: Equatable>(_ animation: Animation = .uwuOut(), value: V) -> some View {
        modifier(UwUAnimationModifier(animation: animation, value: value))
    }
}

/// `withAnimation` that respects Reduce Motion. Pass the environment's value(s).
@MainActor
public func withUwUAnimation<Result>(
    _ animation: Animation = .uwuOut(),
    reduceMotion: Bool,
    _ body: () throws -> Result
) rethrows -> Result {
    try withAnimation(reduceMotion ? nil : animation, body)
}
