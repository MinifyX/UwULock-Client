import SwiftUI
import UwUDesign

/// One item (components/ItemDetail.tsx, mobile/pages/ItemView.tsx): its header with the marks,
/// the fields of its kind, websites, passkeys, notes, own fields, history and the extras.
struct ItemDetailView: View {
    let itemId: String
    @Environment(VaultModel.self) private var vault
    @State private var detail: ItemDetail?
    @State private var failure: String?
    @State private var showHistory = false
    @State private var confirmTrash = false
    @State private var unlocked = 0

    private var summary: ItemSummary? { vault.item(itemId) ?? detail?.summary }

    var body: some View {
        Group {
            if let summary {
                content(summary)
            } else if vault.loaded {
                ContentUnavailableView {
                    VStack(spacing: UwUSpacing.sm) {
                        NyuView(mood: .puzzled).frame(width: 88)
                        Text(vt("Nicht mehr da")).uwuFont(.headline)
                    }
                } description: {
                    Text(vt("Diesen Eintrag gibt es nicht mehr."))
                }
            } else {
                ProgressView()
            }
        }
        .task(id: "\(itemId)-\(vault.revision)-\(unlocked)") { await load() }
        .onAppear { vault.opened(itemId) }
        .shareAsSendHost()
    }

    private func load() async {
        do {
            detail = try await vault.backend.vaultItem(id: itemId)
            failure = nil
        } catch {
            let found = Core.failure(error)
            if found.kind != "not-found" { failure = found.message }
        }
    }

