import Foundation

/// How samples are encoded in a captured chunk. ScreenCaptureKit delivers float32; the other two
/// are handled so a future source (or a format change in macOS) does not break the helper.
public enum SampleEncoding: Equatable {
    case float32
    case int16
    case int32

    public var bytesPerSample: Int {
        switch self {
        case .float32, .int32: return 4
        case .int16: return 2
        }
    }
}

/// Layout of a captured chunk, derived from the stream's AudioStreamBasicDescription.
public struct SourceFormat: Equatable {
    public var sampleRate: Double
    public var channels: Int
    public var encoding: SampleEncoding
    /// true: one plane with frames×channels samples; false: one plane per channel.
    public var isInterleaved: Bool

    public init(sampleRate: Double, channels: Int, encoding: SampleEncoding, isInterleaved: Bool) {
        self.sampleRate = sampleRate
        self.channels = channels
        self.encoding = encoding
        self.isInterleaved = isInterleaved
    }

    public var description: String {
        "\(encoding) \(isInterleaved ? "interleaved" : "planar") \(Int(sampleRate))Hz \(channels)ch"
    }
}

/// One delivered buffer of audio. `planes` holds raw little-endian sample bytes:
/// one element when interleaved, `format.channels` elements when planar.
public struct AudioChunk {
    public var format: SourceFormat
    public var frames: Int
    public var planes: [Data]

    public init(format: SourceFormat, frames: Int, planes: [Data]) {
        self.format = format
        self.frames = frames
        self.planes = planes
    }

    /// Convenience for tests and converters: planar float32 from per-channel arrays.
    public init(sampleRate: Double, planarFloat planes: [[Float]]) {
        let frames = planes.first?.count ?? 0
        self.format = SourceFormat(sampleRate: sampleRate, channels: planes.count, encoding: .float32, isInterleaved: false)
        self.frames = frames
        self.planes = planes.map { plane in plane.withUnsafeBufferPointer { Data(buffer: $0) } }
    }

    /// Convenience for tests: interleaved float32 from one array of frames×channels samples.
    public init(sampleRate: Double, channels: Int, interleavedFloat samples: [Float]) {
        self.format = SourceFormat(sampleRate: sampleRate, channels: channels, encoding: .float32, isInterleaved: true)
        self.frames = channels > 0 ? samples.count / channels : 0
        self.planes = [samples.withUnsafeBufferPointer { Data(buffer: $0) }]
    }
}

/// Pure sample-format conversion: any supported source layout → interleaved s16le / f32le.
public enum PcmDecoder {
    /// Decodes a chunk to planar float in [-1, 1] (ints are scaled by their full-scale value).
    /// Frames that are not fully present in every plane are dropped, never read out of bounds.
    public static func planarFloat(_ chunk: AudioChunk) -> [[Float]] {
        let f = chunk.format
        guard f.channels > 0, chunk.frames > 0, !chunk.planes.isEmpty else { return [] }
        let bps = f.encoding.bytesPerSample

        if f.isInterleaved {
            let plane = chunk.planes[0]
            let available = plane.count / (bps * f.channels)
            let frames = min(chunk.frames, available)
            var out = Array(repeating: [Float](repeating: 0, count: frames), count: f.channels)
            plane.withUnsafeBytes { raw in
                for frame in 0..<frames {
                    for ch in 0..<f.channels {
                        let offset = (frame * f.channels + ch) * bps
                        out[ch][frame] = readSample(raw, offset: offset, encoding: f.encoding)
                    }
                }
            }
            return out
        }

        let planeCount = min(f.channels, chunk.planes.count)
        let available = chunk.planes.prefix(planeCount).map { $0.count / bps }.min() ?? 0
        let frames = min(chunk.frames, available)
        var out: [[Float]] = []
        out.reserveCapacity(f.channels)
        for ch in 0..<f.channels {
            if ch >= planeCount {
                out.append([Float](repeating: 0, count: frames))
                continue
            }
            var plane = [Float](repeating: 0, count: frames)
            chunk.planes[ch].withUnsafeBytes { raw in
                for frame in 0..<frames {
                    plane[frame] = readSample(raw, offset: frame * bps, encoding: f.encoding)
                }
            }
            out.append(plane)
        }
        return out
    }

