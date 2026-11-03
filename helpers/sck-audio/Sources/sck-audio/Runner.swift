import Foundation
import SckAudioCore

/// Owns one capture run: permission → start stream → lazily open output → stream until a
/// signal, the duration, a reader hang-up or a stream error → flush → report → exit.
/// All state changes happen on the main queue; the process lives in `dispatchMain()`.
final class Runner {
    private let options: CaptureOptions
    private let outputPath: String
    private let selfTest: Bool
    private let source: SCKCaptureSource
    private let sink: PcmSink
    private let pipeline: CapturePipeline
    private var startedAt: Date?
    private var firstAudioAt: Date?
    private var outputOpenedAt: Date?
    private var stopping = false
    private var exitCode: HelperExit = .ok
    private var signalSources: [DispatchSourceSignal] = []
    private var progressTimer: DispatchSourceTimer?
    private var safetyTimer: DispatchSourceTimer?
    private let cancelLock = NSLock()
    private var cancelOpen = false

    init(options: CaptureOptions, outputPath: String, selfTest: Bool) {
        self.options = options
        self.outputPath = outputPath
        self.selfTest = selfTest
        self.source = SCKCaptureSource(options: options)
        // Four seconds of audio may wait for a stalled ffmpeg before chunks are dropped.
        self.sink = PcmSink(maxPendingBytes: options.bytesPerSecond * 4)
        self.pipeline = CapturePipeline(options: options, sink: sink)
    }

    func run() {
        guard Environment.isSupportedOS else {
            fail(.unsupportedOS(Environment.osVersionString))
        }
        guard Environment.screenCaptureGranted() else {
            if selfTest { Console.out("self-test: permission-denied") }
            fail(.permissionDenied)
        }

        installSignalHandlers()
        source.onDiagnostic = { line in Console.err(line) }
        pipeline.onFirstChunk = { [weak self] format in
            DispatchQueue.main.async { self?.firstChunkArrived(format) }
        }
        pipeline.onDurationReached = { [weak self] in
            DispatchQueue.main.async { self?.stop(reason: "duration") }
        }
        sink.onError = { [weak self] error in
            DispatchQueue.main.async { self?.sinkFailed(error) }
        }

        source.start(
            onChunk: { [pipeline] chunk in pipeline.handle(chunk) },
            onStop: { [weak self] error in
                DispatchQueue.main.async { self?.streamStopped(error) }
            },
            completion: { [weak self] result in
                DispatchQueue.main.async { self?.captureStarted(result) }
            })
    }

    // MARK: lifecycle

    private func captureStarted(_ result: Result<Void, Error>) {
        if stopping { return }
        switch result {
        case .failure(let error):
            fail(Self.helperError(error))
        case .success:
            startedAt = Date()
            Console.err(Diag.started(options))
            openOutput()
        }
    }

