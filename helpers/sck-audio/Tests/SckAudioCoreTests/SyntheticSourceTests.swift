import XCTest
@testable import SckAudioCore

/// The `--self-test` path end to end, minus the file descriptor: synthetic sine → pipeline →
/// sink → memory. No ScreenCaptureKit, no permission.
final class SyntheticSourceTests: XCTestCase {
    func testOneSecondOfSineThroughPipeline() {
        let options = CaptureOptions(durationSeconds: 1)
        let writer = MemoryWriter()
        let sink = PcmSink(writer: writer, maxPendingBytes: options.bytesPerSecond * 4)
        let pipeline = CapturePipeline(options: options, sink: sink)
        let source = SyntheticSource(sampleRate: options.sampleRate, channels: options.channels)

        let reached = expectation(description: "duration reached")
        pipeline.onDurationReached = { reached.fulfill() }
        pipeline.markReady()

        let started = expectation(description: "started")
        source.start(
            onChunk: { pipeline.handle($0) },
            onStop: { _ in XCTFail("synthetic source never stops on its own") },
            completion: { result in
                if case .success = result { started.fulfill() }
            })
        wait(for: [started, reached], timeout: 10)

        let stopped = expectation(description: "stopped")
        source.stop { stopped.fulfill() }
        wait(for: [stopped], timeout: 5)

        let finished = expectation(description: "sink finished")
        sink.finish { result in
            if case .failure(let error) = result { XCTFail("sink failed: \(error)") }
            finished.fulfill()
        }
        wait(for: [finished], timeout: 5)

        let expected = SelfTest.expectedBytes(options: options, seconds: 1)
        XCTAssertEqual(expected, 192_000)
        XCTAssertGreaterThanOrEqual(writer.bytes.count, expected)
        XCTAssertEqual(writer.bytes.count % options.bytesPerFrame, 0)
        XCTAssertTrue(SelfTest.isOK(actual: writer.bytes.count, expected: expected))
        XCTAssertEqual(pipeline.peak, 0.5, accuracy: 0.01)
        XCTAssertEqual(sink.droppedBytes, 0)
        XCTAssertTrue(writer.closed)
    }

    func testSilenceAndStopGate() {
        let source = SyntheticSource(sampleRate: 48_000, channels: 2, frequency: 0, framesPerChunk: 480)
        guard let chunk = source.nextChunk() else { return XCTFail("expected a chunk before stop") }
        XCTAssertEqual(chunk.frames, 480)
        XCTAssertEqual(chunk.format.channels, 2)
        XCTAssertEqual(chunk.format.sampleRate, 48_000)
        XCTAssertTrue(chunk.format.isInterleaved)
        XCTAssertEqual(chunk.planes.count, 1)
        XCTAssertTrue(chunk.planes[0].allSatisfy { $0 == 0 })

        let stopped = expectation(description: "stopped")
        source.stop { stopped.fulfill() }
        wait(for: [stopped], timeout: 5)
        XCTAssertNil(source.nextChunk())
    }
}
