// Helpers of the extras: local hosts (lib/uwu.ts `isLocalHost`), the icon library's search
// (lib/iconLibrary.ts), own icons from a picture (lib/iconImage.ts), `data:` images, and the names
// of an item's values in the version comparison (ItemExtras.tsx `valueLabel`).
import CoreGraphics
import Foundation
import ImageIO
import SwiftUI
import UniformTypeIdentifiers
import UwUDesign

#if os(iOS)
import UIKit
typealias ExtrasPlatformImage = UIImage
#else
import AppKit
typealias ExtrasPlatformImage = NSImage
#endif

// MARK: Hosts

enum ExtrasHost {
    /// Whether `host` is in the home network: private and link-local addresses, `.local`,
    /// `.lan`, … and names without a dot. The server never asks such a device for its icon.
    static func isLocal(_ host: String?) -> Bool {
        guard var h = host?.lowercased(), !h.isEmpty else { return false }
        if h.hasPrefix("[") { h.removeFirst() }
        if h.hasSuffix("]") { h.removeLast() }
        if h.hasSuffix(".") { h.removeLast() }
        let parts = h.split(separator: ".", omittingEmptySubsequences: false)
        if parts.count == 4, parts.allSatisfy({ !$0.isEmpty && $0.allSatisfy(\.isNumber) }) {
            let a = Int(parts[0]) ?? -1
            let b = Int(parts[1]) ?? -1
            return a == 10 || a == 127 || (a == 172 && b >= 16 && b < 32) || (a == 192 && b == 168)
                || (a == 169 && b == 254) || (a == 100 && b >= 64 && b < 128)
        }
        if h.contains(":") {
            return h.hasPrefix("::1") || h.range(of: "^(f[cd]|fe[89ab])", options: .regularExpression) != nil
        }
        if !h.contains(".") { return true }
        let last = h[h.index(after: h.lastIndex(of: ".")!)...]
        return [
            "local", "lan", "home", "internal", "intranet", "localhost", "localdomain", "corp",
            "private", "arpa",
        ].contains(String(last))
    }

    /// The host of an address as somebody typed it (`nas.local:5000`, `https://…`).
    static func host(of address: String) -> String? {
        let text = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return nil }
        let url = URL(string: text.contains("://") ? text : "http://\(text)")
        guard let host = url?.host(percentEncoded: false), !host.isEmpty else { return nil }
        return host
    }

    /// The first label of a local host as an app's name: `jellyfin` of `jellyfin.local`.
    static func localLabel(_ host: String?) -> String? {
        guard var bare = host?.lowercased() else { return nil }
        if bare.hasPrefix("[") { bare.removeFirst() }
        if bare.hasSuffix("]") { bare.removeLast() }
        if bare.contains(":") { return nil }
        if bare.range(of: #"^\d+\.\d+\.\d+\.\d+$"#, options: .regularExpression) != nil { return nil }
        let label = String(bare.split(separator: ".", omittingEmptySubsequences: false).first ?? "")
        guard label.range(of: "^[a-z0-9_-]{1,63}$", options: .regularExpression) != nil,
            label.range(of: "[a-z]", options: .regularExpression) != nil, label != "localhost"
        else { return nil }
        return label
    }
}

// MARK: Icon library search

extension IconLibrary {
    /// Icons whose name, id or aliases have every word of `query`, the best first (as the web
    /// vault searches).
    func search(_ query: String, most: Int = 60) -> [LibraryIcon] {
        let words = query.lowercased().split(whereSeparator: \.isWhitespace).map(String.init)
        guard let first = words.first else { return [] }
        let whole = words.joined(separator: " ")
        let ranked: [(LibraryIcon, Int)] = icons.compactMap { icon in
            let name = icon.name.lowercased()
            let haystack = ([name, icon.id] + icon.aliases).joined(separator: " ").lowercased()
            guard words.allSatisfy({ haystack.contains($0) }) else { return nil }
            let tier = name == whole ? 0 : name.hasPrefix(first) ? 1 : 2
            return (icon, tier * 1000 + name.count)
        }
        return ranked.enumerated()
            .sorted { $0.element.1 != $1.element.1 ? $0.element.1 < $1.element.1 : $0.offset < $1.offset }
            .prefix(most).map(\.element.0)
    }

    /// Icons that fit an item in the home network: by the device's name, then by the item's
    /// name (whole, else word by word), each once.
    func suggest(host: String?, name: String, most: Int = 6) -> [LibraryIcon] {
        var found: [LibraryIcon] = []
        var seen = Set<String>()
        func add(_ query: String?) -> Int {
            guard let query, !query.trimmingCharacters(in: .whitespaces).isEmpty else { return 0 }
            let icons = search(query, most: most)
            for icon in icons where seen.insert("\(icon.source)/\(icon.id)").inserted {
                found.append(icon)
            }
            return icons.count
        }
        _ = add(ExtrasHost.localLabel(host)?.replacingOccurrences(
            of: "[-_]+", with: " ", options: .regularExpression))
        if add(name) == 0 {
            for word in name.split(whereSeparator: \.isWhitespace) where word.count >= 4 {
                _ = add(String(word))
            }
        }
        return Array(found.prefix(most))
    }
}

// MARK: Pictures

enum ExtrasIconImage {
    struct Unreadable: Error {}

    /// The kinds of picture an own icon can come from.
    static let accepted: [UTType] = [.png, .jpeg, .webP, .svg, .heic]

    private static let maxInput = 8 * 1024 * 1024
    private static let size = 128

