// An item's earlier versions (ItemExtras.tsx `VersionsCard`): what changed per value, secrets
// revealed on demand (hidden again after a minute), restore one, delete one or all.
import SwiftUI
import UwUDesign

struct ExtrasVersionsSection: View {
    let itemId: String
    /// The item can't be read (wrong key, …): nothing to restore onto.
    let itemBroken: Bool
    let backend: ExtrasBackend

    @State private var open = false
    @State private var versions: [UwuVersion]?
    @State private var asking: ExtrasVersionQuestion?
    @State private var busy = false
    @State private var done = 0

    private var store: UwuStore { UwuStore.shared }

    var body: some View {
        Section {
            DisclosureGroup(isExpanded: $open) {
                content
            } label: {
                Label {
                    title
                } icon: {
                    UwUIcon(.history)
                }
            }
        }
        .task(id: open) { if open { await load() } }
        .onChange(of: store.vaultChanges) { _, _ in
            if open { Task { await load() } }
        }
        .onChange(of: itemId) { _, _ in
            versions = nil
            if open { Task { await load() } }
        }
        .sensoryFeedback(.success, trigger: done)
        .confirmationDialog(
            asking.map { Text(features: $0.title) } ?? Text(verbatim: ""),
            isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
            titleVisibility: .visible, presenting: asking
        ) { question in
            Button(role: question.isRestore ? nil : .destructive) {
                run(question)
            } label: {
                Text(features: question.confirm)
            }
            Button(role: .cancel) {} label: { Text(features: "Abbrechen") }
        } message: { question in
            Text(features: question.isRestore
                ? "Der Eintrag bekommt die Werte dieser Version. Der jetzige Stand bleibt als Version erhalten."
                : "Gelöschte Versionen lassen sich nicht zurückholen.")
        }
    }

    private var title: Text {
        guard let versions else { return Text(features: "Frühere Versionen") }
        return versions.count == 1
            ? Text(features: "1 frühere Version") : Text(features: "\(versions.count) frühere Versionen")
    }

    @ViewBuilder
    private var content: some View {
        if let versions {
            if versions.isEmpty {
                Text(features: "Noch keine. Jede Änderung hebt den Stand davor hier auf.")
                    .foregroundStyle(Color.uwuMuted)
            } else {
                HStack(spacing: UwUSpacing.md) {
                    Text(features: "Damals")
                    UwUIcon(.next, size: .xs)
                    Text(features: "Jetzt")
                }
                .uwuFont(.caption, weight: .semibold)
                .foregroundStyle(Color.uwuMuted)
                .accessibilityHidden(true)

                ForEach(versions) { version in
                    ExtrasVersionRow(
                        itemId: itemId, version: version, itemBroken: itemBroken, backend: backend,
                        busy: busy
                    ) { question in
                        asking = question
                    }
                }
                if versions.count > 1 {
                    Button(role: .destructive) {
                        asking = .deleteAll
                    } label: {
                        Label(features: "Alle Versionen löschen", uwuIcon: .delete)
                    }
                    .disabled(busy)
                }
            }
        } else {
            HStack(spacing: UwUSpacing.xs) {
                ProgressView().controlSize(.small)
                Text(features: "Einen Moment …").foregroundStyle(Color.uwuMuted)
            }
        }
    }

    private func load() async {
        do {
            let loaded = try await backend.versions(itemId)
            withAnimation(.uwuOut()) { versions = loaded }
        } catch {
            versions = versions ?? []
            FeatureToasts.shared.show(error: error)
        }
    }

    private func run(_ question: ExtrasVersionQuestion) {
        busy = true
        Task {
            defer { busy = false }
            do {
                switch question {
                case .deleteAll:
                    try await backend.deleteVersions(itemId, versionId: nil)
                    FeatureToasts.shared.show(featureString("Versionen gelöscht."))
                case .restore(let id):
                    try await backend.restoreVersion(itemId, versionId: id)
                    FeatureToasts.shared.show(featureString("Version wiederhergestellt ✧"))
                case .delete(let id):
                    try await backend.deleteVersions(itemId, versionId: id)
                    FeatureToasts.shared.show(featureString("Version gelöscht."))
                }
                done += 1
            } catch {
                FeatureToasts.shared.show(error: error)
            }
            await load()
        }
    }
}

enum ExtrasVersionQuestion: Equatable {
    case deleteAll
    case restore(String)
    case delete(String)

    var isRestore: Bool { if case .restore = self { true } else { false } }

    var title: LocalizedStringKey {
        switch self {
        case .deleteAll: "Alle Versionen löschen?"
        case .restore: "Diese Version wiederherstellen?"
        case .delete: "Version löschen?"
        }
    }

    var confirm: LocalizedStringKey {
        switch self {
        case .deleteAll: "Alle löschen"
        case .restore: "Wiederherstellen"
        case .delete: "Löschen"
        }
    }
}

/// One earlier version: when, how many differences, the changes, and its actions.
private struct ExtrasVersionRow: View {
    let itemId: String
    let version: UwuVersion
    let itemBroken: Bool
    let backend: ExtrasBackend
    let busy: Bool
    let ask: (ExtrasVersionQuestion) -> Void

    private var when: String { FeatureDate.short(version.revisionDate) ?? featureString("Früher") }

