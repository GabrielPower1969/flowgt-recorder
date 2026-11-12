import XCTest
@testable import SckRecordCore

final class ArgumentTests: XCTestCase {
    func testMinimalRecord() throws {
        let c = try ArgumentParser.parse(["--display", "1", "--out", "/tmp/a.mp4", "--size", "1920x1080"])
        guard case .record(let o) = c else { return XCTFail("not record") }
        XCTAssertEqual(o.source, .display(1))
        XCTAssertEqual(o.width, 1920)
        XCTAssertEqual(o.height, 1080)
        XCTAssertEqual(o.fps, 30)
        XCTAssertEqual(o.codec, .hevc)
        XCTAssertNil(o.pip)
        XCTAssertTrue(o.cursor)
        XCTAssertEqual(o.audioTracks, [])
    }

    func testEverything() throws {
        let c = try ArgumentParser.parse([
            "--app", "com.apple.Safari", "--out", "x.mp4", "--size", "1280x720", "--fps", "24", "--codec", "h264",
            "--video-kbps", "900", "--audio-kbps", "96", "--crop", "10,20,300,400", "--system-audio", "--mic-device", "USB Mic",
            "--camera-device", "FaceTime", "--pip", "1000,500,200,circle,1", "--no-cursor", "--exclude-pid", "42",
            "--duration", "3.5", "--fragment", "1",
        ])
        guard case .record(let o) = c else { return XCTFail("not record") }
        XCTAssertEqual(o.source, .app("com.apple.Safari"))
        XCTAssertEqual(o.codec, .h264)
        XCTAssertEqual(o.crop, CropSpec(x: 10, y: 20, w: 300, h: 400))
        XCTAssertTrue(o.systemAudio)
        XCTAssertEqual(o.micDevice, "USB Mic")
        XCTAssertEqual(o.cameraDevice, "FaceTime")
        XCTAssertEqual(o.pip, PipSpec(x: 1000, y: 500, size: 200, shape: .circle, mirror: true))
        XCTAssertFalse(o.cursor)
        XCTAssertEqual(o.excludePid, 42)
        XCTAssertEqual(o.duration, 3.5)
        XCTAssertEqual(o.fragmentSeconds, 1)
        XCTAssertEqual(o.audioTracks, ["system", "mic"])
    }

    func testPipMustFit() {
        XCTAssertThrowsError(try ArgumentParser.parse(["--display", "1", "--out", "x", "--size", "640x480", "--camera", "--pip", "600,0,100,square,0"]))
    }

    func testCameraWithoutPipGetsADefault() throws {
        guard case .record(let o) = try ArgumentParser.parse(["--window", "7", "--out", "x", "--size", "1920x1080", "--camera"]) else { return XCTFail() }
        let p = try XCTUnwrap(o.pip)
        XCTAssertEqual(p.size, 270)
        XCTAssertLessThanOrEqual(p.x + p.size, 1920)
        XCTAssertLessThanOrEqual(p.y + p.size, 1080)
    }

    func testModes() throws {
        XCTAssertEqual(try ArgumentParser.parse(["--list"]), .list)
        XCTAssertEqual(try ArgumentParser.parse(["--check"]), .check)
        XCTAssertEqual(try ArgumentParser.parse(["--help"]), .help)
        XCTAssertThrowsError(try ArgumentParser.parse(["--list", "--check"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--display", "1", "--window", "2", "--out", "x", "--size", "2x2"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--out", "x", "--size", "2x2"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--display", "1", "--out", "x", "--size", "1921x1080"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--bogus"]))
    }

    func testScroll() throws {
        XCTAssertEqual(
            try ArgumentParser.parse(["--scroll", "--display", "1", "--crop", "10,20,800,600", "--exclude-pid", "42"]),
            .scroll(ScrollOptions(display: 1, crop: CropSpec(x: 10, y: 20, w: 800, h: 600), excludePid: 42, fps: 10))
        )
        XCTAssertThrowsError(try ArgumentParser.parse(["--scroll", "--display", "1"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--scroll", "--window", "3", "--crop", "0,0,100,100"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--scroll", "--snapshot", "--display", "1", "--crop", "0,0,100,100"]))
    }

