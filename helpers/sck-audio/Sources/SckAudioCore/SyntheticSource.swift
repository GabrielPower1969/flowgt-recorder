import Foundation

/// A capture source that never touches ScreenCaptureKit: a sine tone (or silence when
/// `frequency` is 0) delivered as interleaved float32 chunks on a private queue, faster than
/// real time. `--self-test` runs on it, so the self test exercises the pipeline, the sink and
/// the output path without any TCC permission and without a display.
public final class SyntheticSource: AudioCaptureSource {
    public let sampleRate: Int
    public let channels: Int
    /// Tone frequency in Hz; 0 produces silence.
    public let frequency: Double
    /// Peak amplitude in [0, 1].
    public let amplitude: Float
    public let framesPerChunk: Int
    /// Wall-clock gap between chunks. 480 frames every 1 ms is ten times real time at 48 kHz.
    public let chunkInterval: TimeInterval

    private let queue = DispatchQueue(label: "sck-audio.synthetic")
    private let lock = NSLock()
    private var timer: DispatchSourceTimer?
    private var phase = 0.0
    private var stopped = false

    public init(
        sampleRate: Int,
        channels: Int,
        frequency: Double = 440,
        amplitude: Float = 0.5,
        framesPerChunk: Int = 480,
        chunkInterval: TimeInterval = 0.001
    ) {
        self.sampleRate = max(1, sampleRate)
        self.channels = max(1, channels)
        self.frequency = frequency
        self.amplitude = amplitude
        self.framesPerChunk = max(1, framesPerChunk)
        self.chunkInterval = max(0.0002, chunkInterval)
    }

    public func start(
        onChunk: @escaping (AudioChunk) -> Void,
        onStop: @escaping (Error) -> Void,
        completion: @escaping (Result<Void, Error>) -> Void
    ) {
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: chunkInterval, leeway: .microseconds(100))
        timer.setEventHandler { [weak self] in
            guard let self, let chunk = self.nextChunk() else { return }
            onChunk(chunk)
        }
        lock.lock()
        self.timer = timer
        lock.unlock()
        timer.resume()
        completion(.success(()))
    }

    /// Cancels the timer; `completion` runs on the source queue after the last chunk.
    public func stop(completion: @escaping () -> Void) {
        lock.lock()
        stopped = true
        let timer = self.timer
        lock.unlock()
        timer?.cancel()
        queue.async { completion() }
    }

    /// The next `framesPerChunk` frames, or nil once stopped. Internal for tests.
    func nextChunk() -> AudioChunk? {
        lock.lock()
        defer { lock.unlock() }
        guard !stopped else { return nil }
        var samples = [Float](repeating: 0, count: framesPerChunk * channels)
        if frequency > 0 {
            let step = 2 * Double.pi * frequency / Double(sampleRate)
            for frame in 0..<framesPerChunk {
                let value = Float(sin(phase)) * amplitude
                phase += step
                if phase > 2 * Double.pi { phase -= 2 * Double.pi }
                for channel in 0..<channels {
                    samples[frame * channels + channel] = value
                }
            }
        }
        return AudioChunk(sampleRate: Double(sampleRate), channels: channels, interleavedFloat: samples)
    }
}
