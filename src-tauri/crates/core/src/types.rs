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

pub const SETTINGS_VERSION: u32 = 2;
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
    /// Output width cap; Retina 2880 → 1920. Height follows aspect ratio, even. 0 = no cap: the
    /// screen's own pixels. The UI offers 1280 (720p), 1920 (1080p), 2560 (1440p), 3840 (4K), 0.
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
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
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
    /// Start a scrolling screenshot. Empty = no shortcut.
    pub scroll_capture: String,
    /// Show or hide the main window. Empty = no shortcut.
    pub show_window: String,
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
    /// Keep the finished recording on this computer. At least one of the two is on.
    pub save_local: bool,
    /// Upload the finished recording to Google Drive (only takes effect when signed in).
    pub save_drive: bool,
    /// When the system starts the app at login, stay in the menu bar / tray without a window.
    /// Whether the app starts at login is the system's own record, not a setting here.
    pub start_hidden_on_login: bool,
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
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum InputSource {
    /// macOS `-f avfoundation -i "<video>:<audio>"`; either side may be None.
    AvFoundation {
        video_index: Option<String>,
        audio_index: Option<String>,
        capture_cursor: bool,
    },
    /// Windows desktop duplication as a lavfi source: `ddagrab=output_idx=N:framerate=F`.
    DdaGrab { output_idx: u32 },
    /// Windows `-f gdigrab -i desktop` fallback.
    GdiGrab { offset: Option<Region> },
    /// Windows `-f dshow -i video="<name>"` / `audio="<name>"`.
    DShow {
        video: Option<String>,
        audio: Option<String>,
    },
    /// Raw PCM from a helper over a FIFO/file: `-f s16le -ar 48000 -ac 2 -i <path>`.
    PcmPipe {
        path: String,
        sample_rate: u32,
        channels: u32,
    },
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

/// What a recording is of, for the floating panel: the screen's number, or the window's app
/// and title with the app's icon (base64 PNG from the picker list).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordTarget {
    pub kind: RecordTargetKind,
    /// 1-based screen number as the picker shows it (screen and region).
    pub screen: Option<u32>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordTargetKind {
    Screen,
    Region,
    Window,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderState {
    pub phase: Phase,
    /// What is being recorded; None when idle.
    pub target: Option<RecordTarget>,
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
    /// Set when the segments were recorded by the native helper (D27); decides how they are finished.
    #[serde(default)]
    pub native_audio: Option<crate::native::NativeAudio>,
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

/// What the running platform can do. The shell reports it and the UI shows or hides controls from
/// this alone, never from the platform's name (docs/PLATFORM-ARCHITECTURE.md section 6, task S-2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// One application window can be recorded on its own.
    pub window_capture: bool,
    /// Computer audio can be recorded without a loopback device installed by the user.
    pub system_audio: bool,
    /// The audio of one application can be recorded on its own.
    pub per_app_audio: bool,
    pub camera: bool,
    pub picture_in_picture: bool,
    pub floating_bar: bool,
    pub tray: bool,
    pub global_shortcuts: bool,
    pub pause_resume: bool,
    /// The recording window can show the frame that is going into the file.
    pub live_preview: bool,
    /// The system draws the window buttons over the top edge of the page and there is no title
    /// bar to drag: the page keeps a strip clear for them and moves the window from it.
    pub overlay_title_bar: bool,
    /// A screen rectangle can be captured frame by frame for a scrolling screenshot. Set by the
    /// shell when its capture path exists; `of` leaves it off.
    pub scroll_capture: bool,
    /// The app can add itself to, and remove itself from, the system's start-at-login list. Set
    /// by the shell; `of` leaves it off.
    pub autostart: bool,
}

impl Capabilities {
    /// `native_engine`: a capture engine that owns capture, clock and file is present (today the
    /// macOS helper). `system_audio`: a system-audio source that needs no user-installed device.
    /// Window capture and the live preview exist only with a native engine; the ffmpeg path has
    /// neither. Linux records nothing yet.
    pub fn of(platform: Platform, native_engine: bool, system_audio: bool) -> Capabilities {
        let desktop = matches!(platform, Platform::MacOs | Platform::Windows);
        Capabilities {
            window_capture: desktop && native_engine,
            system_audio: desktop && system_audio,
            per_app_audio: false,
            camera: desktop,
            picture_in_picture: desktop,
            floating_bar: desktop,
            tray: desktop,
            global_shortcuts: desktop,
            pause_resume: desktop,
            live_preview: desktop && native_engine,
            overlay_title_bar: matches!(platform, Platform::MacOs),
            scroll_capture: false,
            autostart: false,
        }
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    #[test]
    fn a_native_engine_brings_window_capture_and_the_live_preview() {
        let mac = Capabilities::of(Platform::MacOs, true, true);
        assert!(mac.window_capture && mac.live_preview && mac.system_audio && mac.camera && mac.pause_resume);
        assert!(!mac.per_app_audio);
        // The ffmpeg path on Windows today: no engine, no loopback-free system audio.
        let win = Capabilities::of(Platform::Windows, false, false);
        assert!(!win.window_capture && !win.live_preview && !win.system_audio);
        assert!(win.camera && win.picture_in_picture && win.floating_bar && win.tray && win.global_shortcuts && win.pause_resume);
        // macOS without the helper falls back to the same set; only its window frame differs.
        assert_eq!(Capabilities::of(Platform::MacOs, false, false), Capabilities { overlay_title_bar: true, ..win });
        assert!(mac.overlay_title_bar && !win.overlay_title_bar);
        assert_eq!(Capabilities::of(Platform::Linux, true, true), Capabilities::default());
    }

    #[test]
    fn the_wire_format_is_camel_case() {
        let json = serde_json::to_string(&Capabilities::of(Platform::MacOs, true, false)).unwrap();
        assert!(json.contains("\"windowCapture\":true") && json.contains("\"livePreview\":true") && json.contains("\"systemAudio\":false") && json.contains("\"overlayTitleBar\":true"), "{json}");
    }
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

// ───────────────────────────── cloud ─────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveStatus {
    /// This build has a Google OAuth client compiled in.
    pub configured: bool,
    /// The signed-in account; None = signed out.
    pub email: Option<String>,
}

/// One recording in the history: on this computer, in Google Drive, or both (matched by name).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingEntry {
    pub name: String,
    pub path: Option<String>,
    pub bytes: u64,
    /// Local modified time, milliseconds since the epoch.
    pub modified_ms: Option<u64>,
    pub drive_url: Option<String>,
    /// Drive's RFC 3339 creation time.
    pub drive_created: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingHistory {
    pub entries: Vec<RecordingEntry>,
    pub drive_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveProgress {
    pub path: String,
    pub percent: f64,
    pub done: bool,
    /// Link to the file in Google Drive once uploaded.
    pub url: Option<String>,
    pub error: Option<String>,
    /// The local copy was removed after Drive confirmed every byte (Drive-only saving).
    pub local_removed: bool,
}
