// The review one login at a time (HealthPane.tsx `HealthReview`, lib/review.ts): a stack of
// cards swiped (or paged with ← →) back and forth. The stack is laid once: ignoring or fixing
// changes a card, it doesn't reshuffle the rest.
import SwiftUI
import UwUDesign

/// How far a card has to be dragged to count as a swipe (review.ts `SWIPE`).
private let healthSwipe: CGFloat = 80

struct HealthReviewView: View {
    let store: HealthCheckStore

    @Environment(\.dismiss) private var dismiss
    @Environment(\.passwordCheckOpenItem) private var openItem
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.uwuReduceMotion) private var appReduceMotion
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    @State private var cards: [HealthCard] = []
    @State private var index = 0
    @State private var laid = false
    @State private var dragX: CGFloat = 0
    /// +1: the last move went to the next card, -1 to the previous one.
    @State private var direction = 1
    @State private var generatingFor: HealthCard?

    private var noMotion: Bool { reduceMotion || appReduceMotion }
    private var total: Int { cards.count }
    private var card: HealthCard? { cards.indices.contains(index) ? cards[index] : nil }

    var body: some View {
        NavigationStack {
            content
                .navigationTitle(Text(features: "Passwörter durchgehen"))
                #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button { dismiss() } label: {
                            Label(features: "Zum Bericht", uwuIcon: .back)
                        }
                        .keyboardShortcut(.cancelAction)
                    }
                    if total > 0 {
                        ToolbarItem(placement: .principal) {
                            Text(features: "\(min(index + 1, total)) von \(total)")
                                .uwuFont(.subheadline, weight: .semibold)
                                .monospacedDigit()
                                .contentTransition(.numericText())
                                .accessibilityAddTraits(.updatesFrequently)
                        }
                    }
                }
        }
        #if os(macOS)
        .frame(minWidth: 520, idealWidth: 580, minHeight: 600, idealHeight: 680)
        #endif
        .onAppear(perform: lay)
        .sheet(item: $generatingFor) { card in
            HealthGeneratorSheet(store: store) { password in
                generatingFor = nil
                Task { await store.saveNewPassword(card.finding.id, password) }
            }
        }
        .sensoryFeedback(.selection, trigger: index)
        .sensoryFeedback(trigger: abs(dragX) >= healthSwipe) { _, crossed in
            crossed ? .impact(weight: .light) : nil
        }
        .featureToasts()
    }

    /// The stack, without the cards put off for later.
    private func lay() {
        guard !laid, let view = store.view else { return }
        laid = true
        cards = view.cards.filter { !HealthLater.ids.contains($0.finding.id) }
        index = 0
    }

    @ViewBuilder
    private var content: some View {
        if let card {
            ScrollView {
                VStack(spacing: UwUSpacing.md) {
                    Text(features: hintKey)
                        .uwuFont(.footnote)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .center)
                    stack(card)
                    pager
                }
                .padding(UwUSpacing.md)
                .frame(maxWidth: 560)
                .frame(maxWidth: .infinity)
            }
            .scrollBounceBehavior(.basedOnSize)
        } else {
            FeatureEmptyState(
                mood: .sparkle,
                title: Text(features: "Nichts mehr durchzugehen ✧ Ignoriertes findest du im Bericht."))
            {
                Button { dismiss() } label: { Text(features: "Zum Bericht") }
                    .uwuGlassButton()
            }
        }
    }

    private var hintKey: LocalizedStringKey {
        #if os(iOS)
        sizeClass == .compact ? "Zum Blättern wischen." : "Wischen oder Pfeiltasten ← → blättern."
        #else
        "Wischen oder Pfeiltasten ← → blättern."
        #endif
    }

    // MARK: The stack

    private func stack(_ card: HealthCard) -> some View {
        ZStack {
            if index + 1 < total {
                RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous)
                    .fill(.clear)
                    .uwuGlass(in: RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous))
                    .padding(.horizontal, UwUSpacing.md)
                    .offset(y: UwUSpacing.xs)
                    .opacity(0.6)
                    .accessibilityHidden(true)
            }
            HealthReviewCard(
                card: card, store: store, openItem: openItem,
                onGenerate: { generatingFor = card },
                onLater: later)
                .id(card.finding.id)
                .offset(x: dragX)
                .rotationEffect(.degrees(noMotion ? 0 : Double(dragX / 40)))
                .transition(noMotion ? .opacity : .asymmetric(
                    insertion: .move(edge: direction > 0 ? .trailing : .leading).combined(with: .opacity),
                    removal: .move(edge: direction > 0 ? .leading : .trailing).combined(with: .opacity)))
                .simultaneousGesture(drag)
                .accessibilityAdjustableAction { move in
                    switch move {
                    case .increment: go(1)
                    case .decrement: go(-1)
                    @unknown default: break
                    }
                }
        }
    }

    /// Sideways drags only (review.ts `dragStarts`), so the page still scrolls.
    private var drag: some Gesture {
        DragGesture(minimumDistance: 10)
            .onChanged { value in
                let dx = value.translation.width
                guard abs(dx) > abs(value.translation.height) || dragX != 0 else { return }
                dragX = dx
            }
            .onEnded { value in
                let dx = value.translation.width
                let dragging = dragX != 0
                // To the left is the next card (as on a pile of cards), to the right the previous.
                let delta = dx <= -healthSwipe ? 1 : dx >= healthSwipe ? -1 : 0
                withUwUAnimation(.uwuSpring(), reduceMotion: noMotion) {
                    dragX = 0
                }
                if dragging && delta != 0 { go(delta) }
            }
    }

    private var pager: some View {
        HStack {
            Button { go(-1) } label: {
                Label(features: "Zurück", uwuIcon: .previous)
            }
            .disabled(index == 0)
            .keyboardShortcut(.leftArrow, modifiers: [])
            Spacer()
            Button { go(1) } label: {
                Label {
                    Text(features: "Weiter")
                } icon: {
                    UwUIcon(.next)
                }
                .labelStyle(HealthTrailingIconLabelStyle())
            }
            .disabled(index + 1 >= total)
            .keyboardShortcut(.rightArrow, modifiers: [])
        }
        .uwuGlassButton()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(features: "Karten"))
    }

    /// The card `delta` away, never past either end (review.ts `step`).
    private func go(_ delta: Int) {
        let next = min(max(index + delta, 0), max(total - 1, 0))
        guard next != index else { return }
        direction = delta
        withUwUAnimation(.uwuOut(), reduceMotion: noMotion) {
            index = next
        }
    }

    /// "Später": the card leaves the stack for this session; the one after it takes its place.
    private func later() {
        guard let card else { return }
        HealthLater.ids.insert(card.finding.id)
        direction = 1
        withUwUAnimation(.uwuOut(), reduceMotion: noMotion) {
            cards.remove(at: index)
            index = min(max(index, 0), max(cards.count - 1, 0))
        }
    }
}

