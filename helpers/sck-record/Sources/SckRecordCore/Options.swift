import Foundation

/// What to capture. Ids are the system's: CGDirectDisplayID, CGWindowID, bundle identifier.
public enum CaptureSource: Equatable {
    case display(UInt32)
    case window(UInt32)
    case app(String)
}

public enum VideoCodec: String, CaseIterable {
    case hevc
    case h264
}

public enum PipShape: String, CaseIterable {
    case square
    case rounded
    case circle
}

/// Camera picture-in-picture: a square of `size` output pixels with its top-left corner at (x, y),
/// in top-left output coordinates (the Rust core's `PipLayout`).
public struct PipSpec: Equatable {
    public var x: Int
    public var y: Int
    public var size: Int
    public var shape: PipShape
    public var mirror: Bool

    public init(x: Int, y: Int, size: Int, shape: PipShape, mirror: Bool) {
        self.x = x
        self.y = y
        self.size = size
        self.shape = shape
        self.mirror = mirror
    }
}

/// A region of the display in display pixels (not points).
public struct CropSpec: Equatable {
    public var x: Int
    public var y: Int
    public var w: Int
    public var h: Int

    public init(x: Int, y: Int, w: Int, h: Int) {
        self.x = x
        self.y = y
        self.w = w
        self.h = h
    }
}

public struct RecordOptions: Equatable {
    public var source: CaptureSource
    public var output: String
    /// Output frame size in pixels; both even.
    public var width: Int
    public var height: Int
    public var fps = 30
    public var codec = VideoCodec.hevc
    public var videoKbps = 1500
    public var audioKbps = 128
    public var crop: CropSpec?
    public var systemAudio = false
    public var mic = false
    /// Microphone by name; nil = the system's current input.
    public var micDevice: String?
    public var camera = false
    /// Camera by name; nil = the default camera.
    public var cameraDevice: String?
    public var pip: PipSpec?
    public var cursor = true
    /// Windows of this process are left out of the picture (the app's own floating bar).
    public var excludePid: Int32?
    /// Stop by itself after this many seconds (tests); nil = until "stop" on stdin or a signal.
    public var duration: Double?
    /// MP4 fragment interval, so a crash leaves a playable file.
    public var fragmentSeconds = 2.0
    public var progressInterval = 0.5

    public init(source: CaptureSource, output: String, width: Int, height: Int) {
        self.source = source
        self.output = output
        self.width = width
        self.height = height
    }

    /// Audio tracks in the file, in order. The Rust side relies on this order.
    public var audioTracks: [String] {
        var t: [String] = []
        if systemAudio { t.append("system") }
        if mic { t.append("mic") }
        return t
    }
}

/// `--snapshot`: one still of a window or display, as a JPEG no wider than `maxWidth`.
public struct SnapshotOptions: Equatable {
    public var source: CaptureSource
    public var output: String
    public var maxWidth: Int

    public init(source: CaptureSource, output: String, maxWidth: Int) {
        self.source = source
        self.output = output
        self.maxWidth = maxWidth
    }
}

public enum Command: Equatable {
    case record(RecordOptions)
    /// `--snapshot (--window <id> | --display <id>) --out <file.jpg> [--max-width 480]`.
    case snapshot(SnapshotOptions)
    /// `--list`: displays, windows, apps, cameras and microphones as JSON on stdout.
    case list
    /// `--check`: OS and screen permission, one word on stdout.
    case check
    case help
    case version
}

public enum ArgumentError: Error, Equatable, CustomStringConvertible {
    case unknownOption(String)
    case missingValue(String)
    case invalidValue(option: String, value: String, reason: String)
    case missing(String)
    case conflicting([String])

    public var description: String {
        switch self {
        case .unknownOption(let o): return "unknown option \(o)"
        case .missingValue(let o): return "option \(o) needs a value"
        case .invalidValue(let o, let v, let r): return "invalid value '\(v)' for \(o): \(r)"
        case .missing(let w): return "missing \(w)"
        case .conflicting(let m): return "choose only one of \(m.joined(separator: ", "))"
        }
    }
}

