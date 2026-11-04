import Foundation

public struct IntRect: Equatable {
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

public enum PipGeometry {
    /// Centre square of a camera frame.
    public static func squareCrop(width: Int, height: Int) -> IntRect {
        let side = min(width, height)
        return IntRect(x: (width - side) / 2, y: (height - side) / 2, w: side, h: side)
    }

    /// Where the PiP lands in bottom-left (CoreImage) coordinates.
    public static func destination(_ pip: PipSpec, frameHeight: Int) -> IntRect {
        IntRect(x: pip.x, y: frameHeight - pip.y - pip.size, w: pip.size, h: pip.size)
    }

    /// Corner radius of the mask: none for a square, S/8 rounded, S/2 a circle (same as the ffmpeg path).
    public static func cornerRadius(_ shape: PipShape, size: Int) -> Double {
        switch shape {
        case .square: return 0
        case .rounded: return Double(size) / 8
        case .circle: return Double(size) / 2
        }
    }

    /// Point size of a crop given in display pixels.
    public static func points(_ crop: CropSpec, scale: Double) -> (x: Double, y: Double, w: Double, h: Double) {
        let s = max(scale, 0.01)
        return (Double(crop.x) / s, Double(crop.y) / s, Double(crop.w) / s, Double(crop.h) / s)
    }
}

/// Decides, once per tick of the frame clock, whether a frame goes into the file.
///
/// ScreenCaptureKit delivers a frame only when something changed; the writer still needs a frame
/// now and then so the video track keeps up with the audio. With the camera on, every tick is a
/// frame, because the camera picture moves even when the screen does not.
public struct FramePacer: Equatable {
    public let heartbeat: Double
    public private(set) var lastEmit: Double?

    public init(heartbeat: Double = 1.0) {
        self.heartbeat = heartbeat
    }

    public mutating func shouldEmit(now: Double, screenDirty: Bool, cameraLive: Bool) -> Bool {
        let due = lastEmit.map { now - $0 >= heartbeat } ?? true
        if screenDirty || cameraLive || due {
            lastEmit = now
            return true
        }
        return false
    }
}

/// Tracks whether camera frames keep arriving. Two seconds without one is "lost" (unplugged, or
/// taken by another app); frames arriving again make it "back".
public struct CameraWatch: Equatable {
    public enum Transition: Equatable {
        case lost
        case back
    }

    public let timeout: Double
    public private(set) var lastFrame: Double?
    public private(set) var isLive = false
    /// A loss has been reported; the next frames are "back".
    private var reportedLost = false

    public init(timeout: Double = 2.0) {
        self.timeout = timeout
    }

    public mutating func frameArrived(at t: Double) {
        lastFrame = t
    }

    /// Call once per tick; returns a transition when the state changed.
    public mutating func update(now: Double, startedAt: Double) -> Transition? {
        let live = lastFrame.map { now - $0 < timeout } ?? false
        if live == isLive {
            // Never a frame: lost once, after the same grace period.
            if !live, lastFrame == nil, now - startedAt >= timeout, !reportedLost {
                reportedLost = true
                return .lost
            }
            return nil
        }
        isLive = live
        if live {
            return reportedLost ? .back : nil
        }
        reportedLost = true
        return .lost
    }
}
