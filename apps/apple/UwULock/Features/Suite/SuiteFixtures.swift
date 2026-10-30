// Samples for the suite screens (fixture mode and previews). Reserved names and addresses only.
#if DEBUG
import SwiftUI

enum SuiteFixtures {
    private static func record(_ id: String, _ kind: String, _ data: SuiteJSONObject?, broken: Bool = false)
        -> SuiteRecord
    {
        SuiteRecord(id: id, kind: kind, seq: 1, updatedMs: 1_790_000_000_000, data: data, broken: broken, usedBy: nil)
    }

    static let secrets: [String: String] = [
        "sec-ops": "fixture-ops-password",
        "sec-admin": "fixture-admin-password",
        "sec-gateway": "fixture-gateway-password",
    ]

    static func view(_ space: SuiteSpace) -> SuiteSpaceView {
        SuiteSpaceView(space: space, exists: true, records: withUsedBy(space == .rdp ? rdp : ssh))
    }

    private static var ssh: [SuiteRecord] {
        [
            record("grp-web", "group", ["name": "Web", "workspace": "business", "position": 0]),
            record("grp-home", "group", ["name": "Zuhause", "workspace": "private", "position": 0]),
            record("key-laptop", "key", [
                "label": "Laptop", "key_type": "ssh-ed25519",
                "public_key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFixtureOnlyNotARealKey nyu@laptop.test",
                "fingerprint": "SHA256:fixtureOnlyFingerprintNotReal",
            ]),
            record("id-deploy", "identity", [
                "label": "deploy", "username": "deploy", "auth_type": "key", "key_id": "key-laptop",
            ]),
            record("id-ops", "identity", [
                "label": "ops", "username": "ops", "auth_type": "password", "password_secret_id": "sec-ops",
            ]),
            record("sec-ops", "secret", nil),
            record("host-web1", "host", [
                "name": "web-1", "address": "web1.example.com", "port": 22, "workspace": "business",
                "group_id": "grp-web", "identity_id": "id-deploy", "position": 0,
            ]),
            record("host-web2", "host", [
                "name": "web-2", "address": "198.51.100.12", "port": 2222, "workspace": "business",
                "group_id": "grp-web", "identity_id": "id-ops", "position": 1,
            ]),
            record("host-nas", "host", [
                "name": "NAS", "address": "nas.home.test", "port": 22, "workspace": "private",
                "group_id": "grp-home", "position": 0,
            ]),
            record("host-lab", "host", [
                "name": "", "address": "192.0.2.40", "port": 22, "workspace": "private", "position": 0,
            ]),
            record("pf-db", "port_forward", [
                "host_id": "host-web1", "name": "Datenbank", "kind": "local", "bind_address": "127.0.0.1",
                "bind_port": 5433, "target_host": "localhost", "target_port": 5432, "autostart": true,
            ]),
            record("pf-back", "port_forward", [
                "host_id": "host-web1", "name": "", "kind": "remote", "bind_address": "",
                "bind_port": 9000, "target_host": "localhost", "target_port": 3000, "autostart": false,
            ]),
            record("snip-logs", "snippet", [
                "label": "Logs ansehen", "group_path": "Wartung", "body": "journalctl -u nginx -f",
            ]),
            record("snip-disk", "snippet", ["label": "Platz", "body": "df -h"]),
            record("kh-web1", "known_host", [
                "host": "web1.example.com", "port": 22, "key_type": "ssh-ed25519",
                "fingerprint": "SHA256:fixtureKnownHostNotReal",
            ]),
            record("broken-1", "host", nil, broken: true),
        ]
    }

    private static var rdp: [SuiteRecord] {
        [
            record("grp-office", "group", [
                "name": "Büro", "workspace": "business", "position": 0, "identity_id": "id-admin",
                "drives": ["enabled": true, "drives": [["name": "Home", "path": "~"]]],
            ]),
            record("id-admin", "identity", [
                "label": "Administrator", "username": "administrator", "domain": "CORP",
                "auth_type": "password", "password_secret_id": "sec-admin",
            ]),
            record("id-gateway", "identity", [
                "label": "Gateway", "username": "gw-user", "auth_type": "password",
                "password_secret_id": "sec-gateway",
            ]),
            record("sec-admin", "secret", nil),
            record("sec-gateway", "secret", nil),
            record("host-desk", "host", [
                "name": "Arbeitsplatz", "address": "desktop.example.com", "port": 3389, "workspace": "business",
                "group_id": "grp-office", "position": 0, "comment": "Der Rechner im zweiten Stock.",
                "gateway_identity_id": "id-gateway",
                "rdp": [
                    "display": "fixed", "width": 2560, "height": 1440, "smartSizing": true, "colorDepth": 32,
                    "audio": "local", "clipboard": true, "admin": false, "nla": true, "wallpaper": false,
                    "graphicsPipeline": true,
                    "gateway": [
                        "address": "gateway.example.com", "port": 443, "useHostLogin": false, "bypassLocal": true,
                    ],
                ],
            ]),
            record("host-server", "host", [
                "name": "Server", "address": "203.0.113.20", "port": 3390, "workspace": "business",
                "group_id": "grp-office", "position": 1,
                "rdp": ["display": "fullscreen", "audio": "off", "admin": true],
            ]),
            record("host-media", "host", [
                "name": "Mediacenter", "address": "media.home.test", "port": 3389, "workspace": "private",
                "position": 0,
            ]),
            record("broken-2", "group", nil, broken: true),
        ]
    }

    /// What the core says about who uses what: logins by hosts and groups, keys by logins.
    static func withUsedBy(_ records: [SuiteRecord]) -> [SuiteRecord] {
        var users: [String: [String]] = [:]
        for record in records {
            let fields =
                record.kind == "identity" ? ["key_id"]
                : (record.kind == "host" || record.kind == "group") ? ["identity_id", "gateway_identity_id"]
                : []
            for field in fields {
                if let target = SuiteJSON.ref(record.data, field) {
                    users[target, default: []].append(record.id)
                }
            }
        }
        return records.map { record in
            var copy = record
            let used = users[record.id] ?? []
            copy.usedBy = used.isEmpty ? nil : used
            return copy
        }
    }
}
#endif
