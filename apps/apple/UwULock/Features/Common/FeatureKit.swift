// Shared pieces of the feature screens (milestone 5): their string table, the UwULock Server
// extras every screen asks about, dates, empty states, cards, errors and toasts.
import SwiftUI
import UwUDesign

// MARK: Strings

/// The feature screens' string table: Resources/Features.xcstrings (German source, English).
enum FeatureStrings {
    static let table = "Features"
}

/// A string of Features.xcstrings, for places that take a `String` (alerts' messages, toasts).
func featureString(_ key: String.LocalizationValue) -> String {
    String(localized: key, table: FeatureStrings.table)
}

extension Text {
    /// `Text` from Features.xcstrings: `Text(features: "Sends")`.
    init(features key: LocalizedStringKey) {
        self.init(key, tableName: FeatureStrings.table)
    }
}

extension Label where Title == Text, Icon == UwUIcon {
    /// A native `Label` with a suite icon and a title from Features.xcstrings.
    @MainActor
    init(features key: LocalizedStringKey, uwuIcon: UwUIconName, size: UwUIconSize = .md) {
        self.init { Text(key, tableName: FeatureStrings.table) } icon: { UwUIcon(uwuIcon, size: size) }
    }
}

// MARK: Fixtures

/// `-UwULockFixtures YES` (Debug builds only): the feature screens show sample data instead of
/// asking the core, for screenshots and previews. Each area keeps its samples in its own
/// `#if DEBUG` file.
enum FeatureFixtures {
    static var isOn: Bool {
        #if DEBUG
        UserDefaults.standard.bool(forKey: "UwULockFixtures")
        #else
        false
        #endif
    }
}

// MARK: Extras status and core events

/// UwULock Server's extras for the account on screen (lib/uwu.ts `useUwu`), and counters that
/// tick with the core's events so screens can load again (`.onChange(of:)`).
@MainActor
@Observable
final class UwuStore {
    static let shared = UwuStore()

    /// The extras; empty on Bitwarden/Vaultwarden, while locked, or before the first answer.
    private(set) var status: UwuStatus = .empty
    /// Ticks on `uwu-notice` (a new file-request submission, a security notice, …).
    private(set) var notices = 0
    /// The last `uwu-notice`: its kind and id.
    private(set) var lastNotice: (kind: String, id: String?)?
    /// Ticks on `suite-changed` (UwUSSH/UwURDP records changed elsewhere).
    private(set) var suiteChanges = 0
    /// Ticks on `vault-changed` (items, folders, Sends, settings).
    private(set) var vaultChanges = 0
    /// Ticks on `vault-status` (locked, unlocked, switched account, synced).
    private(set) var statusChanges = 0

    @ObservationIgnored private weak var core: Core?
    @ObservationIgnored private var listening: Task<Void, Never>?

    private init() {
        #if DEBUG
        if FeatureFixtures.isOn { status = .fixture }
        #endif
    }

    /// Starts following `core` (once; again for another core). Call from a screen's `.task`.
    func attach(_ core: Core) {
        guard self.core !== core else { return }
        self.core = core
        listening?.cancel()
        guard !FeatureFixtures.isOn else { return }
        listening = Task { [weak self] in
            let events = core.events()
            await self?.refresh()
            for await event in events {
                guard let self else { return }
                switch event {
                case .uwuChanged(let status):
                    self.status = status
                case .vaultStatus(let status):
                    self.statusChanges += 1
                    if status.state != .unlocked {
                        self.status = .empty
                    }
                    await self.refresh()
                case .vaultChanged:
                    self.vaultChanges += 1
                case .suiteChanged:
                    self.suiteChanges += 1
                case .uwuNotice(let kind, let id):
                    self.lastNotice = (kind, id)
                    self.notices += 1
                default:
                    break
                }
            }
        }
    }

    /// Asks the core for the extras again (`uwu_status`); empty when it can't say.
    func refresh() async {
        guard !FeatureFixtures.isOn, let core else { return }
        do {
            status = try await core.uwuStatus()
        } catch {
            status = .empty
        }
    }

    /// Takes the status a command answered with.
    func take(_ status: UwuStatus) {
        self.status = status
    }
}

#if DEBUG
extension UwuStatus {
    /// A UwULock Server with every extra, for fixtures.
    static let fixture = UwuStatus(
        uwu: true, features: UwuFeature.allCases.map(\.rawValue),
        travel: UwuTravel(enabled: false, hiddenCount: nil),
        unseen: UwuUnseen(securityNotices: 0, fileRequestSubmissions: 2), organizations: [],
        sendDomains: [SendDomain(id: "d1", url: "https://send.example.com")], reminders: [:],
        masked: [:], ownIcons: [:], automaticIcons: true,
        limits: UwuLimits(
            maxFileBytes: 100 * 1024 * 1024, versionsPerItem: 20, versionDays: 365,
            fileRequestMaxFiles: 10, fileRequestMaxDays: 30),
        extrasKeyChanged: false)
}
#endif

// MARK: Dates and sizes

