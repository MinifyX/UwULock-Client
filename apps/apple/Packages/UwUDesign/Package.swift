// swift-tools-version: 6.2
// UwUDesign: the UwU Suite's design (@uwusuite/design) for native Apple apps — colour tokens,
// UwU Sans, the icon vocabulary, Nyu, motion and Liquid Glass helpers. Resources are generated
// by tools/sync-design.mjs; the Swift around them is hand-written.
import PackageDescription

let package = Package(
    name: "UwUDesign",
    defaultLocalization: "de",
    platforms: [.iOS("27.0"), .macOS("27.0")],
    products: [.library(name: "UwUDesign", targets: ["UwUDesign"])],
    targets: [
        .target(
            name: "UwUDesign",
            resources: [
                .process("Resources/Colors.xcassets"),
                .process("Resources/Icons.xcassets"),
                .process("Resources/Nyu.xcassets"),
                .copy("Resources/Fonts"),
            ]
        ),
    ]
)