    @ViewBuilder
    private func content(_ summary: ItemSummary) -> some View {
        Form {
            Section { header(summary) }
            if let failure {
                Section { notice(failure) }
            }
            if summary.broken {
                Section {
                    notice(vt("Ein Teil dieses Eintrags ließ sich nicht entschlüsseln. Im Web-Tresor sieht er vielleicht anders aus."))
                }
            }
            if let detail {
                if detail.locked {
                    Section {
                        RepromptCard(itemId: itemId) { unlocked += 1 }
                            .frame(maxWidth: .infinity)
                            .listRowBackground(Color.clear)
                    }
                } else {
                    fields(summary, detail)
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(summary.name.isEmpty ? vt("(ohne Namen)") : summary.name)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar { toolbar(summary) }
        .confirmationDialog(vt("In den Papierkorb?"), isPresented: $confirmTrash, titleVisibility: .visible) {
            Button(vt("In den Papierkorb"), role: .destructive) { vault.trash(summary) }
            Button(vt("Abbrechen"), role: .cancel) {}
        } message: {
            Text(vt("„\(displayName(summary))“ wandert in den Papierkorb. Der Server hebt ihn dort noch 30 Tage auf."))
        }
    }

    private func displayName(_ summary: ItemSummary) -> String {
        summary.name.isEmpty ? vt("(ohne Namen)") : summary.name
    }

    private func notice(_ text: String) -> some View {
        Label {
            Text(text)
        } icon: {
            UwUIcon(.warning, size: .sm)
        }
        .foregroundStyle(.uwuDangerInk)
        .uwuFont(.subheadline)
    }

    // MARK: Header

    private func header(_ summary: ItemSummary) -> some View {
        HStack(alignment: .center, spacing: UwUSpacing.md) {
            ItemTile(item: summary, size: 56)
            VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                HStack(spacing: UwUSpacing.xs) {
                    Text(displayName(summary))
                        .uwuFont(.title2, weight: .bold)
                        .lineLimit(2)
                    if summary.favorite {
                        UwUIcon(.favorite, size: .sm, label: Text(vt("Favorit")))
                            .foregroundStyle(.uwuPinkSolid)
                    }
                }
                ChipFlow(chips: chips(summary))
            }
        }
        .padding(.vertical, UwUSpacing.xs)
    }

    private func chips(_ summary: ItemSummary) -> [Chip] {
        var chips = [Chip(icon: summary.kind.icon, text: summary.kind.label)]
        if let folder = vault.folderName(summary.folderId) {
            chips.append(Chip(icon: .folder, text: folder))
        }
        if let orgId = summary.organizationId,
            let org = vault.overview?.organizations.first(where: { $0.id == orgId })?.name
        {
            let collections = (vault.overview?.collections ?? [])
                .filter { summary.collectionIds.contains($0.id) }.map(\.name)
            chips.append(Chip(icon: .organization,
                              text: collections.isEmpty ? org : "\(org) · \(collections.joined(separator: ", "))"))
        }
        if let masked = vault.uwu.masked[summary.id] {
            chips.append(Chip(icon: .maskedAddress, text: masked.email))
        }
        if vault.due.contains(summary.id), !summary.deleted {
            chips.append(Chip(icon: .reminder, text: vt("Neues Passwort fällig"), tone: .warning))
        }
        if summary.deleted {
            chips.append(Chip(icon: .delete, text: vt("Im Papierkorb"), tone: .muted))
        }
        return chips
    }

    // MARK: Toolbar

    @ToolbarContentBuilder
    private func toolbar(_ summary: ItemSummary) -> some ToolbarContent {
        if summary.deleted {
            ToolbarItemGroup(placement: .primaryAction) {
                Button { vault.restore(summary) } label: { Label(vt("Wiederherstellen"), uwuIcon: .restore) }
                    .help(vt("Wiederherstellen"))
                Button(role: .destructive) { vault.itemToDelete = summary } label: {
                    Label(vt("Endgültig löschen"), uwuIcon: .delete)
                }
                .help(vt("Endgültig löschen"))
            }
        } else {
            ToolbarItem(placement: .primaryAction) {
                Button { vault.edit(summary) } label: { Label(vt("Bearbeiten"), uwuIcon: .edit) }
                    .disabled(summary.broken)
                    .keyboardShortcut("e", modifiers: .command)
                    .help(vt("Bearbeiten"))
            }
            ToolbarItem(placement: .secondaryAction) {
                Button { vault.toggleFavorite(summary) } label: {
                    Label(summary.favorite ? vt("Favorit entfernen") : vt("Zu Favoriten"), uwuIcon: .favorite)
                }
                .help(summary.favorite ? vt("Favorit entfernen") : vt("Zu Favoriten"))
            }
            ToolbarItem(placement: .secondaryAction) {
                Menu {
                    if summary.organizationId == nil {
                        MoveMenu(item: summary)
                    }
                    ShareAsSendButton(itemId: summary.id)
                        .disabled(detail?.locked ?? true || summary.broken)
                    Button {
                        vault.perform { try await vault.backend.openWebVault() }
                    } label: {
                        Label(vt("Im Web-Tresor öffnen"), uwuIcon: .openExternal)
                    }
                    Divider()
                    Button(role: .destructive) { confirmTrash = true } label: {
                        Label(vt("In den Papierkorb"), uwuIcon: .delete)
                    }
                } label: {
                    Label(vt("Mehr"), systemImage: "ellipsis")
                }
            }
        }
    }

    // MARK: Fields

    @ViewBuilder
    private func fields(_ summary: ItemSummary, _ detail: ItemDetail) -> some View {
        let wifi = summary.kind == .wifi ? WifiFields.read(detail.fields ?? []) : nil
        let own = wifi?.others ?? detail.fields ?? []

        if let login = detail.login {
            Section {
                if let username = login.username, !username.isEmpty {
                    DetailRow(vt("Benutzername"), text: username) { vault.copy(itemId, field: "username") }
                }
                if login.hasPassword {
                    SecretRow(itemId: itemId, field: "password", label: vt("Passwort"),
                              copyable: summary.viewPassword)
                }
                if login.hasTotp { TotpRow(itemId: itemId) }
                if (login.username ?? "").isEmpty, !login.hasPassword, !login.hasTotp {
                    Text(vt("Kein Benutzername, kein Passwort.")).foregroundStyle(.secondary)
                }
            }
            if !login.uris.isEmpty {
                WebsitesSection(itemId: itemId, uris: login.uris)
            }
            if login.passkeys > 0 {
                PasskeysSection(itemId: itemId, revision: summary.revisionDate)
            }
        }

        if let card = detail.card {
            Section {
                if let name = card.cardholderName, !name.isEmpty {
                    DetailRow(vt("Karteninhaber"), text: name) { vault.copy(itemId, field: "card-name") }
                }
                if let brand = card.brand, !brand.isEmpty {
                    DetailRow(vt("Marke"), text: brand)
                }
                if let ending = card.numberEnding, !ending.isEmpty {
                    SecretRow(itemId: itemId, field: "card-number", label: vt("Kartennummer"),
                              masked: "•••• •••• •••• \(ending)", colored: false)
                }
                if card.expMonth?.isEmpty == false || card.expYear?.isEmpty == false {
                    let month = card.expMonth ?? ""
                    DetailRow(label: vt("Gültig bis"), copy: { vault.copy(itemId, field: "card-expiry") }) {
                        Text("\(month.count == 1 ? "0" + month : month)/\(card.expYear ?? "")")
                            .font(.uwuMono(.body))
                    }
                }
                if card.hasCode {
                    SecretRow(itemId: itemId, field: "card-code", label: vt("Prüfnummer"),
                              masked: "•••", colored: false)
                }
            }
        }

        if let identity = detail.identity, !identity.isEmpty {
            Section {
                ForEach(identity) { entry in
                    let label = IdentityFields.label(entry.name)
                    if entry.sensitive {
                        SecretRow(itemId: itemId, field: "identity:\(entry.name)", label: label, colored: false)
                    } else {
                        DetailRow(label, text: entry.value) { vault.copy(itemId, field: "identity:\(entry.name)") }
                    }
                }
            }
        }

        if let ssh = detail.sshKey {
            Section {
                if let fingerprint = ssh.fingerprint, !fingerprint.isEmpty {
                    DetailRow(label: vt("Fingerprint"), copy: { vault.copy(itemId, field: "ssh-fingerprint") }) {
                        Text(fingerprint).font(.uwuMono(.callout))
                    }
                }
                if let key = ssh.publicKey, !key.isEmpty {
                    DetailRow(label: vt("Öffentlicher Schlüssel"), copy: { vault.copy(itemId, field: "ssh-public") }) {
                        Text(key).font(.uwuMono(.caption)).lineLimit(4)
                    }
                }
                if ssh.hasPrivateKey {
                    SecretRow(itemId: itemId, field: "ssh-private", label: vt("Privater Schlüssel"),
                              masked: "-----BEGIN ••••••••-----", colored: false, multiline: true)
                }
            }
        }

        if let wifi {
            WifiSection(itemId: itemId, wifi: wifi)
        }

        if let notes = detail.notes, !notes.isEmpty {
            Section {
                DetailRow(label: vt("Notizen"), copy: { vault.copy(itemId, field: "notes") }) {
                    Text(notes)
                }
            } header: {
                Text(vt("Notizen"))
            }
        }

        if !own.isEmpty {
            Section(vt("Eigene Felder")) {
                ForEach(own) { field in ownField(field) }
            }
        }

        if let history = detail.passwordHistory, !history.isEmpty {
            Section {
                DisclosureGroup(isExpanded: $showHistory) {
                    ForEach(history) { entry in
                        SecretRow(itemId: itemId, field: "history:\(entry.index)",
                                  label: whenText(entry.lastUsed) ?? vt("Früher"))
                    }
                } label: {
                    Label(history.count == 1 ? vt("1 früheres Passwort") : vt("\(history.count) frühere Passwörter"),
                          uwuIcon: .history)
                        .foregroundStyle(.secondary)
                }
            }
        }

        ItemExtrasSection(itemId: itemId)

        Section {
            footer(summary, detail)
        }
    }

    @ViewBuilder
    private func ownField(_ field: FieldDetail) -> some View {
        let label = field.name.flatMap { $0.isEmpty ? nil : $0 } ?? vt("Feld \(field.index + 1)")
        switch field.kind {
        case .hidden where field.hasValue:
            SecretRow(itemId: itemId, field: "field:\(field.index)", label: label)
        case .boolean:
            DetailRow(label: label) {
                Text(field.value == "true" ? vt("Ja") : vt("Nein"))
                    .foregroundStyle(field.value == "true" ? Color.uwuSuccessInk : Color.secondary)
            }
        case .linked:
            DetailRow(label: label) {
                Text(vt("verknüpft mit einem anderen Feld")).foregroundStyle(.secondary)
            }
        default:
            DetailRow(label, text: field.value) { vault.copy(itemId, field: "field:\(field.index)") }
        }
    }

    @ViewBuilder
    private func footer(_ summary: ItemSummary, _ detail: ItemDetail) -> some View {
        if let count = detail.attachments, count > 0 {
            Label(vt("\(count) Anhänge – die öffnest du vorerst im Web-Tresor."), systemImage: "paperclip")
                .uwuFont(.footnote)
        }
        let dates = [
            whenText(summary.revisionDate).map { vt("Geändert \($0)") },
            whenText(detail.creationDate).map { vt("Erstellt \($0)") },
            whenText(detail.login?.passwordRevisionDate).map { vt("Passwort geändert \($0)") },
        ].compactMap { $0 }
        if !dates.isEmpty {
            Text(dates.joined(separator: " · "))
                .uwuFont(.footnote)
                .foregroundStyle(.secondary)
        }
        Button {
            vault.perform { try await vault.backend.openWebVault() }
        } label: {
            Label(vt("Im Web-Tresor öffnen"), uwuIcon: .openExternal)
        }
        .buttonStyle(.borderless)
        .uwuFont(.footnote)
    }
}

// MARK: - Chips

struct Chip: Identifiable {
    enum Tone { case plain, warning, muted }
    var icon: UwUIconName
    var text: String
    var tone: Tone = .plain
    var id: String { text }
}

/// The marks under an item's name, wrapping like the desktop's chips.
struct ChipFlow: View {
    let chips: [Chip]

    var body: some View {
        FlowLayout(spacing: UwUSpacing.xs) {
            ForEach(chips) { chip in
                HStack(spacing: UwUSpacing.xxs) {
                    UwUIcon(chip.icon, size: .xs)
                    Text(chip.text).lineLimit(1)
                }
                .uwuFont(.caption, weight: .medium)
                .foregroundStyle(chip.tone == .warning ? Color.uwuWarningInk : Color.secondary)
                .padding(.horizontal, UwUSpacing.sm)
                .padding(.vertical, 3)
                .background(
                    Capsule().fill(chip.tone == .warning ? Color.uwuWarningTint : Color.uwuHairline.opacity(0.5))
                )
            }
        }
    }
}

/// Lays its children out in rows, wrapping when a row is full.
struct FlowLayout: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, row: CGFloat = 0, widest: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                y += row + spacing
                x = 0
                row = 0
            }
            x += size.width + spacing
            row = max(row, size.height)
            widest = max(widest, x - spacing)
        }
        return CGSize(width: proposal.width ?? widest, height: y + row)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, row: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > bounds.minX, x + size.width > bounds.maxX {
                y += row + spacing
                x = bounds.minX
                row = 0
            }
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            row = max(row, size.height)
        }
    }
}

