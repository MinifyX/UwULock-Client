import SwiftUI
import UwUDesign

/// The generator's options, remembered (never a password) under "uwulock.generator" like the
/// desktop's localStorage key (GeneratorDialog.tsx, lib/generator.ts).
enum GeneratorStore {
    static let key = "uwulock.generator"
    static let maxLength = 128

    static func load() -> GeneratorOptions {
        guard let data = UserDefaults.standard.data(forKey: key),
            var options = try? JSONDecoder().decode(GeneratorOptions.self, from: data)
        else { return GeneratorOptions() }
        options.length = min(maxLength, max(5, options.length))
        for set in CharSet.allCases {
            if let value = options[min: set] { options[min: set] = min(maxLength, max(0, value)) }
        }
        // Minimums that can't fit any more start over.
        if required(options) > maxLength {
            for set in CharSet.allCases { options[min: set] = nil }
        }
        return options
    }

    static func save(_ options: GeneratorOptions) {
        if let data = try? JSONEncoder().encode(options) { UserDefaults.standard.set(data, forKey: key) }
    }

    enum CharSet: CaseIterable, Hashable {
        case uppercase, lowercase, digits, symbols

        var label: String {
            switch self {
            case .uppercase: "A–Z"
            case .lowercase: "a–z"
            case .digits: "0–9"
            case .symbols: "!@#$%^&*"
            }
        }
    }

    static func minimum(_ options: GeneratorOptions, _ set: CharSet) -> Int {
        max(1, options[min: set] ?? 0)
    }

    /// What the minimums of the sets that are on add up to (lower case alone when none is).
    static func required(_ options: GeneratorOptions) -> Int {
        let on = CharSet.allCases.filter { options[on: $0] }
        return (on.isEmpty ? [.lowercase] : on).reduce(0) { $0 + minimum(options, $1) }
    }

    /// `withMinimum`: kept so the minimums of the sets that are on fit into 128 together.
    static func withMinimum(_ options: GeneratorOptions, _ set: CharSet, _ value: Int) -> GeneratorOptions {
        var probe = options
        probe[on: set] = true
        probe[min: set] = 1
        let others = required(probe) - 1
        var next = options
        next[min: set] = min(max(value, 1), max(1, maxLength - others))
        return next
    }

    /// `strength`: the meter's level and word.
    static func strength(_ bits: Int) -> (level: Int, label: String) {
        if bits < 50 { return (1, vt("schwach")) }
        if bits < 75 { return (2, vt("okay")) }
        if bits < 100 { return (3, vt("stark")) }
        return (4, vt("sehr stark ✧"))
    }
}

extension GeneratorOptions {
    subscript(on set: GeneratorStore.CharSet) -> Bool {
        get {
            switch set {
            case .uppercase: uppercase
            case .lowercase: lowercase
            case .digits: digits
            case .symbols: symbols
            }
        }
        set {
            switch set {
            case .uppercase: uppercase = newValue
            case .lowercase: lowercase = newValue
            case .digits: digits = newValue
            case .symbols: symbols = newValue
            }
        }
    }

    subscript(min set: GeneratorStore.CharSet) -> Int? {
        get {
            switch set {
            case .uppercase: minUppercase
            case .lowercase: minLowercase
            case .digits: minNumber
            case .symbols: minSpecial
            }
        }
        set {
            switch set {
            case .uppercase: minUppercase = newValue
            case .lowercase: minLowercase = newValue
            case .digits: minNumber = newValue
            case .symbols: minSpecial = newValue
            }
        }
    }
}

/// The password generator's body: the password, the meter, the options. `onUse` set: opened
/// from the editor, the password goes into the field.
struct PasswordGenerator: View {
    var onUse: ((String) -> Void)?
    @Environment(VaultModel.self) private var vault
    @Environment(\.uwuReduceMotion) private var reduceMotion
    @State private var options = GeneratorStore.load()
    @State private var result: Generated?
    @State private var rolls = 0

