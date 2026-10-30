import SwiftUI
import UwUDesign

/// A Send's own page (SendsDialog.tsx `SendDetail`, mobile/pages/Sends.tsx `SendPage`): the link
/// to copy or share, what it carries, how long it lives, who may open it.
struct SendsDetailView: View {
    @Environment(AppModel.self) private var model
    let send: Send
    let store: SendsStore
    let onEdit: () -> Void
    let onDelete: () -> Void
    @State private var busy = false

    var body: some View {
        Form {
            Section { header }

            Section {
                Text(verbatim: send.link)
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .lineLimit(4)
                    .foregroundStyle(Color.uwuInk)
                UwUGlassContainer(spacing: UwUSpacing.xs) {
                    HStack {
                        Button {
                            Task { await SendsLink.copy(model.core, send.link) }
                        } label: {
                            Label(features: "Link kopieren", uwuIcon: .copy)
                        }
                        .uwuProminentButton()
                        .foregroundStyle(Color.uwuOnPink)
                        .keyboardShortcut("c", modifiers: [.command, .shift])
                        .help(Text(features: "Link kopieren"))
                        if let url = URL(string: send.link) {
                            ShareLink(item: url) {
                                Label(features: "Teilen", uwuIcon: .share)
                            }
                            .uwuGlassButton()
                        }
                    }
                }
            } header: {
                Text(features: "Link")
            }

            if send.kind == .text {
                Section {
                    Text(verbatim: send.text ?? "")
                        .textSelection(.enabled)
                } header: {
                    send.entry ? Text(features: "Text · als Eintrag geteilt") : Text(features: "Text")
                }
            } else {
                Section {
                    LabeledContent {
                        if let size = send.size { Text(verbatim: size.fileSize) }
                    } label: {
                        Label { Text(verbatim: send.fileName ?? "") } icon: { UwUIcon(.file, size: .sm) }
                    }
                } header: {
                    Text(features: "Datei")
                }
            }

            if let notes = send.notes, !notes.isEmpty {
                Section {
                    Text(verbatim: notes).textSelection(.enabled)
                } header: {
                    Text(features: "Notizen (nur für dich)")
                }
            }

            Section {
                LabeledContent {
                    if case .expired = send.status {
                        Text(features: "abgelaufen")
                    } else if let expires = FeatureDate.short(send.expirationDate) {
                        Text(verbatim: expires)
                    } else {
                        Text(features: "Nie (bis zum Löschen)")
                    }
                } label: { Text(features: "Läuft ab") }
                LabeledContent {
                    Text(verbatim: FeatureDate.short(send.deletionDate) ?? "–")
                } label: { Text(features: "Wird gelöscht") }
                LabeledContent {
                    if let max = send.maxAccessCount, max > 0 {
                        Text(verbatim: "\(max)")
                    } else {
                        Text(features: "unbegrenzt")
                    }
                } label: { Text(features: "Höchstens so oft öffnen") }
            } header: {
                Text(features: "Gültigkeit")
            }

            Section {
                LabeledContent {
                    Text(verbatim: send.accessText)
                } label: { Text(features: "Wer darf öffnen?") }
                if send.authType != .anyone {
                    Button {
                        busy = true
                        Task {
                            await store.removeAuth(model.core, send)
                            busy = false
                        }
                    } label: {
                        send.authType == .emails
                            ? Label(features: "Adressen entfernen", uwuIcon: .unlocked)
                            : Label(features: "Passwort entfernen", uwuIcon: .unlocked)
                    }
                    .disabled(busy)
                }
                LabeledContent {
                    send.hideEmail ? Text(features: "Ja") : Text(features: "Nein")
                } label: { Text(features: "Meine Adresse nicht zeigen") }
                LabeledContent {
                    send.disabled ? Text(features: "Ja") : Text(features: "Nein")
                } label: { Text(features: "Deaktiviert") }
            } footer: {
                if send.authType == .emails {
                    Text(features: "Nur für: \(send.emails.joined(separator: ", "))")
                }
            }

            Section {
                Button(role: .destructive, action: onDelete) {
                    Label(features: "Send löschen", uwuIcon: .delete)
                        .foregroundStyle(Color.uwuDangerInk)
                }
                .disabled(busy)
            }
        }
        .formStyle(.grouped)
    }

    private var header: some View {
        HStack(alignment: .center, spacing: UwUSpacing.md) {
            SendsKindTile(kind: send.kind, large: true)
            VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                Text(verbatim: send.shownName)
                    .uwuFont(.title3, weight: .bold)
                    .lineLimit(2)
                HStack(spacing: UwUSpacing.xxs) {
                    SendsStateChip(status: send.status)
                    if !send.status.isActive {
                        Text(verbatim: send.statusText)
                            .uwuFont(.caption)
                            .foregroundStyle(Color.uwuWarningInk)
                    }
                }
                Text(verbatim: tags.joined(separator: " · "))
                    .uwuFont(.caption)
                    .foregroundStyle(Color.uwuMuted)
            }
            Spacer(minLength: 0)
            // Icon only, so the name keeps the room on the phone.
            Button(action: onEdit) { Label(features: "Bearbeiten", uwuIcon: .edit) }
                .labelStyle(.iconOnly)
                .uwuGlassButton()
                .keyboardShortcut("e", modifiers: .command)
                .help(Text(features: "Bearbeiten"))
        }
    }

    /// Its kind, how often it was opened and how it is protected, as the desktop's tags.
    private var tags: [String] {
        var out = [send.kind == .file ? featureString("Datei") : featureString("Text"), send.openedText]
        if send.authType == .password { out.append(featureString("Mit Passwort")) }
        if send.authType == .emails { out.append(featureString("Nur bestimmte Adressen")) }
        if send.hideEmail { out.append(featureString("Ohne deine Adresse")) }
        return out
    }
}

#if DEBUG
#Preview("Send") {
    UserDefaults.standard.set(true, forKey: "UwULockFixtures")
    return NavigationStack {
        SendsDetailView(send: SendsFixtures.sends[1], store: SendsStore(), onEdit: {}, onDelete: {})
    }
    .environment(AppModel())
}
#endif