// MARK: - Websites

/// The first website, the others behind "+2 weitere Websites" (WebsitesSection).
struct WebsitesSection: View {
    let itemId: String
    let uris: [LoginUri]
    @Environment(VaultModel.self) private var vault
    @State private var all = false

    var body: some View {
        Section(uris.count == 1 ? vt("Website") : vt("Websites")) {
            ForEach(Array((all ? uris : Array(uris.prefix(1))).enumerated()), id: \.offset) { index, uri in
                WebsiteRow(itemId: itemId, index: index, uri: uri)
            }
            if uris.count > 1 {
                Button {
                    withAnimation(.uwuOut()) { all.toggle() }
                } label: {
                    Text(all ? vt("Weniger zeigen")
                         : uris.count == 2 ? vt("+1 weitere Website") : vt("+\(uris.count - 1) weitere Websites"))
                }
                .buttonStyle(.borderless)
            }
        }
        .onChange(of: itemId) { all = false }
    }
}

struct WebsiteRow: View {
    let itemId: String
    let index: Int
    let uri: LoginUri
    @Environment(VaultModel.self) private var vault

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: UwUSpacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(uri.host ?? vt("Adresse")).uwuFont(.caption).foregroundStyle(.secondary)
                Text(uri.uri)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                if uri.match != nil {
                    Text(vt("Erkennung: \(UriMatch.label(uri.match))"))
                        .uwuFont(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if uri.openable {
                Button {
                    vault.openUri(itemId, index: index)
                } label: {
                    UwUIcon(.openExternal, size: .sm, label: Text(vt("\(uri.uri) im Browser öffnen")))
                }
                .buttonStyle(.borderless)
                .help(vt("Im Browser öffnen"))
            }
            CopyButton(label: vt("Adresse kopieren")) { vault.copy(itemId, field: "uri:\(index)") }
        }
    }
}

