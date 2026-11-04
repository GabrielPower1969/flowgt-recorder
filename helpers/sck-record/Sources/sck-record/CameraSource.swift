import AVFoundation
import CoreMedia
import Foundation

/// Camera frames (BGRA) through AVCaptureSession, delivered on the frame queue so the compositor
/// always sees a consistent latest frame.
final class CameraSource: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    private let session = AVCaptureSession()
    private let queue: DispatchQueue
    private let name: String?
    private var observer: NSObjectProtocol?

    var onFrame: ((CVPixelBuffer, CMTime) -> Void)?
    var onDisconnected: (() -> Void)?

    init(name: String?, frameQueue: DispatchQueue) {
        self.name = name
        self.queue = frameQueue
    }

    func start() throws {
        guard let device = Devices.find(.video, name: name) else {
            throw NSError(domain: "sck-record", code: 1, userInfo: [NSLocalizedDescriptionKey: "no camera"])
        }
        session.beginConfiguration()
        if session.canSetSessionPreset(.hd1280x720) {
            session.sessionPreset = .hd1280x720
        }
        let input = try AVCaptureDeviceInput(device: device)
        guard session.canAddInput(input) else {
            throw NSError(domain: "sck-record", code: 2, userInfo: [NSLocalizedDescriptionKey: "cannot use camera \(device.localizedName)"])
        }
        session.addInput(input)
        let output = AVCaptureVideoDataOutput()
        output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
        output.alwaysDiscardsLateVideoFrames = true
        output.setSampleBufferDelegate(self, queue: queue)
        guard session.canAddOutput(output) else {
            throw NSError(domain: "sck-record", code: 3, userInfo: [NSLocalizedDescriptionKey: "cannot read camera \(device.localizedName)"])
        }
        session.addOutput(output)
        session.commitConfiguration()
        observer = NotificationCenter.default.addObserver(forName: .AVCaptureDeviceWasDisconnected, object: device, queue: nil) { [weak self] _ in
            self?.onDisconnected?()
        }
        Console.err("camera=\(device.localizedName)")
        session.startRunning()
    }

    func stop() {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        session.stopRunning()
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard let pixels = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        onFrame?(pixels, CMSampleBufferGetPresentationTimeStamp(sampleBuffer))
    }
}

/// Microphone buffers (48 kHz, 16-bit, stereo, interleaved) straight to the writer.
final class MicSource: NSObject, AVCaptureAudioDataOutputSampleBufferDelegate {
    private let session = AVCaptureSession()
    private let queue = DispatchQueue(label: "sck-record.mic")
    private let name: String?

    var onBuffer: ((CMSampleBuffer) -> Void)?

    init(name: String?) {
        self.name = name
    }

    func start() throws {
        guard let device = Devices.find(.audio, name: name) else {
            throw NSError(domain: "sck-record", code: 4, userInfo: [NSLocalizedDescriptionKey: "no microphone"])
        }
        let input = try AVCaptureDeviceInput(device: device)
        guard session.canAddInput(input) else {
            throw NSError(domain: "sck-record", code: 5, userInfo: [NSLocalizedDescriptionKey: "cannot use microphone \(device.localizedName)"])
        }
        session.addInput(input)
        let output = AVCaptureAudioDataOutput()
        output.audioSettings = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: 48_000,
            AVNumberOfChannelsKey: 2,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
            AVLinearPCMIsNonInterleaved: false,
        ]
        output.setSampleBufferDelegate(self, queue: queue)
        guard session.canAddOutput(output) else {
            throw NSError(domain: "sck-record", code: 6, userInfo: [NSLocalizedDescriptionKey: "cannot read microphone \(device.localizedName)"])
        }
        session.addOutput(output)
        Console.err("mic=\(device.localizedName)")
        session.startRunning()
    }

    func stop() {
        session.stopRunning()
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard CMSampleBufferGetNumSamples(sampleBuffer) > 0 else { return }
        onBuffer?(sampleBuffer)
    }
}