    var body: some View {
        Section {
            VStack(alignment: .leading, spacing: UwUSpacing.sm) {
                Group {
                    if let result {
                        coloredSecret(result.password)
                    } else {
                        Text(verbatim: "…")
                    }
                }
                .font(.uwuMono(.title3, weight: .medium))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, minHeight: 56, alignment: .leading)
                .contentTransition(.opacity)
                .accessibilityLabel(vt("Erzeugtes Passwort"))
                if let result { meter(result) }
            }
            .padding(.vertical, UwUSpacing.xs)
            HStack {
                Button {
                    rolls += 1
                } label: {
                    Label(vt("Neu würfeln"), uwuIcon: .generate)
                }
                .uwuGlassButton()
                .keyboardShortcut("r", modifiers: .command)
                Spacer()
                if let onUse {
                    Button {
                        if let result { onUse(result.password) }
                    } label: {
                        Text(vt("Übernehmen"))
                    }
                    .uwuProminentButton()
                    .disabled(result == nil)
                } else {
                    Button(action: copy) {
                        Text(vt("Kopieren"))
                    }
                    .uwuProminentButton()
                    .disabled(result == nil)
                }
            }
            .buttonStyle(.borderless)
        }

        Section {
            VStack(alignment: .leading) {
                HStack {
                    Text(vt("Länge"))
                    Text("\(options.length)").bold().monospacedDigit()
                    if let result, result.length > options.length {
                        Text(vt("→ \(result.length), damit die Mindestanzahlen passen"))
                            .uwuFont(.caption).foregroundStyle(.secondary)
                    }
                }
                Slider(
                    value: Binding(
                        get: { Double(min(options.length, 64)) },
                        set: { options.length = Int($0.rounded()) }),
                    in: 5...64, step: 1
                ) {
                    Text(vt("Länge"))
                }
            }
            ForEach(GeneratorStore.CharSet.allCases, id: \.self) { set in
                HStack {
                    Text(set.label).font(.uwuMono(.body))
                    Spacer()
                    if options[on: set] {
                        Stepper(
                            value: Binding(
                                get: { GeneratorStore.minimum(options, set) },
                                set: { options = GeneratorStore.withMinimum(options, set, $0) }),
                            in: 1...GeneratorStore.maxLength
                        ) {
                            Text(vt("mind. \(GeneratorStore.minimum(options, set))"))
                                .uwuFont(.caption).foregroundStyle(.secondary)
                        }
                        .accessibilityLabel(vt("Mindestens \(set.label)"))
                        .fixedSize()
                    }
                    Toggle(isOn: Binding(
                        get: { options[on: set] },
                        set: { on in
                            var next = options
                            next[on: set] = on
                            // At least one set stays on.
                            if GeneratorStore.CharSet.allCases.contains(where: { next[on: $0] }) { options = next }
                        })
                    ) {
                        Text(set.label)
                    }
                    .labelsHidden()
                }
            }
            Toggle(vt("Verwechselbare Zeichen weglassen (l, 1, I, O, 0)"), isOn: $options.avoidAmbiguous)
        } header: {
            Text(vt("Zeichen"))
        }
        .task(id: "\(options.hashValue)-\(rolls)") { await roll() }
    }

    private func meter(_ result: Generated) -> some View {
        let (level, label) = GeneratorStore.strength(result.bits)
        let tint: Color = level == 1 ? .uwuDanger : level == 2 ? .uwuWarning : level == 3 ? .uwuSuccess : .uwuPinkSolid
        return HStack(spacing: UwUSpacing.sm) {
            HStack(spacing: 3) {
                ForEach(1...4, id: \.self) { step in
                    Capsule().fill(step <= level ? tint : Color.uwuHairline)
                        .frame(width: 28, height: 5)
                }
            }
            Text("\(label) · \(vt("\(result.bits) Bit"))")
                .uwuFont(.caption)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }

    private func roll() async {
        GeneratorStore.save(options)
        do {
            let next = try await vault.backend.generatePassword(options: options)
            withUwUAnimation(.uwuOut(), reduceMotion: reduceMotion) { result = next }
        } catch {
            vault.show(error)
        }
    }

    private func copy() {
        guard let result else { return }
        vault.perform {
            try await vault.backend.copyGenerated(text: result.password)
            vault.say(copiedText("password"), tone: .success)
        }
    }
}