// MARK: One card

private struct HealthReviewCard: View {
    let card: HealthCard
    let store: HealthCheckStore
    let openItem: PasswordCheckOpenItem?
    let onGenerate: () -> Void
    let onLater: () -> Void

    var body: some View {
        let finding = card.finding
        let renewed = store.renewed.contains(finding.id)
        VStack(alignment: .leading, spacing: UwUSpacing.md) {
            HStack(spacing: UwUSpacing.sm) {
                UwUIcon(.website, size: .xl)
                    .foregroundStyle(Color.uwuPinkInk)
                    .frame(width: 48, height: 48)
                    .background(Color.uwuPinkTint, in: RoundedRectangle(cornerRadius: UwURadius.control, style: .continuous))
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: HealthWords.name(finding.name))
                        .uwuFont(.title3, weight: .bold)
                        .lineLimit(2)
                    let line = [finding.subtitle, finding.host].compactMap { $0 }.filter { !$0.isEmpty }
                    if !line.isEmpty {
                        Text(verbatim: line.joined(separator: " · "))
                            .uwuFont(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }

            VStack(spacing: 0) {
                ForEach(Array(card.problems.enumerated()), id: \.offset) { offset, problem in
                    if offset > 0 { Divider() }
                    problemRow(problem, renewed: renewed)
                }
            }

            if renewed {
                Label(features: "Neues Passwort gespeichert – jetzt noch auf der Website ändern.", uwuIcon: .success, size: .sm)
                    .uwuFont(.footnote)
                    .foregroundStyle(Color.uwuSuccessInk)
            }

            UwUGlassContainer(spacing: UwUSpacing.xs) {
                VStack(spacing: UwUSpacing.xs) {
                    if finding.host != nil || finding.uri != nil {
                        Button {
                            Task { await store.openChangePage(finding.id) }
                        } label: {
                            Label(features: "Seite öffnen & Passwort ändern", uwuIcon: .openExternal)
                                .frame(maxWidth: .infinity)
                        }
                        .uwuProminentButton()
                        .foregroundStyle(Color.uwuOnPink)
                        .keyboardShortcut(.defaultAction)
                    }
                    Button(action: onGenerate) {
                        Label(features: "Neues Passwort erzeugen & speichern", uwuIcon: .generate)
                            .frame(maxWidth: .infinity)
                    }
                    .uwuGlassButton()
                    HStack(spacing: UwUSpacing.xs) {
                        Button(action: onLater) {
                            Label(features: "Später", uwuIcon: .pending)
                                .frame(maxWidth: .infinity)
                        }
                        .uwuGlassButton()
                        if let openItem {
                            Button { openItem(finding.id) } label: {
                                Label(features: "Eintrag öffnen", uwuIcon: .vault)
                                    .frame(maxWidth: .infinity)
                            }
                            .uwuGlassButton()
                        }
                    }
                }
                .controlSize(.large)
            }
        }
        .featureCard()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(verbatim: HealthWords.name(finding.name)))
        .accessibilityHint(Text(features: "Karte"))
    }

