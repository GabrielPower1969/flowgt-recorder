import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit
import SckAudioCore

extension SourceFormat {
    /// Reads the layout of a CMSampleBuffer's audio from its AudioStreamBasicDescription.
    init?(asbd: AudioStreamBasicDescription) {
        guard asbd.mFormatID == kAudioFormatLinearPCM, asbd.mChannelsPerFrame > 0 else { return nil }
        let flags = asbd.mFormatFlags
        let isFloat = flags & kAudioFormatFlagIsFloat != 0
        let isBigEndian = flags & kAudioFormatFlagIsBigEndian != 0
        let isSigned = flags & kAudioFormatFlagIsSignedInteger != 0
        let isPlanar = flags & kAudioFormatFlagIsNonInterleaved != 0
        guard !isBigEndian else { return nil }
        let encoding: SampleEncoding
        switch (isFloat, asbd.mBitsPerChannel, isSigned) {
        case (true, 32, _): encoding = .float32
        case (false, 16, true): encoding = .int16
        case (false, 32, true): encoding = .int32
        default: return nil
        }
        self.init(
            sampleRate: asbd.mSampleRate, channels: Int(asbd.mChannelsPerFrame),
            encoding: encoding, isInterleaved: !isPlanar)
    }
}

/// The real ScreenCaptureKit source. Audio-only is achieved by attaching to the main display with
/// a 2×2 px, 1 fps video configuration and ignoring every `.screen` sample.
final class SCKCaptureSource: NSObject, AudioCaptureSource, SCStreamOutput, SCStreamDelegate {
    private let options: CaptureOptions
    private let sampleQueue = DispatchQueue(label: "sck-audio.samples")
    private let screenQueue = DispatchQueue(label: "sck-audio.screen")
    private let lock = NSLock()
    private var stream: SCStream?
    private var onChunk: ((AudioChunk) -> Void)?
    private var onStop: ((Error) -> Void)?
    private var stoppedByUs = false
    private var resampler: RateConverter?
    private var reportedFormat: SourceFormat?
    private var reportedUnsupported = false
    private let forceResample = ProcessInfo.processInfo.environment["SCK_AUDIO_FORCE_RESAMPLE"] == "1"

    /// Diagnostics (warnings) the runner prints to stderr.
    var onDiagnostic: ((String) -> Void)?

    init(options: CaptureOptions) {
        self.options = options
    }

    // MARK: AudioCaptureSource

