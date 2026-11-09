import Foundation

/// Wi-Fi items: a note whose custom fields hold the network (lib/wifi.ts).
enum WifiFields {
    static let marker = "uwulock:type"
    static let type = "wifi"

    enum Key: String, CaseIterable {
        case marker, ssid, password, security, hidden, eap, phase2, identity, anonymous, ca

        var fieldName: String {
            switch self {
            case .marker: WifiFields.marker
            case .ssid: "SSID"
            case .password: "Password"
            case .security: "Security"
            case .hidden: "Hidden network"
            case .eap: "EAP method"
            case .phase2: "Phase 2"
            case .identity: "Identity"
            case .anonymous: "Anonymous identity"
            case .ca: "CA certificate"
            }
        }
    }

    static let enterpriseKeys: [Key] = [.eap, .phase2, .identity, .anonymous, .ca]
    static let securities = [
        "WPA3", "WPA2/WPA3", "WPA2", "WPA", "WEP", "None", "WPA2-Enterprise", "WPA3-Enterprise",
    ]
    static let eapMethods = ["PEAP", "TTLS", "TLS", "PWD"]
    static let phase2Methods = ["MSCHAPV2", "PAP", "GTC", "none"]

    static func isEnterprise(_ security: String) -> Bool { security.hasSuffix("-Enterprise") }

    /// SECURITY_LABEL.
    static func securityLabel(_ security: String) -> String {
        switch security {
        case "WPA2/WPA3": vt("WPA2/WPA3 (gemischt)")
        case "WPA": vt("WPA (veraltet)")
        case "WEP": vt("WEP (unsicher)")
        case "None": vt("Keine (offenes Netz)")
        default: security
        }
    }

    /// What `readWifi` makes of an item's fields.
    struct View {
        var text: [Key: String] = [:]
        var hidden = false
        var password: FieldDetail?
        var from: [Key: Int] = [:]
        var others: [FieldDetail] = []

        func value(_ key: Key) -> String { text[key] ?? "" }
    }

    static func read(_ fields: [FieldDetail]) -> View {
        var view = View()
        var taken = Set<Int>()
        func take(_ key: Key) -> FieldDetail? {
            guard let field = fields.first(where: { $0.name == key.fieldName && !taken.contains($0.index) })
            else { return nil }
            taken.insert(field.index)
            view.from[key] = field.index
            return field
        }
        if let marker = fields.first(where: {
            $0.name == marker && $0.kind == .text
                && $0.value?.trimmingCharacters(in: .whitespaces).lowercased() == type
        }) {
            taken.insert(marker.index)
            view.from[.marker] = marker.index
        }
        view.text[.ssid] = take(.ssid)?.value ?? ""
        view.text[.security] = take(.security)?.value?.trimmingCharacters(in: .whitespaces) ?? ""
        view.password = take(.password)
        view.hidden = take(.hidden)?.value?.trimmingCharacters(in: .whitespaces).lowercased() == "true"
        if isEnterprise(view.value(.security)) {
            for key in enterpriseKeys { view.text[key] = take(key)?.value ?? "" }
        }
        view.others = fields.filter { !taken.contains($0.index) }
        return view
    }

    /// `wifiFields`: the fields a Wi-Fi item saves, before the others.
    static func fields(
        text: [Key: String], hidden: Bool, password: String?, from: [Key: Int], others: [FieldDraft]
    ) -> [FieldDraft] {
        func field(_ key: Key, _ kind: FieldKind, _ value: String?) -> FieldDraft {
            FieldDraft(name: key.fieldName, kind: kind, value: value, from: from[key])
        }
        let security = text[.security] ?? ""
        var fields = [
            field(.marker, .text, type),
            field(.ssid, .text, text[.ssid] ?? ""),
            field(.password, .hidden, password),
            field(.security, .text, security),
            field(.hidden, .boolean, hidden ? "true" : "false"),
        ]
        if isEnterprise(security) {
            for key in enterpriseKeys {
                let value = (text[key] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
                if !value.isEmpty { fields.append(field(key, .text, value)) }
            }
        }
        return fields + others
    }
}
