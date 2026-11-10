import Foundation

/// The boundary between ScreenCaptureKit and the testable core. The real implementation
/// (`SCKCaptureSource` in the executable) wraps SCStream; tests drive a fake.
public protocol AudioCaptureSource: AnyObject {
    /// Begins capture. `onChunk` is called on the source's serial queue for every buffer;
    /// `onStop` when the stream ends on its own (error from the system); `completion` once
    /// the stream is running or has failed to start.
    func start(
        onChunk: @escaping (AudioChunk) -> Void,
        onStop: @escaping (Error) -> Void,
        completion: @escaping (Result<Void, Error>) -> Void
    )
    func stop(completion: @escaping () -> Void)
}

/// Converts chunks, feeds the sink and keeps the counters. Thread-safe; `handle` runs on the
/// capture queue, everything else may be read from any thread.
public final class CapturePipeline {
    public let options: CaptureOptions
    public let sink: PcmSink
    private let converter: PcmConverter
    private let lock = NSLock()
    private var ready = false
    private var frames = 0
    private var discardedFrames = 0
    private var chunkCount = 0
    private var peakValue: Float = 0
    private var firstFormat: SourceFormat?
    private var durationFired = false

    /// Fired once, on the capture queue, when `durationSeconds` of audio have been converted.
    public var onDurationReached: (() -> Void)?
    /// Fired once with the format of the first chunk (before or after `markReady`).
    public var onFirstChunk: ((SourceFormat) -> Void)?

    public init(options: CaptureOptions, sink: PcmSink) {
        self.options = options
        self.sink = sink
        self.converter = PcmConverter(targetChannels: options.channels, targetFormat: options.format)
    }

    /// The output is open: from now on chunks are written instead of discarded.
    public func markReady() {
        lock.lock()
        ready = true
        lock.unlock()
    }

    public func handle(_ chunk: AudioChunk) {
        lock.lock()
        chunkCount += 1
        let isFirst = firstFormat == nil
        if isFirst { firstFormat = chunk.format }
        let isReady = ready
        if !isReady { discardedFrames += chunk.frames }
        lock.unlock()

        if isFirst { onFirstChunk?(chunk.format) }
        guard isReady else { return }

        var chunkPeak: Float = 0
        let data = converter.convert(chunk, peak: &chunkPeak)
        sink.enqueue(data)

        lock.lock()
        frames += chunk.frames
        if chunkPeak > peakValue { peakValue = chunkPeak }
        var fire = false
        if let limit = options.durationSeconds, !durationFired,
           Double(frames) >= limit * Double(options.sampleRate) {
            durationFired = true
            fire = true
        }
        lock.unlock()
        if fire { onDurationReached?() }
    }

    public var framesConverted: Int { lock.lock(); defer { lock.unlock() }; return frames }
    public var framesDiscarded: Int { lock.lock(); defer { lock.unlock() }; return discardedFrames }
    public var chunks: Int { lock.lock(); defer { lock.unlock() }; return chunkCount }
    public var peak: Float { lock.lock(); defer { lock.unlock() }; return peakValue }
    public var sourceFormat: SourceFormat? { lock.lock(); defer { lock.unlock() }; return firstFormat }

    /// Seconds of audio actually written to the output.
    public var secondsWritten: Double {
        Double(sink.bytesWritten) / Double(options.bytesPerSecond)
    }

    public func progressLine(wall: TimeInterval) -> String {
        Diag.progress(
            bytes: sink.bytesWritten, seconds: secondsWritten, wall: wall,
            chunks: chunks, droppedBytes: sink.droppedBytes)
    }
}
