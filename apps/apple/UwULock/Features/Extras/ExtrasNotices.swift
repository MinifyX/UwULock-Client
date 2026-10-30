// Pieces for the vault's root: the notice that the extras key changed (ExtrasKeyNotice.tsx) and
// the travel mode badge with its dialog (TravelBadge.tsx).
import SwiftUI
import UwUDesign

extension View {
    /// Shows the "new key for the extras" notice while `status.extrasKeyChanged` (once per
    /// change; "Verstanden" or dismissing marks it seen via `extras_key_seen`).
    func uwuExtrasNotices() -> some View {
        modifier(ExtrasNoticesModifier())
    }
}

private struct ExtrasNoticesModifier: ViewModifier {
    @Environment(AppModel.self) private var model
    @State private var seen = false

    private var store: UwuStore { UwuStore.shared }
    private var backend: ExtrasBackend { ExtrasBackend(core: model.core) }

    func body(content: Content) -> some View {
        content
            .task { store.attach(model.core) }
            .onChange(of: store.status.extrasKeyChanged) { _, changed in
                if changed { seen = false }
            }
            .alert(
                Text(features: "Neuer Schlüssel für die Extras"),
                isPresented: Binding(
                    get: { store.status.extrasKeyChanged && !seen },
                    set: { shown in if !shown { acknowledge() } })
            ) {
                Button {
                    Task {
                        try? await backend.openWebVault(.keys)
                    }
                    acknowledge()
                } label: {
                    Text(features: "Im Web-Tresor öffnen")
                }
                Button(role: .cancel) {
                    acknowledge()
                } label: {
                    Text(features: "Verstanden")
                }
            } message: {
                ExtrasKeyNoticeText.full
            }
    }

    private func acknowledge() {
        guard !seen else { return }
        seen = true
        Task {
            do {
                try await backend.extrasKeySeen()
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }
}

/// The notice's words, shared by the alert and the settings row.
enum ExtrasKeyNoticeText {
    static var lead: Text {
        Text(features: "Der Schlüssel für UwULocks Extras ist ein anderer als beim letzten Mal auf diesem Gerät. Was unter dem alten lag – eigene Icons, Links von Dateianfragen, Hosts von UwUSSH und UwURDP – lässt sich nicht mehr öffnen.")
    }

    static var small: Text {
        Text(features: "Das ist in Ordnung, wenn du (oder jemand mit deinem Master-Passwort) im Web-Tresor neu angefangen hast. Wenn nicht, hat der Server den alten Schlüssel verloren oder entfernt – sag dem Betreiber Bescheid.")
    }

    static var full: Text {
        Text(verbatim: "\(featureString("Der Schlüssel für UwULocks Extras ist ein anderer als beim letzten Mal auf diesem Gerät. Was unter dem alten lag – eigene Icons, Links von Dateianfragen, Hosts von UwUSSH und UwURDP – lässt sich nicht mehr öffnen."))\n\n\(featureString("Das ist in Ordnung, wenn du (oder jemand mit deinem Master-Passwort) im Web-Tresor neu angefangen hast. Wenn nicht, hat der Server den alten Schlüssel verloren oder entfernt – sag dem Betreiber Bescheid."))")
    }
}

// MARK: Travel mode badge

/// A pill while travel mode is on ("Reisemodus · 3 verborgen"); tapping it explains and offers
/// to switch it off in the web vault. Nothing while it is off. Fits a toolbar.
struct TravelModeBadge: View {
    @Environment(AppModel.self) private var model
    @State private var open = false
    @State private var hidden: Int?

    private var store: UwuStore { UwuStore.shared }
    private var travel: UwuTravel { store.status.travel }
    private var count: Int? { travel.hiddenCount ?? hidden }

    var body: some View {
        if travel.enabled {
            Button {
                open = true
            } label: {
                Label {
                    title
                } icon: {
                    UwUIcon(.travelMode, size: .sm)
                }
                .uwuFont(.subheadline, weight: .semibold)
                .foregroundStyle(Color.uwuWarningInk)
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, UwUSpacing.sm)
            .padding(.vertical, UwUSpacing.xxs + 2)
            .uwuGlass(in: Capsule(), interactive: true)
            .help(Text(features: "Reisemodus ist an"))
            .accessibilityLabel(title)
            .accessibilityHint(Text(features: "Reisemodus ist an"))
            .task(id: travel.enabled) {
                let backend = ExtrasBackend(core: model.core)
                if let answer = await backend.travel() { hidden = answer.hiddenCount }
            }
            .alert(Text(features: "Reisemodus ist an"), isPresented: $open) {
                Button {
                    Task {
                        do {
                            try await ExtrasBackend(core: model.core).openWebVault(.travel)
                        } catch {
                            FeatureToasts.shared.show(error: error)
                        }
                    }
                } label: {
                    Text(features: "Im Web-Tresor ausschalten")
                }
                Button(role: .cancel) {} label: { Text(features: "Schließen") }
            } message: {
                TravelModeText.explain(hidden: count)
            }
        }
    }

    private var title: Text {
        guard let count else { return Text(features: "Reisemodus") }
        return count == 1 ? Text(features: "Reisemodus · 1 verborgen") : Text(features: "Reisemodus · \(count) verborgen")
    }
}

enum TravelModeText {
    static func explain(hidden: Int?) -> Text {
        let lead = if let hidden {
            featureString("\(hidden) Einträge aus den Ordnern, die du fürs Reisen markiert hast, sind auf allen Geräten ausgeblendet – auch hier. Sie sind nicht weg: Nach dem Ausschalten kommen sie mit der nächsten Synchronisierung zurück.")
        } else {
            featureString("Die Einträge aus den Ordnern, die du fürs Reisen markiert hast, sind auf allen Geräten ausgeblendet – auch hier. Nach dem Ausschalten kommen sie mit der nächsten Synchronisierung zurück.")
        }
        let off = featureString("Ausschalten geht im Web-Tresor, mit deinem Master-Passwort und dem zweiten Faktor.")
        return Text(verbatim: "\(lead)\n\n\(off)")
    }
}

#if DEBUG
#Preview("Travel badge") {
    NavigationStack {
        Text(verbatim: "Tresor")
            .toolbar { ToolbarItem { TravelModeBadge() } }
    }
    .uwuExtrasNotices()
    .environment(AppModel())
}
#endif
