// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "SAFDashboard",
    platforms: [
        .macOS(.v14)
    ],
    targets: [
        .executableTarget(
            name: "SAFDashboard",
            path: "Sources/SAFDashboard",
            swiftSettings: [
                .swiftLanguageMode(.v5)
            ]
        )
    ]
)
