// "Neues Passwort erzeugen & speichern" (GeneratorDialog.tsx with `onUse`): the generator of
// the core, its options kept like the page keeps them, "Übernehmen" hands the password over.
import SwiftUI
import UwUDesign

struct HealthGeneratorSheet: View {
    let store: HealthCheckStore
    let onUse: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @AppStorage("uwulock.generator") private var stored = ""
    @State private var options = GeneratorOptions()
    @State private var result: Generated?
    @State private var rolls = 0

    private enum CharSet: CaseIterable {
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

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    VStack(alignment: .leading, spacing: UwUSpacing.xs) {
                        Text(verbatim: result?.password ?? "…")
                            .font(.uwuMono(.title3, weight: .medium))
                            .textSelection(.enabled)
                            .lineLimit(nil)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentTransition(.opacity)
                            .accessibilityAddTraits(.updatesFrequently)
                        if let result {
                            let meter = strength(result.bits)
                            Gauge(value: Double(meter.level), in: 0...4) {
                                EmptyView()
                            }
                            .gaugeStyle(.linearCapacity)
                            .tint(meter.tone)
                            .accessibilityHidden(true)
                            Text(verbatim: "\(meter.label) · \(featureString("\(result.bits) Bit"))")
                                .uwuFont(.footnote)
                                .foregroundStyle(meter.tone)
                        }
                    }
                    .uwuAnimation(value: result)
                    HStack {
                        Button {
                            rolls += 1
                        } label: {
                            Label(features: "Neu würfeln", uwuIcon: .generate)
                        }
                        .keyboardShortcut("r")
                        Spacer()
                        Button {
                            if let result { Task { await store.copyGenerated(result.password) } }
                        } label: {
                            Label(features: "Kopieren", uwuIcon: .copy)
                        }
                        .disabled(result == nil)
                    }
                    .buttonStyle(.borderless)
                }

                Section {
                    VStack(alignment: .leading) {
                        HStack {
                            Text(features: "Länge")
                            Spacer()
                            Text(verbatim: "\(options.length)").monospacedDigit().bold()
                        }
                        Slider(
                            value: Binding(
                                get: { Double(min(options.length, 64)) },
                                set: { set(\.length, Int($0)) }),
                            in: 5...64, step: 1)
                        if let result, result.length > options.length {
                            Text(features: "→ \(result.length), damit die Mindestanzahlen passen")
                                .uwuFont(.footnote)
                                .foregroundStyle(.secondary)
                        }
                    }
                    ForEach(CharSet.allCases, id: \.self) { set in
                        Toggle(isOn: Binding(get: { isOn(set) }, set: { turn(set, $0) })) {
                            Text(verbatim: set.label).font(.uwuMono())
                        }
                    }
                } header: {
                    Text(features: "Zeichen")
                }

                Section {
                    ForEach(CharSet.allCases.filter(isOn), id: \.self) { set in
                        Stepper(value: Binding(get: { minimum(set) }, set: { setMinimum(set, $0) }), in: 1...128) {
                            HStack {
                                Text(verbatim: set.label).font(.uwuMono())
                                Spacer()
                                Text(verbatim: "\(minimum(set))").monospacedDigit()
                            }
                        }
                    }
                    Toggle(isOn: Binding(get: { options.avoidAmbiguous }, set: { set(\.avoidAmbiguous, $0) })) {
                        Text(features: "Verwechselbare Zeichen weglassen (l, 1, I, O, 0)")
                    }
                } header: {
                    Text(features: "Mindestens")
                }
            }
            .formStyle(.grouped)
            .navigationTitle(Text(features: "Passwort-Generator"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { Text(features: "Schließen") }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        if let result { onUse(result.password) }
                    } label: {
                        Label(features: "Übernehmen", uwuIcon: .done)
                    }
                    .disabled(result == nil)
                }
            }
        }
        .presentationDetents([.medium, .large])
        #if os(macOS)
        .frame(minWidth: 440, idealWidth: 480, minHeight: 520)
        #endif
        .onAppear(perform: loadOptions)
        .task(id: Roll(options: options, rolls: rolls)) {
            result = await store.generate(options)
            stored = (try? String(data: JSONEncoder().encode(options), encoding: .utf8)) ?? ""
        }
        .sensoryFeedback(.impact(weight: .light), trigger: rolls)
    }

    private struct Roll: Equatable {
        var options: GeneratorOptions
        var rolls: Int
    }

    // MARK: Options

    private func loadOptions() {
        if let data = stored.data(using: .utf8),
           let saved = try? JSONDecoder().decode(GeneratorOptions.self, from: data)
        {
            options = saved
        }
    }

    private func set<V>(_ key: WritableKeyPath<GeneratorOptions, V>, _ value: V) {
        options[keyPath: key] = value
    }

    private func isOn(_ set: CharSet) -> Bool {
        switch set {
        case .uppercase: options.uppercase
        case .lowercase: options.lowercase
        case .digits: options.digits
        case .symbols: options.symbols
        }
    }

    /// At least one set stays on.
    private func turn(_ set: CharSet, _ on: Bool) {
        var next = options
        switch set {
        case .uppercase: next.uppercase = on
        case .lowercase: next.lowercase = on
        case .digits: next.digits = on
        case .symbols: next.symbols = on
        }
        guard next.lowercase || next.uppercase || next.digits || next.symbols else { return }
        options = next
    }

    private func minimumKey(_ set: CharSet) -> WritableKeyPath<GeneratorOptions, Int?> {
        switch set {
        case .uppercase: \.minUppercase
        case .lowercase: \.minLowercase
        case .digits: \.minNumber
        case .symbols: \.minSpecial
        }
    }

    /// How many of a set the password gets at least (generator.ts `minimumOf`).
    private func minimum(_ set: CharSet) -> Int {
        max(1, options[keyPath: minimumKey(set)] ?? 0)
    }

    /// The minimums of the sets that are on fit into 128 characters together (`withMinimum`).
    private func setMinimum(_ set: CharSet, _ value: Int) {
        let others = CharSet.allCases.filter { $0 != set && isOn($0) }.reduce(0) { $0 + minimum($1) }
        options[keyPath: minimumKey(set)] = min(max(value, 1), max(1, 128 - others))
    }

    /// The meter under the password (GeneratorDialog.tsx `strength`).
    private func strength(_ bits: Int) -> (level: Int, label: String, tone: Color) {
        if bits < 50 { return (1, featureString("schwach"), .uwuDangerInk) }
        if bits < 75 { return (2, featureString("okay"), .uwuWarningInk) }
        if bits < 100 { return (3, featureString("stark"), .uwuSuccessInk) }
        return (4, featureString("sehr stark ✧"), .uwuSuccessInk)
    }
}
