import AppKit
import CoreGraphics
import Foundation
import ImageIO
import ScreenCaptureKit
import SckRecordCore

/// `--snapshot`: one JPEG of a window or display for the picker, no wider than `maxWidth`.
/// Needs macOS 14 (SCScreenshotManager); older systems get `unsupportedOs`.
enum Snapshot {
    static func run(_ o: SnapshotOptions) {
        guard Permissions.screenGranted() else {
            Console.event(.error(code: "permission", detail: "screen recording permission is not granted"))
            exit(HelperExit.permissionDenied.rawValue)
        }
        guard #available(macOS 14.0, *) else {
            Console.event(.error(code: "unsupportedOs", detail: "snapshots need macOS 14"))
            exit(HelperExit.unsupportedOS.rawValue)
        }
        // SCScreenshotManager asserts unless the window-server connection exists already.
        _ = CGMainDisplayID()
        // The capture daemon can stall (seen with concurrent clients); never hang the picker.
        DispatchQueue.global().asyncAfter(deadline: .now() + 4) {
            Console.event(.error(code: "captureFailed", detail: "snapshot timed out"))
            exit(HelperExit.captureFailed.rawValue)
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
            let filter: SCContentFilter
            let points: CGSize
            switch o.source {
            case .display(let id):
                guard let d = content.displays.first(where: { $0.displayID == id }) else {
                    Console.event(.error(code: "sourceNotFound", detail: "display \(id) is not connected"))
                    exit(HelperExit.sourceNotFound.rawValue)
                }
                filter = SCContentFilter(display: d, excludingApplications: [], exceptingWindows: [])
                points = CGSize(width: d.width, height: d.height)
            case .window(let id):
                guard let w = content.windows.first(where: { $0.windowID == id }) else {
                    Console.event(.error(code: "sourceNotFound", detail: "window \(id) is not on screen"))
                    exit(HelperExit.sourceNotFound.rawValue)
                }
                filter = SCContentFilter(desktopIndependentWindow: w)
                points = w.frame.size
            case .app:
                exit(HelperExit.usage.rawValue)
            }
            let size = Images.fit(points, maxWidth: o.maxWidth)
            let config = SCStreamConfiguration()
            config.width = size.width
            config.height = size.height
            config.showsCursor = false
            config.scalesToFit = true
            config.pixelFormat = kCVPixelFormatType_32BGRA
            SCScreenshotManager.captureImage(contentFilter: filter, configuration: config) { image, error in
                guard let image, error == nil else {
                    Console.event(.error(code: "captureFailed", detail: "\(error.map { "\($0)" } ?? "no image")"))
                    exit(HelperExit.captureFailed.rawValue)
                }
                guard Images.write(image, jpegTo: URL(fileURLWithPath: o.output), quality: 0.7) else {
                    Console.event(.error(code: "outputFailed", detail: "cannot write \(o.output)"))
                    exit(HelperExit.outputFailed.rawValue)
                }
                exit(HelperExit.ok.rawValue)
            }
        }
    }
}

enum Images {
    /// Even output size no wider than `maxWidth`, same aspect as `points`.
    static func fit(_ points: CGSize, maxWidth: Int) -> (width: Int, height: Int) {
        guard points.width > 0, points.height > 0 else { return (maxWidth & ~1, maxWidth & ~1) }
        let w = min(Double(maxWidth), points.width * 2)
        let h = points.height * w / points.width
        return (max(2, Int(w) & ~1), max(2, Int(h.rounded()) & ~1))
    }

    static func write(_ image: CGImage, jpegTo url: URL, quality: Double) -> Bool {
        guard let dest = CGImageDestinationCreateWithURL(url as CFURL, "public.jpeg" as CFString, 1, nil) else { return false }
        CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
        return CGImageDestinationFinalize(dest)
    }

    /// The app's icon as a `side`×`side` PNG, base64, for the picker tiles.
    static func iconPNG(pid: pid_t, side: Int = 32) -> String? {
        guard let icon = NSRunningApplication(processIdentifier: pid)?.icon else { return nil }
        var rect = CGRect(x: 0, y: 0, width: side, height: side)
        guard let cg = icon.cgImage(forProposedRect: &rect, context: nil, hints: nil) else { return nil }
        guard let ctx = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        ctx.interpolationQuality = .high
        ctx.draw(cg, in: CGRect(x: 0, y: 0, width: side, height: side))
        guard let small = ctx.makeImage() else { return nil }
        let data = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(data, "public.png" as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(dest, small, nil)
        guard CGImageDestinationFinalize(dest) else { return nil }
        return data.base64EncodedString()
    }
}
