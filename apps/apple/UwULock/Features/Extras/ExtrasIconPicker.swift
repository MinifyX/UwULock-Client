// Choosing an item's own icon (mobile/sheets/IconPicker.tsx, OwnIconPicker.tsx): a picture from
// Photos or a file, the device in the home network, the website's icon, or one of the icon
// library. A choice applies at once, as the item detail's icon menu does.
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers
import UwUDesign

/// What the person chose in the icon picker.
enum ExtrasIconChoice: Equatable {
    /// A square PNG `data:` URL (picture, file or library).
    case png(String)
    /// Ask the device in the home network for its icon.
    case device
    /// Back to the website's icon.
    case remove
}

private enum ExtrasIconVariant: String, CaseIterable, Identifiable {
    case `default`, light, dark
    var id: String { rawValue }
    var title: LocalizedStringKey {
        switch self {
        case .default: "Farbig"
        case .light: "Hell"
        case .dark: "Dunkel"
        }
    }
}

struct ExtrasIconPicker: View {
    let summary: ItemSummary
    let uris: [String]
    let hasOwn: Bool
    let onChoose: (ExtrasIconChoice) -> Void

    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    @State private var photo: PhotosPickerItem?
    @State private var importing = false
    @State private var busy = false
    @State private var index: IconLibrary?
    @State private var libraryError: String?
    @State private var query = ""
    @State private var variant: ExtrasIconVariant = .default

    private var store: UwuStore { UwuStore.shared }
    private var libraryOn: Bool { store.status.has(.iconLibrary) }
    private var backend: ExtrasBackend { ExtrasBackend(core: model.core) }

    private var localHost: String? {
        uris.lazy.compactMap(ExtrasHost.host(of:)).first(where: ExtrasHost.isLocal)
    }

    private var found: [LibraryIcon] {
        guard let index else { return [] }
        let text = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty
            ? index.suggest(host: localHost, name: summary.name, most: 15)
            : index.search(text, most: 30)
    }

