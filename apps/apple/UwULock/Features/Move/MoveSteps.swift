import SwiftUI
import UwUDesign

// MARK: Words

extension MoveNoticeCode {
    /// What the preview says about this notice (MoveDialog.tsx `NOTICE`).
    var text: Text {
        switch self {
        case .trash:
            Text(features: "Einträge im Papierkorb bleiben, wo sie sind.")
        case .broken:
            Text(features: "Einträge, Organisationen oder Sends ließen sich nicht entschlüsseln und bleiben zurück.")
        case .readOnly:
            Text(features: "Einträge in Organisationen darfst du nur lesen (oder ihr Passwort nicht sehen) – sie bleiben dort.")
        case .orgMembers:
            Text(features: "Organisationen ziehen ohne ihre Mitglieder um: Lade sie danach im Web-Tresor wieder ein.")
        case .orgsAsFolders:
            Text(features: "Organisationen werden zu Ordnern mit ihrem Namen: Hier kannst du keine (weitere) Familie anlegen.")
        case .sendPassword:
            Text(features: "Sends hatten ein Passwort. Es liegt nur als Hash auf dem Server und zieht nicht mit – sie kommen ohne Passwort an.")
        case .sendEmails:
            Text(features: "Sends nur für bestimmte Adressen kommen ohne diese Einschränkung an.")
        case .sendFileLocked:
            Text(features: "Datei-Sends mit Passwort, deaktiviert oder schon ausgeschöpft lassen sich nicht abrufen und bleiben zurück.")
        case .sendExpired:
            Text(features: "Abgelaufene Sends bleiben zurück.")
        case .sendFileCounted:
            Text(features: "Datei-Sends werden dafür einmal abgerufen – das zählt dort als ein Zugriff.")
        case .tooLarge:
            Text(features: "Dateien sind größer, als dieser Server erlaubt, und bleiben zurück.")
        }
    }

    /// Notices that only inform are calm; those where something stays behind warn.
    var warns: Bool {
        switch self {
        case .broken, .readOnly, .sendFileLocked, .sendExpired, .tooLarge, .trash: true
        default: false
        }
    }
}

extension MoveKind {
    /// One object of this kind (MoveDialog.tsx `KIND`).
    var label: Text {
        switch self {
        case .folder: Text(features: "Ordner")
        case .organization: Text(features: "Organisation")
        case .collection: Text(features: "Sammlung")
        case .item: Text(features: "Eintrag")
        case .attachment: Text(features: "Anhang")
        case .send: Text(features: "Send")
        }
    }

    var icon: UwUIconName {
        switch self {
        case .folder: .folder
        case .organization: .organization
        case .collection: .collection
        case .item: .vault
        case .attachment: .file
        case .send: .send
        }
    }

    /// "187 von 418 – gerade: Eintrag".
    func progress(done: Int, total: Int) -> Text {
        switch self {
        case .folder: Text(features: "\(done) von \(total) – gerade: Ordner")
        case .organization: Text(features: "\(done) von \(total) – gerade: Organisation")
        case .collection: Text(features: "\(done) von \(total) – gerade: Sammlung")
        case .item: Text(features: "\(done) von \(total) – gerade: Eintrag")
        case .attachment: Text(features: "\(done) von \(total) – gerade: Anhang")
        case .send: Text(features: "\(done) von \(total) – gerade: Send")
        }
    }
}

extension TwoFactorKind {
    /// The method's name (LoginScreen.tsx `METHOD_LABEL`).
    var moveLabel: String {
        switch self {
        case .authenticator: featureString("Authenticator-App")
        case .email: featureString("E-Mail")
        case .yubikey: "YubiKey OTP"
        case .duo: "Duo"
        case .webauthn: "Passkey / FIDO2"
        case .u2f: "FIDO U2F"
        case .other: "?"
        }
    }
}

// MARK: Shared pieces

/// The step's buttons at the bottom of the sheet, on glass.
private struct MoveActionBar<Secondary: View, Primary: View>: View {
    @ViewBuilder var secondary: Secondary
    @ViewBuilder var primary: Primary

    var body: some View {
        UwUGlassContainer {
            HStack(spacing: UwUSpacing.sm) {
                secondary
                    .uwuGlassButton()
                Spacer(minLength: 0)
                primary
                    .uwuProminentButton()
                    .foregroundStyle(Color.uwuOnPink)
                    .keyboardShortcut(.defaultAction)
            }
            .controlSize(.large)
        }
        .padding(.horizontal, UwUSpacing.md)
        .padding(.vertical, UwUSpacing.sm)
    }
}

