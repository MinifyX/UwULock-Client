// A QR code drawn by CoreImage (QrCode.tsx): dark modules on white with a quiet zone, scaled
// without smoothing so every module stays crisp, on a white card that cameras read in dark mode.
import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI
import UwUDesign

struct WifiQRCode: View {
    let text: String
    let label: Text

    private var image: CGImage? { Self.render(text) }

    var body: some View {
        Group {
            if let image {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .interpolation(.none)
                    .antialiased(false)
                    .aspectRatio(1, contentMode: .fit)
            } else {
                Color.white
            }
        }
        .padding(UwUSpacing.md)
        .background(Color.white, in: RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: UwURadius.card, style: .continuous)
                .strokeBorder(Color.uwuHairline))
        .frame(maxWidth: 280)
        .accessibilityElement()
        .accessibilityAddTraits(.isImage)
        .accessibilityLabel(label)
    }

    /// The code at one pixel per module (with CoreImage's quiet zone), error correction M.
    static func render(_ text: String) -> CGImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        return CIContext(options: [.useSoftwareRenderer: false]).createCGImage(output, from: output.extent)
    }
}
