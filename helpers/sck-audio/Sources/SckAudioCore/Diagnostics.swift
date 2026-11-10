import Foundation

/// Process exit codes. Documented in README.md; the Rust supervisor matches on them.
public enum HelperExit: Int32 {
    case ok = 0
    case usage = 1
    case permissionDenied = 2
    case unsupportedOS = 3
    case noDisplay = 4
    case streamFailed = 5
    case outputFailed = 6
    case selfTestShort = 7
}

/// Failures the helper reports as `sck-audio: error=<code> <message>`.
public enum HelperError: Error, Equatable {
    case permissionDenied
    case unsupportedOS(String)
    case noDisplay
    case streamFailed(String)
    case outputFailed(String)

    public var code: String {
        switch self {
        case .permissionDenied: return "permission-denied"
        case .unsupportedOS: return "unsupported-os"
        case .noDisplay: return "no-display"
        case .streamFailed: return "stream-failed"
        case .outputFailed: return "output-failed"
        }
    }

    public var exit: HelperExit {
        switch self {
        case .permissionDenied: return .permissionDenied
        case .unsupportedOS: return .unsupportedOS
        case .noDisplay: return .noDisplay
        case .streamFailed: return .streamFailed
        case .outputFailed: return .outputFailed
        }
    }

    public var message: String {
        switch self {
        case .permissionDenied:
            return "screen recording permission not granted (System Settings > Privacy & Security > Screen Recording)"
        case .unsupportedOS(let v):
            return "ScreenCaptureKit audio capture needs macOS 13 or newer, running \(v)"
        case .noDisplay:
            return "no display available to attach the capture stream to"
        case .streamFailed(let m), .outputFailed(let m):
            return m
        }
    }
}

/// Builds the machine-readable stderr lines. Every line starts with `sck-audio:`.
public enum Diag {
    public static let prefix = "sck-audio:"

    /// `sck-audio: k=v k2=v2`. Values containing whitespace are double-quoted; a key with an
    /// empty value is written bare (`sck-audio: started rate=48000`).
    public static func line(_ fields: [(String, String)]) -> String {
        var parts = [prefix]
        for (key, value) in fields {
            if value.isEmpty {
                parts.append(key)
            } else if value.contains(where: { $0.isWhitespace }) {
                parts.append("\(key)=\"\(value.replacingOccurrences(of: "\"", with: "\\\""))\"")
            } else {
                parts.append("\(key)=\(value)")
            }
        }
        return parts.joined(separator: " ")
    }

    public static func started(_ o: CaptureOptions) -> String {
        line([("started", ""), ("rate", "\(o.sampleRate)"), ("channels", "\(o.channels)"), ("format", o.format.rawValue)])
    }

    public static func error(_ e: HelperError) -> String {
        "\(prefix) error=\(e.code) \(e.message)"
    }

    public static func progress(bytes: Int, seconds: Double, wall: Double, chunks: Int, droppedBytes: Int) -> String {
        line([
            ("bytes", "\(bytes)"),
            ("seconds", format1(seconds)),
            ("wall", format1(wall)),
            ("chunks", "\(chunks)"),
            ("dropped", "\(droppedBytes)"),
        ])
    }

    public static func format1(_ v: Double) -> String {
        String(format: "%.1f", v)
    }

    public static func format3(_ v: Double) -> String {
        String(format: "%.3f", v)
    }
}
