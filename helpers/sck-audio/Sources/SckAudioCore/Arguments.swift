import Foundation

/// Output sample format written to the pipe. Mirrors ffmpeg's `-f s16le` / `-f f32le` demuxers.
public enum SampleFormat: String, Equatable, CaseIterable {
    case s16le
    case f32le

    public var bytesPerSample: Int {
        switch self {
        case .s16le: return 2
        case .f32le: return 4
        }
    }
}

/// Everything the capture run needs. Defaults match `InputSource::PcmPipe` in the Rust core
/// (`-f s16le -ar 48000 -ac 2`).
public struct CaptureOptions: Equatable {
    public static let defaultSampleRate = 48_000
    public static let defaultChannels = 2
    public static let defaultSelfTestSeconds = 1.0
    public static let defaultProgressInterval = 5.0

    public var sampleRate: Int
    public var channels: Int
    public var format: SampleFormat
    /// Stop once this many seconds of audio have been written; nil = run until SIGINT/SIGTERM.
    public var durationSeconds: Double?
    /// How long to wait for a FIFO reader before giving up; nil = wait forever.
    public var openTimeoutSeconds: Double?
    /// Interval between `sck-audio: bytes=… seconds=…` lines on stderr.
    public var progressIntervalSeconds: Double

    public init(
        sampleRate: Int = CaptureOptions.defaultSampleRate,
        channels: Int = CaptureOptions.defaultChannels,
        format: SampleFormat = .s16le,
        durationSeconds: Double? = nil,
        openTimeoutSeconds: Double? = nil,
        progressIntervalSeconds: Double = CaptureOptions.defaultProgressInterval
    ) {
        self.sampleRate = sampleRate
        self.channels = channels
        self.format = format
        self.durationSeconds = durationSeconds
        self.openTimeoutSeconds = openTimeoutSeconds
        self.progressIntervalSeconds = progressIntervalSeconds
    }

    public var bytesPerFrame: Int { channels * format.bytesPerSample }
    public var bytesPerSecond: Int { sampleRate * bytesPerFrame }
}

public enum Command: Equatable {
    /// `--out <path>`: stream PCM to a FIFO, a file or `-` (stdout).
    case stream(output: String, options: CaptureOptions)
    /// `--check`: report availability + permission, no capture.
    case check
    /// `--self-test`: capture for `options.durationSeconds` into a temp file and verify the byte count.
    case selfTest(options: CaptureOptions)
    case help
    case version
}

public enum ArgumentError: Error, Equatable, CustomStringConvertible {
    case unknownOption(String)
    case missingValue(String)
    case invalidValue(option: String, value: String, reason: String)
    case noMode
    case conflictingModes([String])

    public var description: String {
        switch self {
        case .unknownOption(let o): return "unknown option \(o)"
        case .missingValue(let o): return "option \(o) needs a value"
        case .invalidValue(let o, let v, let r): return "invalid value '\(v)' for \(o): \(r)"
        case .noMode: return "one of --out <path>, --check or --self-test is required"
        case .conflictingModes(let m): return "choose only one of \(m.joined(separator: ", "))"
        }
    }
}

public enum ArgumentParser {
    public static let usage = """
    sck-audio — stream macOS system audio (ScreenCaptureKit) as raw PCM

    USAGE
      sck-audio --out <fifo-or-file|-> [--rate 48000] [--channels 2] [--format s16le|f32le]
                [--duration <s>] [--open-timeout <s>] [--progress-interval <s>]
      sck-audio --check
      sck-audio --self-test [--duration 1] [--rate …] [--channels …] [--format …]
      sck-audio --help | --version

    OPTIONS
      --out, -o <path>        FIFO, regular file or '-' for stdout. Opened lazily: a FIFO blocks
                              (polls) until the reader — normally ffmpeg — opens its side.
      --rate <hz>             Sample rate, 8000–384000. Default 48000.
      --channels <n>          Channel count, 1–8. Default 2.
      --format <fmt>          s16le (default) or f32le, interleaved, little-endian.
      --duration <s>          Stop after this many seconds of audio. Default: until SIGINT/SIGTERM
                              (self-test default 1; the self test uses a synthetic sine,
                              never ScreenCaptureKit).
      --open-timeout <s>      Give up waiting for a FIFO reader after this long. 0 = forever (default).
      --progress-interval <s> Stderr progress line interval. Default 5.

    EXIT CODES
      0 ok · 1 usage · 2 permission-denied · 3 unsupported-os · 4 no-display
      5 stream-failed · 6 output-failed · 7 self-test short

    All diagnostics go to stderr as `sck-audio: key=value …` lines; stdout carries only
    PCM (with `--out -`) or the single result word/line of --check / --self-test.
    """

