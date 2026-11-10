import AVFoundation
import CoreMedia
import Foundation
import SckAudioCore

/// Records a microphone through AVCaptureSession and hands over every buffer. ffmpeg's own
/// avfoundation input keeps only the latest audio buffer and lost 39 % of them next to a screen
/// capture (measured, D25), which is why the mic goes through this helper too.
final class MicCaptureSource: NSObject, AudioCaptureSource, AVCaptureAudioDataOutputSampleBufferDelegate {
    private let options: CaptureOptions
    private let queue = DispatchQueue(label: "sck-audio.mic")
    private let session = AVCaptureSession()
    private var onChunk: ((AudioChunk) -> Void)?

    var onDiagnostic: ((String) -> Void)?

    init(options: CaptureOptions) {
        self.options = options
    }

    private func device() -> AVCaptureDevice? {
        if let name = options.micDevice {
            let found = AVCaptureDevice.DiscoverySession(
                deviceTypes: [.builtInMicrophone, .externalUnknown], mediaType: .audio, position: .unspecified
            ).devices.first { $0.localizedName == name }
            if let found { return found }
            onDiagnostic?(Diag.line([("warn", "mic-not-found-using-default")]))
        }
        return AVCaptureDevice.default(for: .audio)
    }

    func start(
        onChunk: @escaping (AudioChunk) -> Void,
        onStop: @escaping (Error) -> Void,
        completion: @escaping (Result<Void, Error>) -> Void
    ) {
        self.onChunk = onChunk
        guard let device = device() else {
            completion(.failure(HelperError.streamFailed("no microphone")))
            return
        }
        do {
            let input = try AVCaptureDeviceInput(device: device)
            guard session.canAddInput(input) else {
                throw HelperError.streamFailed("cannot use microphone \(device.localizedName)")
            }
            session.addInput(input)
            let output = AVCaptureAudioDataOutput()
            // AVFoundation converts rate and channels for us, including mono to stereo.
            output.audioSettings = [
                AVFormatIDKey: kAudioFormatLinearPCM,
                AVSampleRateKey: options.sampleRate,
                AVNumberOfChannelsKey: options.channels,
                AVLinearPCMBitDepthKey: 16,
                AVLinearPCMIsFloatKey: false,
                AVLinearPCMIsBigEndianKey: false,
                AVLinearPCMIsNonInterleaved: false,
            ]
            output.setSampleBufferDelegate(self, queue: queue)
            guard session.canAddOutput(output) else {
                throw HelperError.streamFailed("cannot read microphone \(device.localizedName)")
            }
            session.addOutput(output)
        } catch {
            completion(.failure(error))
            return
        }
        onDiagnostic?(Diag.line([("mic", device.localizedName)]))
        queue.async { [session] in
            session.startRunning()
            completion(.success(()))
        }
    }

    func stop(completion: @escaping () -> Void) {
        queue.async { [session] in
            session.stopRunning()
            completion()
        }
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        let frames = CMSampleBufferGetNumSamples(sampleBuffer)
        guard frames > 0,
            let description = CMSampleBufferGetFormatDescription(sampleBuffer),
            let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(description),
            let format = SourceFormat(asbd: asbd.pointee),
            format.isInterleaved,
            let block = CMSampleBufferGetDataBuffer(sampleBuffer)
        else { return }
        let length = CMBlockBufferGetDataLength(block)
        var data = Data(count: length)
        let status = data.withUnsafeMutableBytes { raw -> OSStatus in
            guard let base = raw.baseAddress else { return -1 }
            return CMBlockBufferCopyDataBytes(block, atOffset: 0, dataLength: length, destination: base)
        }
        guard status == kCMBlockBufferNoErr else { return }
        onChunk?(AudioChunk(format: format, frames: frames, planes: [data]))
    }
}
