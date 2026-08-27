// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "DoubaoVoiceBridge",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "doubao-bridge-mac", targets: ["DoubaoBridgeMac"])
    ],
    targets: [
        .executableTarget(
            name: "DoubaoBridgeMac",
            resources: [
                .copy("Resources/Brand")
            ],
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("ApplicationServices"),
                .linkedFramework("Carbon"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("Network"),
                .linkedFramework("ServiceManagement"),
                .linkedFramework("SwiftUI")
            ]
        ),
        .testTarget(
            name: "DoubaoBridgeMacTests",
            dependencies: ["DoubaoBridgeMac"]
        )
    ]
)