    private static let valued: Set<String> = [
        "--out", "-o", "--rate", "--channels", "--format", "--duration", "--open-timeout",
        "--progress-interval",
    ]

    public static func parse(_ arguments: [String]) throws -> Command {
        var options = CaptureOptions()
        var output: String?
        var check = false
        var selfTest = false
        var help = false
        var version = false

        var i = 0
        while i < arguments.count {
            var name = arguments[i]
            var inlineValue: String?
            if name.hasPrefix("--"), let eq = name.firstIndex(of: "=") {
                inlineValue = String(name[name.index(after: eq)...])
                name = String(name[..<eq])
            }
            i += 1

            func value() throws -> String {
                if let v = inlineValue { return v }
                guard i < arguments.count else { throw ArgumentError.missingValue(name) }
                let v = arguments[i]
                i += 1
                return v
            }

            switch name {
            case "--help", "-h": help = true
            case "--version", "-V": version = true
            case "--check": check = true
            case "--self-test": selfTest = true
            case "--out", "-o": output = try value()
            case "--rate":
                options.sampleRate = try parseInt(name, try value(), range: 8_000...384_000)
            case "--channels":
                options.channels = try parseInt(name, try value(), range: 1...8)
            case "--format":
                let v = try value()
                guard let f = SampleFormat(rawValue: v) else {
                    throw ArgumentError.invalidValue(
                        option: name, value: v,
                        reason: "expected one of \(SampleFormat.allCases.map(\.rawValue).joined(separator: "|"))")
                }
                options.format = f
            case "--duration":
                options.durationSeconds = try parseDouble(name, try value(), minimum: 0, inclusive: false)
            case "--open-timeout":
                let t = try parseDouble(name, try value(), minimum: 0, inclusive: true)
                options.openTimeoutSeconds = t == 0 ? nil : t
            case "--progress-interval":
                options.progressIntervalSeconds = try parseDouble(name, try value(), minimum: 0, inclusive: false)
            default:
                if inlineValue != nil || valued.contains(name) {
                    throw ArgumentError.unknownOption(name)
                }
                throw ArgumentError.unknownOption(name)
            }
        }

        if help { return .help }
        if version { return .version }

        var modes: [String] = []
        if check { modes.append("--check") }
        if selfTest { modes.append("--self-test") }
        if output != nil { modes.append("--out") }
        guard modes.count <= 1 else { throw ArgumentError.conflictingModes(modes) }

        if check { return .check }
        if selfTest {
            if options.durationSeconds == nil {
                options.durationSeconds = CaptureOptions.defaultSelfTestSeconds
            }
            return .selfTest(options: options)
        }
        if let out = output {
            guard !out.isEmpty else {
                throw ArgumentError.invalidValue(option: "--out", value: out, reason: "path is empty")
            }
            return .stream(output: out, options: options)
        }
        throw ArgumentError.noMode
    }

    private static func parseInt(_ option: String, _ raw: String, range: ClosedRange<Int>) throws -> Int {
        guard let v = Int(raw) else {
            throw ArgumentError.invalidValue(option: option, value: raw, reason: "not an integer")
        }
        guard range.contains(v) else {
            throw ArgumentError.invalidValue(
                option: option, value: raw, reason: "must be between \(range.lowerBound) and \(range.upperBound)")
        }
        return v
    }

    private static func parseDouble(_ option: String, _ raw: String, minimum: Double, inclusive: Bool) throws -> Double {
        guard let v = Double(raw), v.isFinite else {
            throw ArgumentError.invalidValue(option: option, value: raw, reason: "not a number")
        }
        if inclusive ? v < minimum : v <= minimum {
            throw ArgumentError.invalidValue(
                option: option, value: raw, reason: inclusive ? "must be >= \(minimum)" : "must be > \(minimum)")
        }
        return v
    }
}