    @ViewBuilder
    private func problemRow(_ problem: HealthProblem, renewed: Bool) -> some View {
        let kind = problem.kind
        let ignored = store.view?.isIgnored(card.finding.id, kind) ?? false
        let solved = renewed && kind.isAboutThePassword
        HStack(alignment: .firstTextBaseline, spacing: UwUSpacing.sm) {
            UwUIcon(solved ? .done : ignored ? .hide : .warning, size: .sm)
                .foregroundStyle(solved ? Color.uwuSuccessInk : ignored ? Color.uwuMuted : Color.uwuDangerInk)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: kind.healthTitle)
                    .uwuFont(.body, weight: .semibold)
                    .strikethrough(solved || ignored, color: .secondary)
                Group {
                    if solved {
                        Text(features: "Neues Passwort gespeichert")
                    } else if ignored {
                        Text(features: "Ignoriert")
                    } else {
                        Text(verbatim: HealthWords.detail(problem))
                    }
                }
                .uwuFont(.subheadline)
                .foregroundStyle(.secondary)
                if case .twofa(let documentation) = problem, !solved, !ignored,
                   let documentation, let url = URL(string: documentation)
                {
                    Link(destination: url) {
                        Label(features: "Anleitung zur Zwei-Schritt-Anmeldung", uwuIcon: .link, size: .sm)
                    }
                    .uwuFont(.subheadline, weight: .medium)
                }
            }
            Spacer(minLength: 0)
            if !solved, store.view?.ignored != nil {
                Button {
                    Task { await store.setIgnored(card.finding.id, kind, !ignored) }
                } label: {
                    if ignored { Text(features: "Rückgängig") } else { Text(features: "Ignorieren") }
                }
                .buttonStyle(.borderless)
                .uwuFont(.subheadline, weight: .medium)
                .accessibilityLabel(Text(verbatim: "\(ignored ? featureString("Rückgängig") : featureString("Ignorieren")): \(kind.healthTitle)"))
                .sensoryFeedback(.impact(weight: .light), trigger: ignored)
            }
        }
        .padding(.vertical, UwUSpacing.xs)
        .uwuAnimation(value: ignored)
    }
}

/// "Weiter ›": the icon after the title.
private struct HealthTrailingIconLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: UwUSpacing.xxs) {
            configuration.title
            configuration.icon
        }
    }
}

#if DEBUG
#Preview("Review") {
    HealthReviewView(store: .preview())
}
#endif
