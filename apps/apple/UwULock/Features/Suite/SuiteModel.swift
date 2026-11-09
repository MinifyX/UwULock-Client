// The sections "SSH (UwUSSH)" and "Remote Desktop (UwURDP)" without SwiftUI: reading the
// records' JSON, how the list is grouped and sorted, the copied command and the edits that
// reorder hosts and groups (apps/desktop/src/lib/suiteModel.ts).
//
// The payloads are UwUSSH's and UwURDP's JSON (snake_case; UwURDP's `rdp` settings camelCase).
// The screen reads the fields it shows and sends back only the ones it changed: the core lays
// them over the record, so fields of a newer app stay.
import Foundation

typealias SuiteJSONObject = [String: JSONValue]

/// Reading a record's payload the way the page does: wrong types read as empty.
enum SuiteJSON {
    static func str(_ data: SuiteJSONObject?, _ field: String) -> String {
        data?[field]?.string ?? ""
    }

    static func num(_ data: SuiteJSONObject?, _ field: String, _ fallback: Double = 0) -> Double {
        guard let value = data?[field]?.number, value.isFinite else { return fallback }
        return value
    }

    /// A number as a whole number (ports, sizes, positions).
    static func int(_ data: SuiteJSONObject?, _ field: String, _ fallback: Int = 0) -> Int {
        let value = num(data, field, Double(fallback))
        return abs(value) < 9e15 ? Int(value) : fallback
    }

    static func bool(_ data: SuiteJSONObject?, _ field: String, _ fallback: Bool = false) -> Bool {
        data?[field]?.bool ?? fallback
    }

    static func obj(_ data: SuiteJSONObject?, _ field: String) -> SuiteJSONObject? {
        data?[field]?.object
    }

    /// A pointer at another record, or nil.
    static func ref(_ data: SuiteJSONObject?, _ field: String) -> String? {
        let value = str(data, field)
        return value.isEmpty ? nil : value
    }

    /// Whether the field is there at all (`!== undefined`).
    static func has(_ data: SuiteJSONObject?, _ field: String) -> Bool {
        data?[field] != nil
    }

    static func number(_ value: Int) -> JSONValue { .number(Double(value)) }

    static func optional(_ value: String?) -> JSONValue { value.map(JSONValue.string) ?? .null }
}

/// The workspaces UwUSSH and UwURDP know; a newer app may add more.
let suiteWorkspaces = ["private", "business"]

/// The ways an identity logs in.
let suiteAuthTypes = ["password", "key", "agent", "keyboard-interactive", "cert"]

/// A random UUID (v4), for a record made here.
func suiteNewId() -> String {
    UUID().uuidString.lowercased()
}

extension SuiteSpace {
    /// The section's title.
    var title: String {
        switch self {
        case .ssh: featureString("SSH (UwUSSH)")
        case .rdp: featureString("Remote Desktop (UwURDP)")
        }
    }

    /// The app that owns the section.
    var appName: String { self == .ssh ? "UwUSSH" : "UwURDP" }

    /// The port a host has when it has none.
    var defaultPort: Int { self == .rdp ? 3389 : 22 }
}

extension SuiteRecord {
    /// What the record is called in a list (empty: "(ohne Namen)").
    var title: String {
        let d = data
        switch kind {
        case "host":
            let name = SuiteJSON.str(d, "name")
            return name.isEmpty ? SuiteJSON.str(d, "address") : name
        case "group", "port_forward":
            return SuiteJSON.str(d, "name")
        case "identity":
            let label = SuiteJSON.str(d, "label")
            return label.isEmpty ? SuiteJSON.str(d, "username") : label
        case "key", "snippet":
            return SuiteJSON.str(d, "label")
        case "known_host":
            return suiteHostPort(SuiteJSON.str(d, "address"), SuiteJSON.int(d, "port", 22))
        default:
            return ""
        }
    }

    /// The title a row shows: broken and unnamed records say so.
    var shownTitle: String {
        if broken { return featureString("(lässt sich nicht öffnen)") }
        let title = title
        return title.isEmpty ? featureString("(ohne Namen)") : title
    }

    /// The workspace of a host or group ("private" when it has none).
    var workspace: String {
        let value = SuiteJSON.str(data, "workspace")
        return value.isEmpty ? "private" : value
    }

    var position: Int { SuiteJSON.int(data, "position") }
}