    private var count: String {
        switch version.changes.count {
        case 0: featureString("gleich wie jetzt")
        case 1: featureString("1 Unterschied")
        case let n: featureString("\(n) Unterschiede")
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: UwUSpacing.xs) {
            HStack(spacing: UwUSpacing.xs) {
                if version.broken {
                    UwUIcon(.warning, size: .sm, label: Text(features: "Diese Version lässt sich nicht lesen."))
                        .foregroundStyle(Color.uwuWarningInk)
                }
                Text(verbatim: "\(when) · \(count)")
                    .uwuFont(.subheadline, weight: .semibold)
            }

            ForEach(Array(version.changes.enumerated()), id: \.offset) { _, change in
                ExtrasChangeLine(itemId: itemId, versionId: version.id, change: change, backend: backend)
            }

            HStack(spacing: UwUSpacing.sm) {
                Button(role: .destructive) {
                    ask(.delete(version.id))
                } label: {
                    Label(features: "Version löschen", uwuIcon: .delete)
                }
                Spacer()
                Button {
                    ask(.restore(version.id))
                } label: {
                    Label(features: "Wiederherstellen", uwuIcon: .restore)
                }
                .disabled(version.broken || itemBroken)
            }
            .buttonStyle(.borderless)
            .uwuFont(.subheadline)
            .disabled(busy)
        }
        .padding(.vertical, UwUSpacing.xxs)
        .contextMenu {
            Button { ask(.restore(version.id)) } label: { Label(features: "Wiederherstellen", uwuIcon: .restore) }
                .disabled(version.broken || itemBroken)
            Button(role: .destructive) { ask(.delete(version.id)) } label: {
                Label(features: "Version löschen", uwuIcon: .delete)
            }
        }
        #if os(iOS)
        .swipeActions(edge: .trailing) {
            Button(role: .destructive) { ask(.delete(version.id)) } label: {
                Label(features: "Version löschen", uwuIcon: .delete)
            }
        }
        .swipeActions(edge: .leading) {
            if !(version.broken || itemBroken) {
                Button { ask(.restore(version.id)) } label: {
                    Label(features: "Wiederherstellen", uwuIcon: .restore)
                }
                .tint(.uwuPinkSolid)
            }
        }
        #endif
    }
}

/// One value's change: its name, then before → after (secrets as dots until revealed).
private struct ExtrasChangeLine: View {
    let itemId: String
    let versionId: String
    let change: VersionChange
    let backend: ExtrasBackend

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(verbatim: ExtrasValueName.label(change.field, own: change.label))
                .uwuFont(.caption, weight: .semibold)
                .foregroundStyle(Color.uwuMuted)
            if change.secret {
                HStack(spacing: UwUSpacing.xs) {
                    side(before: true)
                    UwUIcon(.next, size: .xs).foregroundStyle(Color.uwuFaint).accessibilityHidden(true)
                    side(before: false)
                }
            } else {
                HStack(alignment: .firstTextBaseline, spacing: UwUSpacing.xs) {
                    plain(change.kind == "added" ? nil : change.before)
                    UwUIcon(.next, size: .xs).foregroundStyle(Color.uwuFaint).accessibilityHidden(true)
                    plain(change.kind == "removed" ? nil : change.after)
                }
            }
        }
    }

    private func plain(_ value: String?) -> some View {
        Text(verbatim: (value?.isEmpty ?? true) ? "—" : value!)
            .uwuFont(.callout)
            .lineLimit(3)
            .textSelection(.enabled)
            .foregroundStyle((value?.isEmpty ?? true) ? Color.uwuFaint : Color.primary)
    }

    @ViewBuilder
    private func side(before: Bool) -> some View {
        let missing = before ? change.kind == "added" : change.kind == "removed"
        if missing {
            Text(verbatim: "—").foregroundStyle(Color.uwuFaint)
        } else {
            ExtrasSecretValue {
                try await backend.revealVersionField(itemId, versionId: before ? versionId : nil, field: change.field)
            }
        }
    }
}

/// A secret shown as dots, revealed on demand and hidden again after a minute.
struct ExtrasSecretValue: View {
    let reveal: () async throws -> String

    @State private var value: String?
    @State private var loading = false
    @State private var hideAt = 0

    var body: some View {
        HStack(spacing: UwUSpacing.xxs) {
            Text(verbatim: value ?? "••••••••")
                .uwuFont(.callout, weight: value == nil ? nil : .regular)
                .monospaced(value != nil)
                .lineLimit(2)
                .textSelection(.enabled)
            Button {
                toggle()
            } label: {
                UwUIcon(value == nil ? .show : .hide, size: .sm,
                        label: value == nil ? Text(features: "Zeigen") : Text(features: "Verbergen"))
            }
            .buttonStyle(.borderless)
            .disabled(loading)
            #if os(macOS)
            .help(value == nil ? Text(features: "Zeigen") : Text(features: "Verbergen"))
            #endif
        }
        .task(id: hideAt) {
            guard value != nil else { return }
            try? await Task.sleep(for: .seconds(60))
            if !Task.isCancelled { value = nil }
        }
    }

    private func toggle() {
        if value != nil {
            value = nil
            return
        }
        loading = true
        Task {
            defer { loading = false }
            do {
                value = try await reveal()
                hideAt += 1
            } catch {
                FeatureToasts.shared.show(error: error)
            }
        }
    }
}
