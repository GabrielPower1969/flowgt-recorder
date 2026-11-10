import Foundation

/// Process exit codes; the Rust supervisor matches on them.
public enum HelperExit: Int32 {
    case ok = 0
    case usage = 1
    case permissionDenied = 2
    case unsupportedOS = 3
    case sourceNotFound = 4
    case captureFailed = 5
    case outputFailed = 6
}

public enum HelperError: Error, Equatable {
    case permissionDenied(String)
    case unsupportedOS(String)
    case sourceNotFound(String)
    case captureFailed(String)
    case outputFailed(String)

    public var exit: HelperExit {
        switch self {
        case .permissionDenied: return .permissionDenied
        case .unsupportedOS: return .unsupportedOS
        case .sourceNotFound: return .sourceNotFound
        case .captureFailed: return .captureFailed
        case .outputFailed: return .outputFailed
        }
    }

    /// Stable code for the `error` event; the app maps it to a message.
    public var code: String {
        switch self {
        case .permissionDenied: return "permission"
        case .unsupportedOS: return "unsupportedOs"
        case .sourceNotFound: return "sourceNotFound"
        case .captureFailed: return "captureFailed"
        case .outputFailed: return "outputFailed"
        }
    }

    public var detail: String {
        switch self {
        case .permissionDenied(let s), .unsupportedOS(let s), .sourceNotFound(let s), .captureFailed(let s), .outputFailed(let s):
            return s
        }
    }
}

/// One line of stdout. Keys are sorted so the output is stable for tests and logs.
public enum Event: Equatable {
    /// The first frame is in the file.
    case started(width: Int, height: Int, audioTracks: [String])
    case progress(seconds: Double, bytes: UInt64, frames: Int, dropped: Int)
    case warning(code: String, detail: String)
    case stopped(seconds: Double, bytes: UInt64, frames: Int, dropped: Int)
    case error(code: String, detail: String)

    public var json: String {
        var o: [String: Any] = [:]
        switch self {
        case .started(let w, let h, let tracks):
            o = ["event": "started", "width": w, "height": h, "audioTracks": tracks]
        case .progress(let s, let b, let f, let d):
            o = ["event": "progress", "seconds": round3(s), "bytes": b, "frames": f, "dropped": d]
        case .warning(let c, let d):
            o = ["event": "warning", "code": c, "detail": d]
        case .stopped(let s, let b, let f, let d):
            o = ["event": "stopped", "seconds": round3(s), "bytes": b, "frames": f, "dropped": d]
        case .error(let c, let d):
            o = ["event": "error", "code": c, "detail": d]
        }
        let data = try! JSONSerialization.data(withJSONObject: o, options: [.sortedKeys])
        return String(decoding: data, as: UTF8.self)
    }

    /// Three decimals, serialised as written (a Double would print as 0.46700000000000003).
    private func round3(_ v: Double) -> NSDecimalNumber {
        NSDecimalNumber(string: String(format: "%.3f", v))
    }
}

/// Lines on stdin that control the run.
public enum Control: Equatable {
    case stop
    case unknown(String)

    public static func parse(_ line: String) -> Control {
        let t = line.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return t == "stop" || t == "q" ? .stop : .unknown(t)
    }
}
