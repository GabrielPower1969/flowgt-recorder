import AVFoundation
import CoreMedia
import Foundation
import SckRecordCore

/// One recording: permissions → sources → writer → frame clock → stop → report → exit.
///
/// Frames go in on a clock at the output rate: each tick takes the latest screen frame and the
/// latest camera frame, so the camera can never stall the screen and the audio never waits for
/// either (that was the ffmpeg defect, D26). Everything that touches frames runs on `frameQueue`.
final class Recorder {
    private let options: RecordOptions
    private let frameQueue = DispatchQueue(label: "sck-record.frames", qos: .userInteractive)
    private let screen: ScreenSource
    private var camera: CameraSource?
    private var mic: MicSource?
    private var writer: SegmentWriter?
    private var compositor: Compositor?

    private var latestScreen: CVPixelBuffer?
    private var screenDirty = false
    private var latestCamera: CVPixelBuffer?
    private var pacer = FramePacer()
    private var cameraWatch = CameraWatch()
    private var cameraOn = false
    private var startedAt: Double?
    private var reportedStart = false
    private var clock: DispatchSourceTimer?
    private var progressTimer: DispatchSourceTimer?
    private var durationTimer: DispatchSourceTimer?
    private var stopping = false
    private var signalSources: [DispatchSourceSignal] = []

    init(options: RecordOptions) {
        self.options = options
        self.screen = ScreenSource(options: options, frameQueue: frameQueue)
    }

    private static func now() -> CMTime {
        CMClockGetTime(CMClockGetHostTimeClock())
    }

    func run() {
        guard Permissions.isSupportedOS else { fail(.unsupportedOS(Permissions.osVersion)) }
        guard Permissions.screenGranted() else { fail(.permissionDenied("screen recording permission is not granted")) }
        if options.camera, !Permissions.mediaGranted(.video) {
            Console.event(.warning(code: "warn.cameraLost", detail: "camera permission is not granted"))
        } else {
            cameraOn = options.camera
        }
        var micOn = options.mic
        if options.mic, !Permissions.mediaGranted(.audio) {
            micOn = false
            Console.event(.warning(code: "warn.audioDeviceLost", detail: "microphone permission is not granted"))
        }

        installSignalHandlers()
        startStdinReader()

        do {
            writer = try SegmentWriter(options: options)
            if cameraOn, let pip = options.pip {
                compositor = try Compositor(width: options.width, height: options.height, pip: pip)
            }
        } catch {
            fail(Self.helperError(error))
        }

        screen.onFrame = { [weak self] pixels, _ in
            guard let self else { return }
            self.latestScreen = pixels
            self.screenDirty = true
        }
        screen.onAudio = { [weak self] buffer in
            guard let self, let writer = self.writer else { return }
            writer.queue.async { writer.appendSystem(buffer) }
        }
        screen.onStop = { [weak self] error in
            self?.frameQueue.async { self?.captureStopped(error) }
        }
        screen.start { [weak self] result in
            self?.frameQueue.async { self?.screenStarted(result, micOn: micOn) }
        }
    }

    // MARK: start

    private func screenStarted(_ result: Result<Void, Error>, micOn: Bool) {
        if stopping { return }
        if case .failure(let error) = result {
            fail(Self.helperError(error))
        }
        startedAt = Self.now().seconds
        if cameraOn {
            let cam = CameraSource(name: options.cameraDevice, frameQueue: frameQueue)
            cam.onFrame = { [weak self] pixels, time in
                self?.latestCamera = pixels
                self?.cameraWatch.frameArrived(at: time.seconds)
            }
            cam.onDisconnected = { [weak self] in
                self?.frameQueue.async { self?.latestCamera = nil }
            }
            do {
                try cam.start()
                camera = cam
            } catch {
                cameraOn = false
                Console.event(.warning(code: "warn.cameraLost", detail: "\(error.localizedDescription)"))
            }
        }
        if micOn {
            let m = MicSource(name: options.micDevice)
            m.onBuffer = { [weak self] buffer in
                guard let self, let writer = self.writer else { return }
                writer.queue.async { writer.appendMic(buffer) }
            }
            do {
                try m.start()
                mic = m
            } catch {
                Console.event(.warning(code: "warn.audioDeviceLost", detail: "\(error.localizedDescription)"))
            }
        }
        startClock()
        startTimers()
        screen.startRefreshing(every: 3, on: frameQueue)
    }

    private func startClock() {
        let timer = DispatchSource.makeTimerSource(queue: frameQueue)
        let interval = 1.0 / Double(options.fps)
        timer.schedule(deadline: .now() + interval, repeating: interval, leeway: .milliseconds(2))
        timer.setEventHandler { [weak self] in self?.tick() }
        timer.resume()
        clock = timer
    }

