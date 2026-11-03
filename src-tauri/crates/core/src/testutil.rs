//! Shared fixtures for unit tests (compiled only under `cfg(test)`).
use crate::types::*;
use std::path::PathBuf;

pub fn camera_settings() -> CameraSettings {
    CameraSettings {
        enabled: true,
        device_id: Some("0".into()),
        corner: Corner::BottomRight,
        margin: 24,
        size_pct: 22,
        shape: PipShape::Rounded,
        mirror: true,
        offset_x: 0,
        offset_y: 0,
    }
}

/// A macOS plan: screen + camera + mic + system loopback, HEVC videotoolbox, 1080p30.
pub fn sample_plan() -> RecordingPlan {
    let budget = crate::budget::budget_for(400, 96, 1);
    RecordingPlan {
        platform: Platform::MacOs,
        screen: InputSource::AvFoundation { video_index: Some("3".into()), audio_index: None, capture_cursor: true },
        screen_width: 2880,
        screen_height: 1800,
        crop: None,
        camera: Some(InputSource::AvFoundation { video_index: Some("0".into()), audio_index: None, capture_cursor: false }),
        mic: Some(InputSource::AvFoundation { video_index: None, audio_index: Some("1".into()), capture_cursor: false }),
        system_audio: None,
        fps: 30,
        out_width: 1920,
        out_height: 1200,
        encoder: EncoderChoice {
            name: "hevc_videotoolbox".into(),
            codec: VideoCodec::Hevc,
            hardware: true,
            extra_args: vec!["-tag:v".into(), "hvc1".into()],
        },
        budget,
        audio_tracks: AudioTracks::Mixed,
        audio_bitrate_kbps: 96,
        pip: Some(PipLayout { size: 264, x: 1632, y: 912, shape: PipShape::Rounded, mirror: true }),
        output: PathBuf::from("/tmp/seg-000.mp4"),
        duration_limit_s: None,
        fragmented: true,
    }
}
