// A Wi-Fi item's extras inside its detail (lib/wifi.ts, WifiShare.tsx, WifiConnect.tsx,
// QrCode.tsx): sharing as a `WIFI:` QR code, and on iPhone and iPad joining it through the
// system. The detail itself shows the network's fields.
import SwiftUI
import UwUDesign

struct WifiItemSection: View {
    let itemId: String

    @Environment(AppModel.self) private var model
    @Environment(VaultModel.self) private var vault: VaultModel?
    @State private var network: WifiNetwork?
    @State private var sharing = false

    private var store: UwuStore { UwuStore.shared }
    private var backend: WifiBackend { WifiBackend(core: model.core, vault: vault?.backend) }

    /// Rows for the item detail's Wi-Fi section (the detail shows the network's fields):
    /// sharing as a QR code and, on iPhone and iPad, joining the network.
    @ViewBuilder
    var body: some View {
        // The share row is always there (disabled while loading): a List drops empty rows, and
        // with them their `.task`.
        Button {
            sharing = true
        } label: {
            Label(features: "Als QR-Code teilen", uwuIcon: .qrCode)
        }
        .disabled(network?.ssid.trimmingCharacters(in: .whitespaces).isEmpty ?? true)
        #if os(macOS)
        .help(Text(features: "Als QR-Code teilen"))
        #endif
        .task(id: "\(itemId)#\(store.vaultChanges)") { await load() }
        .sheet(isPresented: $sharing) {
            if let network {
                WifiShareSheet(itemId: itemId, network: network, backend: backend)
            }
        }
        #if os(iOS)
        if let network {
            WifiConnectRow(itemId: itemId, network: network, backend: backend)
        }
        #endif
    }

    private func load() async {
        store.attach(model.core)
        do {
            let detail = try await backend.item(itemId)
            network = WifiNetwork(fields: detail.fields ?? [])
        } catch {
            FeatureToasts.shared.show(error: error)
        }
    }
}

// MARK: Share

/// The network as a QR code to join with a camera (WifiShare.tsx). The password is revealed
/// for the code when the sheet opens and forgotten when it closes.
private struct WifiShareSheet: View {
    let itemId: String
    let network: WifiNetwork
    let backend: WifiBackend

    @Environment(\.dismiss) private var dismiss
    @State private var password: String?
    @State private var shown = false
    @State private var error: String?

    private var noName: Bool { network.ssid.trimmingCharacters(in: .whitespaces).isEmpty }
    private var code: String? {
        guard !noName else { return nil }
        if !network.open, network.hasPassword, password == nil { return nil }
        return network.qr(password: password ?? "")
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: UwUSpacing.lg) {
                    Text(features: network.open
                        ? "Mit der Kamera scannen, um „\(network.ssid)“ beizutreten."
                        : "Mit der Kamera scannen, um „\(network.ssid)“ beizutreten. Wer den Code sieht, kennt das Passwort.")
                        .uwuFont(.body)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)

                    if let error {
                        Text(verbatim: error).foregroundStyle(Color.uwuDangerInk)
                    }
                    if noName {
                        Text(features: "Ohne Netzwerknamen (SSID) gibt es keinen Code. Trag ihn im Eintrag ein.")
                            .foregroundStyle(Color.uwuDangerInk)
                    } else if let code {
                        WifiQRCode(text: code, label: Text(features: "QR-Code für das WLAN \(network.ssid)"))
                    } else if error == nil {
                        ProgressView().frame(height: 200)
                    }