    /// One tick of the frame clock (frame queue).
    private func tick() {
        guard !stopping, let writer, let screenFrame = latestScreen else { return }
        let now = Self.now()
        if cameraOn, let started = startedAt, let change = cameraWatch.update(now: now.seconds, startedAt: started) {
            switch change {
            case .lost:
                latestCamera = nil
                Console.event(.warning(code: "warn.cameraLost", detail: "no camera frames for \(Int(cameraWatch.timeout)) s"))
            case .back:
                Console.err("camera-back")
            }
        }
        let cameraLive = cameraOn && cameraWatch.isLive && latestCamera != nil
        guard pacer.shouldEmit(now: now.seconds, screenDirty: screenDirty, cameraLive: cameraLive) else { return }
        let frame: CVPixelBuffer
        if cameraLive, let compositor, let composed = compositor.render(screen: screenFrame, camera: latestCamera) {
            frame = composed
        } else {
            frame = screenFrame
        }
        screenDirty = false
        writer.queue.async { [weak self] in
            if writer.appendVideo(frame, at: now) {
                self?.frameQueue.async { self?.reportStarted() }
            }
        }
    }

    private func reportStarted() {
        guard !reportedStart else { return }
        reportedStart = true
        Console.event(.started(width: options.width, height: options.height, audioTracks: options.audioTracks))
        Console.err("first-frame after-ms=\(Int((Self.now().seconds - (startedAt ?? 0)) * 1000))")
    }

    private func startTimers() {
        let progress = DispatchSource.makeTimerSource(queue: frameQueue)
        progress.schedule(deadline: .now() + options.progressInterval, repeating: options.progressInterval)
        progress.setEventHandler { [weak self] in
            guard let self, let writer = self.writer, let start = writer.sessionStart else { return }
            Console.event(.progress(seconds: Self.now().seconds - start.seconds, bytes: writer.bytesOnDisk, frames: writer.frames, dropped: writer.dropped))
        }
        progress.resume()
        progressTimer = progress

        if let duration = options.duration {
            let t = DispatchSource.makeTimerSource(queue: frameQueue)
            t.schedule(deadline: .now() + duration)
            t.setEventHandler { [weak self] in self?.stop(reason: "duration") }
            t.resume()
            durationTimer = t
        }
    }

    // MARK: control

    private func installSignalHandlers() {
        signal(SIGPIPE, SIG_IGN)
        for sig in [SIGINT, SIGTERM, SIGHUP] {
            signal(sig, SIG_IGN)
            let src = DispatchSource.makeSignalSource(signal: sig, queue: frameQueue)
            src.setEventHandler { [weak self] in self?.stop(reason: "signal-\(sig)") }
            src.resume()
            signalSources.append(src)
        }
    }

    /// "stop" on stdin ends the recording; so does stdin closing (the app is gone).
    private func startStdinReader() {
        Thread.detachNewThread { [weak self] in
            while let line = readLine(strippingNewline: true) {
                if Control.parse(line) == .stop {
                    self?.frameQueue.async { self?.stop(reason: "stdin-stop") }
                    return
                }
            }
            self?.frameQueue.async { self?.stop(reason: "stdin-closed") }
        }
    }

    private func captureStopped(_ error: Error) {
        if stopping { return }
        let helper = Self.helperError(error)
        Console.event(.error(code: helper.code, detail: helper.detail))
        stop(reason: "capture-stopped", exit: helper.exit)
    }

    private func stop(reason: String, exit code: HelperExit = .ok) {
        guard !stopping else { return }
        stopping = true
        clock?.cancel()
        progressTimer?.cancel()
        durationTimer?.cancel()
        Console.err("stopping reason=\(reason)")
        // If anything hangs, leave with what the fragments hold.
        let guardTimer = DispatchSource.makeTimerSource(queue: frameQueue)
        guardTimer.schedule(deadline: .now() + 10)
        guardTimer.setEventHandler {
            Console.err("stop-timeout")
            exit(HelperExit.outputFailed.rawValue)
        }
        guardTimer.resume()

        let end = Self.now()
        camera?.stop()
        mic?.stop()
        screen.stop { [weak self] in
            guard let self, let writer = self.writer else {
                exit(code.rawValue)
            }
            writer.queue.async {
                writer.finish(at: end) { error in
                    let seconds = writer.sessionStart.map { end.seconds - $0.seconds } ?? 0
                    if let error {
                        let helper = Self.helperError(error)
                        Console.event(.error(code: helper.code, detail: helper.detail))
                        exit(code == .ok ? HelperExit.outputFailed.rawValue : code.rawValue)
                    }
                    Console.event(.stopped(seconds: seconds, bytes: writer.bytesOnDisk, frames: writer.frames, dropped: writer.dropped))
                    exit(code.rawValue)
                }
            }
        }
    }

    private func fail(_ error: HelperError) -> Never {
        Console.event(.error(code: error.code, detail: error.detail))
        exit(error.exit.rawValue)
    }

    static func helperError(_ error: Error) -> HelperError {
        if let helper = error as? HelperError { return helper }
        let ns = error as NSError
        if ns.domain == AVFoundationErrorDomain || ns.domain == NSOSStatusErrorDomain {
            return .outputFailed("\(ns.domain) \(ns.code): \(ns.localizedDescription)")
        }
        return .captureFailed("\(ns.domain) \(ns.code): \(ns.localizedDescription)")
    }
}