/// A new masked address from UwUMail, for a site, copied right away (MaskedGenerator).
struct MaskedGenerator: View {
    @Environment(VaultModel.self) private var vault
    @State private var connection: MaskedConnection?
    @State private var site = ""
    @State private var description = ""
    @State private var made: MaskedAddress?
    @State private var busy = false

    var body: some View {
        Group {
            if let connection {
                if !connection.connected || connection.status == "revoked" {
                    Section { MaskedNotConnectedView(connection: connection) }
                } else {
                    form
                }
            } else {
                Section {
                    HStack { Spacer(); ProgressView(); Spacer() }
                }
            }
        }
        .task {
            do {
                connection = try await vault.backend.maskedConnection()
            } catch {
                vault.show(error)
                connection = MaskedConnection(connected: false, server: nil, username: nil, domains: nil,
                                              defaultDomain: nil, status: nil)
            }
        }
    }

    @ViewBuilder
    private var form: some View {
        Section {
            Text(made?.email ?? "…")
                .font(.uwuMono(.title3, weight: .medium))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
        }
        Section {
            TextField(vt("Für Website"), text: $site, prompt: Text(verbatim: "shop.example.com"))
                .autocorrectionDisabled()
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .keyboardType(.URL)
                #endif
            TextField(vt("Beschreibung"), text: $description)
        }
        Section {
            HStack {
                if let made {
                    Button {
                        copy(made.email)
                    } label: {
                        Label(vt("Kopieren"), uwuIcon: .copy)
                    }
                    .uwuGlassButton()
                }
                Spacer()
                Button(action: create) {
                    Label(made == nil ? vt("Adresse erstellen") : vt("Noch eine"), uwuIcon: .maskedAddress)
                }
                .uwuProminentButton()
                .disabled(busy)
            }
            .buttonStyle(.borderless)
        }
    }

    private func create() {
        busy = true
        let site = site.trimmingCharacters(in: .whitespaces)
        let description = String(description.prefix(200))
        Task {
            defer { busy = false }
            do {
                let address = try await vault.backend.createMaskedAddress(
                    forDomain: site.isEmpty ? nil : site, description: description.isEmpty ? nil : description,
                    cipherId: nil)
                made = address
                copy(address.email)
            } catch {
                vault.show(error)
            }
        }
    }

    private func copy(_ email: String) {
        vault.perform {
            try await vault.backend.copyText(text: email)
            vault.say(copiedAddressText(), tone: .success)
        }
    }
}

/// The generator on its own: the iPhone's tab, the Mac's sheet. With UwUMail also a masked address.
struct GeneratorView: View {
    @Environment(VaultModel.self) private var vault
    @State private var mode = Mode.password

    enum Mode: Hashable { case password, masked }

    private var maskable: Bool { vault.uwu.has(.maskedAddresses) }

    var body: some View {
        Form {
            if maskable {
                Section {
                    Picker(vt("Was erzeugt wird"), selection: $mode) {
                        Text(vt("Passwort")).tag(Mode.password)
                        Text(vt("Maskierte Adresse")).tag(Mode.masked)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                }
                .listRowBackground(Color.clear)
            }
            if maskable, mode == .masked {
                MaskedGenerator()
            } else {
                PasswordGenerator()
            }
        }
        .formStyle(.grouped)
        .navigationTitle(maskable ? vt("Generator") : vt("Passwort-Generator"))
    }
}

/// The generator as a sheet: on its own (Mac) or for the editor's password field.
struct GeneratorSheet: View {
    var onUse: ((String) -> Void)?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if let onUse {
                    Form { PasswordGenerator(onUse: onUse) }
                        .formStyle(.grouped)
                        .navigationTitle(vt("Passwort-Generator"))
                } else {
                    GeneratorView()
                }
            }
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(vt("Schließen")) { dismiss() }
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 440, idealWidth: 480, minHeight: 480)
        #endif
        .presentationDetents([.medium, .large])
    }
}