/// `host:port`, an IPv6 address in brackets.
func suiteHostPort(_ address: String, _ port: Int) -> String {
    address.contains(":") && !address.hasPrefix("[") ? "[\(address)]:\(port)" : "\(address):\(port)"
}

/// One line of text for the clipboard: control characters (also U+0085) and Unicode line
/// breaks out, so a pasted command can't run a second one.
func suiteOneLine(_ text: String) -> String {
    String(String.UnicodeScalarView(text.unicodeScalars.filter { scalar in
        !(scalar.properties.generalCategory == .control || scalar.value == 0x2028
            || scalar.value == 0x2029)
    }))
}

/// Words a shell takes as they are; anything else goes in single quotes.
func suiteShellQuote(_ word: String) -> String {
    if word.isEmpty { return "''" }
    let plain = word.unicodeScalars.allSatisfy { scalar in
        scalar.isASCII
            && (CharacterSet.alphanumerics.contains(scalar) || "@%+=:,./_-[]".unicodeScalars.contains(scalar))
    }
    return plain ? word : "'" + word.replacingOccurrences(of: "'", with: "'\\''") + "'"
}

/// `ssh -p 2222 user@host`: what to type to reach a host.
func suiteSshCommand(_ host: SuiteJSONObject?, _ identity: SuiteJSONObject?) -> String {
    let port = SuiteJSON.int(host, "port", 22)
    let user = suiteOneLine(SuiteJSON.str(identity, "username"))
    let target = (user.isEmpty ? "" : "\(user)@") + suiteOneLine(SuiteJSON.str(host, "address"))
    var words = ["ssh"]
    if port != 22 { words += ["-p", String(port)] }
    // A target starting with `-` would be one of ssh's options (`-oProxyCommand=…`).
    if target.hasPrefix("-") { words.append("--") }
    words.append(suiteShellQuote(target))
    return words.joined(separator: " ")
}

/// `host:port` for a remote desktop client.
func suiteRdpAddress(_ host: SuiteJSONObject?) -> String {
    suiteHostPort(suiteOneLine(SuiteJSON.str(host, "address")), SuiteJSON.int(host, "port", 3389))
}

private func suiteCompare(_ a: String, _ b: String) -> ComparisonResult {
    a.localizedStandardCompare(b)
}

/// The records of `kind`, by title.
func suiteOfKind(_ records: [SuiteRecord], _ kind: String) -> [SuiteRecord] {
    records.filter { $0.kind == kind }
        .sorted { suiteCompare($0.title, $1.title) == .orderedAscending }
}

/// The login a host connects with: its own, else (UwURDP) its group's.
func suiteLoginOf(_ host: SuiteRecord, _ index: [String: SuiteRecord]) -> SuiteRecord? {
    if let own = index[SuiteJSON.ref(host.data, "identity_id") ?? ""], own.kind == "identity" {
        return own
    }
    guard let group = index[SuiteJSON.ref(host.data, "group_id") ?? ""],
        let inherited = index[SuiteJSON.ref(group.data, "identity_id") ?? ""],
        inherited.kind == "identity"
    else { return nil }
    return inherited
}

private func suiteByPosition(_ a: SuiteRecord, _ b: SuiteRecord) -> Bool {
    if a.position != b.position { return a.position < b.position }
    let byTitle = suiteCompare(a.title, b.title)
    if byTitle != .orderedSame { return byTitle == .orderedAscending }
    return a.id < b.id
}

struct SuiteHostGroup: Identifiable {
    var group: SuiteRecord?
    var hosts: [SuiteRecord]
    var id: String { group?.id ?? "loose" }
}

struct SuiteWorkspace: Identifiable {
    var workspace: String
    var groups: [SuiteHostGroup]
    var id: String { workspace }
}

func suiteWords(_ query: String) -> [String] {
    query.lowercased().split(whereSeparator: \.isWhitespace).map(String.init)
}

/// Words of a search, all of which must be somewhere in the host.
private func suiteMatches(_ host: SuiteRecord, _ login: SuiteRecord?, _ words: [String]) -> Bool {
    guard !words.isEmpty else { return true }
    let hay = [
        SuiteJSON.str(host.data, "name"), SuiteJSON.str(host.data, "address"),
        SuiteJSON.str(host.data, "comment"), SuiteJSON.str(login?.data, "username"),
    ].joined(separator: " ").lowercased()
    return words.allSatisfy { hay.contains($0) }
}