// MARK: - Passkeys

/// The passkeys of a login, each to delete (PasskeysSection).
struct PasskeysSection: View {
    let itemId: String
    let revision: String?
    @Environment(VaultModel.self) private var vault
    @State private var keys: [PasskeyInfo]?
    @State private var asking: PasskeyInfo?
    @State private var busy = false

    var body: some View {
        Group {
            if let keys, !keys.isEmpty {
                Section(keys.count == 1 ? vt("Passkey") : vt("Passkeys")) {
                    ForEach(keys) { key in row(key) }
                }
            }
        }
        .task(id: "\(itemId)-\(revision ?? "")") { await load() }
        .alert(vt("Passkey löschen?"), isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
               presenting: asking) { key in
            Button(vt("Löschen"), role: .destructive) { remove(key) }
            Button(vt("Abbrechen"), role: .cancel) {}
        } message: { key in
            Text(vt("Mit diesem Passkey meldest du dich danach nicht mehr bei \(site(key)) an. Leg vorher einen anderen Weg zur Anmeldung an, falls du keinen mehr hast."))
        }
    }

    private func site(_ key: PasskeyInfo) -> String {
        !key.rpId.isEmpty ? key.rpId : (key.rpName.flatMap { $0.isEmpty ? nil : $0 } ?? vt("Unbekannte Website"))
    }

