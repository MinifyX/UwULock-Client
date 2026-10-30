import SwiftUI
import UwUDesign

/// A round initial in the account's colour.
struct AccountAvatar: View {
    let label: String
    let email: String
    let seed: String
    var size: CGFloat = 30

    init(account: AccountBrief, size: CGFloat = 30) {
        label = account.label
        email = account.email
        seed = account.id
        self.size = size
    }

    init(status: Status, size: CGFloat = 30) {
        label = status.label ?? ""
        email = status.email ?? ""
        seed = status.accountId ?? email
        self.size = size
    }

    private static let palette: [(Color, Color)] = [
        (.uwuAvatarPink, .uwuAvatarPinkInk), (.uwuAvatarViolet, .uwuAvatarVioletInk),
        (.uwuAvatarSky, .uwuAvatarSkyInk), (.uwuAvatarMint, .uwuAvatarMintInk),
        (.uwuAvatarAmber, .uwuAvatarAmberInk), (.uwuAvatarCoral, .uwuAvatarCoralInk),
    ]

    var body: some View {
        let colors = Self.palette[Int(seed.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0xFFFF }) % Self.palette.count]
        let name = label.isEmpty ? email : label
        Text(verbatim: name.first.map { String($0).uppercased() } ?? "?")
            .uwuFont(.subheadline, weight: .bold)
            .foregroundStyle(colors.1)
            .frame(width: size, height: size)
            .background(colors.0, in: Circle())
            .accessibilityHidden(true)
    }
}

extension AccountBrief {
    var title: String { label.isEmpty ? email : label }
}

/// The account menu (AccountCard.tsx) for the Mac's sidebar or toolbar: other accounts, add,
/// rename, lock, log out.
struct AccountMenu: View {
    @Environment(AppModel.self) private var model
    @State private var renaming: AccountBrief?
    @State private var loggingOut: AccountBrief?

    var body: some View {
        let accounts = model.status?.accounts ?? []
        let active = accounts.first(where: \.active)
        Menu {
            AccountMenuItems(renaming: $renaming, loggingOut: $loggingOut)
        } label: {
            if let active {
                Label {
                    Text(verbatim: active.title)
                } icon: {
                    AccountAvatar(account: active, size: 20)
                }
            } else {
                Label(authStr("Konten"), uwuIcon: .account)
            }
        }
        .accessibilityLabel(Text(authStr("Konto wechseln")))
        .renameAccountSheet($renaming)
        .logoutConfirmation(item: $loggingOut)
    }
}

/// The menu's items, also for context menus and the menu bar.
struct AccountMenuItems: View {
    @Environment(AppModel.self) private var model
    @Binding var renaming: AccountBrief?
    @Binding var loggingOut: AccountBrief?

    var body: some View {
        let accounts = model.status?.accounts ?? []
        let active = accounts.first(where: \.active)
        let others = accounts.filter { !$0.active }
        if !others.isEmpty {
            Section(authStr("Konto wechseln")) {
                ForEach(others) { account in
                    Button {
                        model.switchAccount(account.id)
                    } label: {
                        Text(account.unlocked ? account.title : authStr("\(account.title) (gesperrt)"))
                    }
                }
            }
        }
        Button { model.startAdding() } label: { Label(authStr("Konto hinzufügen"), uwuIcon: .add) }
        if let active {
            Divider()
            Button { renaming = active } label: { Label(authStr("Konto umbenennen"), uwuIcon: .edit) }
            Button { model.lock() } label: { Label(authStr("Sperren"), uwuIcon: .locked) }
                .disabled(model.phase != .unlocked)
            Button(role: .destructive) { loggingOut = active } label: {
                Label(authStr("Abmelden"), uwuIcon: .signOut)
            }
        }
    }
}

