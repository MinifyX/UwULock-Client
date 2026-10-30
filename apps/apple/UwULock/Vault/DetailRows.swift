import SwiftUI
import UwUDesign

/// A labelled value with an optional copy button (ItemDetail.tsx's Row).
struct DetailRow<Value: View>: View {
    let label: String
    var copy: (() -> Void)?
    @ViewBuilder var value: Value

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: UwUSpacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(label)
                    .uwuFont(.caption)
                    .foregroundStyle(.secondary)
                value
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            if let copy {
                CopyButton(label: vt("\(label) kopieren"), action: copy)
            }
        }
        .accessibilityElement(children: .contain)
    }
}

extension DetailRow where Value == Text {
    init(_ label: String, text: String?, copy: (() -> Void)? = nil) {
        self.label = label
        self.copy = (text?.isEmpty ?? true) ? nil : copy
        self.value = Text(text.flatMap { $0.isEmpty ? nil : $0 } ?? "—")
    }
}

struct CopyButton: View {
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            UwUIcon(.copy, size: .sm, label: Text(label))
        }
        .buttonStyle(.borderless)
        .help(label)
    }
}

/// A secret: dots until shown, then hidden again after 60 s (ItemDetail.tsx's SecretRow). The
/// value is only in memory while it's on screen.
struct SecretRow: View {
    let itemId: String
    let field: String
    let label: String
    var masked = "••••••••••••"
    var colored = true
    var multiline = false
    var copyable = true
    @Environment(VaultModel.self) private var vault
    @State private var shown: String?
    @State private var busy = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: UwUSpacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(label).uwuFont(.caption).foregroundStyle(.secondary)
                Group {
                    if let shown {
                        (colored ? coloredSecret(shown) : Text(shown))
                            .textSelection(.enabled)
                    } else {
                        Text(masked).foregroundStyle(.secondary)
                            .accessibilityLabel(vt("verborgen"))
                    }
                }
                .font(.uwuMono(.body))
                .lineLimit(multiline ? nil : 1)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            Button {
                toggle()
            } label: {
                UwUIcon(shown == nil ? .show : .hide, size: .sm,
                        label: Text(shown == nil ? vt("\(label) zeigen") : vt("\(label) verbergen")))
            }
            .buttonStyle(.borderless)
            .disabled(busy)
            .help(shown == nil ? vt("\(label) zeigen") : vt("\(label) verbergen"))
            if copyable {
                CopyButton(label: vt("\(label) kopieren")) { vault.copy(itemId, field: field) }
            }
        }
        .task(id: shown) {
            guard shown != nil else { return }
            try? await Task.sleep(for: .seconds(60))
            if !Task.isCancelled { shown = nil }
        }
        .onDisappear { shown = nil }
    }

    private func toggle() {
        if shown != nil {
            shown = nil
            return
        }
        busy = true
        Task {
            defer { busy = false }
            do {
                shown = try await vault.backend.revealField(id: itemId, field: field)
            } catch {
                vault.show(error)
            }
        }
    }
}

/// The one-time code with its ring (ItemDetail.tsx's TotpRow): asked for every second.
struct TotpRow: View {
    let itemId: String
    @Environment(VaultModel.self) private var vault
    @State private var code: TotpCode?
    @State private var failed = false

    var body: some View {
        HStack(alignment: .center, spacing: UwUSpacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(vt("Einmal-Code (TOTP)")).uwuFont(.caption).foregroundStyle(.secondary)
                if let code {
                    Text(spacedCode(code.code))
                        .font(.uwuMono(.title3, weight: .medium))
                        .foregroundStyle(code.remaining <= 5 ? Color.uwuWarningInk : Color.uwuInk)
                        .contentTransition(.numericText())
                        .textSelection(.enabled)
                    if code.showNext {
                        HStack(spacing: UwUSpacing.xxs) {
                            Text(vt("Nächster: \(spacedCode(code.next))"))
                                .font(.uwuMono(.caption))
                                .foregroundStyle(.secondary)
                            CopyButton(label: vt("Nächsten Code kopieren")) {
                                vault.copy(itemId, field: "totp-next")
                            }
                        }
                    }
                } else {
                    Text(failed ? "—" : "••• •••").font(.uwuMono(.title3)).foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let code {
                TotpRing(remaining: code.remaining, period: code.period)
            }
            CopyButton(label: vt("Code kopieren")) { vault.copy(itemId, field: "totp") }
        }
        .task {
            while !Task.isCancelled {
                do {
                    code = try await vault.backend.totpCode(id: itemId)
                    failed = false
                } catch {
                    failed = true
                }
                try? await Task.sleep(for: .seconds(1))
            }
        }
        .onDisappear { code = nil }
    }
}

struct TotpRing: View {
    let remaining: Int
    let period: Int
    @ScaledMetric(relativeTo: .body) private var side: CGFloat = 26

    var body: some View {
        let soon = remaining <= 5
        ZStack {
            Circle().stroke(Color.uwuHairline, lineWidth: 3)
            Circle()
                .trim(from: 0, to: CGFloat(remaining) / CGFloat(max(period, 1)))
                .stroke(soon ? Color.uwuWarning : Color.uwuPinkSolid, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .uwuAnimation(.linear(duration: 1), value: remaining)
            Text("\(remaining)")
                .font(.uwuMono(.caption2, weight: .medium))
                .foregroundStyle(soon ? Color.uwuWarningInk : Color.secondary)
        }
        .frame(width: side, height: side)
        .accessibilityElement()
        .accessibilityLabel(vt("Noch \(remaining) Sekunden"))
    }
}

/// "Master-Passwort erforderlich" (ItemDetail.tsx's reprompt card).
struct RepromptCard: View {
    let itemId: String
    let onOpen: () -> Void
    @Environment(VaultModel.self) private var vault
    @State private var password = ""
    @State private var busy = false
    @State private var failure: String?

    var body: some View {
        VStack(spacing: UwUSpacing.md) {
            UwUIcon(.masterPassword, size: .xl).foregroundStyle(.uwuPinkSolid)
            Text(vt("Master-Passwort erforderlich")).uwuFont(.headline)
            Text(vt("Dieser Eintrag ist besonders geschützt. Gib dein Master-Passwort ein, um ihn zu öffnen."))
                .uwuFont(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            SecureField(vt("Master-Passwort"), text: $password)
                .textContentType(.password)
                .textFieldStyle(.roundedBorder)
                .onSubmit(open)
                .frame(maxWidth: 320)
            if let failure {
                Text(failure).uwuFont(.footnote).foregroundStyle(.uwuDangerInk)
            }
            Button(busy ? vt("Prüft …") : vt("Öffnen"), action: open)
                .uwuProminentButton()
                .disabled(busy || password.isEmpty)
        }
        .padding(UwUSpacing.xl)
        .frame(maxWidth: 420)
        .uwuGlass(in: RoundedRectangle(cornerRadius: UwURadius.dialog))
    }

    private func open() {
        guard !password.isEmpty, !busy else { return }
        busy = true
        Task {
            defer { busy = false }
            do {
                try await vault.backend.verifyReprompt(id: itemId, password: password)
                password = ""
                onOpen()
            } catch {
                password = ""
                failure = Core.failure(error).message
            }
        }
    }
}