public enum ArgumentParser {
    public static let usage = """
    sck-record — record the screen natively (ScreenCaptureKit + AVAssetWriter) to an MP4

    USAGE
      sck-record (--display <id> | --window <id> | --app <bundle-id>) --out <file.mp4> --size <WxH>
                 [--fps 30] [--codec hevc|h264] [--video-kbps 1500] [--audio-kbps 128]
                 [--crop x,y,w,h] [--system-audio] [--mic] [--mic-device <name>]
                 [--camera] [--camera-device <name>] [--pip x,y,size,square|rounded|circle,0|1]
                 [--no-cursor] [--exclude-pid <pid>] [--duration <s>] [--fragment <s>]
      sck-record --snapshot (--display <id> | --window <id>) --out <file.jpg> [--max-width 480]
      sck-record --list
      sck-record --check
      sck-record --help | --version

    The recording runs until "stop" arrives on stdin, stdin closes, SIGINT/SIGTERM, or --duration.
    stdout carries one JSON object per line: started, progress, warning, stopped, error.
    Audio tracks in the file, in order: system (if --system-audio), mic (if --mic).

    EXIT CODES
      0 ok · 1 usage · 2 permission-denied · 3 unsupported-os · 4 source-not-found
      5 capture-failed · 6 output-failed
    """

    private static let valued: Set<String> = [
        "--display", "--window", "--app", "--out", "-o", "--size", "--fps", "--codec", "--video-kbps",
        "--audio-kbps", "--crop", "--mic-device", "--camera-device", "--pip", "--exclude-pid", "--duration",
        "--fragment", "--progress-interval", "--max-width",
    ]