    private func row(_ key: PasskeyInfo) -> some View {
        let site = site(key)
        let user = key.userName ?? key.userDisplayName
        let meta = [
            key.rpName.flatMap { $0 != site && !$0.isEmpty ? $0 : nil },
            whenText(key.creationDate).map { vt("erstellt \($0)") },
            key.readable ? nil : vt("lässt sich nicht lesen – nur löschen"),
        ].compactMap { $0 }
        return HStack(spacing: UwUSpacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Label {
                    Text(site)
                } icon: {
                    UwUIcon(.passkey, size: .xs)
                }
                .uwuFont(.caption)
                .foregroundStyle(.secondary)
                Text(user ?? vt("ohne Benutzernamen")).foregroundStyle(user == nil ? .secondary : .primary)
                if !meta.isEmpty {
                    Text(meta.joined(separator: " · ")).uwuFont(.caption2).foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Button(role: .destructive) {
                asking = key
            } label: {
                UwUIcon(.delete, size: .sm, label: Text(vt("Passkey für \(site) löschen")))
            }
            .buttonStyle(.borderless)
            .disabled(busy)
            .help(vt("Passkey für \(site) löschen"))
        }
    }

    private func load() async {
        do {
            keys = try await vault.backend.itemPasskeys(id: itemId)
        } catch {
            keys = []
            vault.show(error)
        }
    }

    private func remove(_ key: PasskeyInfo) {
        busy = true
        Task {
            defer { busy = false; asking = nil }
            do {
                try await vault.backend.deletePasskey(
                    id: itemId, index: key.index,
                    credentialId: key.credentialId.isEmpty ? key.fingerprint : key.credentialId)
                // The places of the others have moved: read them again before the next delete.
                keys = nil
                keys = try await vault.backend.itemPasskeys(id: itemId)
                vault.say(vt("Passkey gelöscht."))
            } catch {
                vault.show(error)
            }
        }
    }
}

// MARK: - Wi-Fi

/// A Wi-Fi network's fields (WifiSection); QR code and connecting come from WifiItemSection.
struct WifiSection: View {
    let itemId: String
    let wifi: WifiFields.View
    @Environment(VaultModel.self) private var vault

    private static let enterprise: [(WifiFields.Key, String.LocalizationValue)] = [
        (.eap, "EAP-Methode"), (.phase2, "Phase 2"), (.identity, "Identität"),
        (.anonymous, "Anonyme Identität"), (.ca, "CA-Zertifikat"),
    ]

    var body: some View {
        Section(vt("WLAN")) {
            let ssid = wifi.value(.ssid)
            DetailRow(label: vt("Netzwerkname (SSID)"),
                      copy: ssid.isEmpty || wifi.from[.ssid] == nil ? nil : { vault.copy(itemId, field: "field:\(wifi.from[.ssid]!)") }) {
                Text(ssid.isEmpty ? "—" : ssid).font(.uwuMono(.body))
            }
            if let password = wifi.password, password.hasValue, wifi.value(.security) != "None" {
                SecretRow(itemId: itemId, field: "field:\(password.index)", label: vt("Passwort"))
            }
            DetailRow(vt("Sicherheit"), text: wifi.value(.security).isEmpty ? nil : WifiFields.securityLabel(wifi.value(.security)))
            DetailRow(label: vt("Verstecktes Netzwerk")) {
                Text(wifi.hidden ? vt("Ja") : vt("Nein"))
            }
            if WifiFields.isEnterprise(wifi.value(.security)) {
                ForEach(Self.enterprise, id: \.0) { key, name in
                    let value = wifi.value(key)
                    if !value.isEmpty {
                        let label = vt(name)
                        DetailRow(label: label,
                                  copy: wifi.from[key].map { from in { vault.copy(itemId, field: "field:\(from)") } }) {
                            Text(value == "none" ? vt("Keine") : value)
                        }
                    }
                }
            }
            WifiItemSection(itemId: itemId)
        }
    }
}