                    details
                        .featureCard()
                }
                .padding(UwUSpacing.lg)
            }
            .navigationTitle(Text(features: "WLAN teilen"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button { dismiss() } label: { Text(features: "Fertig") }
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        #if os(iOS)
        .presentationDetents([.large])
        #else
        .frame(minWidth: 420, idealWidth: 460, minHeight: 600)
        #endif
        .task { await reveal() }
        .onDisappear {
            password = nil
            shown = false
        }
    }

    private var details: some View {
        Grid(alignment: .leading, horizontalSpacing: UwUSpacing.md, verticalSpacing: UwUSpacing.sm) {
            GridRow {
                Text(features: "Netzwerkname (SSID)").foregroundStyle(Color.uwuMuted)
                Text(verbatim: network.ssid.isEmpty ? "—" : network.ssid).monospaced().textSelection(.enabled)
            }
            GridRow {
                Text(features: "Sicherheit").foregroundStyle(Color.uwuMuted)
                Text(verbatim: network.securityLabel)
            }
            if !network.open {
                GridRow {
                    Text(features: "Passwort").foregroundStyle(Color.uwuMuted)
                    HStack(spacing: UwUSpacing.xs) {
                        Text(verbatim: password == nil ? "…" : shown ? (password!.isEmpty ? "—" : password!) : "••••••••••••")
                            .monospaced()
                            .textSelection(.enabled)
                        if let password, !password.isEmpty {
                            Button {
                                shown.toggle()
                            } label: {
                                UwUIcon(shown ? .hide : .show, size: .sm,
                                        label: shown ? Text(features: "Passwort verbergen") : Text(features: "Passwort zeigen"))
                            }
                            .buttonStyle(.borderless)
                            #if os(macOS)
                            .help(shown ? Text(features: "Passwort verbergen") : Text(features: "Passwort zeigen"))
                            #endif
                        }
                    }
                }
            }
        }
        .uwuFont(.subheadline)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func reveal() async {
        guard !network.open else { password = ""; return }
        guard network.hasPassword, let field = network.passwordField else {
            password = ""
            return
        }
        do {
            password = try await backend.reveal(itemId, field: field)
        } catch {
            self.error = Core.failure(error).message
        }
    }
}

// MARK: Connect

#if os(iOS)
/// Joining the network through the system's sheet (NEHotspotConfiguration), and where it can't,
/// why — with the password copied for the Wi-Fi settings.
private struct WifiConnectRow: View {
    let itemId: String
    let network: WifiNetwork
    let backend: WifiBackend

    @State private var busy = false
    @State private var problem: WifiJoined?
    @State private var joined = 0

    var body: some View {
        Button {
            connect()
        } label: {
            Label(features: "Verbinden", uwuIcon: .wifi)
        }
        .disabled(busy || network.ssid.isEmpty)
        .sensoryFeedback(.success, trigger: joined)

        if let problem, let explanation = explain(problem) {
            VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                Text(verbatim: explanation)
                    .uwuFont(.subheadline)
                    .foregroundStyle(Color.uwuWarningInk)
                if problem.reason != "no-ssid", network.hasPassword {
                    Button {
                        copyPassword()
                    } label: {
                        Label(features: "Passwort kopieren", uwuIcon: .copy)
                    }
                    .uwuGlassButton()
                }
            }
            .padding(.vertical, UwUSpacing.xxs)
            .accessibilityElement(children: .contain)
        }
    }

    private func connect() {
        busy = true
        problem = nil
        let ssid = network.ssid
        Task {
            defer { busy = false }
            do {
                let answer = try await backend.connect(itemId)
                switch answer.outcome {
                case .saved:
                    joined += 1
                    FeatureToasts.shared.show(featureString("„\(ssid)“ ist gespeichert. Das Gerät verbindet sich, sobald das WLAN in Reichweite ist."))
                case .alreadySaved:
                    FeatureToasts.shared.show(featureString("„\(ssid)“ ist auf diesem Gerät schon gespeichert."))
                case .suggested:
                    FeatureToasts.shared.show(featureString("UwULock schlägt „\(ssid)“ vor. Erlaube es, dann verbindet sich das Gerät, sobald das WLAN in Reichweite ist."))
                case .declined:
                    FeatureToasts.shared.show(featureString("Nicht hinzugefügt – du hast es abgelehnt."))
                case .disallowed, .unsupported, .failed:
                    problem = answer
                }
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }

    private func explain(_ problem: WifiJoined) -> String? {
        switch problem.outcome {
        case .unsupported: Self.reason(problem.reason)
        case .disallowed:
            featureString("Das System lässt UwULock keine WLANs hinzufügen. Füge das WLAN in den WLAN-Einstellungen selbst hinzu.")
        case .failed:
            featureString("Das System hat das WLAN nicht übernommen: \(problem.message ?? "—")")
        default: nil
        }
    }

    static func reason(_ reason: String?) -> String {
        switch reason {
        case "wep":
            featureString("Apps können hier keine WEP-Netze hinzufügen. Füge es in den WLAN-Einstellungen selbst hinzu – und stell den Router besser auf WPA2 oder WPA3 um, WEP ist unsicher.")
        case "eap-tls":
            featureString("EAP-TLS braucht ein Client-Zertifikat, das UwULock nicht hat. Richte das WLAN mit dem Zertifikat in den WLAN-Einstellungen ein.")
        case "eap":
            featureString("UwULock übergibt Enterprise-WLANs mit PEAP, TTLS oder PWD. Wähle die EAP-Methode im Eintrag.")
        case "ca-domain":
            featureString("Bei PEAP und TTLS wird der Server geprüft. UwULock kann das nur über eine Domain im Feld CA-Zertifikat (etwa radius.example.org) und die Zertifikate des Systems. Trag die Domain ein oder richte das WLAN in den WLAN-Einstellungen ein.")
        case "identity":
            featureString("Für ein Enterprise-WLAN fehlt die Identität im Eintrag.")
        case "no-password":
            featureString("Im Eintrag fehlt das Passwort für dieses WLAN.")
        case "password":
            featureString("Es gehen nur WPA-Passwörter mit 8 bis 63 einfachen Zeichen (ASCII, keine Umlaute). Füge das WLAN in den WLAN-Einstellungen selbst hinzu.")
        case "no-ssid":
            featureString("Ohne Netzwerknamen (SSID) gibt es nichts zu verbinden. Trag ihn im Eintrag ein.")
        default:
            featureString("Dieses Gerät kann WLANs nicht aus einer App übernehmen.")
        }
    }

    private func copyPassword() {
        guard let field = network.passwordField else { return }
        Task {
            do {
                try await backend.copy(itemId, field: field)
                FeatureToasts.shared.show(featureString("Passwort kopiert ✧"))
                _ = await backend.settings()
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }
}
#endif

#if DEBUG
#Preview("Wi-Fi") {
    Form {
        Section { WifiItemSection(itemId: WifiFixtures.itemId) }
    }
    .formStyle(.grouped)
    .featureToasts()
    .environment(AppModel())
}
#endif
