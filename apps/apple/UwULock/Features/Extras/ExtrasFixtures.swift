#if DEBUG
// Sample data of the extras for screenshots and previews (`-UwULockFixtures YES`).
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

@MainActor
enum ExtrasFixtures {
    /// The sample login: a NAS in the home network.
    static let loginId = "fixture-login"

    static func item(_ id: String) -> ItemDetail {
        ItemDetail(
            summary: ItemSummary(
                id: id, kind: .login, name: "Heim-NAS", subtitle: "mia@example.com",
                host: "nas.local", favorite: false, folderId: nil, organizationId: nil,
                collectionIds: [], deleted: false, reprompt: false, viewPassword: true,
                hasTotp: false, hasPassword: true, hasUsername: true, broken: false,
                revisionDate: "2026-10-28T09:12:00Z"),
            locked: false,
            login: LoginDetail(
                username: "mia@example.com", hasPassword: true, hasTotp: false,
                passwordRevisionDate: "2026-10-28T09:12:00Z",
                uris: [LoginUri(uri: "https://nas.local:5001", match: nil, host: "nas.local", openable: true)],
                passkeys: 0))
    }

    static var ownIcons: [String: String] = [:]

    static var reminders: [String: ReminderMark] = [
        loginId: ReminderMark(due: "2026-11-15", everyMonths: 6, isDue: false)
    ]

    static let masked = MaskedLink(id: "m1", email: "kirsch.fuchs@mask.example.net")

    static var optIn = EmailOptIn(optedIn: false, since: nil)

    static var versions: [UwuVersion] = [
        UwuVersion(
            id: "v3", revisionDate: "2026-10-20T18:40:00Z", replacedDate: "2026-10-28T09:12:00Z",
            size: 812, broken: false,
            changes: [
                VersionChange(field: "password", label: nil, kind: "changed", secret: true, before: nil, after: nil),
                VersionChange(field: "uri:0", label: nil, kind: "changed", secret: false,
                              before: "http://192.0.2.20:5000", after: "https://nas.local:5001"),
            ]),
        UwuVersion(
            id: "v2", revisionDate: "2026-06-02T07:05:00Z", replacedDate: "2026-10-20T18:40:00Z",
            size: 790, broken: false,
            changes: [
                VersionChange(field: "username", label: nil, kind: "changed", secret: false,
                              before: "admin", after: "mia@example.com"),
                VersionChange(field: "field:0", label: "PIN", kind: "removed", secret: true, before: nil, after: nil),
            ]),
        UwuVersion(id: "v1", revisionDate: "2025-12-24T15:30:00Z", replacedDate: nil, size: 640, broken: true, changes: []),
    ]

    static let library = IconLibrary(
        updated: "2026-10-01",
        sources: [
            LibrarySource(
                id: "selfhst", name: "selfh.st/icons", url: "https://icons.example.org",
                license: "CC BY 4.0", licenseUrl: "https://license.example.org/by/4.0",
                attribution: "selfh.st"),
        ],
        icons: [
            LibraryIcon(source: "selfhst", id: "synology", name: "Synology", variants: ["default", "light", "dark"], aliases: ["nas", "dsm"]),
            LibraryIcon(source: "selfhst", id: "truenas", name: "TrueNAS", variants: ["default"], aliases: ["nas"]),
            LibraryIcon(source: "selfhst", id: "nextcloud", name: "Nextcloud", variants: ["default", "light", "dark"], aliases: ["cloud"]),
            LibraryIcon(source: "selfhst", id: "jellyfin", name: "Jellyfin", variants: ["default"], aliases: ["media"]),
            LibraryIcon(source: "selfhst", id: "home-assistant", name: "Home Assistant", variants: ["default", "dark"], aliases: ["hass"]),
            LibraryIcon(source: "selfhst", id: "openmediavault", name: "openmediavault", variants: ["default"], aliases: ["nas", "omv"]),
        ])

    /// A small round-cornered colour square as a PNG `data:` URL.
    static func png(hue: Double, dark: Bool = false, light: Bool = false) -> String {
        let side = 64
        guard
            let context = CGContext(
                data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: 0,
                space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return "" }
        let brightness = dark ? 0.35 : light ? 0.98 : 0.85
        let saturation = light ? 0.25 : 0.55
        let (r, g, b) = rgb(hue: hue, saturation: saturation, brightness: brightness)
        context.setFillColor(red: r, green: g, blue: b, alpha: 1)
        let path = CGPath(
            roundedRect: CGRect(x: 4, y: 4, width: side - 8, height: side - 8),
            cornerWidth: 14, cornerHeight: 14, transform: nil)
        context.addPath(path)
        context.fillPath()
        context.setFillColor(red: 1, green: 1, blue: 1, alpha: dark ? 0.8 : 0.6)
        context.fillEllipse(in: CGRect(x: 20, y: 20, width: 24, height: 24))
        guard let image = context.makeImage() else { return "" }
        let data = NSMutableData()
        guard let target = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)
        else { return "" }
        CGImageDestinationAddImage(target, image, nil)
        CGImageDestinationFinalize(target)
        return "data:image/png;base64," + (data as Data).base64EncodedString()
    }

    private static func rgb(hue: Double, saturation s: Double, brightness v: Double) -> (Double, Double, Double) {
        let h = (hue.truncatingRemainder(dividingBy: 1)) * 6
        let i = Int(h)
        let f = h - Double(i)
        let p = v * (1 - s)
        let q = v * (1 - s * f)
        let t = v * (1 - s * (1 - f))
        switch i % 6 {
        case 0: return (v, t, p)
        case 1: return (q, v, p)
        case 2: return (p, v, t)
        case 3: return (p, q, v)
        case 4: return (t, p, v)
        default: return (v, p, q)
        }
    }
}
#endif