/// The server's ISO 8601 dates (with or without fractions) and how the screens show them.
enum FeatureDate {
    /// The date in `text`, or nil.
    static func parse(_ text: String?) -> Date? {
        guard let text, !text.isEmpty else { return nil }
        let plain = ISO8601DateFormatter()
        if let date = plain.date(from: text) { return date }
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = fractional.date(from: text) { return date }
        let day = ISO8601DateFormatter()
        day.formatOptions = [.withFullDate]
        return day.date(from: text)
    }

    /// `date` as the server takes it: ISO 8601 with milliseconds, UTC.
    static func iso(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.string(from: date)
    }

    /// A date for a row: "30. Okt. 2026, 14:05".
    static func short(_ text: String?) -> String? {
        parse(text).map { $0.formatted(date: .abbreviated, time: .shortened) }
    }

    /// A day: "30. Okt. 2026".
    static func day(_ text: String?) -> String? {
        parse(text).map { $0.formatted(date: .abbreviated, time: .omitted) }
    }

    /// "vor 3 Tagen", "in 2 Stunden".
    static func relative(_ text: String?) -> String? {
        parse(text).map { $0.formatted(.relative(presentation: .named)) }
    }
}

extension Int {
    /// Bytes as a file size: "1,2 MB".
    var fileSize: String {
        Int64(self).formatted(.byteCount(style: .file))
    }
}

// MARK: Views

/// The empty or error state of a feature screen: Nyu, a title, a line, actions.
struct FeatureEmptyState<Actions: View>: View {
    let mood: NyuMood
    let title: Text
    var message: Text?
    @ViewBuilder var actions: Actions

    init(mood: NyuMood, title: Text, message: Text? = nil, @ViewBuilder actions: () -> Actions = { EmptyView() }) {
        self.mood = mood
        self.title = title
        self.message = message
        self.actions = actions()
    }

    var body: some View {
        ContentUnavailableView {
            VStack(spacing: UwUSpacing.sm) {
                NyuView(mood: mood).frame(width: 96)
                title.uwuFont(.title3, weight: .bold)
            }
        } description: {
            if let message { message }
        } actions: {
            actions
        }
    }
}

extension View {
    /// A Liquid Glass card around custom content (stats, the review card, QR codes).
    func featureCard(padding: CGFloat = UwUSpacing.md) -> some View {
        self
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .uwuGlass(in: RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous))
    }

    /// Shows a command's failure as an alert with the core's words.
    func featureFailureAlert(_ failure: Binding<CoreFailure?>) -> some View {
        alert(
            Text(features: "Das hat nicht geklappt"),
            isPresented: Binding(
                get: { failure.wrappedValue != nil },
                set: { if !$0 { failure.wrappedValue = nil } }),
            presenting: failure.wrappedValue
        ) { _ in
            Button(role: .cancel) {} label: { Text(features: "OK") }
        } message: { failure in
            Text(verbatim: failure.message)
        }
    }

    /// The toasts of `FeatureToasts.shared` over this screen.
    func featureToasts() -> some View {
        modifier(FeatureToastOverlay())
    }
}

// MARK: Toasts

/// Short notes at the bottom ("Kopiert.", "Gespeichert."), as the desktop's toasts.
@MainActor
@Observable
final class FeatureToasts {
    static let shared = FeatureToasts()

    struct Toast: Identifiable, Equatable {
        let id = UUID()
        let text: String
        let danger: Bool
    }

    private(set) var current: Toast?
    @ObservationIgnored private var hiding: Task<Void, Never>?

    /// Shows `text` for a few seconds.
    func show(_ text: String, danger: Bool = false) {
        let toast = Toast(text: text, danger: danger)
        current = toast
        hiding?.cancel()
        hiding = Task { [weak self] in
            try? await Task.sleep(for: .seconds(danger ? 5 : 2.5))
            guard !Task.isCancelled, self?.current == toast else { return }
            self?.current = nil
        }
    }

    /// Shows a failure in red.
    func show(_ failure: CoreFailure) {
        show(failure.message, danger: true)
    }

    /// Shows any error in red, in the core's words when it is one.
    func show(error: any Error) {
        show(Core.failure(error))
    }
}

private struct FeatureToastOverlay: ViewModifier {
    @State private var toasts = FeatureToasts.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            if let toast = toasts.current {
                Label {
                    Text(verbatim: toast.text)
                } icon: {
                    UwUIcon(toast.danger ? .warning : .success, size: .sm)
                        .foregroundStyle(toast.danger ? Color.uwuDanger : Color.uwuPinkSolid)
                }
                .uwuFont(.subheadline, weight: .medium)
                .padding(.horizontal, UwUSpacing.md)
                .padding(.vertical, UwUSpacing.sm)
                .uwuGlass()
                .padding(.bottom, UwUSpacing.xl)
                .padding(.horizontal, UwUSpacing.md)
                .transition(reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity))
                .id(toast.id)
                .accessibilityAddTraits(.updatesFrequently)
                .onAppear {
                    AccessibilityNotification.Announcement(toast.text).post()
                }
            }
        }
        .uwuAnimation(value: toasts.current)
        .sensoryFeedback(trigger: toasts.current) { _, new in
            guard let new else { return nil }
            return new.danger ? .error : .success
        }
    }
}
