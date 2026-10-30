import SwiftUI
import UwUDesign

/// "Als Send teilen …" for an item (ItemExtras.tsx `ShareSendDialog`, mobile/sheets/Share.tsx):
/// opens the sheet that puts chosen values of the item into a Send. Hidden where the server has
/// no Sends.
///
/// The sheet hangs on the button itself, so place it where a view stays alive while the sheet
/// is up (a toolbar, a list row, a form) — not as a row of a `Menu` or context menu.
struct ShareAsSendButton: View {
    let itemId: String
    @State private var uwu = UwuStore.shared
    @State private var sharing = false
    @Environment(\.shareAsSendHost) private var host

    var body: some View {
        if uwu.status.sendsAvailable {
            if let host {
                // Inside a Menu or context menu a sheet on the button never shows: the host
                // (`.shareAsSendHost()` further up) presents it instead.
                Button { host.itemId = itemId } label: {
                    Label(features: "Als Send teilen …", uwuIcon: .send)
                }
                .help(Text(features: "Als Send teilen"))
            } else {
                Button { sharing = true } label: {
                    Label(features: "Als Send teilen …", uwuIcon: .send)
                }
                .help(Text(features: "Als Send teilen"))
                .sheet(isPresented: $sharing) {
                    ShareSendSheet(itemId: itemId)
                }
            }
        }
    }
}

/// Presents the share-as-Send sheet for the `ShareAsSendButton`s below it, so the button also
/// works inside a `Menu`, a toolbar overflow or a context menu.
@MainActor
@Observable
final class ShareAsSendHost {
    var itemId: String?
}

extension EnvironmentValues {
    @Entry var shareAsSendHost: ShareAsSendHost? = nil
}

private struct ShareAsSendHostModifier: ViewModifier {
    @State private var host = ShareAsSendHost()

    func body(content: Content) -> some View {
        content
            .environment(\.shareAsSendHost, host)
            .sheet(item: Binding(
                get: { host.itemId.map(ShareAsSendTarget.init) },
                set: { host.itemId = $0?.id })
            ) { target in
                ShareSendSheet(itemId: target.id)
            }
    }
}

private struct ShareAsSendTarget: Identifiable {
    let id: String
}

extension View {
    /// Hosts the share-as-Send sheet for every `ShareAsSendButton` inside this view (attach it
    /// outside the Menu that holds the button, e.g. on the item detail).
    func shareAsSendHost() -> some View {
        modifier(ShareAsSendHostModifier())
    }
}