    var body: some View {
        NavigationStack {
            Form {
                sources
                if libraryOn { library }
                if hasOwn {
                    Section {
                        Button(role: .destructive) {
                            choose(.remove)
                        } label: {
                            Label(features: "Eigenes Icon entfernen", uwuIcon: .delete)
                        }
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(Text(features: "Icon wählen"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { Text(features: "Abbrechen") }
                        .keyboardShortcut(.cancelAction)
                }
            }
            .disabled(busy)
            .overlay { if busy { ProgressView() } }
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        #else
        .frame(minWidth: 460, idealWidth: 520, minHeight: 520, idealHeight: 640)
        #endif
        .fileImporter(isPresented: $importing, allowedContentTypes: ExtrasIconImage.accepted) { result in
            switch result {
            case .success(let url): take(url: url)
            case .failure: break
            }
        }
        .onChange(of: photo) { _, item in
            guard let item else { return }
            photo = nil
            take(photo: item)
        }
        .task(id: libraryOn) { await loadLibrary() }
    }

    // MARK: Sources

    private var sources: some View {
        Section {
            PhotosPicker(selection: $photo, matching: .images, photoLibrary: .shared()) {
                ExtrasPhotoLabel()
            }
            Button {
                importing = true
            } label: {
                Label(features: "Datei hochladen", uwuIcon: .upload)
            }
            if let localHost {
                Button {
                    choose(.device)
                } label: {
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(features: "Vom Gerät im Heimnetz")
                            Text(verbatim: localHost).uwuFont(.caption).foregroundStyle(Color.uwuMuted)
                        }
                    } icon: {
                        UwUIcon(.home)
                    }
                }
            }
            Button {
                if hasOwn { choose(.remove) } else { dismiss() }
            } label: {
                HStack {
                    Label(features: "Icon der Website", uwuIcon: .website)
                    Spacer()
                    if !hasOwn {
                        UwUIcon(.done, size: .sm, label: Text(features: "Gewählt"))
                            .foregroundStyle(Color.uwuPinkInk)
                    }
                }
                .contentShape(Rectangle())
            }
        } footer: {
            Text(features: "Eigene Icons liegen verschlüsselt im Tresor, wie im Web-Tresor. Die offiziellen Bitwarden-Apps zeigen weiter das Icon der Website.")
        }
        .buttonStyle(.borderless)
        .foregroundStyle(Color.primary)
    }

    // MARK: Library

    private var library: some View {
        Section {
            Picker(selection: $variant) {
                ForEach(ExtrasIconVariant.allCases) { option in
                    Text(features: option.title).tag(option)
                }
            } label: {
                Text(features: "Variante")
            }
            .pickerStyle(.segmented)

            TextField(text: $query, prompt: Text(features: "Zum Beispiel Nextcloud")) {
                Text(features: "Bibliothek durchsuchen")
            }
            .accessibilityLabel(Text(features: "Bibliothek durchsuchen"))
            .autocorrectionDisabled()
            #if os(iOS)
            .textInputAutocapitalization(.never)
            #endif

            if let libraryError {
                Text(verbatim: libraryError).foregroundStyle(Color.uwuDangerInk)
            } else if index == nil {
                HStack(spacing: UwUSpacing.xs) {
                    ProgressView().controlSize(.small)
                    Text(features: "Lädt die Bibliothek …").foregroundStyle(Color.uwuMuted)
                }
            } else if found.isEmpty {
                Text(features: query.trimmingCharacters(in: .whitespaces).isEmpty
                    ? "Tippe einen Namen ein." : "Kein Icon gefunden.")
                    .foregroundStyle(Color.uwuMuted)
            } else {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 76), spacing: UwUSpacing.sm)], spacing: UwUSpacing.sm) {
                    ForEach(found, id: \.self) { icon in
                        ExtrasLibraryTile(icon: icon, variant: variant.rawValue, backend: backend) { png in
                            choose(.png(png))
                        }
                    }
                }
                .padding(.vertical, UwUSpacing.xxs)
            }
        } header: {
            Text(features: "Bibliothek")
        } footer: {
            if let index {
                VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                    ForEach(index.sources, id: \.self) { source in
                        ExtrasLibrarySourceLine(source: source)
                    }
                    Text(features: "Das gewählte Icon holt dein Server; im Eintrag wird es verschlüsselt gespeichert.")
                }
            }
        }
    }

    // MARK: Actions

    private func choose(_ choice: ExtrasIconChoice) {
        onChoose(choice)
        dismiss()
    }

    private func loadLibrary() async {
        guard libraryOn, index == nil else { return }
        do {
            index = try await backend.iconLibrary()
        } catch {
            libraryError = Core.failure(error).message
        }
    }

    private func take(photo item: PhotosPickerItem) {
        busy = true
        Task {
            defer { busy = false }
            do {
                guard let data = try await item.loadTransferable(type: Data.self) else {
                    throw ExtrasIconImage.Unreadable()
                }
                let type = item.supportedContentTypes.first
                choose(.png(try ExtrasIconImage.pngDataURL(from: data, type: type)))
            } catch {
                FeatureToasts.shared.show(featureString("Das Bild ließ sich nicht lesen."), danger: true)
            }
        }
    }

    private func take(url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let data = try Data(contentsOf: url)
            let type = UTType(filenameExtension: url.pathExtension)
            choose(.png(try ExtrasIconImage.pngDataURL(from: data, type: type)))
        } catch {
            FeatureToasts.shared.show(featureString("Das Bild ließ sich nicht lesen."), danger: true)
        }
    }
}

/// A library source's credit line, with its licence linked.
private struct ExtrasLibrarySourceLine: View {
    let source: LibrarySource

    var body: some View {
        if let url = URL(string: source.licenseUrl) {
            HStack(spacing: 0) {
                Text(verbatim: "\(source.name) · ")
                Link(destination: url) { Text(verbatim: source.license) }
                Text(verbatim: " · \(source.attribution)")
            }
        } else {
            Text(verbatim: "\(source.name) · \(source.license) · \(source.attribution)")
        }
    }
}

/// One library icon in the chosen variant (or its default, where it has no such variant).
private struct ExtrasLibraryTile: View {
    let icon: LibraryIcon
    let variant: String
    let backend: ExtrasBackend
    let onPick: (String) -> Void

    @State private var picture: String?
    @State private var failed = false

    private var shown: String { icon.variants.contains(variant) ? variant : "default" }

    var body: some View {
        Button {
            if let picture { onPick(picture) }
        } label: {
            VStack(spacing: UwUSpacing.xxs) {
                ExtrasIconTile(dataURL: picture, fallback: failed ? "×" : "", side: 44)
                Text(verbatim: icon.name)
                    .uwuFont(.caption2)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .foregroundStyle(Color.primary)
            }
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .disabled(picture == nil)
        .accessibilityLabel(Text(features: "\(icon.name) wählen"))
        #if os(macOS)
        .help(Text(verbatim: icon.name))
        #endif
        .task(id: shown) {
            picture = nil
            failed = false
            do {
                picture = try await backend.libraryIcon(icon, variant: shown)
            } catch {
                failed = true
            }
        }
    }
}

/// The photo picker's label, built on the main actor (PhotosPicker's label closure is not).
private struct ExtrasPhotoLabel: View {
    var body: some View {
        Label(features: "Foto auswählen", uwuIcon: .addImage)
    }
}
