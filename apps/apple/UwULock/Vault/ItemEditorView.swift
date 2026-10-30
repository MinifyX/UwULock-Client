import SwiftUI
import UwUDesign

/// The item editor (components/ItemEditor.tsx, mobile/pages/ItemEdit.tsx), as a sheet.
struct ItemEditorView: View {
    let request: EditorRequest
    @Environment(VaultModel.self) private var vault
    @Environment(\.dismiss) private var dismiss
    @State private var form: ItemForm
    @State private var initial: ItemForm
    @State private var loading: Bool
    @State private var failure: String?
    @State private var busy = false
    @State private var generatorFor: GeneratorTarget?
    @State private var newMasked: String?
    @State private var masking = false
    @State private var confirmClose = false
    @State private var nextKey = 1000
    @FocusState private var nameFocused: Bool

    enum GeneratorTarget: Identifiable {
        case password, wifi
        var id: Self { self }
    }

    init(request: EditorRequest) {
        self.request = request
        var form = ItemForm(kind: request.kind)
        form.folderId = request.folderId ?? ""
        _form = State(initialValue: form)
        _initial = State(initialValue: form)
        _loading = State(initialValue: request.itemId != nil)
    }

    private var kind: ItemKind { request.kind }
    private var itemId: String? { request.itemId }
    private var summary: ItemSummary? { vault.item(itemId) }
    private var dirty: Bool { form != initial }
    private var nameMissing: Bool { form.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        NavigationStack {
            Group {
                if loading {
                    VStack(spacing: UwUSpacing.md) {
                        NyuView(mood: .sleepy).frame(width: 80)
                        Text(vt("Einen Moment …")).foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    editor
                }
            }
            .navigationTitle(itemId == nil ? vt("Neu: \(kind.label)") : vt("\(kind.label) bearbeiten"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(vt("Abbrechen"), action: close)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(busy ? vt("Speichert …") : vt("Speichern"), action: save)
                        .disabled(busy || loading || nameMissing)
                        .keyboardShortcut("s", modifiers: .command)
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 520, idealWidth: 600, minHeight: 560, idealHeight: 720)
        #endif
        .interactiveDismissDisabled(dirty || busy)
        .confirmationDialog(vt("Wirklich schließen?"), isPresented: $confirmClose, titleVisibility: .visible) {
            Button(vt("Schließen"), role: .destructive) { dismiss() }
            Button(vt("Weiter bearbeiten"), role: .cancel) {}
        } message: {
            Text(vt("Was du bisher eingegeben hast, geht dabei verloren."))
        }
        .sheet(item: $generatorFor) { target in
            GeneratorSheet(onUse: { password in
                switch target {
                case .password: form.password = .typed(password)
                case .wifi: form.wifi.password = .typed(password)
                }
                generatorFor = nil
            })
        }
        .task { await load() }
    }

    private func close() {
        if dirty, !busy { confirmClose = true } else { dismiss() }
    }

    private func load() async {
        guard let itemId, loading else {
            nameFocused = true
            return
        }
        defer { loading = false }
        do {
            let detail = try await vault.backend.vaultItem(id: itemId)
            if detail.locked {
                failure = vt("Dieser Eintrag fragt zuerst nach deinem Master-Passwort.")
                return
            }
            let next = ItemForm(summary: summary ?? detail.summary, detail: detail)
            form = next
            initial = next
        } catch {
            failure = Core.failure(error).message
        }
    }

    private func save() {
        guard !nameMissing else {
            failure = vt("Ohne Namen findest du den Eintrag später nicht wieder.")
            return
        }
        busy = true
        failure = nil
        let draft = form.draft(kind: kind)
        Task {
            defer { busy = false }
            do {
                let saved = try await vault.backend.saveItem(id: itemId, draft: draft)
                if let newMasked, itemId == nil {
                    do { _ = try await vault.backend.linkMaskedAddress(id: newMasked, cipherId: saved) } catch { vault.show(error) }
                }
                initial = form
                vault.say(itemId == nil ? vt("Angelegt ✧") : vt("Gespeichert ✧"), tone: .success)
                vault.selectedItemId = saved
                dismiss()
            } catch {
                // A conflict (changed elsewhere) and the other refusals come back as their text.
                failure = Core.failure(error).message
            }
        }
    }

    private func key() -> Int {
        nextKey += 1
        return nextKey
    }

    /// A new masked address for the site of the first address, as the username.
    private func mask() {
        masking = true
        let site = form.uris.first { !$0.uri.trimmingCharacters(in: .whitespaces).isEmpty }?.uri
            .trimmingCharacters(in: .whitespaces)
        let name = form.name.trimmingCharacters(in: .whitespaces)
        Task {
            defer { masking = false }
            do {
                let address = try await vault.backend.createMaskedAddress(
                    forDomain: site, description: name.isEmpty ? nil : name, cipherId: itemId)
                if itemId == nil { newMasked = address.id }
                form.username = address.email
                vault.say(vt("Maskierte Adresse erstellt ✧"), tone: .success)
            } catch {
                vault.show(error)
            }
        }
    }

    // MARK: Form

    private var editor: some View {
        Form {
            if let failure {
                Section {
                    Label {
                        Text(failure)
                    } icon: {
                        UwUIcon(.warning, size: .sm)
                    }
                    .foregroundStyle(.uwuDangerInk)
                }
            }

            Section {
                HStack {
                    TextField(vt("Name"), text: $form.name)
                        .focused($nameFocused)
                        .onChange(of: form.name) { _, name in
                            if name.count > 200 { form.name = String(name.prefix(200)) }
                        }
                    Button {
                        form.favorite.toggle()
                    } label: {
                        UwUIcon(.favorite, size: .sm,
                                label: Text(form.favorite ? vt("Favorit entfernen") : vt("Zu Favoriten")))
                            .foregroundStyle(form.favorite ? Color.uwuPinkSolid : Color.secondary)
                            .symbolVariant(form.favorite ? .fill : .none)
                    }
                    .buttonStyle(.borderless)
                    .help(form.favorite ? vt("Favorit entfernen") : vt("Zu Favoriten"))
                }
                Picker(vt("Ordner"), selection: $form.folderId) {
                    Text(vt("Ohne Ordner")).tag("")
                    ForEach(vault.sortedFolders) { folder in Text(folder.name).tag(folder.id) }
                }
                .disabled(summary?.organizationId != nil)
            }

            switch kind {
            case .login: loginSections
            case .card: cardSection
            case .identity: identitySection
            case .sshKey: sshSection
            case .wifi: wifiSections
            case .note: EmptyView()
            }

            Section(vt("Notizen")) {
                TextField(vt("Notizen"), text: $form.notes, axis: .vertical)
                    .lineLimit(kind == .note ? 8... : 3...)
            }

            fieldsSection

            Section {
                Toggle(vt("Vor dem Anzeigen nach dem Master-Passwort fragen"), isOn: $form.reprompt)
            }
        }
        .formStyle(.grouped)
    }

    @ViewBuilder
    private var loginSections: some View {
        Section {
            HStack {
                TextField(vt("Benutzername"), text: $form.username)
                    .textContentType(.username)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
                if vault.uwu.has(.maskedAddresses) {
                    Button(action: mask) {
                        UwUIcon(.maskedAddress, size: .sm, label: Text(vt("Neue maskierte Adresse")))
                    }
                    .buttonStyle(.borderless)
                    .disabled(masking)
                    .help(vt("Neue maskierte Adresse"))
                }
            }
            SecretField(label: vt("Passwort"), secret: $form.password, itemId: itemId, field: "password") {
                Button {
                    generatorFor = .password
                } label: {
                    UwUIcon(.generate, size: .sm, label: Text(vt("Passwort-Generator")))
                }
                .buttonStyle(.borderless)
                .help(vt("Passwort-Generator"))
            }
            SecretField(label: vt("Einmal-Code (TOTP)"), secret: $form.totp, itemId: itemId, field: "totp",
                        hint: vt("Der Schlüssel aus der App: Base32 oder eine otpauth://-Adresse."))
        }
        Section(vt("Websites")) {
            ForEach($form.uris) { $row in
                let index = (form.uris.firstIndex { $0.id == row.id } ?? 0) + 1
                VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                    HStack {
                        TextField(vt("Adresse \(index)"), text: $row.uri, prompt: Text(verbatim: "https://…"))
                            .autocorrectionDisabled()
                            #if os(iOS)
                            .textInputAutocapitalization(.never)
                            .keyboardType(.URL)
                            #endif
                        Button {
                            form.uris.removeAll { $0.id == row.id }
                        } label: {
                            UwUIcon(.delete, size: .sm, label: Text(vt("Adresse \(index) entfernen")))
                        }
                        .buttonStyle(.borderless)
                    }
                    Picker(vt("Wann diese Adresse passt"), selection: $row.match) {
                        Text(vt("Standard")).tag(Int?.none)
                        ForEach(UriMatch.all, id: \.self) { match in Text(UriMatch.label(match)).tag(Int?.some(match)) }
                    }
                    .uwuFont(.footnote)
                }
            }
            Button {
                form.uris.append(.init(id: key(), uri: "", match: nil))
            } label: {
                Label(vt("Website hinzufügen"), uwuIcon: .add)
            }
        }
    }

    private var cardSection: some View {
        Section {
            TextField(vt("Karteninhaber"), text: $form.cardholderName)
                .textContentType(.name)
            Picker(vt("Marke"), selection: $form.brand) {
                Text(vt("Keine Angabe")).tag("")
                ForEach(cardBrands, id: \.self) { Text($0).tag($0) }
                if !form.brand.isEmpty, !cardBrands.contains(form.brand) {
                    Text(form.brand).tag(form.brand)
                }
            }
            SecretField(label: vt("Kartennummer"), secret: $form.number, itemId: itemId, field: "card-number", numeric: true)
            Picker(vt("Gültig bis (Monat)"), selection: $form.expMonth) {
                Text(verbatim: "—").tag("")
                ForEach(1...12, id: \.self) { month in
                    Text(String(format: "%02d", month)).tag(String(month))
                }
                if !form.expMonth.isEmpty, Int(form.expMonth).map({ !(1...12).contains($0) || String($0) != form.expMonth }) ?? true {
                    Text(form.expMonth).tag(form.expMonth)
                }
            }
            TextField(vt("Gültig bis (Jahr)"), text: $form.expYear)
                #if os(iOS)
                .keyboardType(.numberPad)
                #endif
                .onChange(of: form.expYear) { _, year in
                    let clean = String(year.filter(\.isNumber).prefix(4))
                    if clean != year { form.expYear = clean }
                }
            SecretField(label: vt("Prüfnummer"), secret: $form.code, itemId: itemId, field: "card-code", numeric: true)
        }
    }

    private var identitySection: some View {
        Section {
            ForEach(IdentityFields.all, id: \.name) { field in
                let label = IdentityFields.label(field.name)
                if field.sensitive {
                    SecretField(
                        label: label,
                        secret: Binding(
                            get: { form.identitySecrets[field.name] ?? .kept(false) },
                            set: { form.identitySecrets[field.name] = $0 }),
                        itemId: itemId, field: "identity:\(field.name)")
                } else {
                    TextField(label, text: Binding(
                        get: { form.identity[field.name] ?? "" },
                        set: { form.identity[field.name] = $0 }))
                }
            }
        }
    }

    private var sshSection: some View {
        Section {
            SecretField(label: vt("Privater Schlüssel"), secret: $form.privateKey, itemId: itemId,
                        field: "ssh-private", multiline: true)
            TextField(vt("Öffentlicher Schlüssel"), text: $form.publicKey, axis: .vertical)
                .font(.uwuMono(.callout))
                .autocorrectionDisabled()
            VStack(alignment: .leading, spacing: 2) {
                TextField(vt("Fingerprint"), text: $form.fingerprint)
                    .font(.uwuMono(.callout))
                    .autocorrectionDisabled()
                Text(vt("Zum Beispiel aus „ssh-keygen -lf schluessel.pub“."))
                    .uwuFont(.caption).foregroundStyle(.secondary)
            }
        } footer: {
            Text(vt("Ein SSH-Schlüssel braucht alle drei Teile – privater Schlüssel, öffentlicher Schlüssel und Fingerprint. Der Server wirft den Eintrag sonst weg."))
        }
    }

    @ViewBuilder
    private var wifiSections: some View {
        Section(vt("WLAN")) {
            TextField(vt("Netzwerkname (SSID)"), text: Binding(
                get: { form.wifi.ssid },
                set: { ssid in
                    // The name follows the SSID as long as it was the SSID (or empty).
                    let ssid = String(ssid.prefix(64))
                    if form.name.trimmingCharacters(in: .whitespaces).isEmpty || form.name == form.wifi.ssid {
                        form.name = ssid
                    }
                    form.wifi.ssid = ssid
                }))
                .autocorrectionDisabled()
            Picker(vt("Sicherheit"), selection: $form.wifi.security) {
                ForEach(WifiFields.securities, id: \.self) { Text(WifiFields.securityLabel($0)).tag($0) }
                if !WifiFields.securities.contains(form.wifi.security) {
                    Text(form.wifi.security).tag(form.wifi.security)
                }
            }
            if form.wifi.security != "None" {
                SecretField(
                    label: vt("WLAN-Passwort"), secret: $form.wifi.password,
                    itemId: form.wifi.from[.password] == nil ? nil : itemId,
                    field: form.wifi.from[.password].map { "field:\($0)" }
                ) {
                    Button {
                        generatorFor = .wifi
                    } label: {
                        UwUIcon(.generate, size: .sm, label: Text(vt("Passwort-Generator")))
                    }
                    .buttonStyle(.borderless)
                }
            }
            Toggle(vt("Verstecktes Netzwerk (sendet seinen Namen nicht)"), isOn: $form.wifi.hidden)
        }
        if WifiFields.isEnterprise(form.wifi.security) {
            Section(vt("Enterprise (802.1X)")) {
                Picker(vt("EAP-Methode"), selection: $form.wifi.eap) {
                    Text(vt("Keine Angabe")).tag("")
                    ForEach(WifiFields.eapMethods, id: \.self) { Text($0).tag($0) }
                    if !form.wifi.eap.isEmpty, !WifiFields.eapMethods.contains(form.wifi.eap) {
                        Text(form.wifi.eap).tag(form.wifi.eap)
                    }
                }
                Picker(vt("Phase 2"), selection: $form.wifi.phase2) {
                    Text(vt("Keine Angabe")).tag("")
                    ForEach(WifiFields.phase2Methods, id: \.self) { Text($0 == "none" ? vt("Keine") : $0).tag($0) }
                    if !form.wifi.phase2.isEmpty, !WifiFields.phase2Methods.contains(form.wifi.phase2) {
                        Text(form.wifi.phase2).tag(form.wifi.phase2)
                    }
                }
                TextField(vt("Identität"), text: $form.wifi.identity).autocorrectionDisabled()
                TextField(vt("Anonyme Identität"), text: $form.wifi.anonymous).autocorrectionDisabled()
                VStack(alignment: .leading, spacing: 2) {
                    TextField(vt("CA-Zertifikat"), text: $form.wifi.ca).autocorrectionDisabled()
                    Text(vt("Die Domain des Servers oder ein Hinweis, welches Zertifikat gilt."))
                        .uwuFont(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }

    private var fieldsSection: some View {
        Section(vt("Eigene Felder")) {
            ForEach($form.fields) { $field in
                let index = (form.fields.firstIndex { $0.id == field.id } ?? 0) + 1
                VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                    HStack {
                        TextField(vt("Name von Feld \(index)"), text: $field.name, prompt: Text(vt("Feldname")))
                            .uwuFont(.subheadline, weight: .medium)
                        Button {
                            form.fields.removeAll { $0.id == field.id }
                        } label: {
                            UwUIcon(.delete, size: .sm, label: Text(vt("Feld \(index) entfernen")))
                        }
                        .buttonStyle(.borderless)
                    }
                    switch field.kind {
                    case .linked:
                        Text(vt("verknüpft mit einem anderen Feld")).uwuFont(.caption).foregroundStyle(.secondary)
                    case .boolean:
                        Toggle(field.value.value == "true" ? vt("Ja") : vt("Nein"), isOn: Binding(
                            get: { field.value.value == "true" },
                            set: { field.value = .typed($0 ? "true" : "false") }))
                    case .hidden:
                        SecretField(label: vt("Wert"), secret: $field.value,
                                    itemId: field.from == nil ? nil : itemId,
                                    field: field.from.map { "field:\($0)" })
                    case .text:
                        TextField(vt("Wert von Feld \(index)"), text: Binding(
                            get: { field.value.value },
                            set: { field.value = .typed($0) }), prompt: Text(vt("Wert")))
                    }
                }
            }
            HStack(spacing: UwUSpacing.sm) {
                ForEach([FieldKind.text, .hidden, .boolean], id: \.self) { fieldKind in
                    Button {
                        form.fields.append(.init(
                            id: key(), name: "", kind: fieldKind,
                            value: .typed(fieldKind == .boolean ? "false" : ""), from: nil))
                    } label: {
                        Label(fieldKind.label, uwuIcon: .add)
                    }
                    .buttonStyle(.borderless)
                }
            }
        }
    }
}

extension FieldKind {
    /// FIELD_KIND_LABEL.
    var label: String {
        switch self {
        case .text: vt("Text")
        case .hidden: vt("Versteckt")
        case .boolean: vt("Ja/Nein")
        case .linked: vt("Verknüpft")
        }
    }
}

/// A field for a value the editor may not have (ItemEditor.tsx's SecretField): dots until
/// someone asks to see it; typing replaces it, and an empty field never touched leaves it alone.
struct SecretField<Actions: View>: View {
    let label: String
    @Binding var secret: Secret
    var itemId: String?
    var field: String?
    var multiline = false
    var numeric = false
    var hint: String?
    @ViewBuilder var actions: Actions
    @Environment(VaultModel.self) private var vault

    private var canReveal: Bool { itemId != nil && field != nil }
    private var cleared: Bool { !secret.keep && secret.value.isEmpty && canReveal }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: multiline ? .top : .center) {
                input
                if secret.keep, canReveal {
                    Button(action: reveal) {
                        UwUIcon(.show, size: .sm, label: Text(vt("\(label) zeigen")))
                    }
                    .buttonStyle(.borderless)
                    .help(vt("\(label) zeigen"))
                }
                actions
                if !secret.keep, canReveal {
                    Button {
                        secret = .kept(true)
                    } label: {
                        UwUIcon(.undo, size: .sm, label: Text(vt("\(label) unverändert lassen")))
                    }
                    .buttonStyle(.borderless)
                    .help(vt("\(label) unverändert lassen"))
                }
            }
            if secret.keep, canReveal {
                Text(vt("Bleibt, wie es ist.")).uwuFont(.caption).foregroundStyle(.secondary)
            } else if cleared {
                Text(vt("Wird beim Speichern geleert.")).uwuFont(.caption).foregroundStyle(.uwuWarningInk)
            } else if let hint {
                Text(hint).uwuFont(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private var text: Binding<String> {
        Binding(
            get: { secret.value },
            set: { value in
                // Deleting what was typed goes back to leaving the value alone.
                if value.isEmpty, secret.keep { return }
                secret = .typed(value)
            })
    }

    @ViewBuilder
    private var input: some View {
        let prompt = Text(secret.keep && canReveal ? "••••••••••••" : label)
        Group {
            if multiline {
                TextField(label, text: text, prompt: prompt, axis: .vertical)
                    .lineLimit(4...)
            } else {
                TextField(label, text: text, prompt: prompt)
            }
        }
        .font(.uwuMono(.body))
        .autocorrectionDisabled()
        #if os(iOS)
        .textInputAutocapitalization(.never)
        .keyboardType(numeric ? .numberPad : .asciiCapable)
        #endif
        .accessibilityLabel(label)
    }

    private func reveal() {
        guard let itemId, let field else { return }
        Task {
            do {
                secret = .typed(try await vault.backend.revealField(id: itemId, field: field))
            } catch {
                vault.show(error)
            }
        }
    }
}

extension SecretField where Actions == EmptyView {
    init(label: String, secret: Binding<Secret>, itemId: String?, field: String?,
         multiline: Bool = false, numeric: Bool = false, hint: String? = nil) {
        self.init(label: label, secret: secret, itemId: itemId, field: field,
                  multiline: multiline, numeric: numeric, hint: hint) { EmptyView() }
    }
}
