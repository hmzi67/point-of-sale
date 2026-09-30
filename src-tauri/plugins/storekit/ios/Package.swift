// swift-tools-version:5.3
import PackageDescription

let package = Package(
  name: "tauri-plugin-storekit",
  platforms: [
    .macOS(.v10_13),
    // StoreKit 2 needs iOS 15; the code guards with #available so the package
    // still builds at Tauri's default deployment target.
    .iOS(.v13),
  ],
  products: [
    .library(name: "tauri-plugin-storekit", type: .static, targets: ["tauri-plugin-storekit"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-storekit",
      dependencies: [.byName(name: "Tauri")],
      path: "Sources")
  ]
)