/// Which values of an item go into a Send, how long it lives and who may open it; afterwards
/// the link to copy or share, in the same sheet.
struct ShareSendSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let itemId: String

    @State private var uwu = UwuStore.shared
    @State private var detail: ItemDetail?
    @State private var fields: [SendsShareChoice]?
    @State private var options: SendOptions?
    @State private var days = 1
    @State private var maxAccess = 1
    @State private var password = ""
    @State private var onlyFor = false
    @State private var emails = ""
    @State private var domainId = ""
    @State private var hideText = true
    @State private var busy = false
    @State private var error: String?
    @State private var made: SharedSend?
    @State private var askTotp = false
    @State private var unlockPassword = ""
    @State private var unlocking = false

    /// UwULock Server's Send page shows an entry Send as the entry, with live codes.
    private var entry: Bool { uwu.status.uwu }
    private var addresses: [String] { SendsAddresses.split(emails) }
    private var chosen: [SendsShareChoice] { (fields ?? []).filter(\.checked) }
    private var totpTicked: Bool { chosen.contains { $0.field == "totp" } }
    private var locked: Bool { detail?.locked == true }
    private var broken: Bool { detail?.summary.broken == true || detail?.summary.deleted == true }
    private var canCreate: Bool {
        !busy && options != nil && !locked && !broken && !chosen.isEmpty
            && (!onlyFor || (!addresses.isEmpty && SendsAddresses.allValid(addresses)))
    }

    var body: some View {
        NavigationStack {
            Form {
                header
                if let error {
                    Section {
                        Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                            .foregroundStyle(Color.uwuDangerInk)
                    }
                }
                if let made {
                    result(made)
                } else if locked {
                    reprompt
                } else {
                    what
                    validity
                    access
                }
            }
            .formStyle(.grouped)
            .navigationTitle(Text(features: "Als Send teilen"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                if made != nil {
                    ToolbarItem(placement: .confirmationAction) {
                        Button { dismiss() } label: { Text(features: "Fertig") }
                            .keyboardShortcut(.defaultAction)
                    }
                } else {
                    ToolbarItem(placement: .cancellationAction) {
                        Button { dismiss() } label: { Text(features: "Abbrechen") }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        if busy {
                            ProgressView().controlSize(.small)
                                .accessibilityLabel(Text(features: "Erstellt …"))
                        } else {
                            Button { Task { await create() } } label: { Text(features: "Anlegen") }
                                .disabled(!canCreate)
                                .keyboardShortcut(.defaultAction)
                        }
                    }
                }
            }
            .alert(Text(features: "Schlüssel mitgeben"), isPresented: $askTotp) {
                Button(role: .destructive) { tick("totp", true) } label: { Text(features: "Schlüssel mitgeben") }
                Button(role: .cancel) {} label: { Text(features: "Lieber nicht") }
            } message: {
                Text(verbatim: "\(Self.totpWarning) \(featureString("Teile ihn nur, wenn das okay ist."))")
            }
            .task { await load() }
        }
        .interactiveDismissDisabled(busy)
        #if os(macOS)
        .frame(minWidth: 460, idealWidth: 520, minHeight: 520, idealHeight: 640)
        #else
        .presentationDetents([.large])
        #endif
        .sensoryFeedback(.success, trigger: made) { _, new in new != nil }
        .sensoryFeedback(.error, trigger: error) { _, new in new != nil }
    }

    static var totpWarning: String { featureString("Der Schlüssel des Einmal-Codes reist verschlüsselt im Send mit. Die Send-Seite zeigt nur die laufenden Codes – wer den Link hat, kann den Schlüssel aber auslesen und damit auch nach dem Löschen des Sends weiter Codes erzeugen.") }

    // MARK: Sections

    private var header: some View {
        Section {
            HStack(spacing: UwUSpacing.sm) {
                UwUIcon(.send, size: .md)
                    .foregroundStyle(Color.uwuPinkInk)
                    .frame(width: 34, height: 34)
                    .background(Color.uwuPinkTint, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    if let detail {
                        Text(verbatim: detail.summary.name.isEmpty ? featureString("(ohne Namen)") : detail.summary.name)
                            .uwuFont(.body, weight: .semibold)
                            .lineLimit(1)
                    } else {
                        Text(features: "Einen Moment …").foregroundStyle(Color.uwuMuted)
                    }
                    Group {
                        entry ? Text(features: "Wird als Eintrag geteilt") : Text(features: "Wird als Text-Send geteilt")
                    }
                    .uwuFont(.caption)
                    .foregroundStyle(Color.uwuMuted)
                }
            }
            .accessibilityElement(children: .combine)
        }
    }

    private func result(_ made: SharedSend) -> some View {
        Section {
            HStack {
                Spacer()
                NyuView(mood: .cheer).frame(width: 96, height: 96)
                Spacer()
            }
            Text(verbatim: made.link)
                .font(.system(.callout, design: .monospaced))
                .textSelection(.enabled)
                .lineLimit(4)
            UwUGlassContainer(spacing: UwUSpacing.xs) {
                HStack {
                    Button {
                        Task { await SendsLink.copy(model.core, made.link) }
                    } label: { Label(features: "Link kopieren", uwuIcon: .copy) }
                    .uwuProminentButton()
                    .foregroundStyle(Color.uwuOnPink)
                    if let url = URL(string: made.link) {
                        ShareLink(item: url) { Label(features: "Teilen", uwuIcon: .share) }
                            .uwuGlassButton()
                    }
                }
            }
        } header: {
            Text(features: "Link")
        } footer: {
            Text(features: "Wer diesen Link hat, kann die gewählten Werte ansehen – bis \(FeatureDate.short(made.deletionDate) ?? made.deletionDate).")
        }
    }

    private var reprompt: some View {
        Section {
            SecureField(text: $unlockPassword) { Text(features: "Master-Passwort") }
                .textContentType(.password)
                .onSubmit { Task { await unlock() } }
            Button { Task { await unlock() } } label: {
                Label(features: "Entsperren", uwuIcon: .unlocked)
            }
            .disabled(unlockPassword.isEmpty || unlocking)
        } header: {
            Text(features: "Master-Passwort erforderlich")
        } footer: {
            Text(features: "Dieser Eintrag ist besonders geschützt. Gib dein Master-Passwort ein, um ihn zu öffnen.")
        }
    }

    private var what: some View {
        Section {
            if let fields {
                if fields.isEmpty {
                    Text(features: "Dieser Eintrag hat nichts zu teilen.").foregroundStyle(Color.uwuMuted)
                }
                ForEach(fields) { field in
                    Toggle(isOn: Binding(
                        get: { field.checked },
                        set: { on in
                            if on && field.field == "totp" { askTotp = true } else { tick(field.field, on) }
                        })
                    ) {
                        HStack(spacing: UwUSpacing.xs) {
                            if field.secret {
                                UwUIcon(.masterPassword, size: .xs).foregroundStyle(Color.uwuMuted)
                            }
                            Text(verbatim: field.shown ?? field.label)
                        }
                        if field.shown != nil { Text(verbatim: field.label) }
                    }
                    #if os(macOS)
                    .toggleStyle(.checkbox)
                    #endif
                }
            } else if !broken {
                ProgressView { Text(features: "Einen Moment …") }
            }
        } header: {
            Text(features: "Was mitgeht")
        } footer: {
            if totpTicked {
                Text(verbatim: Self.totpWarning)
            } else if entry {
                Text(features: "Die gewählten Werte gehen verschlüsselt an einen Send. Der Schlüssel steckt im Link, nicht auf dem Server. Die Send-Seite zeigt sie als Eintrag mit Kopier-Knöpfen.")
            } else {
                Text(features: "Die gewählten Werte gehen verschlüsselt an einen Send. Der Schlüssel steckt im Link, nicht auf dem Server. Der Einmal-Code-Schlüssel wird nie geteilt.")
            }
        }
    }

    private var validity: some View {
        Section {
            Picker(selection: $days) {
                ForEach(SendsDays.all, id: \.self) { n in
                    n == 1 ? Text(features: "1 Tag").tag(n) : Text(features: "\(n) Tagen").tag(n)
                }
            } label: {
                Label(features: "Löschen nach", uwuIcon: .delete)
            }
            Stepper(value: $maxAccess, in: 0...1000) {
                LabeledContent {
                    if maxAccess > 0 { Text(verbatim: "\(maxAccess)") } else { Text(features: "unbegrenzt") }
                } label: {
                    Label(features: "Höchstens so oft öffnen", uwuIcon: .show)
                }
            }
        } header: {
            Text(features: "Gültigkeit")
        }
    }

    private var access: some View {
        Section {
            if options?.emails == true {
                Toggle(isOn: $onlyFor) { Text(features: "Nur für diese Adressen (mit Code per E-Mail)") }
            }
            if onlyFor {
                TextField(text: $emails, prompt: Text(verbatim: "name@example.com"), axis: .vertical) {
                    Text(features: "E-Mail-Adressen")
                }
                .lineLimit(2...4)
                .autocorrectionDisabled()
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .keyboardType(.emailAddress)
                #endif
            } else {
                SecureField(text: $password, prompt: Text(features: "Ohne Passwort")) {
                    Text(features: "Passwort (optional)")
                }
                .textContentType(.newPassword)
            }
            if let domains = options?.domains, !domains.isEmpty {
                Picker(selection: $domainId) {
                    Text(features: "Hauptadresse des Servers").tag("")
                    ForEach(domains) { domain in
                        Text(verbatim: sendsDomainLabel(domain)).tag(domain.id)
                    }
                } label: {
                    Label(features: "Link-Adresse", uwuIcon: .website)
                }
            }
            Toggle(isOn: $hideText) {
                #if os(macOS)
                Text(features: "Text erst nach einem Klick zeigen")
                #else
                Text(features: "Text erst nach Tipp zeigen")
                #endif
            }
        } footer: {
            Text(features: "Wer den Link hat, sieht diese Felder. Teile ihn nur, wenn das okay ist.")
        }
    }

    // MARK: Actions

    private func tick(_ name: String, _ on: Bool) {
        fields = fields?.map { field in
            var field = field
            if field.field == name { field.checked = on }
            return field
        }
    }

    private func load() async {
        #if DEBUG
        if FeatureFixtures.isOn {
            detail = SendsFixtures.item
            options = SendsFixtures.options
            domainId = SendsFixtures.options.defaultDomainId ?? ""
            fields = SendsShareChoices.make(SendsFixtures.item, entry: entry)
            return
        }
        #endif
        UwuStore.shared.attach(model.core)
        async let found = model.core.vaultItem(id: itemId)
        async let opts = model.core.sendOptions()
        do {
            let item = try await found
            detail = item
            if !item.locked { fields = SendsShareChoices.make(item, entry: entry) }
        } catch {
            self.error = Core.failure(error).message
        }
        do {
            let next = try await opts
            options = next
            domainId = next.defaultDomainId ?? ""
        } catch {
            options = SendOptions(emails: false, domains: [], defaultDomainId: nil)
        }
    }

    private func unlock() async {
        guard !unlockPassword.isEmpty, !unlocking else { return }
        unlocking = true
        defer { unlocking = false }
        do {
            try await model.core.verifyReprompt(id: itemId, password: unlockPassword)
            unlockPassword = ""
            let item = try await model.core.vaultItem(id: itemId)
            detail = item
            fields = SendsShareChoices.make(item, entry: entry)
            error = nil
        } catch {
            self.error = Core.failure(error).message
        }
    }

    private func create() async {
        guard canCreate else { return }
        busy = true
        error = nil
        defer { busy = false }
        let input = ShareInput(
            fields: chosen.map { ShareField(name: $0.field, value: $0.label) },
            deletionDays: days,
            maxAccess: maxAccess > 0 ? maxAccess : nil,
            password: onlyFor || password.isEmpty ? nil : password,
            emails: onlyFor ? addresses : [],
            sendDomainId: domainId.isEmpty ? nil : domainId,
            hideText: hideText,
            entry: entry)
        do {
            let result: SharedSend
            #if DEBUG
            if FeatureFixtures.isOn {
                result = SharedSend(
                    id: "s900", link: "https://vault.example.com/#/send/s900/k3y-sample",
                    deletionDate: SendsDays.iso(in: days))
            } else {
                result = try await model.core.shareAsSend(id: itemId, input: input)
            }
            #else
            result = try await model.core.shareAsSend(id: itemId, input: input)
            #endif
            made = result
            // The link is what the person wants next; the new Send also shows under Sends.
            let copied = await SendsLink.copyQuietly(model.core, result.link)
            FeatureToasts.shared.show(
                copied ? featureString("Send angelegt – der Link ist kopiert ✧") : featureString("Send angelegt ✧"))
        } catch {
            self.error = Core.failure(error).message
        }
    }
}

#if DEBUG
#Preview("Share as Send") {
    UserDefaults.standard.set(true, forKey: "UwULockFixtures")
    return ShareSendSheet(itemId: "i1")
        .environment(AppModel())
}
#endif
