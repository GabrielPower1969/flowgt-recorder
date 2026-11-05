/** Frontend defaults; the Rust side owns the persisted truth (settings::default_settings). */
import {
  DEFAULT_FILENAME_TEMPLATE,
  DEFAULT_TARGET_MB_PER_HOUR,
  SETTINGS_VERSION,
  type Platform,
  type RecorderState,
  type Settings,
} from "./types";

export function defaultOutputDir(platform: Platform): string {
  return platform === "windows" ? "C:\\Users\\demo\\Videos\\FlowGT Recorder" : "/Users/demo/Movies/FlowGT Recorder";
}

export function defaultSettings(platform: Platform = "macos"): Settings {
  return {
    version: SETTINGS_VERSION,
    language: "auto",
    theme: "system",
    outputDir: defaultOutputDir(platform),
    filenameTemplate: DEFAULT_FILENAME_TEMPLATE,
    video: { fps: 30, maxWidth: 1920, targetMbPerHour: DEFAULT_TARGET_MB_PER_HOUR, encoderPreference: "auto" },
    audio: { micDeviceId: null, micEnabled: true, systemAudio: { kind: "auto" }, tracks: "mixed", bitrateKbps: 96 },
    camera: {
      enabled: true,
      deviceId: null,
      corner: "bottom-right",
      margin: 24,
      sizePct: 20,
      shape: "rounded",
      mirror: true,
      offsetX: 0,
      offsetY: 0,
    },
    capture: { kind: "display", displayId: "" },
    shortcuts: { toggle: "CmdOrCtrl+Shift+R", pause: "CmdOrCtrl+Shift+P" },
    countdownSeconds: 3,
    lowDiskWarnGb: 5,
    onboardingDone: false,
    saveLocal: true,
    saveDrive: true,
  };
}

export function idleRecorderState(): RecorderState {
  return {
    phase: "idle",
    target: null,
    sessionId: null,
    meetingName: null,
    elapsedMs: 0,
    bytesWritten: 0,
    estimatedTotalBytes: 0,
    segments: 0,
    cameraConnected: false,
    warnings: [],
  };
}

/** Reference output height the PiP geometry (margin, offsets) is expressed against. */
export const REFERENCE_HEIGHT = 1080;

/** Placeholder until the project has a public docs site (D14: no remote yet). */
export const DOCS_URL = "https://flowgt.co.nz/recorder";