    @inline(__always)
    static func readSample(_ raw: UnsafeRawBufferPointer, offset: Int, encoding: SampleEncoding) -> Float {
        switch encoding {
        case .float32:
            return Float(bitPattern: UInt32(littleEndian: raw.loadUnaligned(fromByteOffset: offset, as: UInt32.self)))
        case .int16:
            return Float(Int16(littleEndian: raw.loadUnaligned(fromByteOffset: offset, as: Int16.self))) / 32768
        case .int32:
            return Float(Double(Int32(littleEndian: raw.loadUnaligned(fromByteOffset: offset, as: Int32.self))) / 2_147_483_648)
        }
    }
}

public enum ChannelMapper {
    /// Maps planar audio to `target` channels:
    /// same count → unchanged; 1 → duplicated; more than target → first `target` channels
    /// (or an average when target is mono); fewer than target → missing channels are silent.
    public static func map(_ planes: [[Float]], to target: Int) -> [[Float]] {
        guard target > 0 else { return [] }
        let source = planes.count
        guard source > 0 else { return [] }
        if source == target { return planes }
        let frames = planes[0].count
        if source == 1 {
            return Array(repeating: planes[0], count: target)
        }
        if source > target {
            if target == 1 {
                var mono = [Float](repeating: 0, count: frames)
                let scale = 1 / Float(source)
                for plane in planes {
                    for i in 0..<min(frames, plane.count) { mono[i] += plane[i] * scale }
                }
                return [mono]
            }
            return Array(planes.prefix(target))
        }
        return planes + Array(repeating: [Float](repeating: 0, count: frames), count: target - source)
    }
}

public enum PcmEncoder {
    /// Clamps to [-1, 1] (NaN → 0) and scales by 32767, rounding to nearest.
    @inline(__always)
    public static func toInt16(_ v: Float) -> Int16 {
        if v.isNaN { return 0 }
        let c = min(1, max(-1, v))
        return Int16(lrintf(c * 32767))
    }

    /// Interleaves planar float into little-endian bytes of the requested format.
    /// f32le is written verbatim (no clamping) — ffmpeg handles out-of-range floats itself.
    public static func interleave(_ planes: [[Float]], format: SampleFormat, peak: inout Float) -> Data {
        let channels = planes.count
        guard channels > 0 else { return Data() }
        let frames = planes.map(\.count).min() ?? 0
        guard frames > 0 else { return Data() }
        let bps = format.bytesPerSample
        var out = Data(count: frames * channels * bps)
        var localPeak = peak
        out.withUnsafeMutableBytes { (dst: UnsafeMutableRawBufferPointer) in
            var offset = 0
            for frame in 0..<frames {
                for ch in 0..<channels {
                    let v = planes[ch][frame]
                    let a = abs(v)
                    if a > localPeak, !a.isNaN { localPeak = a }
                    switch format {
                    case .s16le:
                        dst.storeBytes(of: toInt16(v).littleEndian, toByteOffset: offset, as: Int16.self)
                    case .f32le:
                        dst.storeBytes(of: v.bitPattern.littleEndian, toByteOffset: offset, as: UInt32.self)
                    }
                    offset += bps
                }
            }
        }
        peak = localPeak
        return out
    }
}

/// Decoder + channel map + encoder for one target format.
public struct PcmConverter {
    public let targetChannels: Int
    public let targetFormat: SampleFormat

    public init(targetChannels: Int, targetFormat: SampleFormat) {
        self.targetChannels = targetChannels
        self.targetFormat = targetFormat
    }

    /// Converts one chunk to interleaved target PCM bytes; `peak` is raised to the largest |sample| seen.
    public func convert(_ chunk: AudioChunk, peak: inout Float) -> Data {
        let planes = ChannelMapper.map(PcmDecoder.planarFloat(chunk), to: targetChannels)
        return PcmEncoder.interleave(planes, format: targetFormat, peak: &peak)
    }
}