    /// The picture as a square PNG `data:` URL of at most 128 × 128 pixels, cut from its middle
    /// (an SVG is drawn at the icon's size, never sent as SVG).
    static func pngDataURL(from data: Data, type: UTType? = nil) throws -> String {
        guard data.count <= maxInput else { throw Unreadable() }
        let vector = type?.conforms(to: .svg) ?? false
        guard let image = vector ? vectorImage(data) : rasterImage(data) else { throw Unreadable() }
        let width = image.width
        let height = image.height
        let side = min(width, height)
        guard side > 0 else { throw Unreadable() }
        let out = vector ? size : min(size, side)
        guard
            let context = CGContext(
                data: nil, width: out, height: out, bitsPerComponent: 8, bytesPerRow: 0,
                space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { throw Unreadable() }
        context.interpolationQuality = .high
        let crop = CGRect(x: (width - side) / 2, y: (height - side) / 2, width: side, height: side)
        guard let square = image.cropping(to: crop) else { throw Unreadable() }
        context.draw(square, in: CGRect(x: 0, y: 0, width: out, height: out))
        guard let drawn = context.makeImage() else { throw Unreadable() }
        let png = NSMutableData()
        guard let target = CGImageDestinationCreateWithData(png, UTType.png.identifier as CFString, 1, nil)
        else { throw Unreadable() }
        CGImageDestinationAddImage(target, drawn, nil)
        guard CGImageDestinationFinalize(target) else { throw Unreadable() }
        return "data:image/png;base64," + (png as Data).base64EncodedString()
    }

    private static func rasterImage(_ data: Data) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: 2048,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    private static func vectorImage(_ data: Data) -> CGImage? {
        #if os(macOS)
        guard let image = NSImage(data: data) else { return nil }
        var rect = CGRect(x: 0, y: 0, width: size, height: size)
        return image.cgImage(forProposedRect: &rect, context: nil, hints: nil)
        #else
        guard let image = UIImage(data: data) else { return nil }
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: size, height: size))
        return renderer.image { _ in
            image.draw(in: CGRect(x: 0, y: 0, width: size, height: size))
        }.cgImage
        #endif
    }

    /// The image in a `data:` URL, or nil.
    static func image(dataURL: String) -> Image? {
        guard let comma = dataURL.firstIndex(of: ","),
            let data = Data(base64Encoded: String(dataURL[dataURL.index(after: comma)...])),
            let image = ExtrasPlatformImage(data: data)
        else { return nil }
        #if os(iOS)
        return Image(uiImage: image)
        #else
        return Image(nsImage: image)
        #endif
    }
}

/// A `data:` picture as an icon tile, or a placeholder while it is missing.
struct ExtrasIconTile: View {
    let dataURL: String?
    var fallback: String = "?"
    var side: CGFloat = 44

    var body: some View {
        ZStack {
            if let dataURL, let image = ExtrasIconImage.image(dataURL: dataURL) {
                image.resizable().interpolation(.high).scaledToFit()
            } else {
                Text(verbatim: fallback)
                    .uwuFont(.headline, weight: .bold)
                    .foregroundStyle(Color.uwuPinkInk)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color.uwuPinkTint)
            }
        }
        .frame(width: side, height: side)
        .clipShape(RoundedRectangle(cornerRadius: side * 0.22, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: side * 0.22, style: .continuous)
                .strokeBorder(Color.uwuHairline))
        .accessibilityHidden(true)
    }
}

// MARK: Names of values

enum ExtrasValueName {
    /// What a value of an item is called (ItemExtras.tsx `valueLabel`).
    static func label(_ field: String, own: String?) -> String {
        if let own, !own.isEmpty { return own }
        switch field {
        case "name": return featureString("Name")
        case "username": return featureString("Benutzername")
        case "password": return featureString("Passwort")
        case "totp": return featureString("Einmal-Code (TOTP)")
        case "notes": return featureString("Notizen")
        case "card-name": return featureString("Karteninhaber")
        case "card-brand": return featureString("Marke")
        case "card-number": return featureString("Kartennummer")
        case "card-expiry": return featureString("Gültig bis")
        case "card-code": return featureString("Prüfnummer")
        case "ssh-private": return featureString("Privater Schlüssel")
        case "ssh-public": return featureString("Öffentlicher Schlüssel")
        case "ssh-fingerprint": return featureString("Fingerprint")
        default: break
        }
        let parts = field.split(separator: ":", maxSplits: 1).map(String.init)
        let rest = parts.count > 1 ? parts[1] : ""
        switch parts.first {
        case "uri": return featureString("Website \((Int(rest) ?? 0) + 1)")
        case "field": return featureString("Feld \((Int(rest) ?? 0) + 1)")
        case "identity": return identity(rest)
        default: return field
        }
    }

    private static func identity(_ name: String) -> String {
        switch name {
        case "title": featureString("Anrede")
        case "firstName": featureString("Vorname")
        case "middleName": featureString("Zweiter Vorname")
        case "lastName": featureString("Nachname")
        case "username": featureString("Benutzername")
        case "company": featureString("Firma")
        case "email": featureString("E-Mail")
        case "phone": featureString("Telefon")
        case "address1": featureString("Adresse")
        case "address2": featureString("Adresse 2")
        case "address3": featureString("Adresse 3")
        case "postalCode": featureString("Postleitzahl")
        case "city": featureString("Ort")
        case "state": featureString("Bundesland")
        case "country": featureString("Land")
        case "ssn": featureString("Sozialversicherungsnummer")
        case "passportNumber": featureString("Reisepassnummer")
        case "licenseNumber": featureString("Führerscheinnummer")
        default: name
        }
    }
}
