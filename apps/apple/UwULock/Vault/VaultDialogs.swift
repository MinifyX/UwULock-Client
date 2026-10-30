import SwiftUI
import UwUDesign

/// What the vault asks and says on top of any screen: the editor, the folder dialogs, deleting
/// for good, the toast (VaultScreen.tsx's modals, lib/toast.ts).
struct VaultDialogs: ViewModifier {
    @Binding var place: SidebarItem?
    @Environment(VaultModel.self) private var vault
    @State private var folderName = ""

    func body(content: Content) -> some View {
        @Bindable var vault = vault
        content
            .sheet(item: $vault.editor) { request in
                ItemEditorView(request: request)
            }
            .sheet(isPresented: $vault.showGenerator) {
                GeneratorSheet(onUse: nil)
            }
            .alert(
                vault.folderRequest?.folder == nil ? vt("Neuer Ordner") : vt("Ordner umbenennen"),
                isPresented: Binding(
                    get: { vault.folderRequest != nil },
                    set: { if !$0 { vault.folderRequest = nil } }),
                presenting: vault.folderRequest
            ) { request in
                TextField(vt("Name"), text: $folderName)
                Button(vt("Abbrechen"), role: .cancel) {}
                Button(request.folder == nil ? vt("Anlegen") : vt("Übernehmen")) {
                    vault.saveFolder(request.folder, name: folderName)
                }
                .disabled(folderName.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .onChange(of: vault.folderRequest) { _, request in
                folderName = request?.folder?.name ?? ""
            }
            .alert(
                vt("Ordner löschen?"),
                isPresented: Binding(
                    get: { vault.folderToDelete != nil },
                    set: { if !$0 { vault.folderToDelete = nil } }),
                presenting: vault.folderToDelete
            ) { folder in
                Button(vt("Löschen"), role: .destructive) {
                    vault.deleteFolder(folder, selection: $place)
                }
                Button(vt("Abbrechen"), role: .cancel) {}
            } message: { folder in
                Text(vt("„\(folder.name)“ verschwindet. Die Einträge darin bleiben – dann ohne Ordner."))
            }
            .alert(
                vt("Endgültig löschen?"),
                isPresented: Binding(
                    get: { vault.itemToDelete != nil },
                    set: { if !$0 { vault.itemToDelete = nil } }),
                presenting: vault.itemToDelete
            ) { item in
                if let masked = vault.uwu.masked[item.id] {
                    Button(vt("Löschen und \(masked.email) abschalten"), role: .destructive) {
                        vault.deleteForever(item, disableMasked: true)
                    }
                    Button(vt("Nur löschen"), role: .destructive) {
                        vault.deleteForever(item, disableMasked: false)
                    }
                } else {
                    Button(vt("Endgültig löschen"), role: .destructive) {
                        vault.deleteForever(item, disableMasked: false)
                    }
                }
                Button(vt("Abbrechen"), role: .cancel) {}
            } message: { item in
                Text(vt("„\(item.name)“ wird auf dem Server gelöscht. Das lässt sich nicht rückgängig machen – auch nicht im Web-Tresor."))
            }
            .overlay(alignment: .bottom) { VaultToastView() }
            .sensoryFeedback(trigger: vault.toast) { _, toast in
                switch toast?.tone {
                case .success: .success
                case .danger: .error
                default: nil
                }
            }
    }
}

extension View {
    func vaultDialogs(place: Binding<SidebarItem?>) -> some View {
        modifier(VaultDialogs(place: place))
    }
}

/// The toast: comes up from below, goes after a few seconds (lib/toast.ts).
struct VaultToastView: View {
    @Environment(VaultModel.self) private var vault
    @Environment(\.uwuReduceMotion) private var reduceMotion

    var body: some View {
        ZStack {
            if let toast = vault.toast {
                Text(toast.text)
                    .uwuFont(.subheadline, weight: .medium)
                    .foregroundStyle(toast.tone == .danger ? Color.uwuDangerInk : Color.uwuInk)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, UwUSpacing.lg)
                    .padding(.vertical, UwUSpacing.sm)
                    .uwuGlass(in: Capsule())
                    .padding(.horizontal, UwUSpacing.lg)
                    .padding(.bottom, UwUSpacing.xxl * 2)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .id(toast.id)
                    .onTapGesture { vault.toast = nil }
                    .accessibilityAddTraits(.isStaticText)
                    .task(id: toast.id) {
                        AccessibilityNotification.Announcement(toast.text).post()
                        try? await Task.sleep(for: .seconds(toast.tone == .danger ? 5 : 3))
                        if !Task.isCancelled, vault.toast?.id == toast.id { vault.toast = nil }
                    }
            }
        }
        .uwuAnimation(value: vault.toast?.id)
        .allowsHitTesting(vault.toast != nil)
    }
}
