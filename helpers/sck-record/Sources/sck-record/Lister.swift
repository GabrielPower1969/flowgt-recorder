import AVFoundation
import CoreGraphics
import Foundation
import ScreenCaptureKit
import SckRecordCore

/// `--list`: everything the app can offer in its pickers, as one JSON object on stdout.
enum Lister {
    static func run() {
        guard Permissions.screenGranted() else {
            Console.event(.error(code: "permission", detail: "screen recording permission is not granted"))
            exit(HelperExit.permissionDenied.rawValue)
        }
        SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true) { content, error in
            if let error {
                Console.event(.error(code: "captureFailed", detail: "\(error)"))
                exit(HelperExit.captureFailed.rawValue)
            }
            guard let content else {
                Console.event(.error(code: "captureFailed", detail: "no shareable content"))
                exit(HelperExit.captureFailed.rawValue)
            }
            var displays: [[String: Any]] = []
            for (i, d) in content.displays.enumerated() {
                let scale = Display.scale(d.displayID)
                displays.append([
                    "id": d.displayID,
                    "index": i,
                    "x": d.frame.origin.x, "y": d.frame.origin.y,
                    "pointWidth": d.width, "pointHeight": d.height,
                    "width": Int((Double(d.width) * scale).rounded()),
                    "height": Int((Double(d.height) * scale).rounded()),
                    "scale": scale,
                    "main": d.displayID == CGMainDisplayID(),
                ])
            }
            let ourPid = getppid()
            let apps = content.applications
                .filter { $0.processID != ourPid && !$0.bundleIdentifier.isEmpty }
                .map { ["bundleId": $0.bundleIdentifier, "name": $0.applicationName, "pid": $0.processID] as [String: Any] }
            let windows = content.windows
                .filter { $0.isOnScreen && $0.frame.width >= 64 && $0.frame.height >= 64 && $0.windowLayer == 0 }
                .filter { $0.owningApplication?.processID != ourPid }
                .map { w -> [String: Any] in
                    [
                        "id": w.windowID,
                        "title": w.title ?? "",
                        "app": w.owningApplication?.applicationName ?? "",
                        "bundleId": w.owningApplication?.bundleIdentifier ?? "",
                        "pid": w.owningApplication?.processID ?? 0,
                        "x": w.frame.origin.x, "y": w.frame.origin.y,
                        "pointWidth": w.frame.width, "pointHeight": w.frame.height,
                    ]
                }
            let cameras = AVCaptureDevice.DiscoverySession(deviceTypes: Devices.cameraTypes, mediaType: .video, position: .unspecified).devices
                .map { ["name": $0.localizedName, "uniqueId": $0.uniqueID, "default": $0 == AVCaptureDevice.default(for: .video)] as [String: Any] }
            let mics = AVCaptureDevice.DiscoverySession(deviceTypes: Devices.micTypes, mediaType: .audio, position: .unspecified).devices
                .map { ["name": $0.localizedName, "uniqueId": $0.uniqueID, "default": $0 == AVCaptureDevice.default(for: .audio)] as [String: Any] }
            let object: [String: Any] = ["displays": displays, "apps": apps, "windows": windows, "cameras": cameras, "microphones": mics]
            let data = try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
            Console.out(String(decoding: data, as: UTF8.self))
            exit(HelperExit.ok.rawValue)
        }
    }
}

enum Display {
    /// Pixels per point of a display (2 on Retina).
    static func scale(_ id: CGDirectDisplayID) -> Double {
        let points = CGDisplayPixelsWide(id)
        guard points > 0, let mode = CGDisplayCopyDisplayMode(id) else { return 1 }
        return Double(mode.pixelWidth) / Double(points)
    }
}

enum Devices {
    static var cameraTypes: [AVCaptureDevice.DeviceType] {
        if #available(macOS 14.0, *) {
            return [.builtInWideAngleCamera, .external, .continuityCamera, .deskViewCamera]
        }
        return [.builtInWideAngleCamera, .externalUnknown]
    }

    static var micTypes: [AVCaptureDevice.DeviceType] {
        if #available(macOS 14.0, *) {
            return [.microphone, .external]
        }
        return [.builtInMicrophone, .externalUnknown]
    }

    static func find(_ type: AVMediaType, name: String?) -> AVCaptureDevice? {
        let types = type == .video ? cameraTypes : micTypes
        if let name {
            let all = AVCaptureDevice.DiscoverySession(deviceTypes: types, mediaType: type, position: .unspecified).devices
            if let d = all.first(where: { $0.localizedName == name }) { return d }
            Console.err("warn=device-not-found name=\(name) using-default")
        }
        return AVCaptureDevice.default(for: type)
    }
}