    func start(
        onChunk: @escaping (AudioChunk) -> Void,
        onStop: @escaping (Error) -> Void,
        completion: @escaping (Result<Void, Error>) -> Void
    ) {
        self.onChunk = onChunk
        self.onStop = onStop

        SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true) { [weak self] content, error in
            guard let self else { return }
            if let error {
                completion(.failure(Self.map(error)))
                return
            }
            guard let content, !content.displays.isEmpty else {
                completion(.failure(HelperError.noDisplay))
                return
            }
            let mainID = CGMainDisplayID()
            let display = content.displays.first { $0.displayID == mainID } ?? content.displays[0]

            let filter = SCContentFilter(display: display, excludingWindows: [])
            let config = SCStreamConfiguration()
            config.capturesAudio = true
            config.excludesCurrentProcessAudio = true
            config.sampleRate = options.sampleRate
            config.channelCount = options.channels
            // Video is unavoidable on an SCStream; make it as cheap as possible and ignore it.
            config.width = 2
            config.height = 2
            config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
            config.showsCursor = false
            config.queueDepth = 3

            let stream = SCStream(filter: filter, configuration: config, delegate: self)
            do {
                try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: sampleQueue)
                try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: screenQueue)
            } catch {
                completion(.failure(Self.map(error)))
                return
            }
            lock.lock()
            self.stream = stream
            lock.unlock()

            onDiagnostic?(Diag.line([
                ("display", "\(display.displayID)"),
                ("size", "\(display.width)x\(display.height)"),
                ("displays", "\(content.displays.count)"),
            ]))

            stream.startCapture { error in
                if let error {
                    completion(.failure(Self.map(error)))
                } else {
                    completion(.success(()))
                }
            }
        }
    }

    func stop(completion: @escaping () -> Void) {
        lock.lock()
        stoppedByUs = true
        let stream = self.stream
        lock.unlock()
        guard let stream else {
            completion()
            return
        }
        stream.stopCapture { _ in completion() }
    }

    // MARK: SCStreamDelegate

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        lock.lock()
        let expected = stoppedByUs
        lock.unlock()
        if !expected { onStop?(Self.map(error)) }
    }

    // MARK: SCStreamOutput

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio else { return }
        guard CMSampleBufferIsValid(sampleBuffer), CMSampleBufferDataIsReady(sampleBuffer) else { return }
        guard let chunk = makeChunk(sampleBuffer) else { return }
        onChunk?(chunk)
    }

    private func makeChunk(_ sampleBuffer: CMSampleBuffer) -> AudioChunk? {
        guard let description = CMSampleBufferGetFormatDescription(sampleBuffer),
              let asbdPointer = CMAudioFormatDescriptionGetStreamBasicDescription(description)
        else { return nil }
        let asbd = asbdPointer.pointee
        let frames = CMSampleBufferGetNumSamples(sampleBuffer)
        guard frames > 0 else { return nil }

        guard let format = SourceFormat(asbd: asbd) else {
            if !reportedUnsupported {
                reportedUnsupported = true
                onDiagnostic?(Diag.line([
                    ("warn", "unsupported-format"),
                    ("formatID", String(format: "%08x", asbd.mFormatID)),
                    ("flags", String(format: "%08x", asbd.mFormatFlags)),
                    ("bits", "\(asbd.mBitsPerChannel)"),
                ]))
            }
            return nil
        }
        if reportedFormat != format {
            reportedFormat = format
            onDiagnostic?(Diag.line([
                ("source-format", ""),
                ("encoding", "\(format.encoding)"),
                ("layout", format.isInterleaved ? "interleaved" : "planar"),
                ("rate", "\(Int(format.sampleRate))"),
                ("channels", "\(format.channels)"),
                ("frames-per-buffer", "\(frames)"),
            ]))
        }

        let bufferCount = format.isInterleaved ? 1 : format.channels
        let abl = AudioBufferList.allocate(maximumBuffers: bufferCount)
        defer { free(abl.unsafeMutablePointer) }
        var blockBuffer: CMBlockBuffer?
        let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: nil,
            bufferListOut: abl.unsafeMutablePointer,
            bufferListSize: AudioBufferList.sizeInBytes(maximumBuffers: bufferCount),
            blockBufferAllocator: kCFAllocatorDefault,
            blockBufferMemoryAllocator: kCFAllocatorDefault,
            flags: UInt32(kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment),
            blockBufferOut: &blockBuffer)
        guard status == noErr else { return nil }

        // The AudioBufferList points into blockBuffer's memory: copy while it is alive.
        let planes: [Data]? = withExtendedLifetime(blockBuffer) {
            var copies: [Data] = []
            copies.reserveCapacity(abl.count)
            for buffer in abl {
                guard let data = buffer.mData else { return nil }
                copies.append(Data(bytes: data, count: Int(buffer.mDataByteSize)))
            }
            return copies
        }
        guard let planes else { return nil }
        var chunk = AudioChunk(format: format, frames: frames, planes: planes)

        let needsResample = format.sampleRate != Double(options.sampleRate) || forceResample
        if needsResample {
            if resampler == nil {
                resampler = RateConverter(
                    sourceRate: format.sampleRate, channels: format.channels, targetRate: Double(options.sampleRate))
                onDiagnostic?(Diag.line([
                    ("warn", "resampling"),
                    ("from", "\(Int(format.sampleRate))"),
                    ("to", "\(options.sampleRate)"),
                    ("converter", resampler == nil ? "unavailable" : "AVAudioConverter"),
                ]))
            }
            guard let resampler, let converted = resampler.convert(planes: PcmDecoder.planarFloat(chunk)) else {
                return nil
            }
            chunk = converted
        }
        return chunk
    }

    // MARK: errors

    static func map(_ error: Error) -> Error {
        if let helper = error as? HelperError { return helper }
        let ns = error as NSError
        if ns.domain == SCStreamErrorDomain {
            switch ns.code {
            case SCStreamError.Code.userDeclined.rawValue:
                return HelperError.permissionDenied
            case SCStreamError.Code.noDisplayList.rawValue, SCStreamError.Code.noCaptureSource.rawValue:
                return HelperError.noDisplay
            default:
                break
            }
        }
        return HelperError.streamFailed("\(ns.domain) \(ns.code): \(ns.localizedDescription)")
    }
}
