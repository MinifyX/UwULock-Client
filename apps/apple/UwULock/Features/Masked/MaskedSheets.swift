import SwiftUI
import UwUDesign

// MARK: New address

/// A new masked address (mobile/pages/Masked.tsx `NewAddress`, the desktop dialog's form): for a
/// site, with a description, optionally linked to an item. Made, it is copied.
struct MaskedNewAddressSheet: View {
    /// Where the mails go, when known.
    let to: String?
    let items: [ItemSummary]
    let create: (_ site: String?, _ description: String?, _ cipherId: String?) async throws -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var site = ""
    @State private var description = ""
    @State private var cipherId: String?
    @State private var picking = false
    @State private var busy = false
    @State private var error: String?
    @State private var made = 0
    @FocusState private var focused: Bool

    private var dirty: Bool { !site.isEmpty || !description.isEmpty || cipherId != nil }

    private var linkedName: String? {
        guard let cipherId else { return nil }
        guard let item = items.first(where: { $0.id == cipherId }) else { return nil }
        return item.name.isEmpty ? featureString("(ohne Namen)") : item.name
    }

    var body: some View {
        NavigationStack {
            Form {
                if let error {
                    Section {
                        Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                            .foregroundStyle(.uwuDangerInk)
                    }
                }
                Section {
                    TextField(text: $site, prompt: Text(verbatim: "shop.example.com")) {
                        Text(features: "Für Website")
                    }
                    .focused($focused)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .keyboardType(.URL)
                    #endif
                    TextField(text: $description, prompt: Text(features: "Wofür ist die Adresse?")) {
                        Text(features: "Beschreibung")
                    }
                    .onChange(of: description) { _, value in
                        if value.count > 200 { description = String(value.prefix(200)) }
                    }
                } footer: {
                    if let to {
                        Text(features: "Mails an die Adresse landen bei \(to). UwUMail erstellt sie, du kannst sie jederzeit abschalten.")
                    } else {
                        Text(features: "UwUMail erstellt die Adresse, du kannst sie jederzeit abschalten.")
                    }
                }
                if !items.isEmpty {
                    Section {
                        Button {
                            picking = true
                        } label: {
                            HStack {
                                Label(features: "Eintrag", uwuIcon: .link)
                                    .foregroundStyle(.uwuInk)
                                Spacer()
                                if let linkedName {
                                    Text(verbatim: linkedName).foregroundStyle(.uwuMuted)
                                } else {
                                    Text(features: "Kein Eintrag").foregroundStyle(.uwuMuted)
                                }
                            }
                        }
                        if cipherId != nil {
                            Button(role: .destructive) {
                                cipherId = nil
                            } label: {
                                Text(features: "Verknüpfung entfernen")
                            }
                        }
                    } footer: {
                        Text(features: "Verknüpft zeigt der Eintrag die Adresse an.")
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(Text(features: "Neue maskierte Adresse"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(role: .cancel) {
                        dismiss()
                    } label: {
                        Text(features: "Abbrechen")
                    }
                    .disabled(busy)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        Task { await submit() }
                    } label: {
                        if busy {
                            Text(features: "Erstellt …")
                        } else {
                            Text(features: "Erstellen")
                        }
                    }
                    .disabled(busy)
                    .keyboardShortcut(.defaultAction)
                }
            }
            .sheet(isPresented: $picking) {
                MaskedItemPicker(items: items, selected: cipherId) { picked in
                    cipherId = picked
                }
            }
        }
        .interactiveDismissDisabled(dirty || busy)
        .sensoryFeedback(.success, trigger: made)
        .onAppear { focused = true }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        #else
        .frame(minWidth: 440, minHeight: 360)
        #endif
    }

    private func submit() async {
        busy = true
        error = nil
        defer { busy = false }
        do {
            let site = site.trimmingCharacters(in: .whitespacesAndNewlines)
            let description = description.trimmingCharacters(in: .whitespacesAndNewlines)
            try await create(
                site.isEmpty ? nil : site, description.isEmpty ? nil : description, cipherId)
            made += 1
            dismiss()
        } catch {
            self.error = Core.failure(error).message
        }
    }
}

// MARK: Link

/// Links a masked address to an item of the vault.
struct MaskedLinkSheet: View {
    let address: MaskedAddress
    let items: [ItemSummary]
    let link: (_ cipherId: String) async throws -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        MaskedItemList(
            items: items, selected: address.cipherId, error: error, busy: busy,
            title: Text(features: "Mit Eintrag verknüpfen"),
            lead: Text(features: "Welcher Eintrag gehört zu \(address.email)?")
        ) { picked in
            guard let picked, picked != address.cipherId else {
                dismiss()
                return
            }
            busy = true
            error = nil
            Task {
                defer { busy = false }
                do {
                    try await link(picked)
                    dismiss()
                } catch {
                    self.error = Core.failure(error).message
                }
            }
        }
    }
}

/// Picks the item a new address is linked to.
private struct MaskedItemPicker: View {
    let items: [ItemSummary]
    let selected: String?
    let pick: (String?) -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        MaskedItemList(
            items: items, selected: selected, error: nil, busy: false,
            title: Text(features: "Eintrag wählen"), lead: nil
        ) { picked in
            pick(picked)
            dismiss()
        }
    }
}

