import Foundation

/// The byte-count arithmetic behind `--self-test`.
public enum SelfTest {
    /// Pass when at least this fraction of the expected bytes arrived.
    public static let okRatio = 0.8

    /// `rate × channels × bytesPerSample × seconds`, rounded to the nearest byte.
    public static func expectedBytes(options: CaptureOptions, seconds: Double) -> Int {
        Int((Double(options.bytesPerSecond) * seconds).rounded())
    }

    public static func isOK(actual: Int, expected: Int) -> Bool {
        guard expected > 0 else { return actual > 0 }
        return Double(actual) >= okRatio * Double(expected)
    }

    /// `self-test: bytes=384000 expected≈384000 ok peak=0.123`
    public static func line(actual: Int, expected: Int, peak: Float) -> String {
        let verdict = isOK(actual: actual, expected: expected) ? "ok" : "short"
        return "self-test: bytes=\(actual) expected≈\(expected) \(verdict) peak=\(Diag.format3(Double(peak)))"
    }
}
