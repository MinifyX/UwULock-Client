// swift-tools-version:5.3

import PackageDescription

let package = Package(
  name: "tauri-plugin-uwulock-mobile",
  platforms: [
    .iOS(.v14)
  ],
  products: [
    .library(
      name: "tauri-plugin-uwulock-mobile",
      type: .static,
      targets: ["tauri-plugin-uwulock-mobile"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-uwulock-mobile",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