/// The hosts by workspace (private first, then business, then any a newer app made), each by
/// group in the dragged order; hosts without a group last. Empty groups show only without a
/// search.
func suiteWorkspaceList(_ records: [SuiteRecord], query: String = "") -> [SuiteWorkspace] {
    let index = Dictionary(records.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    let words = suiteWords(query)
    let groups = records.filter { $0.kind == "group" && $0.data != nil }.sorted(by: suiteByPosition)
    let hosts = records.filter { $0.kind == "host" && $0.data != nil }.sorted(by: suiteByPosition)
    var names = Set(suiteWorkspaces)
    for record in groups + hosts { names.insert(record.workspace) }
    let order = suiteWorkspaces + names.filter { !suiteWorkspaces.contains($0) }.sorted()
    let known = Set(groups.map(\.id))
    var result: [SuiteWorkspace] = []
    for workspace in order {
        let shown = hosts.filter {
            $0.workspace == workspace && suiteMatches($0, suiteLoginOf($0, index), words)
        }
        var list: [SuiteHostGroup] = []
        for group in groups where group.workspace == workspace {
            let members = shown.filter { SuiteJSON.ref($0.data, "group_id") == group.id }
            if !members.isEmpty || words.isEmpty { list.append(SuiteHostGroup(group: group, hosts: members)) }
        }
        let loose = shown.filter { !known.contains(SuiteJSON.ref($0.data, "group_id") ?? "") }
        if !loose.isEmpty { list.append(SuiteHostGroup(group: nil, hosts: loose)) }
        if !list.isEmpty { result.append(SuiteWorkspace(workspace: workspace, groups: list)) }
    }
    return result
}

/// The edits that move a host (or a group) one place up (`-1`) or down (`1`) among its
/// siblings: everyone gets the position of their new place, and only those whose position
/// changes are written.
func suiteMoveOps(_ siblings: [SuiteRecord], id: String, step: Int) -> [SuiteOp] {
    var list = siblings.sorted(by: suiteByPosition)
    guard let from = list.firstIndex(where: { $0.id == id }) else { return [] }
    let to = from + step
    guard to >= 0, to < list.count else { return [] }
    let moved = list.remove(at: from)
    list.insert(moved, at: to)
    return list.enumerated().compactMap { position, record in
        if SuiteJSON.has(record.data, "position") && record.position == position { return nil }
        return .put(
            id: record.id, kind: record.kind, seq: record.seq,
            patch: ["position": SuiteJSON.number(position)])
    }
}

/// A host's siblings: same workspace, same group (a group's: the groups of its workspace).
func suiteSiblings(_ records: [SuiteRecord], of record: SuiteRecord) -> [SuiteRecord] {
    let workspace = record.workspace
    if record.kind == "group" {
        return records.filter { $0.kind == "group" && $0.workspace == workspace }
    }
    let group = SuiteJSON.ref(record.data, "group_id")
    return records.filter {
        $0.kind == "host" && $0.workspace == workspace && SuiteJSON.ref($0.data, "group_id") == group
    }
}

/// The next free position at the end of a list.
func suiteNextPosition(_ siblings: [SuiteRecord]) -> Int {
    siblings.reduce(0) { max($0, $1.position + 1) }
}

/// A host's port forwards, by name.
func suiteTunnels(_ records: [SuiteRecord], hostId: String) -> [SuiteRecord] {
    suiteOfKind(records, "port_forward").filter { SuiteJSON.ref($0.data, "host_id") == hostId }
}

/// `-L 8080:localhost:80` or `-R …`, as ssh would take it.
func suiteTunnelSpec(_ tunnel: SuiteJSONObject?) -> String {
    let kind = SuiteJSON.str(tunnel, "kind")
    let flag = kind == "remote" ? "-R" : kind == "local" ? "-L" : "?"
    let bind = SuiteJSON.str(tunnel, "bind_address")
    let listen = (bind.isEmpty ? "" : "\(bind):") + String(SuiteJSON.int(tunnel, "bind_port"))
    return "\(flag) \(listen):\(SuiteJSON.str(tunnel, "target_host")):\(SuiteJSON.int(tunnel, "target_port"))"
}

/// A port as typed: 1–65535, else nil.
func suitePort(_ text: String) -> Int? {
    guard let value = Int(text.trimmingCharacters(in: .whitespaces)), (1...65535).contains(value) else {
        return nil
    }
    return value
}

// MARK: Words

func suiteWorkspaceLabel(_ workspace: String) -> String {
    switch workspace {
    case "private": featureString("Privat")
    case "business": featureString("Geschäftlich")
    default: workspace
    }
}

func suiteAuthLabel(_ auth: String) -> String {
    switch auth {
    case "password": featureString("Passwort")
    case "key": featureString("Schlüssel")
    case "agent": featureString("SSH-Agent")
    case "keyboard-interactive": featureString("Tastatur-interaktiv")
    case "cert": featureString("Zertifikat")
    default: auth
    }
}

func suiteKindLabel(_ kind: String) -> String {
    switch kind {
    case "host": featureString("Host")
    case "group": featureString("Gruppe")
    case "identity": featureString("Anmeldung")
    case "key": featureString("Schlüssel")
    case "snippet": featureString("Snippet")
    case "known_host": featureString("Bekannter Host")
    case "port_forward": featureString("Weiterleitung")
    default: kind
    }
}

/// The core's failures of this section in the page's words (lib/errors.ts).
func suiteFailureText(_ failure: CoreFailure) -> String {
    switch failure.kind {
    case "suite-conflict", "space-changed":
        return featureString(
            "Das wurde inzwischen woanders geändert. UwULock hat neu geladen und nichts überschrieben – ändere es bitte noch einmal.")
    case "in-use":
        let names = failure.message.replacingOccurrences(of: "still used by: ", with: "")
        return featureString("Wird noch verwendet von: \(names)")
    case "key-format":
        return featureString(
            "Das ist kein privater Schlüssel, den UwULock lesen kann (OpenSSH, PEM oder PuTTY).")
    case "key-passphrase":
        return failure.message.contains("needs")
            ? featureString("Dieser Schlüssel ist mit einer Passphrase geschützt. Gib sie mit ein.")
            : featureString("Die Passphrase passt nicht zu diesem Schlüssel.")
    case "no-app":
        return featureString("Die App ließ sich nicht öffnen. Ist sie auf diesem Gerät installiert?")
    default:
        return failure.message
    }
}

// MARK: Drive redirection (UwURDP)

struct SuiteDrive: Equatable, Identifiable {
    var id = UUID()
    var name: String
    var path: String
    var orig: SuiteJSONObject

    static func == (a: SuiteDrive, b: SuiteDrive) -> Bool {
        a.name == b.name && a.path == b.path && a.orig == b.orig
    }
}

struct SuiteDrives: Equatable {
    enum Mode: String { case inherit, off, on }
    var mode: Mode
    var list: [SuiteDrive]
    var orig: SuiteJSONObject

    init(_ value: SuiteJSONObject?, inherit: Bool) {
        guard let value else {
            mode = inherit ? .inherit : .off
            list = []
            orig = [:]
            return
        }
        mode = SuiteJSON.bool(value, "enabled") ? .on : .off
        orig = value
        list = (value["drives"]?.array ?? []).compactMap { $0.object }.map {
            SuiteDrive(name: SuiteJSON.str($0, "name"), path: SuiteJSON.str($0, "path"), orig: $0)
        }
    }

    /// nil takes the group's (a host) or is off (a group).
    var written: JSONValue? {
        if mode == .inherit { return nil }
        if mode == .off && list.isEmpty && orig.isEmpty { return nil }
        var out = orig
        out["enabled"] = .bool(mode == .on)
        out["drives"] = .array(
            list.filter {
                !$0.name.trimmingCharacters(in: .whitespaces).isEmpty
                    || !$0.path.trimmingCharacters(in: .whitespaces).isEmpty
            }.map { drive in
                var item = drive.orig
                item["name"] = .string(drive.name.trimmingCharacters(in: .whitespaces))
                item["path"] = .string(drive.path.trimmingCharacters(in: .whitespaces))
                return .object(item)
            })
        return .object(out)
    }
}

/// What drive redirection says in a detail.
func suiteDrivesText(_ drives: SuiteJSONObject?, inherit: Bool) -> String {
    guard let drives else { return inherit ? featureString("Wie die Gruppe") : featureString("Aus") }
    guard SuiteJSON.bool(drives, "enabled") else { return featureString("Aus") }
    let list = (drives["drives"]?.array ?? []).compactMap(\.object)
    guard !list.isEmpty else { return featureString("An") }
    return list.map { item in
        let path = SuiteJSON.str(item, "path")
        return "\(SuiteJSON.str(item, "name")) → \(path == "*" ? featureString("alle Laufwerke") : path)"
    }.joined(separator: ", ")
}
