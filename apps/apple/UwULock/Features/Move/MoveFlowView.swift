import SwiftUI
import UwUDesign
#if os(iOS)
import UIKit
#endif

/// Where the move dialog stands (MoveDialog.tsx `Phase`).
enum MovePhase: Equatable {
    case checking
    case noTarget(target: MoveTarget?, error: String?)
    case login(MoveTarget)
    case twoFactor(methods: [TwoFactorMethod], message: String?)
    case newDevice
    case preview(MovePreview)
    case running(preview: MovePreview, progress: MoveProgress?)
    case finished(preview: MovePreview, finished: MoveFinished)
    case failed(preview: MovePreview, error: String)

    var isRunning: Bool {
        if case .running = self { return true }
        return false
    }

    /// A name per step, for the transition between them.
    var name: String {
        switch self {
        case .checking: "checking"
        case .noTarget: "no-target"
        case .login: "login"
        case .twoFactor: "two-factor"
        case .newDevice: "new-device"
        case .preview: "preview"
        case .running: "running"
        case .finished: "finished"
        case .failed: "failed"
        }
    }
}

/// Moving a vault in from Bitwarden (cloud or self-hosted) or Vaultwarden (MoveDialog.tsx,
/// lib/moving.ts): log in to the source, look at what will move, move it. The account on screen
/// is where it goes, and it must be on a UwULock Server. Presented as a sheet.
struct MoveFlowView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    @State private var phase: MovePhase = .checking
    @State private var stopping = false
    #if DEBUG
    @State private var fixtureRun: Task<Void, Never>?
    #endif

    init() {}

    #if DEBUG
    /// Opens on `phase`, for previews.
    init(phase: MovePhase) {
        _phase = State(initialValue: phase)
    }
    #endif

    var body: some View {
        NavigationStack {
            content
                .navigationTitle(Text(features: "Von Bitwarden umziehen"))
                #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button(role: .cancel) {
                            close()
                        } label: {
                            if phase.isRunning {
                                Text(features: "Anhalten")
                            } else if case .finished = phase {
                                Text(features: "Schließen")
                            } else {
                                Text(features: "Abbrechen")
                            }
                        }
                        .disabled(phase.isRunning && stopping)
                        .keyboardShortcut(.cancelAction)
                    }
                }
        }
        .uwuAnimation(value: phase.name)
        .interactiveDismissDisabled(phase.isRunning)
        .task { await begin() }
        .task { await follow() }
        .onChange(of: phase.isRunning) { _, running in
            keepAwake(running)
            if !running { stopping = false }
        }
        .onDisappear {
            keepAwake(false)
            // However the sheet goes, the source account's session is dropped; a running move
            // stops after its current step. Calling it twice is harmless.
            #if DEBUG
            fixtureRun?.cancel()
            if FeatureFixtures.isOn { return }
            #endif
            let core = model.core
            Task { try? await core.moveClose() }
        }
        .sensoryFeedback(trigger: phase.name) { _, new in
            switch new {
            case "finished": .success
            case "failed": .error
            default: nil
            }
        }
        #if os(iOS)
        .presentationDetents([.large])
        #else
        .frame(minWidth: 520, idealWidth: 560, minHeight: 520, idealHeight: 620)
        #endif
    }

    @ViewBuilder
    private var content: some View {
        switch phase {
        case .checking:
            ProgressView { Text(features: "Einen Moment …") }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .noTarget(_, let error):
            FeatureEmptyState(
                mood: error == nil ? .puzzled : .sad,
                title: Text(features: "Umziehen geht hier nicht"),
                message: error.map { Text(verbatim: $0) }
                    ?? Text(features: "Umziehen geht nur in ein Konto auf einem UwULock Server. Füge dein UwULock-Konto hinzu (unten links), wechsle dorthin und öffne das hier noch einmal.")
            ) {
                Button {
                    close()
                } label: {
                    Text(features: "Schließen")
                }
                .uwuProminentButton()
            }
        case .login(let target):
            MoveSourceLogin(target: target, onDone: loggedIn)
        case .twoFactor(let methods, let message):
            MoveTwoFactorForm(methods: methods, message: message, onBack: back, onDone: loggedIn)
        case .newDevice:
            MoveNewDeviceForm(onBack: back, onDone: loggedIn)
        case .preview(let preview):
            MovePreviewView(preview: preview) { start(preview) }
        case .running(_, let progress):
            MoveRunningView(progress: progress, stopping: stopping) { cancel() }
        case .finished(let preview, let finished):
            MoveFinishedView(finished: finished, onContinue: { start(preview) }, onClose: close)
        case .failed(let preview, let error):
            MoveFailedView(error: error, onRetry: { start(preview) }, onClose: close)
        }
    }

    // MARK: Steps

    private func set(_ next: MovePhase) {
        withUwUAnimation(reduceMotion: reduceMotion) { phase = next }
    }

    private func begin() async {
        guard phase == .checking else { return }
        #if DEBUG
        if FeatureFixtures.isOn {
            set(Self.fixturePhase)
            if case .running = phase { runFixture(MoveFixtures.preview) }
            return
        }
        #endif
        do {
            let target = try await model.core.moveTarget()
            set(target.uwulock ? .login(target) : .noTarget(target: target, error: nil))
        } catch {
            set(.noTarget(target: nil, error: Core.failure(error).message))
        }
    }

    /// The move's events: progress, the end, a failure. Only while it runs.
    private func follow() async {
        guard !FeatureFixtures.isOn else { return }
        for await event in model.core.events() {
            guard case .running(let preview, _) = phase else { continue }
            switch event {
            case .moveProgress(let progress):
                phase = .running(preview: preview, progress: progress)
            case .moveFinished(let finished):
                set(.finished(preview: preview, finished: finished))
            case .moveFailed(let failure):
                set(.failed(preview: preview, error: failure.message))
            default:
                break
            }
        }
    }

    private func loggedIn(_ step: MoveStep) {
        switch step {
        case .done(let preview): set(.preview(preview))
        case .twoFactor(let methods, let message): set(.twoFactor(methods: methods, message: message))
        case .newDevice: set(.newDevice)
        }
    }

    private func start(_ preview: MovePreview) {
        set(.running(preview: preview, progress: nil))
        #if DEBUG
        if FeatureFixtures.isOn {
            runFixture(preview)
            return
        }
        #endif
        Task {
            do {
                try await model.core.moveStart()
            } catch {
                set(.failed(preview: preview, error: Core.failure(error).message))
            }
        }
    }

    private func cancel() {
        stopping = true
        #if DEBUG
        if FeatureFixtures.isOn {
            fixtureRun?.cancel()
            if case .running(let preview, _) = phase {
                set(.finished(preview: preview, finished: MoveFixtures.cancelled))
            }
            return
        }
        #endif
        Task { try? await model.core.moveCancel() }
    }

    /// Back to the login: whatever the target was, it still is.
    private func back() {
        #if DEBUG
        if FeatureFixtures.isOn {
            set(.login(MoveFixtures.target))
            return
        }
        #endif
        Task {
            try? await model.core.moveClose()
            do {
                set(.login(try await model.core.moveTarget()))
            } catch {
                set(.noTarget(target: nil, error: Core.failure(error).message))
            }
        }
    }

    private func close() {
        if phase.isRunning {
            // Stops after the object it is on; the sheet stays until it has.
            cancel()
            return
        }
        if !FeatureFixtures.isOn {
            let core = model.core
            Task { try? await core.moveClose() }
        }
        dismiss()
    }

    /// The screen stays on while the move runs, so the device doesn't sleep in the middle.
    private func keepAwake(_ on: Bool) {
        #if os(iOS)
        UIApplication.shared.isIdleTimerDisabled = on
        #endif
    }

    // MARK: Fixtures

    #if DEBUG
    private static var fixturePhase: MovePhase {
        let preview = MoveFixtures.preview
        switch MoveFixtures.requested {
        case "no-target": return .noTarget(target: nil, error: nil)
        case "two-factor": return .twoFactor(methods: MoveFixtures.methods, message: nil)
        case "new-device": return .newDevice
        case "preview": return .preview(preview)
        case "running":
            return .running(preview: preview, progress: MoveProgress(done: 187, total: 418, kind: .item))
        case "finished": return .finished(preview: preview, finished: MoveFixtures.finished)
        case "cancelled": return .finished(preview: preview, finished: MoveFixtures.cancelled)
        case "failed":
            return .failed(preview: preview, error: "Der Server ist gerade nicht erreichbar.")
        default: return .login(MoveFixtures.target)
        }
    }

    /// Plays a move through the sample counts.
    private func runFixture(_ preview: MovePreview) {
        fixtureRun?.cancel()
        fixtureRun = Task {
            let steps: [(MoveKind, Int)] = [
                (.folder, preview.folders.todo), (.organization, preview.organizations.todo),
                (.collection, preview.collections.todo), (.item, preview.items.todo),
                (.attachment, preview.attachments.todo), (.send, preview.sends.todo),
            ]
            for (kind, total) in steps where total > 0 {
                for done in stride(from: 0, through: total, by: max(1, total / 12)) {
                    try? await Task.sleep(for: .milliseconds(120))
                    guard !Task.isCancelled else { return }
                    phase = .running(preview: preview, progress: MoveProgress(done: done, total: total, kind: kind))
                }
            }
            guard !Task.isCancelled else { return }
            set(.finished(preview: preview, finished: MoveFixtures.finished))
        }
    }
    #endif
}

#if DEBUG
#Preview("Anmeldung") {
    MoveFlowView(phase: .login(MoveFixtures.target)).environment(AppModel())
}

#Preview("Zweistufig") {
    MoveFlowView(phase: .twoFactor(methods: MoveFixtures.methods, message: nil)).environment(AppModel())
}

#Preview("Vorschau") {
    MoveFlowView(phase: .preview(MoveFixtures.preview)).environment(AppModel())
}

#Preview("Läuft") {
    MoveFlowView(phase: .running(
        preview: MoveFixtures.preview, progress: MoveProgress(done: 187, total: 418, kind: .item)))
        .environment(AppModel())
}

#Preview("Fertig") {
    MoveFlowView(phase: .finished(preview: MoveFixtures.preview, finished: MoveFixtures.finished))
        .environment(AppModel())
}

#Preview("Fehlgeschlagen") {
    MoveFlowView(phase: .failed(preview: MoveFixtures.preview, error: "Der Server ist gerade nicht erreichbar."))
        .environment(AppModel())
}
#endif
