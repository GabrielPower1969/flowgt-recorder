import AVFoundation
import Foundation
import SckAudioCore

/// Fallback only: ScreenCaptureKit honours `SCStreamConfiguration.sampleRate`, so this is used
/// when the delivered rate differs from the requested one (or SCK_AUDIO_FORCE_RESAMPLE=1 for testing).
/// Input and output are planar float32; channel count is preserved.
final class RateConverter {
    private let converter: AVAudioConverter
    private let inputFormat: AVAudioFormat
    private let outputFormat: AVAudioFormat

    init?(sourceRate: Double, channels: Int, targetRate: Double) {
        guard channels > 0,
              let input = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: sourceRate,
                                        channels: AVAudioChannelCount(channels), interleaved: false),
              let output = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: targetRate,
                                         channels: AVAudioChannelCount(channels), interleaved: false),
              let converter = AVAudioConverter(from: input, to: output)
        else { return nil }
        self.converter = converter
        self.inputFormat = input
        self.outputFormat = output
    }

    func convert(planes: [[Float]]) -> AudioChunk? {
        let channels = Int(inputFormat.channelCount)
        let frames = planes.first?.count ?? 0
        guard planes.count == channels, frames > 0,
              let input = AVAudioPCMBuffer(pcmFormat: inputFormat, frameCapacity: AVAudioFrameCount(frames)),
              let inputData = input.floatChannelData
        else { return nil }
        for ch in 0..<channels {
            planes[ch].withUnsafeBufferPointer { src in
                inputData[ch].update(from: src.baseAddress!, count: frames)
            }
        }
        input.frameLength = AVAudioFrameCount(frames)

        let ratio = outputFormat.sampleRate / inputFormat.sampleRate
        let capacity = AVAudioFrameCount((Double(frames) * ratio).rounded(.up)) + 64
        guard let output = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: capacity) else { return nil }

        var consumed = false
        var error: NSError?
        let status = converter.convert(to: output, error: &error) { _, outStatus in
            if consumed {
                outStatus.pointee = .noDataNow
                return nil
            }
            consumed = true
            outStatus.pointee = .haveData
            return input
        }
        guard status != .error, error == nil, let outData = output.floatChannelData else { return nil }
        let outFrames = Int(output.frameLength)
        guard outFrames > 0 else { return nil }
        var result: [[Float]] = []
        result.reserveCapacity(channels)
        for ch in 0..<channels {
            result.append(Array(UnsafeBufferPointer(start: outData[ch], count: outFrames)))
        }
        return AudioChunk(sampleRate: outputFormat.sampleRate, planarFloat: result)
    }
}
