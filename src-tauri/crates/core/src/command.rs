//! RecordingPlan → ffmpeg argv. Pure. Tested with lavfi inputs in integration tests.
use crate::types::*;
use std::path::Path;

/// Output size for a captured screen: scale to max_width, keep aspect, force even dimensions.
pub fn output_size(screen_w: u32, screen_h: u32, max_width: u32) -> (u32, u32) {
    todo!()
}

/// Assemble the full plan from Settings + probed context. `output` is the segment path.
pub fn make_plan(settings: &Settings, platform: Platform, screen_w: u32, screen_h: u32, encoder: EncoderChoice, output: &Path, system_audio: Option<InputSource>) -> crate::Result<RecordingPlan> {
    todo!()
}

/// The recording command for one segment:
/// inputs (screen, camera, mic, system) → filter_complex (scale/crop, pip overlay, amix or two tracks)
/// → -c:v enc -b:v -maxrate -bufsize -g fps*2 -pix_fmt yuv420p → -c:a aac -b:a → -movflags +frag_keyframe+empty_moov+default_base_moof
/// → -progress pipe:1 -nostats -y output. `-t` when duration_limit_s.
pub fn recording_command(ffmpeg: &str, plan: &RecordingPlan) -> FfmpegCommand {
    todo!()
}

/// Concat segments (stream copy, fast): -f concat -safe 0 -i list.txt -c copy -movflags +faststart out.mp4
pub fn concat_command(ffmpeg: &str, list_file: &Path, output: &Path) -> FfmpegCommand {
    todo!()
}

/// Single segment → final: remux with faststart (-c copy).
pub fn remux_command(ffmpeg: &str, input: &Path, output: &Path) -> FfmpegCommand {
    todo!()
}

/// Re-encode to hit a target size: two-pass not needed; CBR-ish at computed bitrate for the known duration.
pub fn compress_command(ffmpeg: &str, input: &Path, output: &Path, duration_ms: u64, target_mb: f64, encoder: &EncoderChoice, audio_kbps: u32) -> FfmpegCommand {
    todo!()
}

/// ffprobe JSON: -v error -print_format json -show_format -show_streams input
pub fn probe_command(ffprobe: &str, input: &Path) -> FfmpegCommand {
    todo!()
}

/// Parse the ffprobe JSON into MediaInfo.
pub fn parse_probe(json: &str, path: &str) -> crate::Result<MediaInfo> {
    todo!()
}

/// SSIM/PSNR comparison: -i ref -i dist -lavfi "[0:v][1:v]ssim=stats_file=-;[0:v][1:v]psnr" -f null -
pub fn quality_command(ffmpeg: &str, reference: &Path, distorted: &Path) -> FfmpegCommand {
    todo!()
}

/// Device-listing commands per platform.
pub fn list_devices_command(ffmpeg: &str, platform: Platform) -> FfmpegCommand {
    todo!()
}

pub fn list_encoders_command(ffmpeg: &str) -> FfmpegCommand {
    todo!()
}