    private func openOutput() {
        let path = outputPath
        let timeout = options.openTimeoutSeconds
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            do {
                let (writer, kind) = try OutputOpener.open(path: path, timeout: timeout) {
                    self?.isOpenCancelled ?? true
                }
                DispatchQueue.main.async { self?.outputOpened(writer, kind: kind) }
            } catch {
                DispatchQueue.main.async { self?.outputFailed(error) }
            }
        }
    }

    private var isOpenCancelled: Bool {
        cancelLock.lock()
        defer { cancelLock.unlock() }
        return cancelOpen
    }

    private func outputOpened(_ writer: FileDescriptorWriter, kind: OutputKind) {
        if stopping {
            try? writer.flushAndClose()
            return
        }
        outputOpenedAt = Date()
        sink.attach(writer)
        pipeline.markReady()
        Console.err(Diag.line([
            ("writing", ""),
            ("path", outputPath),
            ("kind", kind.rawValue),
            ("discarded-frames", "\(pipeline.framesDiscarded)"),
        ]))
        startTimers()
    }

    private func outputFailed(_ error: Error) {
        if stopping { return }
        if case WriterError.cancelled = error { return }
        exitCode = .outputFailed
        Console.err(Diag.error(.outputFailed(Self.describe(error))))
        stop(reason: "output-failed")
    }

    private func firstChunkArrived(_ format: SourceFormat) {
        firstAudioAt = Date()
        let latency = startedAt.map { Date().timeIntervalSince($0) * 1000 } ?? 0
        Console.err(Diag.line([
            ("first-audio", ""),
            ("after-ms", "\(Int(latency))"),
            ("format", format.description),
        ]))
    }

    private func sinkFailed(_ error: Error) {
        if stopping { return }
        if case WriterError.readerClosed = error {
            Console.err(Diag.line([("reader-closed", "")]))
            stop(reason: "reader-closed")
            return
        }
        exitCode = .outputFailed
        Console.err(Diag.error(.outputFailed(Self.describe(error))))
        stop(reason: "write-failed")
    }

    private func streamStopped(_ error: Error) {
        if stopping { return }
        let helper = Self.helperError(error)
        exitCode = helper.exit
        Console.err(Diag.error(helper))
        stop(reason: "stream-stopped")
    }

    private func startTimers() {
        let progress = DispatchSource.makeTimerSource(queue: .main)
        let interval = options.progressIntervalSeconds
        progress.schedule(deadline: .now() + interval, repeating: interval)
        progress.setEventHandler { [weak self] in
            guard let self, let started = self.startedAt else { return }
            Console.err(self.pipeline.progressLine(wall: Date().timeIntervalSince(started)))
        }
        progress.resume()
        progressTimer = progress

        // If the stream starves (no buffers ever arrive) the frame-based duration never fires;
        // this wall-clock guard stops the run anyway so --self-test cannot hang.
        if let duration = options.durationSeconds {
            let guardTimer = DispatchSource.makeTimerSource(queue: .main)
            guardTimer.schedule(deadline: .now() + duration + max(3, duration * 0.5))
            guardTimer.setEventHandler { [weak self] in self?.stop(reason: "duration-wallclock") }
            guardTimer.resume()
            safetyTimer = guardTimer
        }
    }

    private func installSignalHandlers() {
        // SIGPIPE: ffmpeg closing the FIFO must surface as EPIPE on write, not kill us.
        signal(SIGPIPE, SIG_IGN)
        for sig in [SIGINT, SIGTERM, SIGHUP] {
            signal(sig, SIG_IGN)
            let src = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            src.setEventHandler { [weak self] in self?.stop(reason: "signal-\(sig)") }
            src.resume()
            signalSources.append(src)
        }
    }

    private func stop(reason: String) {
        guard !stopping else { return }
        stopping = true
        cancelLock.lock()
        cancelOpen = true
        cancelLock.unlock()
        progressTimer?.cancel()
        safetyTimer?.cancel()
        Console.err(Diag.line([("stopping", ""), ("reason", reason)]))
        source.stop { [weak self] in
            DispatchQueue.main.async { self?.finishSink() }
        }
    }

    private func finishSink() {
        sink.finish { [weak self] result in
            DispatchQueue.main.async {
                if case .failure(let error) = result, self?.exitCode == .ok {
                    self?.exitCode = .outputFailed
                    Console.err(Diag.error(.outputFailed(Self.describe(error))))
                }
                self?.finalReport()
            }
        }
    }

    private func finalReport() -> Never {
        let wall = startedAt.map { Date().timeIntervalSince($0) } ?? 0
        let bytes = sink.bytesWritten
        Console.err(Diag.line([
            ("stopped", ""),
            ("bytes", "\(bytes)"),
            ("seconds", Diag.format1(pipeline.secondsWritten)),
            ("wall", Diag.format1(wall)),
            ("chunks", "\(pipeline.chunks)"),
            ("dropped", "\(sink.droppedBytes)"),
            ("discarded-frames", "\(pipeline.framesDiscarded)"),
            ("peak", Diag.format3(Double(pipeline.peak))),
        ]))

        if selfTest {
            let duration = options.durationSeconds ?? CaptureOptions.defaultSelfTestSeconds
            let expected = SelfTest.expectedBytes(options: options, seconds: duration)
            let onDisk = (try? FileManager.default.attributesOfItem(atPath: outputPath)[.size] as? NSNumber)?.intValue
            if let onDisk, onDisk != bytes {
                Console.err(Diag.line([("warn", "size-mismatch"), ("file", "\(onDisk)"), ("counted", "\(bytes)")]))
            }
            try? FileManager.default.removeItem(atPath: outputPath)
            Console.out(SelfTest.line(actual: bytes, expected: expected, peak: pipeline.peak))
            if exitCode == .ok, !SelfTest.isOK(actual: bytes, expected: expected) {
                exitCode = .selfTestShort
            }
        }
        exit(exitCode.rawValue)
    }

    private func fail(_ error: HelperError) -> Never {
        Console.err(Diag.error(error))
        exit(error.exit.rawValue)
    }

    static func helperError(_ error: Error) -> HelperError {
        if let helper = error as? HelperError { return helper }
        return .streamFailed(describe(error))
    }

    static func describe(_ error: Error) -> String {
        if let writerError = error as? WriterError {
            switch writerError {
            case .readerClosed: return "reader closed the pipe"
            case .io(let e, let m): return "write failed errno=\(e) \(m)"
            case .openFailed(let p, let e, let m): return "cannot open \(p): errno=\(e) \(m)"
            case .openTimeout(let p): return "no reader opened \(p) in time"
            case .cancelled: return "cancelled"
            }
        }
        return "\(error)"
    }
}
