/**
 * IPC contract — mirrors src-tauri/crates/core/src/types.rs 1:1 (camelCase on the wire).
 * Change both files together, in one commit.
 */

export type Platform = "macos" | "windows" | "linux";
export type Language = "auto" | "zh" | "en";
export type Theme = "system" | "light" | "dark";
export type EncoderPreference = "auto" | "hevc" | "h264";
export type AudioTracks = "mixed" | "separate";
export type Corner = "top-left" | "top-right" | "bottom-left" | "bottom-right";
export type PipShape = "square" | "rounded" | "circle";

export const SETTINGS_VERSION = 1;
export const DEFAULT_FILENAME_TEMPLATE = "{date} {time} {name}";
export const DEFAULT_TARGET_MB_PER_HOUR = 400;

export type SystemAudioSource =
  | { kind: "off" }
  | { kind: "auto" }
  | { kind: "device"; id: string }
  | { kind: "helper" };

export interface VideoSettings {
  fps: 24 | 30 | 60 | number;
  maxWidth: number;
  targetMbPerHour: number;
  encoderPreference: EncoderPreference;
}

export interface AudioSettings {
  micDeviceId: string | null;
  micEnabled: boolean;
  systemAudio: SystemAudioSource;
  tracks: AudioTracks;
  bitrateKbps: 64 | 96 | 128 | number;
}

export interface CameraSettings {
  enabled: boolean;
  deviceId: string | null;
  corner: Corner;
  margin: number;
  /** side of the square as % of output height, 10–50 */
  sizePct: number;
  shape: PipShape;
  mirror: boolean;
  offsetX: number;
  offsetY: number;
}

export interface Region { x: number; y: number; w: number; h: number }

export type CaptureTarget =
  | { kind: "display"; displayId: string }
  | { kind: "region"; displayId: string; region: Region }
  | { kind: "window"; windowId: string };

export interface ShortcutSettings { toggle: string; pause: string }

export interface Settings {
  version: number;
  language: Language;
  theme: Theme;
  outputDir: string;
  filenameTemplate: string;
  video: VideoSettings;
  audio: AudioSettings;
  camera: CameraSettings;
  capture: CaptureTarget;
  shortcuts: ShortcutSettings;
  countdownSeconds: number;
  lowDiskWarnGb: number;
  onboardingDone: boolean;
  saveLocal: boolean;
  saveDrive: boolean;
}

export type DeviceKind = "screen" | "camera" | "microphone";
export interface Device {
  id: string;
  name: string;
  kind: DeviceKind;
  isDefault: boolean;
  width: number | null;
  height: number | null;
}
export interface DeviceList {
  screens: Device[];
  cameras: Device[];
  microphones: Device[];
  loopbacks: Device[];
}

export type VideoCodec = "hevc" | "h264" | "mpeg4";
export interface EncoderChoice { name: string; codec: VideoCodec; hardware: boolean; extraArgs: string[] }

export interface BitrateBudget {
  totalKbps: number;
  videoKbps: number;
  audioKbpsTotal: number;
  maxrateKbps: number;
  bufsizeKbps: number;
  expectedMbPerHour: number;
}

export interface Progress {
  frame: number;
  fps: number;
  outTimeMs: number;
  totalSize: number;
  bitrateKbps: number;
  speed: number;
  dropFrames: number;
  dupFrames: number;
  finished: boolean;
}

export type Phase = "idle" | "countdown" | "starting" | "recording" | "paused" | "finalizing";

export interface Warning { code: string; detail: string }

export interface RecorderState {
  phase: Phase;
  sessionId: string | null;
  meetingName: string | null;
  elapsedMs: number;
  bytesWritten: number;
  estimatedTotalBytes: number;
  segments: number;
  cameraConnected: boolean;
  warnings: Warning[];
}

export interface MediaInfo {
  path: string;
  bytes: number;
  durationMs: number;
  width: number;
  height: number;
  fps: number;
  videoCodec: string;
  audioCodec: string;
  audioTracks: number;
  bitrateKbps: number;
}

export interface RecordingResult {
  sessionId: string;
  path: string;
  info: MediaInfo;
  budget: BitrateBudget;
  overBudgetRatio: number;
  recovered: boolean;
}

export interface RecoverableSession {
  sessionId: string;
  meetingName: string;
  createdAt: string;
  segments: number;
  estimatedDurationMs: number;
  bytes: number;
  finalOutput: string;
}

export interface DiskStatus {
  path: string;
  freeBytes: number;
  totalBytes: number;
  hoursRecordable: number;
  low: boolean;
}

export type PermissionState = "granted" | "denied" | "notDetermined" | "notApplicable";
export interface PermissionStatus { screen: PermissionState; camera: PermissionState; microphone: PermissionState }
export type PermissionKind = "screen" | "camera" | "microphone";

export interface AppInfo {
  version: string;
  platform: Platform;
  ffmpegPath: string;
  ffmpegVersion: string;
  ffmpegBundled: boolean;
  encoder: EncoderChoice | null;
  systemAudioAvailable: boolean;
}

export interface DriveStatus { configured: boolean; email: string | null }
export interface DriveProgress { path: string; percent: number; done: boolean; url: string | null; error: string | null; localRemoved: boolean }

/** Tauri command names (snake_case) and their signatures. Implemented in src-tauri/src/commands.rs. */
export interface Commands {
  get_app_info: () => AppInfo;
  list_devices: () => DeviceList;
  check_permissions: () => PermissionStatus;
  request_permission: (args: { kind: PermissionKind }) => PermissionState;
  open_privacy_settings: (args: { kind: PermissionKind }) => void;
  get_settings: () => Settings;
  save_settings: (args: { settings: Settings }) => Settings;
  pick_output_dir: () => string | null;
  check_disk: (args: { path?: string }) => DiskStatus;
  preview_filename: (args: { name: string }) => string;
  start_recording: (args: { meetingName: string }) => RecorderState;
  pause_recording: () => RecorderState;
  resume_recording: () => RecorderState;
  stop_recording: () => RecordingResult;
  cancel_recording: () => RecorderState;
  get_recorder_state: () => RecorderState;
  reveal_in_file_manager: (args: { path: string }) => void;
  probe_file: (args: { path: string }) => MediaInfo;
  compress_to_budget: (args: { path: string; targetMb: number }) => string; // job id
  list_recoverable: () => RecoverableSession[];
  recover_session: (args: { sessionId: string }) => RecordingResult;
  discard_session: (args: { sessionId: string }) => void;
  set_floating_visible: (args: { visible: boolean }) => void;
  screen_thumbnail: (args: { id: string }) => string | null; // data URL; null = no permission yet
  relaunch_app: () => void;
  drive_status: () => DriveStatus;
  drive_sign_in: () => DriveStatus;
  drive_sign_out: () => DriveStatus;
  drive_upload: (args: { path: string }) => void;
  open_drive_link: (args: { url: string }) => void;
}

/** Events emitted by the Rust side (payload types). */
export interface Events {
  "recorder://state": RecorderState;
  "recorder://progress": Progress;
  "recorder://warning": Warning;
  "recorder://finished": RecordingResult;
  "recorder://error": { code: string; detail: string };
  "shortcut://toggle": null;
  "shortcut://pause": null;
  "compress://progress": { jobId: string; percent: number; done: boolean; output: string | null; error: string | null };
  "countdown://tick": { remaining: number };
  "drive://progress": DriveProgress;
}
