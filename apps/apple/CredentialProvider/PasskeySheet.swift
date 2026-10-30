// The AutoFill sheet: what the extension shows while it unlocks, the passkeys and logins to pick
// from, and why it stopped. In UwUDesign's look (tint, UwU Sans, icons, Liquid Glass for the
// controls), with the colours of the app; the controller only changes the model.
import SwiftUI
import UwUDesign

/// The first characters of an id, for the log: enough to tell two apart, nothing to use.
func short(_ data: Data?) -> String {
    guard let data, !data.isEmpty else { return "-" }
    return String(Base64URL.encode(data).prefix(6)) + "…(\(data.count))"
}

/// Folded for searching: no case, no accents.
func folded(_ text: String) -> String {
    text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
}

@MainActor @Observable
final class PasskeyModel {
    struct Choice: Identifiable {
        let id: String
        let title: String
        let subtitle: String
        /// Matches the page or app: listed first, under its own heading.
        let suggested: Bool
        /// A passkey (its own icon), not a login.
        var passkey = false
        /// Title, subtitle and the hosts of its addresses, folded once for the search.
        let haystack: String

        init(
            id: String, title: String, subtitle: String, suggested: Bool, passkey: Bool = false,
            keywords: [String] = []
        ) {
            self.id = id
            self.title = title
            self.subtitle = subtitle
            self.suggested = suggested
            self.passkey = passkey
            haystack = folded(([title, subtitle] + keywords).joined(separator: "\n"))
        }
    }

    var title = "UwULock"
    var message: String? = String(localized: "Wird geladen …")
    var choices: [Choice] = []
    /// A passkey request's passkeys for the site, above the logins (`choices`) it may fill instead.
    var passkeys: [Choice] = []
    var busy = true
    var query = ""
    /// Passwords: a search over all logins, not only the suggested ones.
    var searchable = false
    /// The button that asks for Face ID, Touch ID or the passcode with a tap, when the system
    /// didn't let the sheet ask by itself. Its label; nil while it isn't offered.
    var unlockButton: String?
    @ObservationIgnored var pick: (Choice) -> Void = { _ in }
    @ObservationIgnored var cancel: () -> Void = {}
    @ObservationIgnored var unlock: () -> Void = {}

    /// What the search lets through: every word of it somewhere in the name, the user name or an
    /// address; everything while it is empty.
    var shown: [Choice] {
        let words = folded(query).split(whereSeparator: \.isWhitespace).map(String.init)
        if words.isEmpty { return choices }
        return choices.filter { choice in words.allSatisfy { choice.haystack.contains($0) } }
    }
}

