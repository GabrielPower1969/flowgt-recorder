import CoreImage
import CoreVideo
import Foundation
import Metal
import SckRecordCore

/// Draws the camera square over a screen frame with CoreImage on the GPU.
final class Compositor {
    private let context: CIContext
    private let pool: CVPixelBufferPool
    private let width: Int
    private let height: Int
    private let pip: PipSpec
    private let destination: IntRect
    private let mask: CIImage?

    init(width: Int, height: Int, pip: PipSpec) throws {
        self.width = width
        self.height = height
        self.pip = pip
        self.destination = PipGeometry.destination(pip, frameHeight: height)
        // No colour management: screen and camera are treated as the same RGB, which keeps the
        // screen pixels exactly as captured.
        let options: [CIContextOption: Any] = [.workingColorSpace: NSNull(), .outputColorSpace: NSNull(), .cacheIntermediates: false]
        if let device = MTLCreateSystemDefaultDevice() {
            context = CIContext(mtlDevice: device, options: options)
        } else {
            context = CIContext(options: options)
        }
        let attrs: [String: Any] = [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: width,
            kCVPixelBufferHeightKey as String: height,
            kCVPixelBufferIOSurfacePropertiesKey as String: [:] as [String: Any],
            kCVPixelBufferMetalCompatibilityKey as String: true,
        ]
        var pool: CVPixelBufferPool?
        guard CVPixelBufferPoolCreate(nil, nil, attrs as CFDictionary, &pool) == kCVReturnSuccess, let pool else {
            throw HelperError.outputFailed("cannot create the frame pool")
        }
        self.pool = pool

        let radius = PipGeometry.cornerRadius(pip.shape, size: pip.size)
        if radius > 0, let generator = CIFilter(name: "CIRoundedRectangleGenerator") {
            let rect = CGRect(x: destination.x, y: destination.y, width: destination.w, height: destination.h)
            generator.setValue(CIVector(cgRect: rect), forKey: "inputExtent")
            generator.setValue(radius, forKey: "inputRadius")
            generator.setValue(CIColor.white, forKey: "inputColor")
            mask = generator.outputImage?.cropped(to: rect)
        } else {
            mask = nil
        }
    }

    /// Screen frame plus camera → a buffer from the pool. `camera` nil draws the screen alone.
    func render(screen: CVPixelBuffer, camera: CVPixelBuffer?) -> CVPixelBuffer? {
        var out: CVPixelBuffer?
        guard CVPixelBufferPoolCreatePixelBuffer(nil, pool, &out) == kCVReturnSuccess, let out else { return nil }
        let base = CIImage(cvPixelBuffer: screen)
        var image = base
        if let camera {
            let cam = CIImage(cvPixelBuffer: camera)
            let cw = CVPixelBufferGetWidth(camera)
            let ch = CVPixelBufferGetHeight(camera)
            let crop = PipGeometry.squareCrop(width: cw, height: ch)
            var square = cam.cropped(to: CGRect(x: crop.x, y: crop.y, width: crop.w, height: crop.h))
                .transformed(by: CGAffineTransform(translationX: CGFloat(-crop.x), y: CGFloat(-crop.y)))
            if pip.mirror {
                square = square.transformed(by: CGAffineTransform(scaleX: -1, y: 1).translatedBy(x: CGFloat(-crop.w), y: 0))
            }
            let s = CGFloat(pip.size) / CGFloat(crop.w)
            let placed = square
                .transformed(by: CGAffineTransform(scaleX: s, y: s))
                .transformed(by: CGAffineTransform(translationX: CGFloat(destination.x), y: CGFloat(destination.y)))
            if let mask, let blend = CIFilter(name: "CIBlendWithMask") {
                blend.setValue(placed, forKey: kCIInputImageKey)
                blend.setValue(base, forKey: kCIInputBackgroundImageKey)
                blend.setValue(mask, forKey: kCIInputMaskImageKey)
                image = blend.outputImage ?? placed.composited(over: base)
            } else {
                image = placed.composited(over: base)
            }
        }
        context.render(image, to: out, bounds: CGRect(x: 0, y: 0, width: width, height: height), colorSpace: nil)
        return out
    }
}