extension View {
    fileprivate func moveActions<S: View, P: View>(
        @ViewBuilder secondary: () -> S, @ViewBuilder primary: () -> P
    ) -> some View {
        let bar = MoveActionBar(secondary: secondary, primary: primary)
        return safeAreaInset(edge: .bottom) { bar }
    }
}

/// An error line in a form.
private struct MoveErrorRow: View {
    let text: String

    var body: some View {
        Label { Text(verbatim: text) } icon: { UwUIcon(.warning, size: .sm) }
            .foregroundStyle(.uwuDangerInk)
            .accessibilityAddTraits(.isStaticText)
            .onAppear { AccessibilityNotification.Announcement(text).post() }
    }
}

// MARK: Source login

/// The account the move comes from (MoveDialog.tsx `SourceLogin`).
struct MoveSourceLogin: View {
    let target: MoveTarget
    let onDone: (MoveStep) -> Void

    @Environment(AppModel.self) private var model
    @State private var kind: ServerKind = .bitwardenUS
    @State private var url = ""
    @State private var email = ""
    @State private var password = ""
    @State private var reveal = false
    @State private var busy = false
    @State private var error: String?
    @FocusState private var focus: Field?

    private enum Field { case url, email, password }

    private var ready: Bool {
        !busy && !password.isEmpty && !email.trimmingCharacters(in: .whitespaces).isEmpty
            && (kind != .selfHosted || !url.trimmingCharacters(in: .whitespaces).isEmpty)
    }

    var body: some View {
        Form {
            Section {
                HStack(alignment: .top, spacing: UwUSpacing.md) {
                    NyuView(mood: .cheer)
                        .frame(width: 56)
                        .accessibilityHidden(true)
                    Text(features: "Melde dich bei dem Konto an, aus dem du umziehst. Diese Anmeldung bleibt nur, solange dieser Dialog offen ist; am alten Konto ändert sich nichts.")
                }
            } footer: {
                VStack(alignment: .leading, spacing: UwUSpacing.xxs) {
                    Text(features: "Ziel: \(target.email) auf \(target.server).")
                    if target.families {
                        Text(features: "Organisationen werden dort zu Familien, wenn du welche anlegen darfst.")
                    } else {
                        Text(features: "Organisationen kommen dort in Ordner mit ihrem Namen.")
                    }
                }
            }

            Section {
                Picker(selection: $kind) {
                    Text(verbatim: "bitwarden.com").tag(ServerKind.bitwardenUS)
                    Text(verbatim: "bitwarden.eu").tag(ServerKind.bitwardenEU)
                    Text(features: "Selbst gehostet").tag(ServerKind.selfHosted)
                } label: {
                    Text(features: "Server")
                }
                .pickerStyle(.segmented)
                if kind == .selfHosted {
                    TextField(text: $url, prompt: Text(verbatim: "https://vault.example.org")) {
                        Text(features: "Server-Adresse")
                    }
                    .focused($focus, equals: .url)
                    .autocorrectionDisabled()
                    .textContentType(.URL)
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .keyboardType(.URL)
                    #endif
                    .submitLabel(.next)
                    .onSubmit { focus = .email }
                }
            }
            .disabled(busy)

            Section {
                TextField(text: $email) { Text(features: "E-Mail-Adresse") }
                    .focused($focus, equals: .email)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .keyboardType(.emailAddress)
                    #endif
                    .submitLabel(.next)
                    .onSubmit { focus = .password }
                HStack {
                    Group {
                        if reveal {
                            TextField(text: $password) { Text(features: "Master-Passwort") }
                        } else {
                            SecureField(text: $password) { Text(features: "Master-Passwort") }
                        }
                    }
                    .focused($focus, equals: .password)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
                    .submitLabel(.go)
                    .onSubmit { if ready { Task { await submit() } } }
                    Button {
                        reveal.toggle()
                    } label: {
                        UwUIcon(reveal ? .hide : .show, size: .sm)
                    }
                    .buttonStyle(.borderless)
                    .accessibilityLabel(reveal ? Text(features: "Passwort verbergen") : Text(features: "Passwort zeigen"))
                    .help(reveal ? Text(features: "Passwort verbergen") : Text(features: "Passwort zeigen"))
                }
            }
            .disabled(busy)

            if let error {
                Section { MoveErrorRow(text: error) }
            }
        }
        .formStyle(.grouped)
        .moveActions {
            EmptyView()
        } primary: {
            Button {
                Task { await submit() }
            } label: {
                if busy {
                    HStack(spacing: UwUSpacing.xs) {
                        ProgressView().controlSize(.small)
                        Text(features: "Liest beide Tresore …")
                    }
                } else {
                    Text(features: "Anmelden")
                }
            }
            .disabled(!ready)
        }
        .onAppear { focus = .email }
    }

