import AVFoundation
import CoreGraphics
import Foundation

enum Permissions {
    static var isSupportedOS: Bool {
        if #available(macOS 13.0, *) { return true }
        return false
    }

    static var osVersion: String {
        let v = ProcessInfo.processInfo.operatingSystemVersion
        return "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)"
    }

    static func screenGranted() -> Bool {
        CGPreflightScreenCaptureAccess()
    }

    /// Camera / microphone: ask once if never asked (the prompt is attributed to the parent app),
    /// wait for the answer, and report whether capture is allowed.
    static func mediaGranted(_ type: AVMediaType) -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: type) {
        case .authorized:
            return true
        case .notDetermined:
            let sem = DispatchSemaphore(value: 0)
            var ok = false
            AVCaptureDevice.requestAccess(for: type) { granted in
                ok = granted
                sem.signal()
            }
            _ = sem.wait(timeout: .now() + 120)
            return ok
        default:
            return false
        }
    }
}
