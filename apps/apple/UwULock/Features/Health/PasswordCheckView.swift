// Passwortprüfung (HealthPane.tsx, mobile/pages/Check.tsx, lib/health.ts): the score with its
// headline, "Passwörter durchgehen", the groups with their logins, the addresses in breaches
// and what was ignored. The breach sources are only asked again with "Nochmal prüfen" or a
// pull; otherwise their last answers count for passwords that haven't changed since.
import SwiftUI
import UwUDesign

struct PasswordCheckView: View {
    @Environment(AppModel.self) private var model
    @State private var store = HealthCheckStore()
    @State private var reviewing = false

    var body: some View {
        content
            .navigationTitle(Text(features: "Passwortprüfung"))
            .toolbar { toolbar }
            .task {
                UwuStore.shared.attach(model.core)
                await store.attach(model.core)
            }
            .onDisappear {
                if !reviewing { store.detach() }
            }
            .sheet(isPresented: $reviewing) {
                HealthReviewView(store: store)
                    #if os(iOS)
                    .presentationDetents([.large])
                    #endif
            }
            .sensoryFeedback(.success, trigger: store.checked)
            .featureToasts()
    }

    @ViewBuilder
    private var content: some View {
        if let view = store.view {
            HealthReportList(view: view, store: store, onReview: { reviewing = true })
        } else if let error = store.error {
            FeatureEmptyState(
                mood: .sad, title: Text(features: "Die Prüfung hat nicht geklappt"),
                message: Text(verbatim: error))
            {
                Button {
                    Task { await store.load(fresh: false) }
                } label: {
                    Label(features: "Nochmal versuchen", uwuIcon: .refresh)
                }
                .uwuGlassButton()
            }
        } else {
            VStack(spacing: UwUSpacing.md) {
                NyuView(mood: .sleepy).frame(width: 96)
                ProgressView {
                    Text(verbatim: store.busy ?? featureString("Lädt …"))
                        .uwuFont(.subheadline)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if let view = store.view, !view.cards.isEmpty {
            ToolbarItem {
                Button { reviewing = true } label: {
                    Label(features: "Durchgehen", uwuIcon: .review)
                }
                .disabled(store.busy != nil)
                .help(Text(features: "Passwörter durchgehen"))
            }
        }
        ToolbarItem(placement: .primaryAction) {
            Button {
                Task { await store.load(fresh: true) }
            } label: {
                if store.view?.hasSources == true, store.view?.checkedAt != nil {
                    Label(features: "Nochmal prüfen", uwuIcon: .refresh)
                } else {
                    Label(features: "Jetzt prüfen", uwuIcon: .refresh)
                }
            }
            .disabled(store.busy != nil)
            .keyboardShortcut("r")
            .help(Text(features: "Nochmal prüfen"))
        }
    }
}

// MARK: The report

private struct HealthReportList: View {
    let view: HealthView
    let store: HealthCheckStore
    let onReview: () -> Void

    @Environment(\.passwordCheckOpenItem) private var openItem

    var body: some View {
        let groups = Dictionary(uniqueKeysWithValues: HealthGroup.allCases.map { ($0, view.entries($0)) })
        let shown = view.shownGroups
        let open = shown.reduce(0) { $0 + (groups[$1]?.count ?? 0) }
        let score = view.score(groups)
        let ignored = view.ignoredEntries
        ScrollViewReader { proxy in
            List {
                Section {
                    VStack(spacing: UwUSpacing.md) {
                        HealthScoreCard(view: view, score: score, open: open, breached: groups[.breached]?.count ?? 0, busy: store.busy)
                        if !view.cards.isEmpty {
                            Button(action: onReview) {
                                Label(features: "Passwörter durchgehen", uwuIcon: .start)
                                    .frame(maxWidth: .infinity)
                            }
                            .uwuProminentButton()
                            .foregroundStyle(Color.uwuOnPink)
                            .controlSize(.large)
                            .disabled(store.busy != nil)
                        }
                    }
                    .padding(.vertical, UwUSpacing.xs)
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
                    if let busy = store.busy {
                        HealthProgressRow(text: busy, progress: store.progress)
                    }
                }
                .listSectionSeparator(.hidden)

                if view.checkedAt == nil, store.busy == nil, view.hasSources {
                    Section {
                        Text(features: "Findet schwache und doppelte Passwörter, Logins ohne https – und mit „Jetzt prüfen“ Passwörter aus bekannten Datenlecks. Dafür fragt dein Server Have I Been Pwned (die ersten fünf Zeichen eines SHA-1) und XposedOrNot (die ersten zehn Zeichen eines Keccak-512) nach dem Hash jedes Passworts; das Passwort selbst verlässt dieses Gerät nie. Dein Server sieht diese Zeichen dabei, schreibt sie aber nirgends auf.")
                            .uwuFont(.subheadline)
                            .foregroundStyle(.secondary)
                        Button {
                            Task { await store.load(fresh: true) }
                        } label: {
                            Label(features: "Jetzt prüfen", uwuIcon: .securityCheck)
                        }
                    }
                }

                if let error = store.error {
                    HealthNote(text: Text(verbatim: error), icon: .error, tone: .uwuDangerInk)
                }
                if view.report.breachesIncomplete {
                    HealthNote(
                        text: Text(features: "Eine Quelle für Datenlecks hat nicht für alle Passwörter geantwortet. Der Rest des Berichts stimmt; prüf später noch einmal."),
                        icon: .warning, tone: .uwuWarningInk)
                }

                Section {
                    ForEach(shown) { group in
                        let count = groups[group]?.count ?? 0
                        Button {
                            withAnimation { proxy.scrollTo(group, anchor: .top) }
                        } label: {
                            LabeledContent {
                                Text(verbatim: "\(count)").monospacedDigit()
                            } label: {
                                Label {
                                    Text(features: group.title)
                                } icon: {
                                    HealthGroupIcon(group: group)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .disabled(count == 0)
                        .contentShape(Rectangle())
                    }
                } header: {
                    Text(features: "Hinweise")
                }

                if view.switches.emailCheck {
                    HealthEmailSection(view: view, store: store)
                }

                ForEach(shown) { group in
                    if let entries = groups[group], !entries.isEmpty {
                        groupSection(group, entries).id(group)
                    }
                }

                if !ignored.isEmpty {
                    Section {
                        ForEach(ignored) { entry in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(verbatim: HealthWords.name(entry.name))
                                    Text(verbatim: entry.kind.healthTitle)
                                        .uwuFont(.subheadline)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button {
                                    Task { await store.setIgnored(entry.itemId, entry.kind, false) }
                                } label: {
                                    Label(features: "Rückgängig", uwuIcon: .undo)
                                }
                                .buttonStyle(.borderless)
                            }
                            .contextMenu { openButton(entry.itemId) }
                        }
                    } header: {
                        sectionHeader(Text(features: "Ignoriert"), icon: .hide, count: ignored.count)
                    } footer: {
                        Text(features: "Diese Hinweise zeigt die Prüfung nicht mehr, auf keinem Gerät.")
                    }
                }

                Section {
                } footer: {
                    VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                        if view.twofaFailed || view.sitesFailed {
                            Text(features: "Eine Liste von Websites war gerade nicht zu haben; dieser Teil fehlt im Bericht.")
                        }
                        if view.hasSources {
                            Text(features: "Schwache und doppelte Passwörter findet UwULock auf diesem Gerät. Für Datenlecks fragt dein Server nur nach dem Anfang eines Hashs (k-Anonymität); das Passwort verlässt dieses Gerät nie.")
                        } else if view.uwu {
                            Text(features: "Findet schwache und doppelte Passwörter und Logins ohne https. Den Abgleich mit Datenlecks hat die Verwaltung dieses Servers ausgeschaltet.")
                        } else {
                            Text(features: "Findet schwache und doppelte Passwörter und Logins ohne https – auf diesem Gerät. Datenlecks prüft UwULock über einen UwULock Server; dieser Server bietet das nicht an.")
                        }
                    }
                }
            }
            #if os(macOS)
            .listStyle(.inset)
            #endif
            .refreshable { await store.load(fresh: view.hasSources) }
            .uwuAnimation(value: view)
        }
    }

    private func sectionHeader(_ title: Text, icon: UwUIconName, count: Int) -> some View {
        HStack(spacing: UwUSpacing.xs) {
            UwUIcon(icon, size: .sm)
            title
            Spacer()
            Text(verbatim: "\(count)").monospacedDigit()
        }
    }

    private func groupSection(_ group: HealthGroup, _ entries: [HealthEntry]) -> some View {
        Section {
            ForEach(entries) { entry in
                HealthEntryRow(entry: entry, group: group, view: view, store: store, openItem: openItem)
            }
        } header: {
            HStack(spacing: UwUSpacing.xs) {
                UwUIcon(group.icon, size: .sm).foregroundStyle(group.tone)
                Text(features: group.title)
                Spacer()
                Text(verbatim: "\(entries.count)").monospacedDigit()
            }
        } footer: {
            VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                Text(features: group.lead)
                if group == .siteBreach, !view.siteSources.isEmpty {
                    let sources = HealthWords.sourcesWithLicenses(view.siteSources)
                    Text(features: "Listen der Datenlecks: \(sources)")
                    sourceLinks(view.siteSources)
                }
                if group == .twofa, let source = view.twofaSource {
                    let name = source.name
                    let license = source.license ?? ""
                    Text(features: "Liste der Websites: \(name), \(license)")
                    sourceLinks([source])
                }
            }
        }
    }

    @ViewBuilder
    private func sourceLinks(_ sources: [HealthSource]) -> some View {
        ForEach(sources.filter { $0.url != nil }, id: \.name) { source in
            if let text = source.url, let url = URL(string: text) {
                Link(destination: url) {
                    Text(verbatim: source.name)
                }
            }
        }
    }

    @ViewBuilder
    private func openButton(_ id: String) -> some View {
        if let openItem {
            Button { openItem(id) } label: {
                Label(features: "Öffnen", uwuIcon: .vault)
            }
        }
    }
}

// MARK: Pieces

/// The ring with the score, the headline and the lead (Check.tsx's `m-score`).
private struct HealthScoreCard: View {
    let view: HealthView
    let score: Int
    let open: Int
    let breached: Int
    let busy: String?

    var body: some View {
        HStack(spacing: UwUSpacing.md) {
            Gauge(value: Double(score), in: 0...100) {
                EmptyView()
            } currentValueLabel: {
                Text(verbatim: "\(score)")
                    .uwuFont(.title3, weight: .bold)
                    .contentTransition(.numericText())
            }
            .gaugeStyle(.accessoryCircularCapacity)
            .tint(healthScoreTone(score))
            .scaleEffect(1.25)
            .frame(width: 76, height: 76)
            .accessibilityLabel(Text(features: "\(score) von 100"))

            VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                Text(verbatim: healthHeadline(score: score, open: open))
                    .uwuFont(.headline, weight: .bold)
                Group {
                    if breached == 1 {
                        Text(features: "Ein Passwort steckt in Datenlecks. Das geht zuerst.")
                    } else if breached > 1 {
                        Text(features: "\(breached) Passwörter stecken in Datenlecks. Die gehen zuerst.")
                    } else if open > 0 {
                        Text(features: "Geh die Hinweise der Reihe nach durch.")
                    }
                }
                .uwuFont(.subheadline)
                .foregroundStyle(.secondary)
                HStack(spacing: UwUSpacing.xs) {
                    Text(features: "\(view.report.checked) Passwörter geprüft")
                    if let when = FeatureDate.short(view.checkedAt) {
                        Text(verbatim: "·")
                        Text(features: "Stand: \(when)")
                    }
                }
                .uwuFont(.caption)
                .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 0)
        }
        .featureCard()
        .accessibilityElement(children: .combine)
    }
}

/// While the sources are asked: the line, and a bar per source.
private struct HealthProgressRow: View {
    let text: String
    let progress: CheckProgress?

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.xs) {
            HStack(spacing: UwUSpacing.xs) {
                ProgressView().controlSize(.small)
                Text(verbatim: text)
                    .uwuFont(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if let progress {
                let sources = [("Have I Been Pwned", progress.hibp), ("XposedOrNot", progress.xon)]
                    .compactMap { name, source in source.map { (name, $0) } }
                if sources.isEmpty, progress.total > 0 {
                    ProgressView(value: Double(progress.done), total: Double(progress.total))
                }
                ForEach(sources, id: \.0) { name, source in
                    ProgressView(value: Double(source.done), total: Double(max(source.total, 1))) {
                        Text(verbatim: name).uwuFont(.caption)
                    }
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.updatesFrequently)
    }
}

/// A line of the report with an icon in a state's colour.
private struct HealthNote: View {
    let text: Text
    let icon: UwUIconName
    let tone: Color

    var body: some View {
        Section {
            Label {
                text.uwuFont(.subheadline)
            } icon: {
                UwUIcon(icon, size: .sm)
            }
            .foregroundStyle(tone)
        }
    }
}

/// The group's icon in its tone, on its tint.
private struct HealthGroupIcon: View {
    let group: HealthGroup

    var body: some View {
        UwUIcon(group.icon, size: .sm)
            .foregroundStyle(group.tone)
            .frame(width: 28, height: 28)
            .background(group.tint, in: RoundedRectangle(cornerRadius: UwURadius.small, style: .continuous))
    }
}

/// One login of a group: open it, ignore the hint, the 2FA instructions.
private struct HealthEntryRow: View {
    let entry: HealthEntry
    let group: HealthGroup
    let view: HealthView
    let store: HealthCheckStore
    let openItem: PasswordCheckOpenItem?

    var body: some View {
        let row = HStack(spacing: UwUSpacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: HealthWords.name(entry.name))
                    .foregroundStyle(.primary)
                if !entry.detail.isEmpty {
                    Text(verbatim: entry.detail)
                        .uwuFont(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            Spacer(minLength: 0)
            if let url = entry.documentation {
                Link(destination: url) {
                    UwUIcon(.info, size: .sm)
                }
                .buttonStyle(.borderless)
                .help(Text(features: "Anleitung zur Zwei-Schritt-Anmeldung"))
                .accessibilityLabel(Text(features: "Anleitung zur Zwei-Schritt-Anmeldung"))
            }
        }
        Group {
            if let openItem {
                Button { openItem(entry.id) } label: { row.contentShape(Rectangle()) }
                    .buttonStyle(.plain)
            } else {
                row
            }
        }
        .contextMenu {
            if let openItem {
                Button { openItem(entry.id) } label: {
                    Label(features: "Öffnen", uwuIcon: .vault)
                }
            }
            if let url = entry.documentation {
                Link(destination: url) {
                    Label(features: "Anleitung zur Zwei-Schritt-Anmeldung", uwuIcon: .link)
                }
            }
            if view.ignored != nil {
                ignoreButton
            }
        }
        .swipeActions(edge: .trailing) {
            if view.ignored != nil {
                ignoreButton.tint(.uwuMuted)
            }
        }
    }

    private var ignoreButton: some View {
        Button {
            Task { await store.setIgnored(entry.id, group.kind, true) }
        } label: {
            Label(features: "Ignorieren", uwuIcon: .hide)
        }
    }
}

// MARK: The addresses

/// The addresses at XposedOrNot: only with the account's consent.
private struct HealthEmailSection: View {
    let view: HealthView
    let store: HealthCheckStore

    var body: some View {
        let opted = view.emailOptIn?.optedIn ?? false
        Section {
            Toggle(isOn: Binding(
                get: { opted },
                set: { on in Task { await store.setEmailOptIn(on) } }))
            {
                Label(features: "Adressen in Datenlecks prüfen", uwuIcon: .mail)
            }
            if opted {
                Button {
                    Task { await store.checkEmails() }
                } label: {
                    if store.emailBusy {
                        Label(features: "Prüft Adressen …", uwuIcon: .pending)
                    } else {
                        Label(features: "Adressen prüfen", uwuIcon: .securityCheck)
                    }
                }
                .disabled(store.emailBusy)
                ForEach(store.emailResults ?? []) { result in
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(verbatim: result.email)
                            Text(verbatim: HealthWords.emailResult(result))
                                .uwuFont(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                    } icon: {
                        UwUIcon(result.status == "found" ? .warning : result.status == "clean" ? .success : .pending, size: .sm)
                            .foregroundStyle(
                                result.status == "found" ? Color.uwuDangerInk
                                    : result.status == "clean" ? Color.uwuSuccessInk : Color.uwuMuted)
                    }
                }
            }
        } header: {
            Text(features: "Deine Adressen in Datenlecks")
        } footer: {
            if opted {
                Text(features: "Dein Server fragt XposedOrNot nach deiner Kontoadresse und den Adressen, die in Logins als Benutzername stehen. Dafür geht jede Adresse im Klartext an XposedOrNot; dein Server merkt sich die Antworten eine Woche lang, nur unter einem Hash der Adresse.")
            } else {
                Text(features: "Die Passwortprüfung fragt dann auch, ob deine Kontoadresse und die Adressen, die in Logins als Benutzername stehen, in Datenlecks auftauchen. Dafür schickt dein Server jede dieser Adressen im Klartext an XposedOrNot (xposedornot.com) – deine Passwörter und deine anderen Daten nicht. Die Antworten merkt er sich eine Woche lang, nur unter einem Hash der Adresse.")
            }
        }
        .task { await store.refreshEmailOptIn() }
    }

}

#if DEBUG
#Preview("Passwortprüfung") {
    NavigationStack {
        HealthReportList(view: HealthFixtures.view, store: .preview(), onReview: {})
            .navigationTitle(Text(features: "Passwortprüfung"))
    }
}
#endif