    private func submit() async {
        busy = true
        error = nil
        defer { busy = false }
        #if DEBUG
        if FeatureFixtures.isOn {
            password = ""
            onDone(.done(MoveFixtures.preview))
            return
        }
        #endif
        do {
            let server = kind == .selfHosted
                ? ServerInput(kind: kind, url: url.trimmingCharacters(in: .whitespaces))
                : ServerInput(kind: kind)
            let step = try await model.core.moveLogin(
                server: server, email: email.trimmingCharacters(in: .whitespaces), password: password)
            password = ""
            onDone(step)
        } catch {
            self.error = Core.failure(error).message
        }
    }
}

// MARK: Two-step login

/// The source's second step (LoginScreen.tsx `TwoFactor`, without "remember").
struct MoveTwoFactorForm: View {
    let methods: [TwoFactorMethod]
    let message: String?
    let onBack: () -> Void
    let onDone: (MoveStep) -> Void

    @Environment(AppModel.self) private var model
    @State private var provider: Int?
    @State private var code = ""
    @State private var busy = false
    @State private var sent = false
    @State private var error: String?
    @FocusState private var focused: Bool

    init(methods: [TwoFactorMethod], message: String?, onBack: @escaping () -> Void,
         onDone: @escaping (MoveStep) -> Void) {
        self.methods = methods
        self.message = message
        self.onBack = onBack
        self.onDone = onDone
        _provider = State(initialValue: methods.first(where: \.supported)?.provider)
        _error = State(initialValue: message)
    }

    private var usable: [TwoFactorMethod] { methods.filter(\.supported) }
    private var method: TwoFactorMethod? { usable.first { $0.provider == provider } }
    private var unsupported: [TwoFactorMethod] { methods.filter { !$0.supported } }

    var body: some View {
        Group {
            if method == nil {
                FeatureEmptyState(
                    mood: .puzzled, title: Text(features: "Zweistufige Anmeldung"),
                    message: Text(features: "Dein Konto verlangt eine Methode, die UwULock noch nicht kann (\(methods.map(\.kind.moveLabel).joined(separator: ", "))). Richte im Web-Tresor zusätzlich eine Authenticator-App oder E-Mail-Codes ein.")
                ) {
                    Button(action: onBack) { Label(features: "Zurück", uwuIcon: .back) }
                        .uwuGlassButton()
                }
            } else {
                form
            }
        }
        .onChange(of: message) { _, new in error = new }
    }