/// iPhone: all accounts in a sheet (Accounts.tsx). A tap switches; add; lock all.
struct AccountsSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var renaming: AccountBrief?
    @State private var loggingOut: AccountBrief?

    var body: some View {
        let accounts = model.status?.accounts ?? []
        NavigationStack {
            List {
                Section {
                    ForEach(accounts) { account in
                        Button {
                            if !account.active { model.switchAccount(account.id) }
                            dismiss()
                        } label: {
                            HStack(spacing: UwUSpacing.sm) {
                                AccountAvatar(account: account, size: 36)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(verbatim: account.title).uwuFont(.body, weight: .medium)
                                    Text(verbatim: "\(account.email) · \(account.server)")
                                        .uwuFont(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                if account.active {
                                    UwUIcon(.done, size: .sm, label: Text(authStr("Geöffnet")))
                                        .foregroundStyle(.tint)
                                } else if !account.unlocked {
                                    Text(authStr("Gesperrt"))
                                        .uwuFont(.caption, weight: .medium)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .contextMenu {
                            Button { renaming = account } label: { Label(authStr("Konto umbenennen"), uwuIcon: .edit) }
                            Button(role: .destructive) { loggingOut = account } label: {
                                Label(authStr("Abmelden"), uwuIcon: .signOut)
                            }
                        }
                        .swipeActions {
                            Button(role: .destructive) { loggingOut = account } label: {
                                Label(authStr("Abmelden"), uwuIcon: .signOut)
                            }
                        }
                    }
                }
                Section {
                    Button {
                        dismiss()
                        model.startAdding()
                    } label: { Label(authStr("Konto hinzufügen"), uwuIcon: .add) }
                    Button {
                        dismiss()
                        model.lock()
                    } label: {
                        Label(accounts.count > 1 ? authStr("Alle sperren") : authStr("Jetzt sperren"), uwuIcon: .locked)
                    }
                    .disabled(model.phase != .unlocked)
                }
            }
            .navigationTitle(Text(authStr("Konten")))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(role: .close) { dismiss() }
                }
            }
        }
        .renameAccountSheet($renaming)
        .logoutConfirmation(item: $loggingOut)
        .presentationDetents([.medium, .large])
    }
}

/// Renaming an account on this device only (AccountCard.tsx's modal).
struct RenameAccountView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let account: AccountBrief
    @State private var label: String
    @State private var problem: String?
    @State private var busy = false

    init(account: AccountBrief) {
        self.account = account
        _label = State(initialValue: account.label == account.server ? "" : account.label)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(authStr("Name"), text: $label, prompt: Text(verbatim: account.server))
                        .onChange(of: label) { _, new in
                            if new.count > 40 { label = String(new.prefix(40)) }
                        }
                        .onSubmit(save)
                } footer: {
                    VStack(alignment: .leading) {
                        Text(authStr("Nur hier auf diesem Gerät. Leer lassen: dann steht wieder \(account.server) da."))
                        if let problem {
                            Text(problem).foregroundStyle(.uwuDangerInk)
                        }
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(Text(authStr("Konto umbenennen")))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(authStr("Abbrechen")) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(authStr("Übernehmen"), action: save).disabled(busy)
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 380, minHeight: 200)
        #endif
        .presentationDetents([.medium])
    }

    private func save() {
        busy = true
        problem = nil
        Task {
            defer { busy = false }
            do {
                try await model.renameAccount(account.id, label: label.trimmingCharacters(in: .whitespaces))
                dismiss()
            } catch {
                problem = Core.failure(error).authMessage
            }
        }
    }
}

extension View {
    func renameAccountSheet(_ account: Binding<AccountBrief?>) -> some View {
        sheet(item: account) { RenameAccountView(account: $0) }
    }

    /// "Von diesem Gerät abmelden?" for the active account (LockScreen, Settings).
    func logoutConfirmation(isPresented: Binding<Bool>, email: String?, action: @escaping () -> Void) -> some View {
        confirmationDialog(Text(authStr("Von diesem Gerät abmelden?")), isPresented: isPresented, titleVisibility: .visible) {
            Button(authStr("Abmelden"), role: .destructive, action: action)
            Button(authStr("Abbrechen"), role: .cancel) {}
        } message: {
            Text(email.map { authStr("\($0) wird von diesem Gerät entfernt – die Sitzung, die Schlüssel und der zwischengespeicherte Tresor. Auf dem Server bleibt alles, wie es ist.") }
                ?? authStr("Die Anmeldung und die verschlüsselte Kopie des Tresors werden von diesem Gerät gelöscht. Dein Tresor auf dem Server bleibt, wie er ist."))
        }
    }

    /// The same for any account (the account menu, the accounts sheet).
    func logoutConfirmation(item: Binding<AccountBrief?>) -> some View {
        modifier(LogoutItemConfirmation(account: item))
    }
}

private struct LogoutItemConfirmation: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var account: AccountBrief?

    func body(content: Content) -> some View {
        content.logoutConfirmation(
            isPresented: Binding(get: { account != nil }, set: { if !$0 { account = nil } }),
            email: account?.email
        ) {
            if let id = account?.id { model.logOut(id: id) }
        }
    }
}

/// Another device asks to log in with this account (`auth-request`). The core has no command to
/// answer it yet, so this says what happened and where to look.
struct AuthRequestAlert: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content.alert(
            Text(authStr("Anmeldung auf einem anderen Gerät")),
            isPresented: Binding(get: { model.authRequest != nil }, set: { if !$0 { model.authRequest = nil } })
        ) {
            Button(authStr("Web-Tresor öffnen")) {
                Task { try? await model.core.openWebVault() }
            }
            Button(authStr("Ignorieren"), role: .cancel) {}
        } message: {
            Text(authStr("Ein anderes Gerät möchte sich mit deinem Konto anmelden. UwULock kann solche Anfragen noch nicht bestätigen. Warst du das nicht, ignoriere sie einfach – ohne Bestätigung passiert nichts."))
        }
    }
}
