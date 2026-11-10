import AVFoundation
import CoreMedia
import Foundation
import SckRecordCore

/// One MP4 through AVAssetWriter: video (HEVC or H.264), then the system track, then the mic
/// track, all on the same host clock. Every call runs on `queue`.
final class SegmentWriter {
    let queue = DispatchQueue(label: "sck-record.writer")
    private let writer: AVAssetWriter
    private let video: AVAssetWriterInput
    private let adaptor: AVAssetWriterInputPixelBufferAdaptor
    private let system: AVAssetWriterInput?
    private let mic: AVAssetWriterInput?
    private let path: String
    private(set) var sessionStart: CMTime?
    private(set) var frames = 0
    private(set) var dropped = 0
    private var finished = false
    private var droppedAudio = 0

    init(options: RecordOptions) throws {
        path = options.output
        let url = URL(fileURLWithPath: options.output)
        try? FileManager.default.removeItem(at: url)
        writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        writer.shouldOptimizeForNetworkUse = false
        // Fragments: a crash leaves everything up to the last fragment playable.
        writer.movieFragmentInterval = CMTime(seconds: options.fragmentSeconds, preferredTimescale: 600)

        var compression: [String: Any] = [
            AVVideoAverageBitRateKey: options.videoKbps * 1000,
            AVVideoMaxKeyFrameIntervalKey: options.fps * 2,
            AVVideoExpectedSourceFrameRateKey: options.fps,
            AVVideoAllowFrameReorderingKey: false,
        ]
        let codec: AVVideoCodecType
        switch options.codec {
        case .hevc:
            codec = .hevc
            compression[AVVideoProfileLevelKey] = kVTProfileLevel_HEVC_Main_AutoLevel
        case .h264:
            codec = .h264
            compression[AVVideoProfileLevelKey] = AVVideoProfileLevelH264HighAutoLevel
        }
        let settings: [String: Any] = [
            AVVideoCodecKey: codec,
            AVVideoWidthKey: options.width,
            AVVideoHeightKey: options.height,
            AVVideoCompressionPropertiesKey: compression,
        ]
        video = AVAssetWriterInput(mediaType: .video, outputSettings: settings)
        video.expectsMediaDataInRealTime = true
        adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: video, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: options.width,
            kCVPixelBufferHeightKey as String: options.height,
        ])
        guard writer.canAdd(video) else { throw HelperError.outputFailed("cannot add the video track") }
        writer.add(video)

        let audioSettings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: 48_000,
            AVNumberOfChannelsKey: 2,
            AVEncoderBitRateKey: options.audioKbps * 1000,
        ]
        let w = writer
        func audioInput() throws -> AVAssetWriterInput {
            let input = AVAssetWriterInput(mediaType: .audio, outputSettings: audioSettings)
            input.expectsMediaDataInRealTime = true
            guard w.canAdd(input) else { throw HelperError.outputFailed("cannot add an audio track") }
            w.add(input)
            return input
        }
        let systemInput = options.systemAudio ? try audioInput() : nil
        let micInput = options.mic ? try audioInput() : nil
        system = systemInput
        mic = micInput

        guard writer.startWriting() else {
            throw HelperError.outputFailed("cannot start writing \(options.output): \(writer.error?.localizedDescription ?? "unknown")")
        }
    }

    var bytesOnDisk: UInt64 {
        (try? FileManager.default.attributesOfItem(atPath: path)[.size] as? NSNumber)?.uint64Value ?? 0
    }

    /// Appends a frame; the first one starts the session. Returns false when the frame was dropped.
    func appendVideo(_ pixels: CVPixelBuffer, at time: CMTime) -> Bool {
        guard !finished else { return false }
        if sessionStart == nil {
            writer.startSession(atSourceTime: time)
            sessionStart = time
        }
        guard video.isReadyForMoreMediaData else {
            dropped += 1
            return false
        }
        if adaptor.append(pixels, withPresentationTime: time) {
            frames += 1
            return true
        }
        dropped += 1
        if writer.status == .failed {
            Console.err("writer-failed \(writer.error?.localizedDescription ?? "")")
        }
        return false
    }

    func appendSystem(_ buffer: CMSampleBuffer) {
        append(buffer, to: system)
    }

    func appendMic(_ buffer: CMSampleBuffer) {
        append(buffer, to: mic)
    }

    private func append(_ buffer: CMSampleBuffer, to input: AVAssetWriterInput?) {
        guard let input, !finished, let start = sessionStart else { return }
        // Audio before the first frame cannot be in the file (the session starts there).
        let pts = CMSampleBufferGetPresentationTimeStamp(buffer)
        guard CMTimeCompare(pts, start) >= 0 else { return }
        guard input.isReadyForMoreMediaData else {
            droppedAudio += 1
            return
        }
        if !input.append(buffer) {
            droppedAudio += 1
            if writer.status == .failed {
                Console.err("writer-failed-audio \(writer.error?.localizedDescription ?? "")")
            }
        }
    }

    func finish(at time: CMTime, completion: @escaping (Error?) -> Void) {
        guard !finished else {
            completion(nil)
            return
        }
        finished = true
        guard writer.status == .writing, sessionStart != nil else {
            let err = writer.error ?? HelperError.outputFailed("nothing was written")
            writer.cancelWriting()
            completion(err)
            return
        }
        video.markAsFinished()
        system?.markAsFinished()
        mic?.markAsFinished()
        writer.endSession(atSourceTime: time)
        let droppedAudio = self.droppedAudio
        writer.finishWriting { [writer] in
            if droppedAudio > 0 { Console.err("dropped-audio-buffers=\(droppedAudio)") }
            completion(writer.status == .completed ? nil : (writer.error ?? HelperError.outputFailed("finishing failed")))
        }
    }
}