    private var form: some View {
        Form {
            Section {
                if usable.count > 1 {
                    Picker(selection: $provider) {
                        ForEach(usable) { method in
                            Text(verbatim: method.kind.moveLabel).tag(Int?.some(method.provider))
                        }
                    } label: {
                        Text(features: "Methode")
                    }
                    .pickerStyle(.segmented)
                    .onChange(of: provider) {
                        code = ""
                        error = nil
                        focused = true
                    }
                }
                lead
            } header: {
                Text(features: "Zweistufige Anmeldung")
            } footer: {
                if !unsupported.isEmpty {
                    Text(features: "Noch nicht unterstützt: \(unsupported.map(\.kind.moveLabel).joined(separator: ", ")).")
                }
            }
            Section {
                TextField(text: $code) { Text(features: "Code") }
                    .focused($focused)
                    .textContentType(.oneTimeCode)
                    .autocorrectionDisabled()
                    .monospacedDigit()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .keyboardType(method?.kind == .yubikey ? .asciiCapable : .numberPad)
                    #endif
                    .submitLabel(.continue)
                    .onSubmit { Task { await submit() } }
                    .disabled(busy)
            }
            if let error {
                Section { MoveErrorRow(text: error) }
            }
        }
        .formStyle(.grouped)
        .moveActions {
            HStack(spacing: UwUSpacing.sm) {
                Button(action: onBack) { Text(features: "Zurück") }
                    .disabled(busy)
                if method?.kind == .email {
                    Button {
                        Task { await sendEmail() }
                    } label: {
                        if sent { Text(features: "Nochmal senden") } else { Text(features: "Code senden") }
                    }
                    .disabled(busy)
                }
            }
        } primary: {
            Button {
                Task { await submit() }
            } label: {
                if busy { Text(features: "Prüft …") } else { Text(features: "Weiter") }
            }
            .disabled(busy || code.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .onAppear { focused = true }
    }

    @ViewBuilder
    private var lead: some View {
        let to = method?.hint ?? featureString("deine E-Mail-Adresse")
        switch method?.kind {
        case .authenticator:
            Text(features: "Gib den sechsstelligen Code aus deiner Authenticator-App ein.")
        case .email:
            if sent {
                Text(features: "Der Code ist unterwegs an \(to).")
            } else {
                Text(features: "Lass dir einen Code an \(to) schicken und gib ihn hier ein.")
            }
        case .yubikey:
            Text(features: "Stecke deinen YubiKey ein und tippe ihn an.")
        default:
            EmptyView()
        }
    }

    private func submit() async {
        guard let provider, !code.trimmingCharacters(in: .whitespaces).isEmpty, !busy else { return }
        busy = true
        error = nil
        defer { busy = false }
        #if DEBUG
        if FeatureFixtures.isOn {
            onDone(.done(MoveFixtures.preview))
            return
        }
        #endif
        do {
            let next = try await model.core.moveLoginTwoFactor(
                provider: provider, code: code.trimmingCharacters(in: .whitespaces))
            if case .twoFactor = next {
                error = featureString("Der Code wurde nicht angenommen.")
                code = ""
            } else {
                onDone(next)
            }
        } catch {
            self.error = Core.failure(error).message
        }
    }

    private func sendEmail() async {
        error = nil
        #if DEBUG
        if FeatureFixtures.isOn {
            sent = true
            return
        }
        #endif
        do {
            try await model.core.moveLoginSendEmail()
            sent = true
        } catch {
            self.error = Core.failure(error).message
        }
    }
}

// MARK: New device

/// The source doesn't know this device: the mailed code (LoginScreen.tsx `NewDevice`).
struct MoveNewDeviceForm: View {
    let onBack: () -> Void
    let onDone: (MoveStep) -> Void

    @Environment(AppModel.self) private var model
    @State private var code = ""
    @State private var busy = false
    @State private var error: String?
    @FocusState private var focused: Bool

    var body: some View {
        Form {
            Section {
                Text(features: "Bitwarden kennt dieses Gerät noch nicht und hat dir einen Code per E-Mail geschickt.")
            } header: {
                Text(features: "Neues Gerät bestätigen")
            }
            Section {
                TextField(text: $code) { Text(features: "Code aus der E-Mail") }
                    .focused($focused)
                    .textContentType(.oneTimeCode)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
                    .submitLabel(.continue)
                    .onSubmit { Task { await submit() } }
                    .disabled(busy)
            }
            if let error {
                Section { MoveErrorRow(text: error) }
            }
        }
        .formStyle(.grouped)
        .moveActions {
            Button(action: onBack) { Text(features: "Zurück") }
                .disabled(busy)
        } primary: {
            Button {
                Task { await submit() }
            } label: {
                if busy { Text(features: "Prüft …") } else { Text(features: "Weiter") }
            }
            .disabled(busy || code.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .onAppear { focused = true }
    }

    private func submit() async {
        guard !code.trimmingCharacters(in: .whitespaces).isEmpty, !busy else { return }
        busy = true
        error = nil
        defer { busy = false }
        #if DEBUG
        if FeatureFixtures.isOn {
            onDone(.done(MoveFixtures.preview))
            return
        }
        #endif
        do {
            let next = try await model.core.moveLoginNewDevice(code: code)
            if case .newDevice = next {
                error = featureString("Der Code wurde nicht angenommen.")
            } else {
                onDone(next)
            }
        } catch {
            self.error = Core.failure(error).message
        }
    }
}

// MARK: Preview

/// What will move (MoveDialog.tsx `PreviewView`).
struct MovePreviewView: View {
    let preview: MovePreview
    let onStart: () -> Void

    private var nothing: Bool {
        [preview.folders, preview.organizations, preview.collections, preview.items,
         preview.attachments, preview.sends].allSatisfy { $0.todo == 0 }
    }

    var body: some View {
        Form {
            Section {
                MoveCountRow(label: Text(features: "Ordner"), icon: .folder, count: preview.folders)
                MoveCountRow(
                    label: preview.families
                        ? Text(features: "Organisationen → Familien")
                        : Text(features: "Organisationen → Ordner"),
                    icon: .organization, count: preview.organizations)
                MoveCountRow(label: Text(features: "Sammlungen"), icon: .collection, count: preview.collections)
                MoveCountRow(label: Text(features: "Einträge"), icon: .vault, count: preview.items)
                MoveCountRow(label: Text(features: "Anhänge"), icon: .file, count: preview.attachments)
                MoveCountRow(label: Text(features: "Sends"), icon: .send, count: preview.sends)
            } header: {
                Text(features: "Von \(preview.source) (\(preview.sourceEmail)) nach \(preview.target) (\(preview.targetEmail)):")
                    .textCase(nil)
            } footer: {
                if preview.fileBytes > 0 {
                    Text(features: "Dateien zusammen: \(preview.fileBytes.fileSize)")
                }
            }

            if !preview.notices.isEmpty {
                Section {
                    ForEach(preview.notices, id: \.code) { notice in
                        Label {
                            HStack(alignment: .firstTextBaseline, spacing: 0) {
                                Text(verbatim: "\(notice.count)× ").bold()
                                notice.code.text
                            }
                        } icon: {
                            UwUIcon(notice.code.warns ? .warning : .info, size: .sm)
                                .foregroundStyle(notice.code.warns ? Color.uwuWarningInk : Color.uwuMuted)
                        }
                    }
                }
            }

            Section {
                Text(features: "Jeder Send bekommt einen neuen Link. Am alten Konto wird nichts gelöscht. Ein zweiter Umzug holt nur, was neu dazugekommen ist.")
                    .foregroundStyle(.uwuMuted)
            }
        }
        .formStyle(.grouped)
        .moveActions {
            EmptyView()
        } primary: {
            Button(action: onStart) {
                if nothing {
                    Text(features: "Alles schon umgezogen ✧")
                } else {
                    Label(features: "Umziehen", uwuIcon: .start)
                }
            }
            .disabled(nothing)
        }
    }
}

/// One kind in the preview: still to move, and what moved already.
private struct MoveCountRow: View {
    let label: Text
    let icon: UwUIconName
    let count: MoveCount

    var body: some View {
        if count.total > 0 {
            LabeledContent {
                HStack(spacing: UwUSpacing.xs) {
                    Text(count.todo, format: .number)
                        .monospacedDigit()
                        .foregroundStyle(.uwuInk)
                    if count.moved > 0 {
                        Text(features: "(\(count.moved) schon umgezogen)")
                            .uwuFont(.caption)
                            .foregroundStyle(.uwuMuted)
                    }
                }
            } label: {
                Label { label } icon: { UwUIcon(icon, size: .sm) }
            }
        }
    }
}

// MARK: Running

/// The move under way: how far, and a way to stop.
struct MoveRunningView: View {
    let progress: MoveProgress?
    let stopping: Bool
    let onStop: () -> Void

    var body: some View {
        VStack(spacing: UwUSpacing.lg) {
            Spacer(minLength: 0)
            NyuView(mood: .cheer)
                .frame(width: 112)
                .accessibilityHidden(true)
            VStack(spacing: UwUSpacing.sm) {
                Group {
                    if let progress {
                        progress.kind.progress(done: progress.done, total: progress.total)
                    } else {
                        Text(features: "Es geht los …")
                    }
                }
                .uwuFont(.headline)
                .monospacedDigit()
                .multilineTextAlignment(.center)
                .contentTransition(.numericText())
                .accessibilityAddTraits(.updatesFrequently)

                if let progress, progress.total > 0 {
                    ProgressView(value: Double(min(progress.done, progress.total)), total: Double(progress.total))
                } else {
                    ProgressView(value: 0, total: 1)
                }
            }
            .featureCard()
            Text(features: "Alles wird hier auf diesem Gerät entschlüsselt und für UwULock neu verschlüsselt. Anhalten geht jederzeit – ein neuer Umzug macht dort weiter.")
                .uwuFont(.footnote)
                .foregroundStyle(.uwuMuted)
                .multilineTextAlignment(.center)
            Spacer(minLength: 0)
        }
        .padding(UwUSpacing.lg)
        .frame(maxWidth: 520)
        .frame(maxWidth: .infinity)
        .uwuAnimation(value: progress)
        .moveActions {
            EmptyView()
        } primary: {
            Button(action: onStop) {
                if stopping {
                    HStack(spacing: UwUSpacing.xs) {
                        ProgressView().controlSize(.small)
                        Text(features: "Anhalten")
                    }
                } else {
                    Text(features: "Anhalten")
                }
            }
            .disabled(stopping)
        }
    }
}

// MARK: Finished

/// What the move did (MoveDialog.tsx `FinishedView`).
struct MoveFinishedView: View {
    let finished: MoveFinished
    let onContinue: () -> Void
    let onClose: () -> Void

    private var parts: [(Int, Text, UwUIconName)] {
        let moved = finished.summary.moved
        return [
            (moved.items, Text(features: "Einträge"), .vault),
            (moved.attachments, Text(features: "Anhänge"), .file),
            (moved.folders, Text(features: "Ordner"), .folder),
            (moved.organizations, Text(features: "Organisationen"), .organization),
            (moved.collections, Text(features: "Sammlungen"), .collection),
            (moved.sends, Text(features: "Sends"), .send),
        ].filter { $0.0 > 0 }
    }

    var body: some View {
        let failed = finished.summary.failed
        Form {
            Section {
                HStack(spacing: UwUSpacing.md) {
                    NyuView(mood: finished.cancelled ? .sleepy : (failed.isEmpty ? .sparkle : .happy))
                        .frame(width: 64)
                        .accessibilityHidden(true)
                    Group {
                        if finished.cancelled {
                            Text(features: "Angehalten. Weitermachen setzt dort an, wo es stehen geblieben ist.")
                        } else {
                            Text(features: "Umgezogen ✧")
                                .uwuFont(.title3, weight: .bold)
                        }
                    }
                }
            }
            if !parts.isEmpty {
                Section {
                    ForEach(Array(parts.enumerated()), id: \.offset) { _, part in
                        LabeledContent {
                            Text(part.0, format: .number).monospacedDigit()
                        } label: {
                            Label { part.1 } icon: { UwUIcon(part.2, size: .sm) }
                        }
                    }
                } footer: {
                    if finished.summary.orgsAsFolders > 0 {
                        Text(features: "Der Server hat keine Familie angelegt; \(finished.summary.orgsAsFolders) Organisationen sind jetzt Ordner.")
                    }
                }
            } else if finished.summary.orgsAsFolders > 0 {
                Section {
                    Text(features: "Der Server hat keine Familie angelegt; \(finished.summary.orgsAsFolders) Organisationen sind jetzt Ordner.")
                        .foregroundStyle(.uwuMuted)
                }
            }
            if !failed.isEmpty {
                Section {
                    ForEach(Array(failed.prefix(20).enumerated()), id: \.offset) { _, failure in
                        Label {
                            Text(verbatim: failure.message)
                        } icon: {
                            UwUIcon(failure.kind.icon, size: .sm)
                                .foregroundStyle(.uwuDangerInk)
                        }
                        .accessibilityElement(children: .combine)
                    }
                } header: {
                    Label {
                        Text(features: "\(failed.count) Dinge sind nicht umgezogen. Ein neuer Umzug versucht sie noch einmal.")
                    } icon: {
                        UwUIcon(.warning, size: .sm)
                    }
                    .foregroundStyle(.uwuDangerInk)
                    .textCase(nil)
                }
            }
        }
        .formStyle(.grouped)
        .moveActions {
            if finished.cancelled {
                Button(action: onClose) { Text(features: "Schließen") }
            }
        } primary: {
            if finished.cancelled {
                Button(action: onContinue) { Text(features: "Weitermachen") }
            } else {
                Button(action: onClose) { Text(features: "Fertig") }
            }
        }
    }
}

// MARK: Failed

/// The move stopped with an error; what moved stays.
struct MoveFailedView: View {
    let error: String
    let onRetry: () -> Void
    let onClose: () -> Void

    var body: some View {
        FeatureEmptyState(
            mood: .sad, title: Text(verbatim: error),
            message: Text(features: "Was schon umgezogen ist, bleibt. Weitermachen setzt dort an, wo es stehen geblieben ist.")
        )
        .moveActions {
            Button(action: onClose) { Text(features: "Schließen") }
        } primary: {
            Button(action: onRetry) { Label(features: "Weitermachen", uwuIcon: .refresh) }
        }
    }
}
