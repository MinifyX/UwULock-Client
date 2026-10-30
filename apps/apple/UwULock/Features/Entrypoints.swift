import SwiftUI

// Entry points of the feature screens that are built in parallel (milestone 5).
// The shell, the item detail and the settings link to these names; each one
// moves into its own file under Features/<Area>/ once it is built, and its
// stub here is deleted. Keep the signatures.

/// Passwortprüfung (health.ts, HealthPane.tsx).
struct PasswordCheckView: View {
    var body: some View { ContentUnavailableView("Passwortprüfung", systemImage: "checkmark.shield") }
}

/// UwUSSH / UwURDP hosts and logins (suiteModel.ts).
struct SuiteView: View {
    enum Kind: Hashable, Sendable { case ssh, rdp }
    let kind: Kind
    var body: some View { ContentUnavailableView(kind == .ssh ? "SSH (UwUSSH)" : "Remote Desktop (UwURDP)", systemImage: "terminal") }
}

/// Sends list, editor and sharing (sendModel.ts).
struct SendsView: View {
    var body: some View { ContentUnavailableView("Sends", systemImage: "paperplane") }
}

/// Dateianfragen (FileRequestsDialog.tsx).
struct FileRequestsView: View {
    var body: some View { ContentUnavailableView("Dateianfragen", systemImage: "tray") }
}

/// Maskierte Adressen (MaskedDialog.tsx).
struct MaskedAddressesView: View {
    var body: some View { ContentUnavailableView("Maskierte Adressen", systemImage: "theatermasks") }
}

/// Import from another app's export file (ImportDialog.tsx, importFlow.ts). Presented as a sheet.
struct ImportFlowView: View {
    var body: some View { ContentUnavailableView("Importieren", systemImage: "square.and.arrow.down") }
}

/// Moving a vault in from Bitwarden/Vaultwarden (MoveDialog.tsx, moving.ts). Presented as a sheet.
struct MoveFlowView: View {
    var body: some View { ContentUnavailableView("Umziehen", systemImage: "arrow.right.arrow.left") }
}

/// The UwU extras of one item inside its detail: icon, versions, reminder (ItemExtras.tsx).
struct ItemExtrasSection: View {
    let itemId: String
    var body: some View { EmptyView() }
}

/// "Als Send teilen" for an item (share_as_send). Shown in the item's toolbar/menu.
struct ShareAsSendButton: View {
    let itemId: String
    var body: some View { EmptyView() }
}

/// Wi-Fi item: QR code and Connect (wifi.ts). Shown inside a Wi-Fi item's detail.
struct WifiItemSection: View {
    let itemId: String
    var body: some View { EmptyView() }
}

/// Settings → Extras: UwULock Server's extras key, travel mode, health e-mail opt-in (uwu.ts).
struct ExtrasSettingsSection: View {
    var body: some View { EmptyView() }
}
