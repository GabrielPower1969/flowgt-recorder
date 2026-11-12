import CoreMedia
import CoreVideo
import Foundation
import ScreenCaptureKit
import SckRecordCore

/// `--scroll`: the pixels of one rectangle of a display, one record per frame that changed, on
/// stdout. The app stitches them into a scrolling screenshot. ScreenCaptureKit only delivers a
/// complete frame when something in the rectangle changed, so an idle page costs nothing.
/// Record: "FGTF", width, height (little-endian UInt32), then width*height*4 bytes of BGRA rows.
final class ScrollFrames: NSObject, SCStreamOutput, SCStreamDelegate {
    private static var current: ScrollFrames?
    private let options: ScrollOptions
    private let queue = DispatchQueue(label: "sck-record.scroll")
    private var stream: SCStream?
    private var stopping = false
    private let out = FileHandle.standardOutput

    init(options: ScrollOptions) {
        self.options = options
    }

    static func run(_ o: ScrollOptions) {
        guard Permissions.screenGranted() else {
            Console.err("error=permission screen recording permission is not granted")
            exit(HelperExit.permissionDenied.rawValue)
        }
        let frames = ScrollFrames(options: o)
        current = frames
        frames.start()
        // "stop" on stdin ends it; so does stdin closing (the app is gone).
        Thread.detachNewThread {
            while let line = readLine(strippingNewline: true) {
                if line == "stop" { break }
            }
            frames.stop(code: HelperExit.ok.rawValue)
        }
    }

    private func start() {
        _ = CGMainDisplayID()
        SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true) { [self] content, error in
            guard let content, error == nil else {
                Console.err("error=captureFailed \(error.map { "\($0)" } ?? "no shareable content")")
                exit(HelperExit.captureFailed.rawValue)
            }
            guard let display = content.displays.first(where: { $0.displayID == options.display }) else {
                Console.err("error=sourceNotFound display \(options.display) is not connected")
                exit(HelperExit.sourceNotFound.rawValue)
            }
            let excluded = content.applications.filter { $0.processID == options.excludePid }
            let filter = SCContentFilter(display: display, excludingApplications: excluded, exceptingWindows: [])
            let scale = Display.scale(display.displayID)
            let p = PipGeometry.points(options.crop, scale: scale)
            let config = SCStreamConfiguration()
            config.sourceRect = CGRect(x: p.x, y: p.y, width: p.w, height: p.h)
            config.width = options.crop.w
            config.height = options.crop.h
            config.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(options.fps))
            config.pixelFormat = kCVPixelFormatType_32BGRA
            config.showsCursor = false
            config.queueDepth = 3
            config.colorSpaceName = CGColorSpace.sRGB
            let stream = SCStream(filter: filter, configuration: config, delegate: self)
            do {
                try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
            } catch {
                Console.err("error=captureFailed \(error)")
                exit(HelperExit.captureFailed.rawValue)
            }
            self.stream = stream
            Console.err("scroll display=\(options.display) size=\(options.crop.w)x\(options.crop.h) scale=\(scale) excluded=\(excluded.count)")
            stream.startCapture { error in
                if let error {
                    Console.err("error=captureFailed \(ScreenSource.map(error))")
                    exit(HelperExit.captureFailed.rawValue)
                }
            }
        }
    }

    private func stop(code: Int32) {
        queue.async { [self] in
            guard !stopping else { return }
            stopping = true
            guard let stream else { exit(code) }
            stream.stopCapture { _ in exit(code) }
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        Console.err("error=captureFailed \(ScreenSource.map(error))")
        exit(HelperExit.captureFailed.rawValue)
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, !stopping, CMSampleBufferIsValid(sampleBuffer),
              let attachments = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let status = attachments.first?[.status] as? Int, status == SCFrameStatus.complete.rawValue,
              let pixels = CMSampleBufferGetImageBuffer(sampleBuffer)
        else { return }
        CVPixelBufferLockBaseAddress(pixels, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixels, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(pixels) else { return }
        let width = CVPixelBufferGetWidth(pixels)
        let height = CVPixelBufferGetHeight(pixels)
        let stride = CVPixelBufferGetBytesPerRow(pixels)
        var record = Data(capacity: 12 + width * height * 4)
        record.append(contentsOf: Array("FGTF".utf8))
        for value in [UInt32(width), UInt32(height)] {
            withUnsafeBytes(of: value.littleEndian) { record.append(contentsOf: $0) }
        }
        for row in 0..<height {
            record.append(base.advanced(by: row * stride).assumingMemoryBound(to: UInt8.self), count: width * 4)
        }
        do {
            try out.write(contentsOf: record)
        } catch {
            // The reader is gone.
            exit(HelperExit.ok.rawValue)
        }
    }
}