    public static func parse(_ arguments: [String]) throws -> Command {
        var source: CaptureSource?
        var output: String?
        var size: (Int, Int)?
        var fps: Int?
        var codec: VideoCodec?
        var videoKbps: Int?
        var audioKbps: Int?
        var crop: CropSpec?
        var systemAudio = false
        var mic = false
        var micDevice: String?
        var camera = false
        var cameraDevice: String?
        var pip: PipSpec?
        var cursor = true
        var excludePid: Int32?
        var duration: Double?
        var fragment: Double?
        var progressInterval: Double?
        var list = false
        var check = false
        var snapshot = false
        var maxWidth: Int?
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
            case "--list": list = true
            case "--check": check = true
            case "--snapshot": snapshot = true
            case "--max-width": maxWidth = try parseInt(name, try value(), range: 16...8192)
            case "--display":
                if source != nil { throw ArgumentError.conflicting(["--display", "--window", "--app"]) }
                source = .display(UInt32(try parseInt(name, try value(), range: 0...Int(UInt32.max))))
            case "--window":
                if source != nil { throw ArgumentError.conflicting(["--display", "--window", "--app"]) }
                source = .window(UInt32(try parseInt(name, try value(), range: 1...Int(UInt32.max))))
            case "--app":
                if source != nil { throw ArgumentError.conflicting(["--display", "--window", "--app"]) }
                let v = try value()
                guard !v.isEmpty else { throw ArgumentError.invalidValue(option: name, value: v, reason: "empty") }
                source = .app(v)
            case "--out", "-o": output = try value()
            case "--size":
                let v = try value()
                let parts = v.lowercased().split(separator: "x").map(String.init)
                guard parts.count == 2, let w = Int(parts[0]), let h = Int(parts[1]), w >= 2, h >= 2, w % 2 == 0, h % 2 == 0 else {
                    throw ArgumentError.invalidValue(option: name, value: v, reason: "expected WxH, both even and >= 2")
                }
                size = (w, h)
            case "--fps": fps = try parseInt(name, try value(), range: 1...120)
            case "--codec":
                let v = try value()
                guard let c = VideoCodec(rawValue: v) else {
                    throw ArgumentError.invalidValue(option: name, value: v, reason: "expected hevc|h264")
                }
                codec = c
            case "--video-kbps": videoKbps = try parseInt(name, try value(), range: 100...200_000)
            case "--audio-kbps": audioKbps = try parseInt(name, try value(), range: 32...320)
            case "--crop":
                let v = try value()
                let n = v.split(separator: ",").map { Int($0) }
                guard n.count == 4, let x = n[0], let y = n[1], let w = n[2], let h = n[3], x >= 0, y >= 0, w > 0, h > 0 else {
                    throw ArgumentError.invalidValue(option: name, value: v, reason: "expected x,y,w,h")
                }
                crop = CropSpec(x: x, y: y, w: w, h: h)
            case "--system-audio": systemAudio = true
            case "--mic": mic = true
            case "--mic-device":
                mic = true
                micDevice = try value()
            case "--camera": camera = true
            case "--camera-device":
                camera = true
                cameraDevice = try value()
            case "--pip":
                let v = try value()
                let parts = v.split(separator: ",").map(String.init)
                guard parts.count == 5, let x = Int(parts[0]), let y = Int(parts[1]), let s = Int(parts[2]),
                      let shape = PipShape(rawValue: parts[3]), ["0", "1"].contains(parts[4]), x >= 0, y >= 0, s >= 2
                else {
                    throw ArgumentError.invalidValue(option: name, value: v, reason: "expected x,y,size,square|rounded|circle,0|1")
                }
                pip = PipSpec(x: x, y: y, size: s, shape: shape, mirror: parts[4] == "1")
            case "--no-cursor": cursor = false
            case "--exclude-pid": excludePid = Int32(try parseInt(name, try value(), range: 1...Int(Int32.max)))
            case "--duration": duration = try parseDouble(name, try value(), minimum: 0, inclusive: false)
            case "--fragment": fragment = try parseDouble(name, try value(), minimum: 0, inclusive: false)
            case "--progress-interval": progressInterval = try parseDouble(name, try value(), minimum: 0, inclusive: false)
            default:
                throw ArgumentError.unknownOption(name)
            }
        }

        if help { return .help }
        if version { return .version }
        var modes: [String] = []
        if list { modes.append("--list") }
        if check { modes.append("--check") }
        if snapshot { modes.append("--snapshot") } else if source != nil || output != nil { modes.append("--out") }
        guard modes.count <= 1 else { throw ArgumentError.conflicting(modes) }
        if list { return .list }
        if check { return .check }
        if snapshot {
            guard let source else { throw ArgumentError.missing("--display or --window") }
            if case .app = source { throw ArgumentError.invalidValue(option: "--snapshot", value: "--app", reason: "snapshots take a display or a window") }
            guard let output, !output.isEmpty else { throw ArgumentError.missing("--out <file.jpg>") }
            return .snapshot(SnapshotOptions(source: source, output: output, maxWidth: maxWidth ?? 480))
        }

        guard let source else { throw ArgumentError.missing("one of --display, --window, --app") }
        guard let output, !output.isEmpty else { throw ArgumentError.missing("--out <file.mp4>") }
        guard let size else { throw ArgumentError.missing("--size WxH") }
        var o = RecordOptions(source: source, output: output, width: size.0, height: size.1)
        if let fps { o.fps = fps }
        if let codec { o.codec = codec }
        if let videoKbps { o.videoKbps = videoKbps }
        if let audioKbps { o.audioKbps = audioKbps }
        o.crop = crop
        o.systemAudio = systemAudio
        o.mic = mic
        o.micDevice = micDevice
        o.camera = camera
        o.cameraDevice = cameraDevice
        if camera {
            let p = pip ?? PipSpec(x: max(0, o.width - o.height / 4 - 24), y: max(0, o.height - o.height / 4 - 24), size: o.height / 4 & ~1, shape: .rounded, mirror: true)
            guard p.x + p.size <= o.width, p.y + p.size <= o.height else {
                throw ArgumentError.invalidValue(option: "--pip", value: "\(p.x),\(p.y),\(p.size)", reason: "does not fit in \(o.width)x\(o.height)")
            }
            o.pip = p
        }
        o.cursor = cursor
        o.excludePid = excludePid
        o.duration = duration
        if let fragment { o.fragmentSeconds = fragment }
        if let progressInterval { o.progressInterval = progressInterval }
        return .record(o)
    }

    private static func parseInt(_ option: String, _ raw: String, range: ClosedRange<Int>) throws -> Int {
        guard let v = Int(raw) else {
            throw ArgumentError.invalidValue(option: option, value: raw, reason: "not an integer")
        }
        guard range.contains(v) else {
            throw ArgumentError.invalidValue(option: option, value: raw, reason: "must be between \(range.lowerBound) and \(range.upperBound)")
        }
        return v
    }

    private static func parseDouble(_ option: String, _ raw: String, minimum: Double, inclusive: Bool) throws -> Double {
        guard let v = Double(raw), v.isFinite else {
            throw ArgumentError.invalidValue(option: option, value: raw, reason: "not a number")
        }
        if inclusive ? v < minimum : v <= minimum {
            throw ArgumentError.invalidValue(option: option, value: raw, reason: inclusive ? "must be >= \(minimum)" : "must be > \(minimum)")
        }
        return v
    }
}
