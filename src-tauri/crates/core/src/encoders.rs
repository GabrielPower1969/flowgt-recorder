//! Pick the video encoder from `ffmpeg -hide_banner -encoders` output.
use crate::types::*;

/// Names present in the `-encoders` listing (" V....D hevc_videotoolbox  …").
pub fn parse_encoder_names(encoders_output: &str) -> Vec<String> {
    todo!()
}

/// Fallback chain per platform and preference:
/// macOS  auto: hevc_videotoolbox → h264_videotoolbox → libx264 → libopenh264 → mpeg4
/// windows auto: hevc_nvenc → hevc_qsv → hevc_amf → hevc_mf → h264_nvenc → h264_qsv → h264_amf → h264_mf → libx264 → libopenh264 → mpeg4
/// `hevc` preference drops the h264 hardware entries first; `h264` skips hevc entirely.
/// extra_args per encoder (e.g. videotoolbox: -tag:v hvc1 -realtime 1 -allow_sw 1 -profile:v main; nvenc: -preset p4 -tune ll -rc vbr; libx264: -preset veryfast -tune zerolatency -pix_fmt yuv420p).
pub fn choose(available: &[String], platform: Platform, pref: EncoderPreference) -> crate::Result<EncoderChoice> {
    todo!()
}

/// Whether `available` was probed to actually work (the app may run a 1-frame test encode); pure helper to build that test command.
pub fn self_test_command(ffmpeg: &str, enc: &EncoderChoice) -> FfmpegCommand {
    todo!()
}
