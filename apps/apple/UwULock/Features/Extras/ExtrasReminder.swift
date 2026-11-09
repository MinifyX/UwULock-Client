// The reminder to renew a login's password (ItemExtras.tsx `ReminderCard`): when it is due, change
// it (every N months or on a date) or remove it.
import SwiftUI
import UwUDesign

/// The reminder's rows inside the item's extras.
struct ExtrasReminderSection: View {
    let itemId: String
    let reminder: ReminderMark?
    let backend: ExtrasBackend
    /// Called after a change so the extras show it at once (fixtures don't tick the store).
    let changed: () -> Void

    @State private var editing = false
    @State private var busy = false
    @State private var removed = 0

    var body: some View {
        Section {
            if let reminder {
                HStack(alignment: .top, spacing: UwUSpacing.sm) {
                    UwUIcon(.reminder)
                        .foregroundStyle(reminder.isDue ? Color.uwuWarningInk : Color.uwuPinkInk)
                    VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                        HStack(spacing: UwUSpacing.xs) {
                            Text(features: "Passwort erneuern").uwuFont(.body, weight: .semibold)
                            if reminder.isDue {
                                Text(features: "Jetzt fällig")
                                    .uwuFont(.caption, weight: .semibold)
                                    .foregroundStyle(Color.uwuWarningInk)
                                    .padding(.horizontal, UwUSpacing.xs)
                                    .padding(.vertical, 2)
                                    .background(Color.uwuWarningTint, in: Capsule())
                            }
                        }
                        Text(verbatim: Self.describe(reminder))
                            .uwuFont(.subheadline)
                            .foregroundStyle(Color.uwuMuted)
                    }
                }
                .accessibilityElement(children: .combine)
                #if os(iOS)
                .swipeActions(edge: .trailing) {
                    Button(role: .destructive) { remove() } label: {
                        Label(features: "Erinnerung entfernen", uwuIcon: .delete)
                    }
                }
                #endif
                .contextMenu {
                    Button { editing = true } label: { Label(features: "Ändern", uwuIcon: .edit) }
                    Button(role: .destructive) { remove() } label: {
                        Label(features: "Erinnerung entfernen", uwuIcon: .delete)
                    }
                }

                Button { editing = true } label: { Label(features: "Ändern", uwuIcon: .edit) }
                    .disabled(busy)
                Button(role: .destructive) { remove() } label: {
                    Label(features: "Erinnerung entfernen", uwuIcon: .delete)
                }
                .disabled(busy)
                #if os(macOS)
                .help(Text(features: "Erinnerung entfernen"))
                #endif
            } else {
                Button { editing = true } label: {
                    Label(features: "Ans Erneuern des Passworts erinnern", uwuIcon: .reminder)
                }
            }
        }
        .sensoryFeedback(.success, trigger: removed)
        .sheet(isPresented: $editing) {
            ExtrasReminderEditor(reminder: reminder) { due, months in
                try await backend.setReminder(itemId, due: due, everyMonths: months)
                FeatureToasts.shared.show(featureString("Erinnerung gespeichert ✧"))
                changed()
            }
        }
    }

    private func remove() {
        busy = true
        Task {
            defer { busy = false }
            do {
                try await backend.deleteReminder(itemId)
                removed += 1
                FeatureToasts.shared.show(featureString("Erinnerung entfernt."))
                changed()
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }

    /// "alle 6 Monate · fällig am 15.11.2026".
    static func describe(_ reminder: ReminderMark) -> String {
        var parts: [String] = []
        if let months = reminder.everyMonths {
            parts.append(months == 1 ? featureString("jeden Monat") : featureString("alle \(months) Monate"))
        }
        if let due = reminder.due, let day = day(due) {
            parts.append(featureString("fällig am \(day)"))
        }
        return parts.joined(separator: " · ")
    }

    /// A `YYYY-MM-DD` day, as the page shows it (medium, in UTC so it never moves a day).
    static func day(_ text: String) -> String? {
        guard let date = parseDay(text) else { return nil }
        var style = Date.FormatStyle(date: .abbreviated, time: .omitted)
        style.timeZone = TimeZone(identifier: "UTC")!
        return date.formatted(style)
    }

    static func parseDay(_ text: String) -> Date? {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let parts = text.prefix(10).split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        return calendar.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2]))
    }

    static func dayText(_ date: Date) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let c = calendar.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
    }
}

private enum ExtrasReminderMode: Hashable {
    case months, date
}

/// Editing a reminder: every N months (from the last password change) or on a date.
private struct ExtrasReminderEditor: View {
    let reminder: ReminderMark?
    let save: (_ due: String?, _ everyMonths: Int?) async throws -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var mode: ExtrasReminderMode
    @State private var months: Int
    @State private var date: Date
    @State private var saving = false
    @State private var failure: CoreFailure?

    private static let choices = [1, 3, 6, 12, 24]

    init(reminder: ReminderMark?, save: @escaping (_ due: String?, _ everyMonths: Int?) async throws -> Void) {
        self.reminder = reminder
        self.save = save
        let byMonths = reminder?.everyMonths != nil || reminder?.due == nil
        _mode = State(initialValue: byMonths ? .months : .date)
        _months = State(initialValue: reminder?.everyMonths ?? 6)
        let due = reminder?.due.flatMap(ExtrasReminderSection.parseDay)
        _date = State(initialValue: due ?? Calendar.current.date(byAdding: .month, value: 6, to: .now) ?? .now)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker(selection: $mode) {
                        Text(features: "Regelmäßig").tag(ExtrasReminderMode.months)
                        Text(features: "An einem Datum").tag(ExtrasReminderMode.date)
                    } label: {
                        Text(features: "Wann")
                    }
                    .pickerStyle(.segmented)
                }
                if mode == .months {
                    Section {
                        Picker(selection: $months) {
                            ForEach(Self.choices, id: \.self) { n in
                                Text(features: n == 1 ? "1 Monat" : "\(n) Monate").tag(n)
                            }
                        } label: {
                            Text(features: "Alle")
                        }
                    } footer: {
                        Text(features: "Gezählt ab der letzten Passwortänderung; ändert sich das Passwort, beginnt es neu.")
                    }
                } else {
                    Section {
                        DatePicker(selection: $date, displayedComponents: .date) {
                            Text(features: "Datum")
                        }
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(Text(features: "Passwort erneuern"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { Text(features: "Abbrechen") }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button { submit() } label: { Text(features: "Speichern") }
                        .disabled(saving)
                }
            }
            .featureFailureAlert($failure)
        }
        #if os(iOS)
        .presentationDetents([.medium])
        #else
        .frame(minWidth: 380, minHeight: 260)
        #endif
    }

    private func submit() {
        saving = true
        Task {
            defer { saving = false }
            do {
                if mode == .months {
                    try await save(nil, months)
                } else {
                    // The picked day as the person sees it, sent as a plain date.
                    let local = Calendar.current.dateComponents([.year, .month, .day], from: date)
                    let text = String(format: "%04d-%02d-%02d", local.year ?? 0, local.month ?? 0, local.day ?? 0)
                    try await save(text, nil)
                }
                dismiss()
            } catch {
                failure = Core.failure(error)
            }
        }
    }
}