/// The vault's items, logins first, searchable; tapping one picks it.
private struct MaskedItemList: View {
    let items: [ItemSummary]
    let selected: String?
    let error: String?
    let busy: Bool
    let title: Text
    let lead: Text?
    let pick: (String?) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    private var shown: [ItemSummary] {
        let words = query.trimmingCharacters(in: .whitespaces).lowercased()
        return items
            .filter { !$0.deleted }
            .filter { item in
                guard !words.isEmpty else { return true }
                return [item.name, item.subtitle ?? "", item.host ?? ""]
                    .joined(separator: " ").lowercased().contains(words)
            }
            .sorted { a, b in
                if (a.kind == .login) != (b.kind == .login) { return a.kind == .login }
                return a.name.localizedStandardCompare(b.name) == .orderedAscending
            }
    }

    var body: some View {
        NavigationStack {
            List {
                if let lead {
                    Section { lead.foregroundStyle(.uwuMuted) }
                }
                if let error {
                    Section {
                        Label { Text(verbatim: error) } icon: { UwUIcon(.warning, size: .sm) }
                            .foregroundStyle(.uwuDangerInk)
                    }
                }
                Section {
                    ForEach(shown) { item in
                        Button {
                            pick(item.id)
                        } label: {
                            HStack(spacing: UwUSpacing.sm) {
                                UwUIcon(item.kind.icon, size: .md)
                                    .foregroundStyle(.uwuMuted)
                                    .accessibilityHidden(true)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(verbatim: item.name.isEmpty ? featureString("(ohne Namen)") : item.name)
                                        .foregroundStyle(.uwuInk)
                                    if let sub = item.host ?? item.subtitle, !sub.isEmpty {
                                        Text(verbatim: sub)
                                            .uwuFont(.caption)
                                            .foregroundStyle(.uwuMuted)
                                    }
                                }
                                Spacer()
                                if item.id == selected {
                                    UwUIcon(.done, size: .sm)
                                        .foregroundStyle(.uwuPinkSolid)
                                        .accessibilityLabel(Text(features: "Ausgewählt"))
                                }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(busy)
                    }
                }
            }
            .overlay {
                if shown.isEmpty {
                    if query.isEmpty {
                        FeatureEmptyState(mood: .puzzled, title: Text(features: "Keine Einträge"))
                    } else {
                        ContentUnavailableView.search(text: query)
                    }
                }
            }
            .searchable(text: $query, prompt: Text(features: "Einträge durchsuchen"))
            .navigationTitle(title)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(role: .cancel) {
                        dismiss()
                    } label: {
                        Text(features: "Abbrechen")
                    }
                }
                if busy {
                    ToolbarItem(placement: .confirmationAction) { ProgressView().controlSize(.small) }
                }
            }
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        #else
        .frame(minWidth: 420, minHeight: 440)
        #endif
    }
}

#if DEBUG
#Preview("Neue Adresse") {
    MaskedNewAddressSheet(to: "nyu@example.com", items: MaskedFixtures.items) { _, _, _ in }
}

#Preview("Verknüpfen") {
    MaskedLinkSheet(address: MaskedFixtures.addresses[1], items: MaskedFixtures.items) { _ in }
}
#endif
