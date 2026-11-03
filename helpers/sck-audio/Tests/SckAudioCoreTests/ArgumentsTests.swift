import XCTest
@testable import SckAudioCore

final class ArgumentsTests: XCTestCase {
    func testDefaultsForStream() throws {
        let cmd = try ArgumentParser.parse(["--out", "/tmp/a.fifo"])
        guard case .stream(let output, let o) = cmd else { return XCTFail("expected stream, got \(cmd)") }
        XCTAssertEqual(output, "/tmp/a.fifo")
        XCTAssertEqual(o.sampleRate, 48_000)
        XCTAssertEqual(o.channels, 2)
        XCTAssertEqual(o.format, .s16le)
        XCTAssertNil(o.durationSeconds)
        XCTAssertNil(o.openTimeoutSeconds)
        XCTAssertEqual(o.progressIntervalSeconds, 5)
        XCTAssertEqual(o.bytesPerSecond, 192_000)
    }

    func testAllOptionsBothSpellings() throws {
        let cmd = try ArgumentParser.parse([
            "-o", "out.pcm", "--rate=44100", "--channels", "1", "--format=f32le",
            "--duration", "2.5", "--open-timeout=10", "--progress-interval", "1",
        ])
        guard case .stream(let output, let o) = cmd else { return XCTFail("expected stream") }
        XCTAssertEqual(output, "out.pcm")
        XCTAssertEqual(o.sampleRate, 44_100)
        XCTAssertEqual(o.channels, 1)
        XCTAssertEqual(o.format, .f32le)
        XCTAssertEqual(o.durationSeconds, 2.5)
        XCTAssertEqual(o.openTimeoutSeconds, 10)
        XCTAssertEqual(o.progressIntervalSeconds, 1)
        XCTAssertEqual(o.bytesPerSecond, 44_100 * 4)
    }

    func testOpenTimeoutZeroMeansForever() throws {
        let cmd = try ArgumentParser.parse(["--out", "x", "--open-timeout", "0"])
        guard case .stream(_, let o) = cmd else { return XCTFail() }
        XCTAssertNil(o.openTimeoutSeconds)
    }

    func testCheck() throws {
        XCTAssertEqual(try ArgumentParser.parse(["--check"]), .check)
    }

    func testSelfTestDefaultDuration() throws {
        let cmd = try ArgumentParser.parse(["--self-test"])
        guard case .selfTest(let o) = cmd else { return XCTFail() }
        XCTAssertEqual(o.durationSeconds, 1)
        let cmd2 = try ArgumentParser.parse(["--self-test", "--duration", "5"])
        guard case .selfTest(let o2) = cmd2 else { return XCTFail() }
        XCTAssertEqual(o2.durationSeconds, 5)
    }

    func testHelpAndVersionWin() throws {
        XCTAssertEqual(try ArgumentParser.parse(["--out", "x", "--help"]), .help)
        XCTAssertEqual(try ArgumentParser.parse(["-h"]), .help)
        XCTAssertEqual(try ArgumentParser.parse(["--version"]), .version)
    }

    func testNoModeIsAnError() {
        XCTAssertThrowsError(try ArgumentParser.parse([])) { XCTAssertEqual($0 as? ArgumentError, .noMode) }
        XCTAssertThrowsError(try ArgumentParser.parse(["--rate", "48000"])) {
            XCTAssertEqual($0 as? ArgumentError, .noMode)
        }
    }

    func testConflictingModes() {
        XCTAssertThrowsError(try ArgumentParser.parse(["--check", "--self-test"])) {
            XCTAssertEqual($0 as? ArgumentError, .conflictingModes(["--check", "--self-test"]))
        }
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--check"]))
    }

    func testInvalidValues() {
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--rate", "abc"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--rate", "100"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--channels", "0"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--channels", "9"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--format", "u8"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--duration", "0"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--duration", "-1"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--progress-interval", "0"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", ""]))
    }

    func testMissingValueAndUnknownOption() {
        XCTAssertThrowsError(try ArgumentParser.parse(["--out"])) {
            XCTAssertEqual($0 as? ArgumentError, .missingValue("--out"))
        }
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--rate"])) {
            XCTAssertEqual($0 as? ArgumentError, .missingValue("--rate"))
        }
        XCTAssertThrowsError(try ArgumentParser.parse(["--bogus"])) {
            XCTAssertEqual($0 as? ArgumentError, .unknownOption("--bogus"))
        }
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "stray"])) {
            XCTAssertEqual($0 as? ArgumentError, .unknownOption("stray"))
        }
    }

    func testUsageMentionsEveryExitCode() {
        for code in ["permission-denied", "unsupported-os", "no-display", "stream-failed", "output-failed"] {
            XCTAssertTrue(ArgumentParser.usage.contains(code), "usage lacks \(code)")
        }
    }
}
