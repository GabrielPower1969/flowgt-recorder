import CoreMedia
import Foundation
import ScreenCaptureKit
import SckRecordCore

/// ScreenCaptureKit: video frames of a display, window or app, already scaled to the output size,
/// plus the system audio when asked. Frames arrive on `frameQueue`, audio on its own queue.
final class ScreenSource: NSObject, SCStreamOutput, SCStreamDelegate {
    private let options: RecordOptions
    private let frameQueue: DispatchQueue
    private let audioQueue = DispatchQueue(label: "sck-record.system-audio")
    private var stream: SCStream?
    private let lock = NSLock()
    private var stoppedByUs = false
    private var makeFilter: ((SCShareableContent) -> SCContentFilter?)?
    private var refreshTimer: DispatchSourceTimer?
    /// A recorded window has left the on-screen list (minimised, or on another Space) and
    /// `warn.windowHidden` was sent; cleared with `warn.windowBack`.
    private var windowHidden = false

    /// A new, complete frame: the pixel buffer and its capture time (host clock).
    var onFrame: ((CVPixelBuffer, CMTime) -> Void)?
    var onAudio: ((CMSampleBuffer) -> Void)?
    var onStop: ((Error) -> Void)?

    init(options: RecordOptions, frameQueue: DispatchQueue) {
        self.options = options
        self.frameQueue = frameQueue
    }

    func start(completion: @escaping (Result<Void, Error>) -> Void) {
        // SCStream asserts (CGS_REQUIRE_INIT) unless the window-server connection exists; the
        // display branch gets it from `Display.scale`, a window or app would not.
        _ = CGMainDisplayID()
        SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true) { [weak self] content, error in
            guard let self else { return }
            if let error {
                completion(.failure(Self.map(error)))
                return
            }
            guard let content else {
                completion(.failure(HelperError.captureFailed("no shareable content")))
                return
            }
            var scale = 1.0
            let excluded = content.applications.filter { $0.processID == self.options.excludePid }
            switch self.options.source {
            case .display(let id):
                guard let display = content.displays.first(where: { $0.displayID == id }) else {
                    completion(.failure(HelperError.sourceNotFound("display \(id) is not connected")))
                    return
                }
                scale = Display.scale(display.displayID)
            case .window(let id):
                guard content.windows.contains(where: { $0.windowID == id }) else {
                    completion(.failure(HelperError.sourceNotFound("window \(id) is not on screen")))
                    return
                }
            case .app(let bundle):
                guard content.applications.contains(where: { $0.bundleIdentifier == bundle }) else {
                    completion(.failure(HelperError.sourceNotFound("no running app \(bundle)")))
                    return
                }
            }
            self.makeFilter = { [options = self.options] content in Self.filter(for: options, in: content) }
            guard let filter = self.makeFilter?(content) else {
                completion(.failure(HelperError.sourceNotFound("the source disappeared")))
                return
            }

            let config = SCStreamConfiguration()
            config.width = self.options.width
            config.height = self.options.height
            config.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(self.options.fps))
            config.pixelFormat = kCVPixelFormatType_32BGRA
            config.showsCursor = self.options.cursor
            config.queueDepth = 5
            config.colorSpaceName = CGColorSpace.sRGB
            if case .window = self.options.source {
                config.scalesToFit = true
            }
            if let crop = self.options.crop {
                let p = PipGeometry.points(crop, scale: scale)
                config.sourceRect = CGRect(x: p.x, y: p.y, width: p.w, height: p.h)
            }
            if self.options.systemAudio {
                config.capturesAudio = true
                config.excludesCurrentProcessAudio = true
                config.sampleRate = 48_000
                config.channelCount = 2
            }

