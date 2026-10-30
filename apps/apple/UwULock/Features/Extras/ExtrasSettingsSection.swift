// Settings → the UwU extras (SettingsDialog.tsx `EmailBreachSetting`, mobile/pages/Settings.tsx,
// ExtrasKeyNotice.tsx, TravelBadge.tsx): what the server offers, a changed extras key, travel
// mode, the web vault's pages for keys and organizations, and the e-mail breach check opt-in.
import SwiftUI
import UwUDesign

struct ExtrasSettingsSection: View {
    @Environment(AppModel.self) private var model
    @State private var optIn: EmailOptIn?
    @State private var optInBusy = false
    @State private var travelHidden: Int?
    @State private var keySeen = false

    private var store: UwuStore { UwuStore.shared }
    private var status: UwuStatus { store.status }
    private var backend: ExtrasBackend { ExtrasBackend(core: model.core) }

    var body: some View {
        extras
            .task(id: store.statusChanges) {
                store.attach(model.core)
                optIn = await backend.emailOptIn()
            }
        if status.uwu, status.has(.travelMode) {
            travel
                .task(id: status.travel.enabled) {
                    if status.travel.enabled, let answer = await backend.travel() {
                        travelHidden = answer.hiddenCount
                    }
                }
        }
        if let optIn {
            emailSection(optIn)
        }
    }

    // MARK: Extras

