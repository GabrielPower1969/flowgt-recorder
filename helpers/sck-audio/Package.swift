// swift-tools-version: 5.9
// sck-audio: ScreenCaptureKit system-audio helper for FlowGT Recorder (macOS 13+).
// The pure logic lives in SckAudioCore so `swift test` never touches ScreenCaptureKit.
import PackageDescription

let package = Package(
    name: "sck-audio",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "sck-audio", targets: ["sck-audio"]),
        .library(name: "SckAudioCore", targets: ["SckAudioCore"]),
    ],
    targets: [
        .target(
            name: "SckAudioCore",
            path: "Sources/SckAudioCore"
        ),
        .executableTarget(
            name: "sck-audio",
            dependencies: ["SckAudioCore"],
            path: "Sources/sck-audio",
            linkerSettings: [
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("AVFoundation"),
            ]
        ),
        .testTarget(
            name: "SckAudioCoreTests",
            dependencies: ["SckAudioCore"],
            path: "Tests/SckAudioCoreTests"
        ),
    ],
    swiftLanguageVersions: [.v5]
)