            let stream = SCStream(filter: filter, configuration: config, delegate: self)
            do {
                try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: self.frameQueue)
                if self.options.systemAudio {
                    try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: self.audioQueue)
                }
            } catch {
                completion(.failure(Self.map(error)))
                return
            }
            self.lock.lock()
            self.stream = stream
            self.lock.unlock()
            Console.err("screen-source=\(self.options.source) size=\(self.options.width)x\(self.options.height) scale=\(scale) excluded=\(excluded.count)")
            stream.startCapture { error in
                if let error {
                    completion(.failure(Self.map(error)))
                } else {
                    completion(.success(()))
                }
            }
        }
    }

    /// The filter from a content snapshot. Excluded apps are matched by pid; a window that did
    /// not exist when the snapshot was taken is not excluded, hence `startRefreshing`.
    static func filter(for options: RecordOptions, in content: SCShareableContent) -> SCContentFilter? {
        let excluded = content.applications.filter { $0.processID == options.excludePid }
        switch options.source {
        case .display(let id):
            guard let display = content.displays.first(where: { $0.displayID == id }) else { return nil }
            return SCContentFilter(display: display, excludingApplications: excluded, exceptingWindows: [])
        case .window(let id):
            guard let window = content.windows.first(where: { $0.windowID == id }) else { return nil }
            return SCContentFilter(desktopIndependentWindow: window)
        case .app(let bundle):
            let apps = content.applications.filter { $0.bundleIdentifier == bundle }
            guard !apps.isEmpty else { return nil }
            // The display holding the app's first window, else the main one.
            let firstWindow = content.windows.first { w in apps.contains { $0.processID == w.owningApplication?.processID } && w.isOnScreen }
            let display = content.displays.first { d in firstWindow.map { d.frame.contains(CGPoint(x: $0.frame.midX, y: $0.frame.midY)) } ?? false }
                ?? content.displays.first { $0.displayID == CGMainDisplayID() } ?? content.displays[0]
            return SCContentFilter(display: display, including: apps, exceptingWindows: [])
        }
    }

    /// Rebuild the filter now and then so windows the excluded app opened after the start (the
    /// app's own bar) drop out of the picture too, and notice a recorded window that is no
    /// longer on screen. Cheap: one content query per interval.
    func startRefreshing(every seconds: Double, on queue: DispatchQueue) {
        guard options.excludePid != nil || Self.isWindow(options.source) else { return }
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 1, repeating: seconds)
        timer.setEventHandler { [weak self] in self?.refreshFilter() }
        timer.resume()
        refreshTimer = timer
    }

    private func refreshFilter() {
        lock.lock()
        let stream = self.stream
        let stopped = stoppedByUs
        lock.unlock()
        guard let stream, !stopped, let makeFilter else { return }
        let source = options.source
        SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true) { [weak self] content, _ in
            guard let self, let content else { return }
            if case .window(let id) = source {
                let onScreen = content.windows.contains { $0.windowID == id && $0.isOnScreen }
                self.lock.lock()
                let changed = onScreen == self.windowHidden
                self.windowHidden = !onScreen
                self.lock.unlock()
                if changed {
                    Console.event(onScreen
                        ? .warning(code: "warn.windowBack", detail: "window \(id) is on screen again")
                        : .warning(code: "warn.windowHidden", detail: "window \(id) is not on screen (minimised or on another Space); frames stop until it is back"))
                }
                // Keep the filter: a minimised window comes back by itself; rebuilding would fail.
                return
            }
            guard let filter = makeFilter(content) else { return }
            stream.updateContentFilter(filter) { error in
                if let error { Console.err("filter-refresh-failed \(error.localizedDescription)") }
            }
        }
    }

    private static func isWindow(_ source: CaptureSource) -> Bool {
        if case .window = source { return true }
        return false
    }

    func stop(completion: @escaping () -> Void) {
        refreshTimer?.cancel()
        lock.lock()
        stoppedByUs = true
        let stream = self.stream
        lock.unlock()
        guard let stream else {
            completion()
            return
        }
        stream.stopCapture { _ in completion() }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        lock.lock()
        let expected = stoppedByUs
        lock.unlock()
        if !expected { onStop?(Self.map(error)) }
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard CMSampleBufferIsValid(sampleBuffer), CMSampleBufferDataIsReady(sampleBuffer) else { return }
        switch type {
        case .audio:
            onAudio?(sampleBuffer)
        case .screen:
            // Only complete frames carry new pixels; idle/blank ones are skipped.
            guard let attachments = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
                  let status = attachments.first?[.status] as? Int, status == SCFrameStatus.complete.rawValue,
                  let pixels = CMSampleBufferGetImageBuffer(sampleBuffer)
            else { return }
            onFrame?(pixels, CMSampleBufferGetPresentationTimeStamp(sampleBuffer))
        default:
            break
        }
    }

    static func map(_ error: Error) -> Error {
        if let helper = error as? HelperError { return helper }
        let ns = error as NSError
        if ns.domain == SCStreamErrorDomain {
            switch ns.code {
            case SCStreamError.Code.userDeclined.rawValue:
                return HelperError.permissionDenied("screen recording was declined")
            case SCStreamError.Code.noDisplayList.rawValue, SCStreamError.Code.noCaptureSource.rawValue:
                return HelperError.sourceNotFound("\(ns.localizedDescription)")
            default:
                break
            }
        }
        return HelperError.captureFailed("\(ns.domain) \(ns.code): \(ns.localizedDescription)")
    }
}
