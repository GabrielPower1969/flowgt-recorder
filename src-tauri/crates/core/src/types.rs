//! Shared data model. This file is the contract between the core crate, the
//! Tauri app crate and the TypeScript frontend (`src/lib/types.ts` mirrors it
//! 1:1, camelCase on the wire). Change both together, in one commit.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ───────────────────────────── platform ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

// ───────────────────────────── settings ─────────────────────────────

pub const SETTINGS_VERSION: u32 = 1;
pub const DEFAULT_FILENAME_TEMPLATE: &str = "{date} {time} {name}";
pub const DEFAULT_TARGET_MB_PER_HOUR: u32 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Auto,
    Zh,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EncoderPreference {
    /// hevc hardware → h264 hardware → libx264 → libopenh264 → mpeg4
    #[default]
    Auto,
    Hevc,
    H264,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSettings {
    /// Frames per second. 24 | 30 | 60.
    pub fps: u32,
    /// Output width cap; Retina 2880 → 1920. Height follows aspect ratio, even.
    pub max_width: u32,
    /// Size budget, drives the bitrate (see budget.rs).
    pub target_mb_per_hour: u32,
    pub encoder_preference: EncoderPreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioTracks {
    /// Mic and system audio mixed into one stereo AAC track.
    #[default]
    Mixed,
    /// Two AAC tracks: 0 = mic, 1 = system (for later transcription).
    Separate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", tag = "kind", content = "id")]
pub enum SystemAudioSource {
    /// Do not record system audio.
    Off,
    /// Pick the best available: ScreenCaptureKit helper (macOS 13+) → loopback device → off.
    #[default]
    Auto,
    /// A specific audio input device (e.g. BlackHole 2ch, "Stereo Mix").
    Device(String),
    /// The bundled ScreenCaptureKit audio helper (macOS).
    Helper,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSettings {
    /// None = platform default microphone.
    pub mic_device_id: Option<String>,
    pub mic_enabled: bool,
    pub system_audio: SystemAudioSource,
    pub tracks: AudioTracks,
    /// Per track. 64 | 96 | 128.
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PipShape {
    Square,
    #[default]
    Rounded,
    Circle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraSettings {
    pub enabled: bool,
    /// None = platform default camera.
    pub device_id: Option<String>,
    pub corner: Corner,
    /// Distance from the two edges of the corner, in output pixels.
    pub margin: u32,
    /// Side of the square PiP as a percentage of the output HEIGHT (10–50).
    pub size_pct: u32,
    pub shape: PipShape,
    pub mirror: bool,
    /// Extra offset from the corner anchor (drag), in output pixels.
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum CaptureTarget {
    /// Whole display. `display_id` is the device id from DeviceList.screens.
    Display { display_id: String },
    /// A region of a display, in that display's native pixels.
    Region { display_id: String, region: Region },
    /// Single window. Needs the native (ScreenCaptureKit / WGC) path; not in v0.1.
    Window { window_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutSettings {
    /// Tauri global-shortcut syntax, e.g. "CmdOrCtrl+Shift+R".
    pub toggle: String,
    pub pause: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub language: Language,
    pub theme: Theme,
    /// Absolute path. Empty string = platform default (~/Movies/FlowGT Recorder or %USERPROFILE%\Videos\FlowGT Recorder).
    pub output_dir: String,
    /// Tokens: {date} {time} {name} {datetime} {yyyy} {mm} {dd} {hh} {min}.
    pub filename_template: String,
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub camera: CameraSettings,
    pub capture: CaptureTarget,
    pub shortcuts: ShortcutSettings,
    pub countdown_seconds: u32,
    pub low_disk_warn_gb: u32,
    pub onboarding_done: bool,
}

// ───────────────────────────── devices ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Screen,
    Camera,
    Microphone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// Stable id used in Settings and in ffmpeg args: avfoundation index ("0"),
    /// dshow name ("Logi Webcam"), ddagrab output index ("0").
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub is_default: bool,
    /// Screens only: native pixel size when known.
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    pub screens: Vec<Device>,
    pub cameras: Vec<Device>,
    pub microphones: Vec<Device>,
    /// Audio inputs that look like loopback/virtual devices (BlackHole, Stereo Mix, virtual-audio-capturer).
    pub loopbacks: Vec<Device>,
}

// ───────────────────────────── encoders ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    Hevc,
    H264,
    Mpeg4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncoderChoice {
    /// ffmpeg encoder name, e.g. "hevc_videotoolbox".
    pub name: String,
    pub codec: VideoCodec,
    pub hardware: bool,
    /// Encoder-specific args appended after `-c:v <name>` (quality/profile/tag flags).
    pub extra_args: Vec<String>,
}

// ───────────────────────────── plan & command ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitrateBudget {
    pub total_kbps: u32,
    pub video_kbps: u32,
    pub audio_kbps_total: u32,
    pub maxrate_kbps: u32,
    pub bufsize_kbps: u32,
    pub expected_mb_per_hour: f64,
}

/// A video or audio input as ffmpeg sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum InputSource {
    /// macOS `-f avfoundation -i "<video>:<audio>"`; either side may be None.
    AvFoundation { video_index: Option<String>, audio_index: Option<String>, capture_cursor: bool },
    /// Windows desktop duplication as a lavfi source: `ddagrab=output_idx=N:framerate=F`.
    DdaGrab { output_idx: u32 },
    /// Windows `-f gdigrab -i desktop` fallback.
    GdiGrab { offset: Option<Region> },
    /// Windows `-f dshow -i video="<name>"` / `audio="<name>"`.
    DShow { video: Option<String>, audio: Option<String> },
    /// Raw PCM from a helper over a FIFO/file: `-f s16le -ar 48000 -ac 2 -i <path>`.
    PcmPipe { path: String, sample_rate: u32, channels: u32 },
    /// Synthetic lavfi source for tests/benchmarks, e.g. "testsrc2=size=1920x1080:rate=30" or "sine=frequency=440".
    Lavfi { graph: String },
}

/// Everything needed to build the ffmpeg command for ONE segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPlan {
    pub platform: Platform,
    pub screen: InputSource,
    /// Native size of the captured screen/region (before scaling).
    pub screen_width: u32,
    pub screen_height: u32,
    /// Crop applied to the screen input (region capture); None = full.
    pub crop: Option<Region>,
    pub camera: Option<InputSource>,
    pub mic: Option<InputSource>,
    pub system_audio: Option<InputSource>,
    pub fps: u32,
    pub out_width: u32,
    pub out_height: u32,
    pub encoder: EncoderChoice,
    pub budget: BitrateBudget,
    pub audio_tracks: AudioTracks,
    pub audio_bitrate_kbps: u32,
    pub pip: Option<PipLayout>,
    pub output: PathBuf,
    /// Stop after this many seconds (tests/benchmarks); None = until SIGINT/'q'.
    pub duration_limit_s: Option<u32>,
    /// Write fragmented MP4 so a kill -9 leaves a playable file.
    pub fragmented: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipLayout {
    /// Side of the square, output pixels.
    pub size: u32,
    /// Top-left position in output pixels (already resolved from corner+margin+offset).
    pub x: u32,
    pub y: u32,
    pub shape: PipShape,
    pub mirror: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl FfmpegCommand {
    pub fn shell_string(&self) -> String {
        let mut s = self.program.clone();
        for a in &self.args {
            s.push(' ');
            if a.contains(' ') || a.contains(';') || a.contains('[') || a.contains('\'') {
                s.push('"');
                s.push_str(&a.replace('"', "\\\""));
                s.push('"');
            } else {
                s.push_str(a);
            }
        }
        s
    }
}

// ───────────────────────────── progress & state ─────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub frame: u64,
    pub fps: f64,
    pub out_time_ms: u64,
    pub total_size: u64,
    pub bitrate_kbps: f64,
    pub speed: f64,
    pub drop_frames: u64,
    pub dup_frames: u64,
    /// true when the block ended with `progress=end`.
    pub finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    #[default]
    Idle,
    Countdown,
    Starting,
    Recording,
    Paused,
    Finalizing,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderState {
    pub phase: Phase,
    pub session_id: Option<String>,
    pub meeting_name: Option<String>,
    /// Wall-clock ms since the first segment started, excluding paused time.
    pub elapsed_ms: u64,
    pub bytes_written: u64,
    pub estimated_total_bytes: u64,
    pub segments: u32,
    pub camera_connected: bool,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    /// i18n key, e.g. "warn.lowDisk", "warn.cameraLost", "warn.frameDrop", "warn.overBudget".
    pub code: String,
    pub detail: String,
}

// ───────────────────────────── sessions & recovery ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub index: u32,
    pub path: PathBuf,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Filled from ffprobe when the segment is closed.
    pub duration_ms: Option<u64>,
    pub bytes: Option<u64>,
}

/// Written next to the segments as `<session>.manifest.json` and updated on
/// every state change, so a crashed app can finish the job next start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionManifest {
    pub version: u32,
    pub session_id: String,
    pub meeting_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub final_output: PathBuf,
    pub work_dir: PathBuf,
    pub segments: Vec<Segment>,
    pub status: SessionStatus,
    pub plan: RecordingPlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Recording,
    Paused,
    Finalizing,
    Complete,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverableSession {
    pub session_id: String,
    pub meeting_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub segments: u32,
    pub estimated_duration_ms: u64,
    pub bytes: u64,
    pub final_output: PathBuf,
}

// ───────────────────────────── results ─────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub bytes: u64,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_codec: String,
    pub audio_codec: String,
    pub audio_tracks: u32,
    pub bitrate_kbps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    pub session_id: String,
    pub path: String,
    pub info: MediaInfo,
    pub budget: BitrateBudget,
    /// bytes / (duration_h × target) − 1, e.g. 0.12 = 12 % over budget.
    pub over_budget_ratio: f64,
    pub recovered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskStatus {
    pub path: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
    /// At the current budget.
    pub hours_recordable: f64,
    pub low: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionState {
    Granted,
    Denied,
    NotDetermined,
    /// Platform does not gate this (e.g. Windows screen capture).
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub screen: PermissionState,
    pub camera: PermissionState,
    pub microphone: PermissionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: Platform,
    pub ffmpeg_path: String,
    pub ffmpeg_version: String,
    pub ffmpeg_bundled: bool,
    pub encoder: Option<EncoderChoice>,
    pub system_audio_available: bool,
}
