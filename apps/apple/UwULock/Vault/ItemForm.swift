import Foundation

/// A value the editor may not have: a password, a card number, a hidden field (lib/itemForm.ts's
/// `Sec`). `keep` means the item's own value stays: the editor never saw it, and it is sent as
/// `nil`.
struct Secret: Equatable {
    var keep: Bool
    var value: String

    static func kept(_ has: Bool) -> Secret { Secret(keep: has, value: "") }
    static func typed(_ value: String) -> Secret { Secret(keep: false, value: value) }

    /// `nil` keeps the stored value, `""` clears it.
    var sent: String? { keep ? nil : value }
}

/// The item editor's form, apart from how it is drawn (lib/itemForm.ts).
struct ItemForm: Equatable {
    struct UriRow: Equatable, Identifiable {
        var id: Int
        var uri: String
        var match: Int?
    }

    struct FieldRow: Equatable, Identifiable {
        var id: Int
        var name: String
        var kind: FieldKind
        var value: Secret
        /// Which field of the item this row was, for the value and the link.
        var from: Int?
    }

    struct Wifi: Equatable {
        var ssid = ""
        var password = Secret.kept(false)
        var security = "WPA2"
        var hidden = false
        var eap = "PEAP"
        var phase2 = "MSCHAPV2"
        var identity = ""
        var anonymous = ""
        var ca = ""
        var from: [WifiFields.Key: Int] = [:]
    }

    var name = ""
    var folderId = ""
    var favorite = false
    var reprompt = false
    var notes = ""
    var username = ""
    var password = Secret.kept(false)
    var totp = Secret.kept(false)
    var uris: [UriRow] = []
    var cardholderName = ""
    var brand = ""
    var number = Secret.kept(false)
    var expMonth = ""
    var expYear = ""
    var code = Secret.kept(false)
    var identity: [String: String] = [:]
    var identitySecrets: [String: Secret] = [:]
    var privateKey = Secret.kept(false)
    var publicKey = ""
    var fingerprint = ""
    var fields: [FieldRow] = []
    var wifi = Wifi()

    init(kind: ItemKind) {
        if kind == .login { uris = [UriRow(id: 1, uri: "", match: nil)] }
    }

    static func row(_ field: FieldDetail) -> FieldRow {
        FieldRow(
            id: field.index + 1, name: field.name ?? "", kind: field.kind,
            value: field.kind == .hidden ? .kept(field.hasValue) : .typed(field.value ?? ""),
            from: field.index)
    }

    /// `formOf`: the item as it is now, as far as the editor is allowed to know it.
    init(summary: ItemSummary, detail: ItemDetail) {
        self.init(kind: summary.kind)
        name = summary.name
        folderId = summary.folderId ?? ""
        favorite = summary.favorite
        reprompt = summary.reprompt
        notes = detail.notes ?? ""
        if let login = detail.login {
            username = login.username ?? ""
            password = .kept(login.hasPassword)
            totp = .kept(login.hasTotp)
            uris = login.uris.enumerated().map { UriRow(id: $0.offset + 1, uri: $0.element.uri, match: $0.element.match) }
        }
        if let card = detail.card {
            cardholderName = card.cardholderName ?? ""
            brand = card.brand ?? ""
            number = .kept(!(card.numberEnding ?? "").isEmpty)
            expMonth = card.expMonth ?? ""
            expYear = card.expYear ?? ""
            code = .kept(card.hasCode)
        }
        for field in IdentityFields.all {
            let entry = detail.identity?.first { $0.name == field.name }
            if field.sensitive {
                identitySecrets[field.name] = .kept(entry != nil)
            } else {
                identity[field.name] = entry?.value ?? ""
            }
        }
        if let ssh = detail.sshKey {
            privateKey = .kept(ssh.hasPrivateKey)
            publicKey = ssh.publicKey ?? ""
            fingerprint = ssh.fingerprint ?? ""
        }
        if summary.kind == .wifi {
            let view = WifiFields.read(detail.fields ?? [])
            let security = view.value(.security)
            let enterprise = WifiFields.isEnterprise(security)
            wifi = Wifi(
                ssid: view.value(.ssid),
                password: view.password.map { $0.kind == .hidden ? .kept($0.hasValue) : .typed($0.value ?? "") } ?? .kept(false),
                security: security.isEmpty ? "WPA2" : security,
                hidden: view.hidden,
                // An Enterprise network without a method gets the usual one shown, and saved.
                eap: view.value(.eap).isEmpty ? (enterprise ? "" : "PEAP") : view.value(.eap),
                phase2: view.value(.phase2).isEmpty ? (enterprise ? "" : "MSCHAPV2") : view.value(.phase2),
                identity: view.value(.identity),
                anonymous: view.value(.anonymous),
                ca: view.value(.ca),
                from: view.from)
            fields = view.others.map(Self.row)
            return
        }
        fields = (detail.fields ?? []).map(Self.row)
    }

    /// `draftOf`: what goes back to the core.
    func draft(kind: ItemKind) -> Draft {
        let draftKind = DraftKind(rawValue: kind.rawValue) ?? .note
        var draft = Draft(
            kind: kind == .wifi ? .note : draftKind,
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            notes: notes,
            favorite: favorite,
            reprompt: reprompt,
            folderId: folderId.isEmpty ? nil : folderId,
            fields: fields.map { field in
                let clean = field.name.trimmingCharacters(in: .whitespacesAndNewlines)
                return FieldDraft(
                    name: clean.isEmpty ? nil : clean, kind: field.kind,
                    value: field.kind == .linked ? nil : field.value.sent, from: field.from)
            })
        switch kind {
        case .login:
            draft.login = LoginDraft(
                username: username, password: password.sent, totp: totp.sent,
                uris: uris.compactMap { row in
                    let clean = row.uri.trimmingCharacters(in: .whitespacesAndNewlines)
                    return clean.isEmpty ? nil : UriDraft(uri: clean, match: row.match)
                })
        case .card:
            draft.card = CardDraft(
                cardholderName: cardholderName, brand: brand, number: number.sent,
                expMonth: expMonth, expYear: expYear, code: code.sent)
        case .identity:
            var values: [String: String] = [:]
            for field in IdentityFields.all {
                if field.sensitive {
                    if let secret = identitySecrets[field.name], !secret.keep { values[field.name] = secret.value }
                } else {
                    values[field.name] = identity[field.name] ?? ""
                }
            }
            draft.identity = values
        case .sshKey:
            draft.sshKey = SshKeyDraft(privateKey: privateKey.sent, publicKey: publicKey, fingerprint: fingerprint)
        case .wifi:
            draft.fields = WifiFields.fields(
                text: [
                    .ssid: wifi.ssid.trimmingCharacters(in: .whitespacesAndNewlines), .security: wifi.security,
                    .eap: wifi.eap, .phase2: wifi.phase2, .identity: wifi.identity,
                    .anonymous: wifi.anonymous, .ca: wifi.ca,
                ],
                hidden: wifi.hidden, password: wifi.password.sent, from: wifi.from, others: draft.fields)
        case .note:
            break
        }
        return draft
    }
}
