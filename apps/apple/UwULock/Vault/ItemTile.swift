import SwiftUI
import UwUDesign

/// An item's icon (components/ItemTile.tsx): its own or the site's icon, else the first letter of
/// a login, else the kind's icon, on a colour picked from the name.
struct ItemTile: View {
    let item: ItemSummary
    var size: CGFloat = 36
    @Environment(VaultModel.self) private var vault
    @ScaledMetric(relativeTo: .body) private var scale: CGFloat = 1

    private static let palette: [(Color, Color)] = [
        (.uwuAvatarPink, .uwuAvatarPinkInk), (.uwuAvatarViolet, .uwuAvatarVioletInk),
        (.uwuAvatarSky, .uwuAvatarSkyInk), (.uwuAvatarMint, .uwuAvatarMintInk),
        (.uwuAvatarAmber, .uwuAvatarAmberInk), (.uwuAvatarCoral, .uwuAvatarCoralInk),
    ]

    private var hue: Int {
        var hash: Int32 = 0
        for unit in (item.name.isEmpty ? item.id : item.name).utf16 {
            hash = hash &* 31 &+ Int32(unit)
        }
        return Int(hash.magnitude % 6)
    }

    private var letter: String {
        let source = item.name.isEmpty ? (item.host ?? "") : item.name
        return source.first.map { String($0).uppercased() } ?? "?"
    }

    var body: some View {
        let side = size * scale
        let (background, ink) = Self.palette[hue]
        ZStack {
            if let icon = vault.icons[item.id] {
                RoundedRectangle(cornerRadius: side * 0.28).fill(.uwuSurface)
                icon.resizable().scaledToFit().padding(side * 0.14)
            } else {
                RoundedRectangle(cornerRadius: side * 0.28).fill(background)
                if item.kind == .login {
                    Text(letter)
                        .font(.system(size: side * 0.46, weight: .bold, design: .rounded))
                        .foregroundStyle(ink)
                } else {
                    item.kind.icon.image.resizable().scaledToFit()
                        .frame(width: side * 0.5, height: side * 0.5)
                        .foregroundStyle(ink)
                }
            }
        }
        .frame(width: side, height: side)
        .overlay(RoundedRectangle(cornerRadius: side * 0.28).strokeBorder(.uwuHairline))
        .accessibilityHidden(true)
    }
}
