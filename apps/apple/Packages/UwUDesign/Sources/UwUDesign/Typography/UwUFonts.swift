import CoreText
import Foundation

/// Registers the bundled fonts (UwU Sans, the picker's fonts, JetBrains Mono) for this process.
/// CoreText reads the WOFF2 files as they come from the design package. Call once at launch;
/// further calls do nothing.
public enum UwUFonts {
    public static func register() { _ = registered }

    private static let registered: Void = {
        guard let dir = Bundle.module.url(forResource: "Fonts", withExtension: nil),
              let files = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
        else {
            assertionFailure("UwUDesign: the bundled fonts are missing")
            return
        }
        let fonts = files.filter { $0.pathExtension == "woff2" }
        CTFontManagerRegisterFontURLs(fonts as CFArray, .process, true) { errors, _ in
            for error in (errors as? [CFError]) ?? [] {
                // "Already registered" is fine (previews, tests); anything else is a packaging bug.
                if CFErrorGetCode(error) != CTFontManagerError.alreadyRegistered.rawValue {
                    assertionFailure("UwUDesign: font registration failed: \(error)")
                }
            }
            return true
        }
    }()
}
