// A Wi-Fi network as an item's fields hold it, and its `WIFI:` code (lib/wifi.ts). The field
// names are a contract with other apps: never translated.
import Foundation

struct WifiNetwork: Equatable {
    static let marker = "uwulock:type"
    static let type = "wifi"

    enum Name {
        static let ssid = "SSID"
        static let password = "Password"
        static let security = "Security"
        static let hidden = "Hidden network"
        static let eap = "EAP method"
        static let phase2 = "Phase 2"
        static let identity = "Identity"
        static let anonymous = "Anonymous identity"
        static let ca = "CA certificate"
    }

    var ssid = ""
    var security = ""
    var eap = ""
    var phase2 = ""
    var identity = ""
    var anonymous = ""
    var ca = ""
    var hidden = false
    /// The password's field, for revealing and copying it by index (`field:<n>`).
    var password: FieldDetail?

    var enterprise: Bool { Self.isEnterprise(security) }
    var open: Bool { security == "None" }
    /// The network has a password worth showing.
    var hasPassword: Bool { (password?.hasValue ?? false) && !open }
    var passwordField: String? { password.map { "field:\($0.index)" } }

    static func isEnterprise(_ security: String) -> Bool { security.hasSuffix("-Enterprise") }

    /// Reads a network out of an item's fields. The first field of each name counts; Enterprise
    /// fields only on an Enterprise network.
    init(fields: [FieldDetail]) {
        var taken = Set<Int>()
        if let marker = fields.first(where: {
            $0.name == Self.marker && $0.kind == .text
                && $0.value?.trimmingCharacters(in: .whitespaces).lowercased() == Self.type
        }) {
            taken.insert(marker.index)
        }
        func take(_ name: String) -> FieldDetail? {
            guard let field = fields.first(where: { $0.name == name && !taken.contains($0.index) }) else {
                return nil
            }
            taken.insert(field.index)
            return field
        }
        ssid = take(Name.ssid)?.value ?? ""
        security = take(Name.security)?.value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        password = take(Name.password)
        hidden = take(Name.hidden)?.value?.trimmingCharacters(in: .whitespaces).lowercased() == "true"
        if Self.isEnterprise(security) {
            eap = take(Name.eap)?.value ?? ""
            phase2 = take(Name.phase2)?.value ?? ""
            identity = take(Name.identity)?.value ?? ""
            anonymous = take(Name.anonymous)?.value ?? ""
            ca = take(Name.ca)?.value ?? ""
        }
    }

    /// `\ ; , : "` escaped with a backslash, as the `WIFI:` format wants.
    static func escape(_ value: String) -> String {
        var out = ""
        for char in value {
            if "\\;,:\"".contains(char) { out.append("\\") }
            out.append(char)
        }
        return out
    }

    /// The `WIFI:` text phones join from with their camera.
    func qr(password: String) -> String {
        let type = enterprise ? "WPA2-EAP" : open ? "nopass" : security == "WEP" ? "WEP" : "WPA"
        let hex = ssid.range(of: "^(?:[0-9a-fA-F]{2})+$", options: .regularExpression) != nil
        let name = hex ? "\"\(Self.escape(ssid))\"" : Self.escape(ssid)
        var parts = ["T:\(type)", "S:\(name)"]
        func add(_ key: String, _ value: String) {
            if !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                parts.append("\(key):\(Self.escape(value))")
            }
        }
        if enterprise {
            add("E", eap)
            add("PH2", phase2 == "none" ? "" : phase2)
            add("A", anonymous)
            add("I", identity)
        }
        if type != "nopass" { add("P", password) }
        if hidden { parts.append("H:true") }
        return "WIFI:" + parts.joined(separator: ";") + ";;"
    }

    /// How the security shows (SECURITY_LABEL); the raw value when unknown, "—" when empty.
    var securityLabel: String {
        switch security {
        case "WPA3": "WPA3"
        case "WPA2/WPA3": featureString("WPA2/WPA3 (gemischt)")
        case "WPA2": "WPA2"
        case "WPA": featureString("WPA (veraltet)")
        case "WEP": featureString("WEP (unsicher)")
        case "None": featureString("Keine (offenes Netz)")
        case "WPA2-Enterprise": "WPA2-Enterprise"
        case "WPA3-Enterprise": "WPA3-Enterprise"
        case "": "—"
        default: security
        }
    }
}
