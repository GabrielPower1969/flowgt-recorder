import CoreImage
import CoreVideo
import Foundation

/// Keeps a small JPEG of the latest output frame at one path, so the app's floating panel can
/// show exactly what is going into the file (screen or window, camera square included).
///
/// Encoding runs on its own low-priority queue, at most one frame at a time; a frame that arrives
/// while the previous one is still being written is skipped. The file is replaced with rename(2),
/// so a reader never sees a half-written image.
final class PreviewWriter {
    private let queue = DispatchQueue(label: "sck-record.preview", qos: .utility)
    private let frameQueue: DispatchQueue
    private let context: CIContext
    private let url: URL
    private let tmp: URL
    private let maxWidth: Double
    /// Frame-queue state.
    private var lastOffer: Double = 0
    private var busy = false
    private var written = 0

    init(path: String, frameQueue: DispatchQueue, maxWidth: Int = 480) {
        self.frameQueue = frameQueue
        self.url = URL(fileURLWithPath: path)
        self.tmp = URL(fileURLWithPath: path + ".tmp")
        self.maxWidth = Double(maxWidth)
        context = CIContext(options: [.workingColorSpace: NSNull(), .outputColorSpace: NSNull(), .cacheIntermediates: false])
    }

    /// Frame queue. Takes the frame when `interval` has passed and nothing is being written.
    func offer(_ frame: CVPixelBuffer, now: Double, interval: Double) {
        guard now - lastOffer >= interval, !busy else { return }
        lastOffer = now
        busy = true
        queue.async { [self] in
            write(frame)
            frameQueue.async { self.busy = false }
        }
    }

    private func write(_ frame: CVPixelBuffer) {
        var image = CIImage(cvPixelBuffer: frame)
        let w = image.extent.width
        if w > maxWidth {
            let s = maxWidth / w
            image = image.transformed(by: CGAffineTransform(scaleX: s, y: s))
        }
        let rect = CGRect(x: 0, y: 0, width: image.extent.width.rounded(.down), height: image.extent.height.rounded(.down))
        guard let cg = context.createCGImage(image, from: rect) else {
            Console.err("preview: cannot render")
            return
        }
        guard Images.write(cg, jpegTo: tmp, quality: 0.6) else {
            Console.err("preview: cannot write \(tmp.path)")
            return
        }
        if rename(tmp.path, url.path) != 0 {
            Console.err("preview: rename failed errno=\(errno)")
            return
        }
        written += 1
        if written == 1 { Console.err("preview first-frame \(Int(rect.width))x\(Int(rect.height))") }
    }
}