    func testSnapshot() throws {
        XCTAssertEqual(
            try ArgumentParser.parse(["--snapshot", "--window", "12", "--out", "t.jpg"]),
            .snapshot(SnapshotOptions(source: .window(12), output: "t.jpg", maxWidth: 480))
        )
        XCTAssertEqual(
            try ArgumentParser.parse(["--snapshot", "--display", "1", "--out", "d.jpg", "--max-width", "320"]),
            .snapshot(SnapshotOptions(source: .display(1), output: "d.jpg", maxWidth: 320))
        )
        XCTAssertThrowsError(try ArgumentParser.parse(["--snapshot", "--out", "t.jpg"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--snapshot", "--app", "com.x", "--out", "t.jpg"]))
        XCTAssertThrowsError(try ArgumentParser.parse(["--snapshot", "--list"]))
    }
}

final class EventTests: XCTestCase {
    func testJsonIsStable() {
        XCTAssertEqual(Event.started(width: 1920, height: 1080, audioTracks: ["mic"]).json, #"{"audioTracks":["mic"],"event":"started","height":1080,"width":1920}"#)
        XCTAssertEqual(Event.progress(seconds: 1.23456, bytes: 10, frames: 3, dropped: 0).json, #"{"bytes":10,"dropped":0,"event":"progress","frames":3,"seconds":1.235}"#)
        XCTAssertEqual(Event.warning(code: "warn.cameraLost", detail: "no frames for 2 s").json, #"{"code":"warn.cameraLost","detail":"no frames for 2 s","event":"warning"}"#)
        XCTAssertEqual(Event.error(code: "permission", detail: "x").json, #"{"code":"permission","detail":"x","event":"error"}"#)
    }

    func testControl() {
        XCTAssertEqual(Control.parse("stop\n"), .stop)
        XCTAssertEqual(Control.parse(" Q "), .stop)
        XCTAssertEqual(Control.parse("pause"), .unknown("pause"))
    }
}

final class GeometryTests: XCTestCase {
    func testSquareCrop() {
        XCTAssertEqual(PipGeometry.squareCrop(width: 1280, height: 720), IntRect(x: 280, y: 0, w: 720, h: 720))
        XCTAssertEqual(PipGeometry.squareCrop(width: 480, height: 640), IntRect(x: 0, y: 80, w: 480, h: 480))
    }

    func testDestinationFlipsY() {
        let p = PipSpec(x: 1600, y: 800, size: 256, shape: .rounded, mirror: false)
        XCTAssertEqual(PipGeometry.destination(p, frameHeight: 1080), IntRect(x: 1600, y: 24, w: 256, h: 256))
    }

    func testRadius() {
        XCTAssertEqual(PipGeometry.cornerRadius(.square, size: 200), 0)
        XCTAssertEqual(PipGeometry.cornerRadius(.rounded, size: 200), 25)
        XCTAssertEqual(PipGeometry.cornerRadius(.circle, size: 200), 100)
    }

    func testPoints() {
        let p = PipGeometry.points(CropSpec(x: 200, y: 100, w: 400, h: 300), scale: 2)
        XCTAssertEqual(p.x, 100)
        XCTAssertEqual(p.h, 150)
    }
}

final class PacerTests: XCTestCase {
    func testDirtyOrCameraOrHeartbeat() {
        var p = FramePacer(heartbeat: 1)
        XCTAssertTrue(p.shouldEmit(now: 0, screenDirty: false, cameraLive: false), "first tick always emits")
        XCTAssertFalse(p.shouldEmit(now: 0.5, screenDirty: false, cameraLive: false))
        XCTAssertTrue(p.shouldEmit(now: 0.6, screenDirty: true, cameraLive: false))
        XCTAssertFalse(p.shouldEmit(now: 1.0, screenDirty: false, cameraLive: false))
        XCTAssertTrue(p.shouldEmit(now: 1.6, screenDirty: false, cameraLive: false), "heartbeat")
        XCTAssertTrue(p.shouldEmit(now: 1.63, screenDirty: false, cameraLive: true), "camera: every tick")
    }
}

final class CameraWatchTests: XCTestCase {
    func testLostAndBack() {
        var w = CameraWatch(timeout: 2)
        w.frameArrived(at: 0.1)
        XCTAssertNil(w.update(now: 0.2, startedAt: 0))
        XCTAssertTrue(w.isLive)
        XCTAssertNil(w.update(now: 1.5, startedAt: 0))
        XCTAssertEqual(w.update(now: 2.2, startedAt: 0), .lost)
        XCTAssertFalse(w.isLive)
        XCTAssertNil(w.update(now: 3, startedAt: 0))
        w.frameArrived(at: 3.1)
        XCTAssertEqual(w.update(now: 3.2, startedAt: 0), .back)
        XCTAssertTrue(w.isLive)
    }

    func testNeverArrivesIsLostOnceAfterGrace() {
        var w = CameraWatch(timeout: 2)
        XCTAssertNil(w.update(now: 1, startedAt: 0))
        XCTAssertEqual(w.update(now: 2.5, startedAt: 0), .lost)
        XCTAssertNil(w.update(now: 3, startedAt: 0))
        w.frameArrived(at: 4)
        XCTAssertEqual(w.update(now: 4.1, startedAt: 0), .back)
    }

    func testPreviewOption() throws {
        let cmd = try ArgumentParser.parse(["--display", "1", "--out", "o.mp4", "--size", "640x480", "--preview", "/w/preview.jpg", "--preview-fps", "4"])
        guard case .record(let o) = cmd else { return XCTFail("expected record") }
        XCTAssertEqual(o.preview, "/w/preview.jpg")
        XCTAssertEqual(o.previewInterval, 0.25, accuracy: 0.0001)
        let plain = try ArgumentParser.parse(["--display", "1", "--out", "o.mp4", "--size", "640x480"])
        guard case .record(let p) = plain else { return XCTFail("expected record") }
        XCTAssertNil(p.preview)
        XCTAssertEqual(p.previewInterval, 0.125, accuracy: 0.0001)
        XCTAssertThrowsError(try ArgumentParser.parse(["--display", "1", "--out", "o.mp4", "--size", "640x480", "--preview", ""]))
    }
}
