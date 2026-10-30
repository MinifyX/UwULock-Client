import SwiftUI
import UwUDesign

/// Shared layout of the lock and login screens: Nyu, the wordmark and a line or two above a
/// glass card.
struct AuthScaffold<Content: View>: View {
    let mood: NyuMood
    var title: String?
    var lead: String?
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(spacing: UwUSpacing.xl) {
                VStack(spacing: UwUSpacing.md) {
                    NyuView(mood: mood)
                        .frame(width: 120)
                        .accessibilityHidden(true)
                    UwUWordmark(style: .title, mood: nil)
                    if let title {
                        Text(title)
                            .uwuFont(.title3, weight: .semibold)
                            .multilineTextAlignment(.center)
                    }
                    if let lead {
                        Text(lead)
                            .uwuFont(.callout)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: 420)
                    }
                }
                VStack(alignment: .leading, spacing: UwUSpacing.lg) { content }
                    .padding(UwUSpacing.xl)
                    .frame(maxWidth: 440)
                    .uwuGlass(in: RoundedRectangle(cornerRadius: UwURadius.dialog, style: .continuous))
            }
            .padding(.horizontal, UwUSpacing.lg)
            .padding(.vertical, UwUSpacing.xxl)
            .frame(maxWidth: .infinity)
        }
        .scrollBounceBehavior(.basedOnSize)
        .scrollDismissesKeyboardIfPhone()
        .defaultScrollAnchor(.center)
        .background(.uwuCanvas)
    }
}

/// The card's heading.
struct AuthCardTitle: View {
    let text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .uwuFont(.title2, weight: .bold)
            .accessibilityAddTraits(.isHeader)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The card's lead paragraph.
struct AuthLead: View {
    let text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .uwuFont(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A labelled field, as the desktop's `.field`: label above, hint below.
struct AuthField<Field: View>: View {
    let label: String
    var hint: String?
    @ViewBuilder let field: Field

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
            Text(label)
                .uwuFont(.subheadline, weight: .medium)
                .accessibilityHidden(true)
            field
                .accessibilityLabel(Text(label))
                .authField()
            if let hint {
                Text(hint)
                    .uwuFont(.footnote)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// What the last attempt said, under the fields. Plain: security errors get no Nyu.
struct AuthErrorText: View {
    let text: String?

    var body: some View {
        if let text {
            Label {
                Text(text).fixedSize(horizontal: false, vertical: true)
            } icon: {
                UwUIcon(.warning, size: .sm)
            }
            .uwuFont(.footnote)
            .foregroundStyle(.uwuDangerInk)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityAddTraits(.isStaticText)
        }
    }
}

/// The main button's label: a spinner and the busy text while it works.
struct BusyLabel: View {
    let title: String
    var busy: String?

    var body: some View {
        HStack(spacing: UwUSpacing.xs) {
            if busy != nil {
                ProgressView().controlSize(.small)
            }
            Text(busy ?? title)
        }
        .frame(maxWidth: .infinity)
    }
}

extension View {
    func authField() -> some View {
        #if os(iOS)
        self
            .padding(.horizontal, UwUSpacing.md)
            .frame(minHeight: 48)
            .background(.uwuSurface, in: RoundedRectangle(cornerRadius: UwURadius.control, style: .continuous))
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
        #else
        self.textFieldStyle(.roundedBorder).controlSize(.large)
        #endif
    }

    /// iPhone: a drag on the form puts the keyboard away.
    @ViewBuilder
    func scrollDismissesKeyboardIfPhone() -> some View {
        #if os(iOS)
        scrollDismissesKeyboard(.interactively)
        #else
        self
        #endif
    }

    /// Haptic feedback on the iPhone, unless switched off in Settings → Appearance.
    func uwuHaptic<T: Equatable>(_ feedback: SensoryFeedback, trigger: T) -> some View {
        modifier(HapticModifier(feedback: feedback, trigger: trigger))
    }
}

private struct HapticModifier<T: Equatable>: ViewModifier {
    let feedback: SensoryFeedback
    let trigger: T
    @AppStorage(AppSettingsKey.haptics) private var haptics = true

    func body(content: Content) -> some View {
        #if os(iOS)
        content.sensoryFeedback(trigger: trigger) { _, _ in haptics ? feedback : nil }
        #else
        content
        #endif
    }
}
