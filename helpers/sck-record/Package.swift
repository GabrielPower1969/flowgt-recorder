// swift-tools-version: 5.9
// sck-record: native screen recorder for FlowGT Recorder (macOS 13+).
// ScreenCaptureKit (screen + system audio), AVCaptureSession (camera, mic), AVAssetWriter (HEVC/AAC MP4).
// The pure logic lives in SckRecordCore so `swift test` never touches the capture frameworks.
import PackageDescription

let package = Package(
    name: "sck-record",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "sck-record", targets: ["sck-record"]),
        .library(name: "SckRecordCore", targets: ["SckRecordCore"]),
    ],
    targets: [
        .target(name: "SckRecordCore", path: "Sources/SckRecordCore"),
        .executableTarget(
            name: "sck-record",
            dependencies: ["SckRecordCore"],
            path: "Sources/sck-record",
            linkerSettings: [
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("CoreImage"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("Metal"),
            ]
        ),
        .testTarget(
            name: "SckRecordCoreTests",
            dependencies: ["SckRecordCore"],
            path: "Tests/SckRecordCoreTests"
        ),
    ],
    swiftLanguageVersions: [.v5]
)