struct PasskeySheet: View {
    @Bindable var model: PasskeyModel
    @FocusState private var searching: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            if model.searchable && !model.busy {
                searchField
                    .padding(.horizontal, UwUSpacing.md)
                    .padding(.bottom, UwUSpacing.sm)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: UwUSpacing.lg) {
                    if model.busy {
                        VStack(spacing: UwUSpacing.sm) {
                            ProgressView().controlSize(.large)
                            if let message = model.message {
                                Text(message)
                                    .uwuFont(.subheadline)
                                    .foregroundStyle(Color.uwuMuted)
                                    .multilineTextAlignment(.center)
                            }
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, UwUSpacing.xxl)
                    } else if let message = model.message {
                        note(.info, message)
                    }
                    if let label = model.unlockButton {
                        Button {
                            model.unlock()
                        } label: {
                            Label {
                                Text(label).uwuFont(.body, weight: .semibold)
                            } icon: {
                                UwUIcon(.unlocked)
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, UwUSpacing.xxs)
                        }
                        .uwuProminentButton()
                        .controlSize(.large)
                    }
                    if !model.busy { results }
                }
                .padding(.horizontal, UwUSpacing.md)
                .padding(.bottom, UwUSpacing.xl)
            }
            #if os(iOS)
            .scrollDismissesKeyboard(.interactively)
            #endif
        }
        .frame(minWidth: 320, minHeight: 240)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color.uwuCanvas.ignoresSafeArea())
        .uwuTheme()
    }

    private var header: some View {
        HStack(alignment: .center, spacing: UwUSpacing.sm) {
            UwUIcon(.locked)
                .foregroundStyle(Color.uwuPinkInk)
                .frame(width: 36, height: 36)
                .background(
                    Color.uwuPinkTint,
                    in: RoundedRectangle(cornerRadius: UwURadius.control + 1, style: .continuous))
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: "UwULock")
                    .uwuFont(.caption, weight: .semibold)
                    .foregroundStyle(Color.uwuPinkInk)
                Text(model.title)
                    .uwuFont(.headline)
                    .foregroundStyle(Color.uwuInk)
                    .lineLimit(2)
            }
            Spacer(minLength: UwUSpacing.xs)
            Button(role: .cancel) {
                model.cancel()
            } label: {
                Text("Abbrechen").uwuFont(.body, weight: .medium)
            }
            .uwuGlassButton()
        }
        .padding(.horizontal, UwUSpacing.md)
        .padding(.top, UwUSpacing.lg)
        .padding(.bottom, UwUSpacing.sm)
    }

    private var searchField: some View {
        HStack(spacing: UwUSpacing.xs) {
            UwUIcon(.search, size: .sm).foregroundStyle(Color.uwuMuted)
            TextField("Name, Benutzer oder Adresse", text: $model.query)
                .focused($searching)
                .uwuFont(.body)
                .foregroundStyle(Color.uwuInk)
                .autocorrectionDisabled()
                .textFieldStyle(.plain)
            #if os(iOS)
                .textInputAutocapitalization(.never)
                .submitLabel(.search)
            #endif
            if !model.query.isEmpty {
                Button {
                    model.query = ""
                } label: {
                    UwUIcon(.close, size: .sm, label: Text("Suche leeren"))
                        .foregroundStyle(Color.uwuFaint)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, UwUSpacing.sm)
        .padding(.vertical, 11)
        .uwuGlass(in: Capsule(), interactive: true)
        .overlay(
            Capsule().stroke(searching ? Color.uwuPinkSolid : Color.clear, lineWidth: 1.5))
    }

    @ViewBuilder private var results: some View {
        let shown = model.shown
        if !model.passkeys.isEmpty {
            section(String(localized: "Passkeys"), model.passkeys)
        }
        if !model.searchable {
            if !shown.isEmpty { section(nil, shown) }
        } else if model.choices.isEmpty {
            note(
                .inbox,
                String(
                    localized:
                        "In UwULocks Liste für AutoFill sind keine Logins. Öffne UwULock und entsperre den Tresor – danach sind sie hier."
                ))
        } else if shown.isEmpty {
            note(.search, String(localized: "Nichts gefunden für „\(model.query)“."))
        } else {
            let suggested = shown.filter(\.suggested)
            let others = shown.filter { !$0.suggested }
            if !suggested.isEmpty {
                section(String(localized: "Passend zu dieser Seite"), suggested)
            } else if model.query.isEmpty && model.passkeys.isEmpty {
                note(
                    .info,
                    String(localized: "Kein Login passt zu dieser Seite. Wähle einen aus allen Logins."))
            }
            if !others.isEmpty {
                section(
                    model.query.isEmpty || suggested.isEmpty
                        ? String(localized: "Alle Logins") : String(localized: "Weitere Treffer"),
                    others)
            }
        }
    }

    private func section(_ heading: String?, _ choices: [PasskeyModel.Choice]) -> some View {
        VStack(alignment: .leading, spacing: UwUSpacing.xs) {
            if let heading {
                Text(heading.uppercased())
                    .uwuFont(.caption, weight: .semibold)
                    .foregroundStyle(Color.uwuMuted)
                    .padding(.leading, 6)
            }
            LazyVStack(spacing: 0) {
                ForEach(Array(choices.enumerated()), id: \.element.id) { index, choice in
                    if index > 0 {
                        Rectangle().fill(Color.uwuHairline).frame(height: 1).padding(.leading, 62)
                    }
                    row(choice)
                }
            }
            .background(Color.uwuSurface)
            .clipShape(RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous)
                    .stroke(Color.uwuBorder, lineWidth: 1))
        }
    }

    private func row(_ choice: PasskeyModel.Choice) -> some View {
        Button {
            model.pick(choice)
        } label: {
            HStack(spacing: UwUSpacing.sm) {
                Group {
                    if choice.passkey {
                        UwUIcon(.passkey)
                    } else {
                        Text(String(choice.title.trimmingCharacters(in: .whitespaces).prefix(1)).uppercased())
                            .uwuFont(.headline)
                    }
                }
                .foregroundStyle(Color.uwuPinkInk)
                .frame(width: 36, height: 36)
                .background(Color.uwuPinkTint, in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(choice.title)
                        .uwuFont(.body, weight: .medium)
                        .foregroundStyle(Color.uwuInk)
                        .lineLimit(1)
                    if !choice.subtitle.isEmpty {
                        Text(choice.subtitle)
                            .uwuFont(.footnote)
                            .foregroundStyle(Color.uwuMuted)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: UwUSpacing.xs)
                UwUIcon(.next, size: .xs).foregroundStyle(Color.uwuFaint)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(RowStyle())
    }

    private func note(_ icon: UwUIconName, _ text: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            UwUIcon(icon).foregroundStyle(Color.uwuPinkInk)
            Text(text)
                .uwuFont(.subheadline)
                .foregroundStyle(Color.uwuMuted)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(14)
        .background(Color.uwuSurface, in: RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous)
                .stroke(Color.uwuBorder, lineWidth: 1))
    }
}

/// A row that tints while pressed.
struct RowStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? Color.uwuPinkTint.opacity(0.6) : Color.clear)
    }
}

// MARK: Previews (fixtures; reserved example names only)

extension PasskeyModel {
    static func preview(_ configure: (PasskeyModel) -> Void) -> PasskeyModel {
        UwUFonts.register()
        let model = PasskeyModel()
        configure(model)
        return model
    }
}

#Preview("Unlocking") {
    PasskeySheet(model: .preview { $0.title = String(localized: "Passkey für \("shop.example.com")") })
}

#Preview("Passkeys and logins") {
    PasskeySheet(
        model: .preview { model in
            model.busy = false
            model.message = nil
            model.searchable = true
            model.title = String(localized: "Anmelden bei \("shop.example.com")")
            model.passkeys = [
                .init(
                    id: "passkey:a", title: "alex@example.com", subtitle: "Alex Example",
                    suggested: true, passkey: true)
            ]
            model.choices = [
                .init(id: "1", title: "Example Shop", subtitle: "alex · shop.example.com", suggested: true),
                .init(id: "2", title: "Example Mail", subtitle: "alex · mail.example.org", suggested: false),
                .init(id: "3", title: "Router", subtitle: "admin · router.test", suggested: false),
            ]
        })
}

#Preview("Unlock button") {
    PasskeySheet(
        model: .preview { model in
            model.busy = false
            model.title = String(localized: "Passwort für \("shop.example.com")")
            model.message = String(
                localized: "Das System hat Face ID, Touch ID oder den Code nicht von selbst gestartet.")
            model.unlockButton = String(localized: "Mit Face ID entsperren")
        })
}

#Preview("No logins") {
    PasskeySheet(
        model: .preview { model in
            model.busy = false
            model.message = nil
            model.searchable = true
            model.title = String(localized: "Passwort für \("shop.example.com")")
        })
}