    private var extras: some View {
        Section {
            if status.uwu {
                LabeledContent {
                    Text(features: "An")
                        .foregroundStyle(Color.uwuSuccessInk)
                } label: {
                    Label(features: "UwU-Extras dieses Servers", uwuIcon: .sync)
                }
                if !status.features.isEmpty {
                    VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                        Text(features: "Was er bietet")
                            .uwuFont(.caption, weight: .semibold)
                            .foregroundStyle(Color.uwuMuted)
                        Text(verbatim: status.features.map(Self.featureName).joined(separator: " · "))
                            .uwuFont(.subheadline)
                    }
                    .accessibilityElement(children: .combine)
                }
                if status.extrasKeyChanged && !keySeen {
                    keyNotice
                }
                Button {
                    open(.keys)
                } label: {
                    Label(features: "Schlüssel im Web-Tresor", uwuIcon: .securityKey)
                }
                .help(Text(features: "Im Web-Tresor öffnen"))
                ForEach(status.organizations, id: \.id) { organization in
                    Button {
                        open(.organization, id: organization.id)
                    } label: {
                        Label {
                            Text(features: "\(organization.name) im Web-Tresor verwalten")
                        } icon: {
                            UwUIcon(.organization)
                        }
                    }
                    .help(Text(features: "Im Web-Tresor öffnen"))
                }
            } else {
                LabeledContent {
                    Text(features: "Aus")
                        .foregroundStyle(Color.uwuMuted)
                } label: {
                    Label(features: "UwU-Extras dieses Servers", uwuIcon: .sync)
                }
            }
        } header: {
            Text(features: "UwU-Extras")
        } footer: {
            if !status.uwu {
                Text(features: "Eigene Icons, Versionen, Erinnerungen, Reisemodus und mehr gibt es mit UwULock Server. Bitwarden und Vaultwarden bieten sie nicht.")
            }
        }
    }

    private var keyNotice: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.xs) {
            Label {
                Text(features: "Neuer Schlüssel für die Extras").uwuFont(.body, weight: .semibold)
            } icon: {
                UwUIcon(.warning).foregroundStyle(Color.uwuWarningInk)
            }
            ExtrasKeyNoticeText.lead.uwuFont(.subheadline)
            ExtrasKeyNoticeText.small.uwuFont(.caption).foregroundStyle(Color.uwuMuted)
            HStack(spacing: UwUSpacing.sm) {
                Button {
                    open(.keys)
                } label: {
                    Text(features: "Im Web-Tresor öffnen")
                }
                .uwuGlassButton()
                Button {
                    seeKey()
                } label: {
                    Text(features: "Verstanden")
                }
                .uwuProminentButton()
                .foregroundStyle(Color.uwuOnPink)
            }
            .padding(.top, UwUSpacing.xxs)
        }
        .padding(.vertical, UwUSpacing.xxs)
        .listRowBackground(Color.uwuWarningTint)
    }

    // MARK: Travel mode

    private var travel: some View {
        Section {
            LabeledContent {
                Text(verbatim: travelState)
                    .foregroundStyle(status.travel.enabled ? Color.uwuWarningInk : Color.uwuMuted)
            } label: {
                Label(features: "Reisemodus", uwuIcon: .travelMode)
            }
            Button {
                open(.travel)
            } label: {
                Label(
                    features: status.travel.enabled ? "Im Web-Tresor ausschalten" : "Im Web-Tresor einrichten",
                    uwuIcon: .openExternal)
            }
        } footer: {
            if status.travel.enabled {
                TravelModeText.explain(hidden: status.travel.hiddenCount ?? travelHidden)
            } else {
                Text(features: "Ordner, die du fürs Reisen markierst, sind dann auf allen Geräten ausgeblendet. Ein- und ausschalten geht im Web-Tresor, mit deinem Master-Passwort und dem zweiten Faktor.")
            }
        }
    }

    private var travelState: String {
        guard status.travel.enabled else { return featureString("Aus") }
        guard let n = status.travel.hiddenCount ?? travelHidden else { return featureString("An") }
        return n == 1 ? featureString("An · 1 verborgen") : featureString("An · \(n) verborgen")
    }

    // MARK: E-mail breach check

    private func emailSection(_ optIn: EmailOptIn) -> some View {
        Section {
            Toggle(isOn: Binding(get: { optIn.optedIn }, set: { change($0) })) {
                Label(features: "Adressen in Datenlecks prüfen", uwuIcon: .mail)
            }
            .disabled(optInBusy)
        } footer: {
            Text(features: "Die Passwortprüfung fragt dann auch, ob deine Kontoadresse und die Adressen, die in Logins als Benutzername stehen, in Datenlecks auftauchen. Dafür schickt dein Server jede dieser Adressen im Klartext an XposedOrNot (xposedornot.com) – deine Passwörter und deine anderen Daten nicht. Die Antworten merkt er sich eine Woche lang, nur unter einem Hash der Adresse.")
        }
    }

    // MARK: Actions

    private func change(_ on: Bool) {
        guard !optInBusy else { return }
        optInBusy = true
        Task {
            defer { optInBusy = false }
            do {
                optIn = try await backend.setEmailOptIn(on)
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }

    private func open(_ place: WebVaultPlace, id: String? = nil) {
        Task {
            do {
                try await backend.openWebVault(place, id: id)
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }

    private func seeKey() {
        keySeen = true
        Task {
            do {
                try await backend.extrasKeySeen()
            } catch {
                keySeen = false
                FeatureToasts.shared.show(error: error)
            }
        }
    }

    /// The German name of an extra the server offers (its own word when unknown).
    static func featureName(_ raw: String) -> String {
        switch UwuFeature(rawValue: raw) {
        case .icons: featureString("Icons")
        case .ownIcons: featureString("Eigene Icons")
        case .iconLibrary: featureString("Icon-Bibliothek")
        case .versions: featureString("Versionen")
        case .travelMode: featureString("Reisemodus")
        case .reminders: featureString("Erinnerungen")
        case .fileRequests: featureString("Dateianfragen")
        case .maskedAddresses: featureString("Maskierte Adressen")
        case .sendDomains: featureString("Eigene Send-Domains")
        case .sendEmails: featureString("Sends per E-Mail")
        case .sends: featureString("Sends")
        case .suite: featureString("UwUSSH und UwURDP")
        case nil: raw
        }
    }
}

#if DEBUG
#Preview("Extras settings") {
    Form {
        ExtrasSettingsSection()
    }
    .formStyle(.grouped)
    .featureToasts()
    .environment(AppModel())
}
#endif
