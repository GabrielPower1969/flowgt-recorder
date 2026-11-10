import XCTest
@testable import SckAudioCore

final class ExpectationTests: XCTestCase {
    func testExpectedBytesDefaultFormat() {
        // 48000 Hz × 2 ch × 2 bytes × 2 s
        let o = CaptureOptions()
        XCTAssertEqual(SelfTest.expectedBytes(options: o, seconds: 2), 384_000)
        XCTAssertEqual(SelfTest.expectedBytes(options: o, seconds: 0.5), 96_000)
    }

    func testExpectedBytesOtherFormats() {
        let f32 = CaptureOptions(sampleRate: 44_100, channels: 1, format: .f32le)
        XCTAssertEqual(SelfTest.expectedBytes(options: f32, seconds: 1), 176_400)
        let mono16 = CaptureOptions(sampleRate: 16_000, channels: 1, format: .s16le)
        XCTAssertEqual(SelfTest.expectedBytes(options: mono16, seconds: 3), 96_000)
    }

    func testVerdictBoundaryAtEightyPercent() {
        XCTAssertTrue(SelfTest.isOK(actual: 384_000, expected: 384_000))
        XCTAssertTrue(SelfTest.isOK(actual: 400_000, expected: 384_000))
        XCTAssertTrue(SelfTest.isOK(actual: 307_200, expected: 384_000))   // exactly 80 %
        XCTAssertFalse(SelfTest.isOK(actual: 307_199, expected: 384_000))
        XCTAssertFalse(SelfTest.isOK(actual: 0, expected: 384_000))
        XCTAssertTrue(SelfTest.isOK(actual: 1, expected: 0))
        XCTAssertFalse(SelfTest.isOK(actual: 0, expected: 0))
    }

    func testLineFormat() {
        XCTAssertEqual(
            SelfTest.line(actual: 384_000, expected: 384_000, peak: 0.1234),
            "self-test: bytes=384000 expected≈384000 ok peak=0.123")
        XCTAssertEqual(
            SelfTest.line(actual: 1_000, expected: 384_000, peak: 0),
            "self-test: bytes=1000 expected≈384000 short peak=0.000")
    }
}

final class DiagnosticsTests: XCTestCase {
    func testLineFormatting() {
        XCTAssertEqual(Diag.line([("started", ""), ("rate", "48000")]), "sck-audio: started rate=48000")
        XCTAssertEqual(Diag.line([("path", "/tmp/with space.fifo")]), "sck-audio: path=\"/tmp/with space.fifo\"")
        XCTAssertEqual(Diag.line([]), "sck-audio:")
    }

    func testStartedAndError() {
        XCTAssertEqual(Diag.started(CaptureOptions()), "sck-audio: started rate=48000 channels=2 format=s16le")
        XCTAssertTrue(Diag.error(.permissionDenied).hasPrefix("sck-audio: error=permission-denied "))
        XCTAssertEqual(Diag.error(.streamFailed("boom")), "sck-audio: error=stream-failed boom")
        XCTAssertEqual(Diag.error(.noDisplay).split(separator: " ")[1], "error=no-display")
        XCTAssertTrue(Diag.error(.unsupportedOS("12.6")).contains("12.6"))
    }

    func testProgressLine() {
        XCTAssertEqual(
            Diag.progress(bytes: 960_000, seconds: 5, wall: 5.04, chunks: 234, droppedBytes: 0),
            "sck-audio: bytes=960000 seconds=5.0 wall=5.0 chunks=234 dropped=0")
    }

    func testExitCodesAreStable() {
        XCTAssertEqual(HelperError.permissionDenied.exit.rawValue, 2)
        XCTAssertEqual(HelperError.unsupportedOS("x").exit.rawValue, 3)
        XCTAssertEqual(HelperError.noDisplay.exit.rawValue, 4)
        XCTAssertEqual(HelperError.streamFailed("x").exit.rawValue, 5)
        XCTAssertEqual(HelperError.outputFailed("x").exit.rawValue, 6)
        XCTAssertEqual(HelperExit.selfTestShort.rawValue, 7)
    }
}
