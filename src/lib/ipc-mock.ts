/**
 * In-memory fake of the Rust side, used by vitest and by `npm run dev` in a plain browser.
 * Realistic data: 2 screens, 1 camera, 2 mics, 1 loopback; a fake recording that ticks
 * `recorder://progress` every 500 ms and grows at the 889 kbps budget; permissions that flip to
 * granted on request; settings persisted in localStorage.
 *
 * Tests drive it through the exported `mock` (reset(), emit(), calls, knobs).
 */
import type {
  AppInfo,
  Device,
  DeviceList,
  DiskStatus,
  DriveStatus,
  Events,
  MediaInfo,
  PermissionKind,
  PermissionState,
  PermissionStatus,
  Progress,
  RecorderState,
  RecordingResult,
  RecoverableSession,
  Settings,
} from "./types";
import type { CommandArgs, CommandName, CommandResult, Transport, UnlistenFn } from "./ipc";
import { defaultSettings, idleRecorderState } from "./defaults";
import { budgetFromMbPerHour, outputSize, overBudgetRatio, previewFilename } from "./format";

export const MOCK_STORAGE_KEY = "flowgt-recorder.mock.settings";
export const MOCK_TICK_MS = 500;
export const MOCK_KBPS = 889; // 400 MB/h

type Handler<K extends CommandName> = (args: CommandArgs<K>[0]) => CommandResult<K> | Promise<CommandResult<K>>;
type Handlers = { [K in CommandName]: Handler<K> };

const clone = <T>(v: T): T => (v === undefined ? v : (JSON.parse(JSON.stringify(v)) as T));

function device(partial: Omit<Device, "width" | "height"> & Partial<Pick<Device, "width" | "height">>): Device {
  return { width: null, height: null, ...partial };
}

export function mockDevices(): DeviceList {
  return {
    screens: [
      device({ id: "1", name: "Studio Display", kind: "screen", isDefault: true, width: 5120, height: 2880 }),
      device({ id: "2", name: "Built-in Retina Display", kind: "screen", isDefault: false, width: 2880, height: 1800 }),
    ],
    cameras: [device({ id: "0", name: "FaceTime HD Camera", kind: "camera", isDefault: true, width: 1920, height: 1080 })],
    microphones: [
      device({ id: "0", name: "MacBook Pro Microphone", kind: "microphone", isDefault: true }),
      device({ id: "1", name: "AirPods Pro", kind: "microphone", isDefault: false }),
    ],
    loopbacks: [device({ id: "2", name: "BlackHole 2ch", kind: "microphone", isDefault: false })],
  };
}

export function mockAppInfo(): AppInfo {
  return {
    version: "0.1.0",
    platform: "macos",
    ffmpegPath: "/opt/homebrew/bin/ffmpeg",
    ffmpegVersion: "7.1.1",
    ffmpegBundled: false,
    encoder: { name: "hevc_videotoolbox", codec: "hevc", hardware: true, extraArgs: ["-tag:v", "hvc1"] },
    systemAudioAvailable: true,
  };
}

function loadPersisted(): Settings | null {
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem(MOCK_STORAGE_KEY) : null;
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<Settings>;
    return { ...defaultSettings("macos"), ...parsed };
  } catch {
    return null;
  }
}

export class MockBackend implements Transport {
  settings: Settings = defaultSettings("macos");
  devices: DeviceList = mockDevices();
  permissions: PermissionStatus = { screen: "notDetermined", camera: "notDetermined", microphone: "notDetermined" };
  appInfo: AppInfo = mockAppInfo();
  disk = { freeBytes: 120_000_000_000, totalBytes: 1_000_000_000_000 };
  drive: DriveStatus = { configured: true, email: null };
  recoverable: RecoverableSession[] = [];
  state: RecorderState = idleRecorderState();
  lastResult: RecordingResult | null = null;
  /** Every command received, oldest first. */
  calls: { cmd: CommandName; args: unknown }[] = [];
  /** What `pick_output_dir` answers; null = user cancelled the dialog. */
  nextPickedDir: string | null = "/Users/demo/Desktop/Recordings";
  /** Multiply the final file size, e.g. 1.12 → 12 % over budget. */
  sizeFactor = 1;
  /** Artificial latency per command (ms). Tests keep 0. */
  latencyMs = 0;
  /** Milliseconds between compress://progress ticks. */
  compressTickMs = 150;

  private listeners = new Map<string, Set<(payload: unknown) => void>>();
  private timers = new Set<ReturnType<typeof setTimeout>>();
  private intervals = new Set<ReturnType<typeof setInterval>>();
  private segmentMs = 0;
  private segmentBytes = 0;
  private segmentFrames = 0;
  private baseElapsedMs = 0;
  private baseBytes = 0;
  private startedAt = new Date();
  private jobCounter = 0;

  constructor() {
    this.settings = loadPersisted() ?? defaultSettings("macos");
  }

  /** Back to defaults: timers cleared, settings reset (and unpersisted), listeners kept. */
  reset(): void {
    this.clearTimers();
    this.settings = defaultSettings("macos");
    try {
      localStorage.removeItem(MOCK_STORAGE_KEY);
    } catch {
      /* ignore */
    }
    this.devices = mockDevices();
    this.permissions = { screen: "notDetermined", camera: "notDetermined", microphone: "notDetermined" };
    this.appInfo = mockAppInfo();
    this.disk = { freeBytes: 120_000_000_000, totalBytes: 1_000_000_000_000 };
    this.recoverable = [];
    this.drive = { configured: true, email: null };
    this.state = idleRecorderState();
    this.lastResult = null;
    this.calls = [];
    this.nextPickedDir = "/Users/demo/Desktop/Recordings";
    this.sizeFactor = 1;
    this.latencyMs = 0;
    this.compressTickMs = 150;
    this.segmentMs = this.segmentBytes = this.segmentFrames = 0;
    this.baseElapsedMs = this.baseBytes = 0;
  }

  callsTo(cmd: CommandName): unknown[] {
    return this.calls.filter((c) => c.cmd === cmd).map((c) => c.args);
  }

  // ---- Transport ---------------------------------------------------------------------------

  async invoke<K extends CommandName>(cmd: K, args?: CommandArgs<K>[0]): Promise<CommandResult<K>> {
    this.calls.push({ cmd, args: clone(args) });
    if (this.latencyMs > 0) await new Promise((r) => setTimeout(r, this.latencyMs));
    const handler = this.handlers[cmd] as Handler<K> | undefined;
    if (!handler) throw new Error(`mock: unknown command ${String(cmd)}`);
    const result = await handler(args as CommandArgs<K>[0]);
    return clone(result);
  }

  async listen<K extends keyof Events>(event: K, cb: (payload: Events[K]) => void): Promise<UnlistenFn> {
    let set = this.listeners.get(event);
    if (!set) {
      set = new Set();
      this.listeners.set(event, set);
    }
    const wrapped = cb as (payload: unknown) => void;
    set.add(wrapped);
    return () => {
      set?.delete(wrapped);
    };
  }

  /** Push an event to every listener (tests use this for warnings, errors, shortcuts). */
  emit<K extends keyof Events>(event: K, payload: Events[K]): void {
    const set = this.listeners.get(event);
    if (!set) return;
    for (const cb of [...set]) cb(clone(payload));
  }

  listenerCount(event: keyof Events): number {
    return this.listeners.get(event)?.size ?? 0;
  }

  /** Emulates the global shortcut: Rust acts, then emits `shortcut://…`. */
  async pressShortcut(which: "toggle" | "pause"): Promise<void> {
    if (which === "toggle") {
      if (this.state.phase === "idle") await this.invoke("start_recording", { meetingName: this.state.meetingName ?? "" });
      else if (this.state.phase === "recording" || this.state.phase === "paused") await this.invoke("stop_recording");
      this.emit("shortcut://toggle", null);
    } else {
      if (this.state.phase === "recording") await this.invoke("pause_recording");
      else if (this.state.phase === "paused") await this.invoke("resume_recording");
      this.emit("shortcut://pause", null);
    }
  }

  // ---- Commands -----------------------------------------------------------------------------

  private handlers: Handlers = {
    get_app_info: () => this.appInfo,
    list_devices: () => this.devices,
    check_permissions: () => this.permissions,
    request_permission: ({ kind }) => {
      this.permissions[kind] = "granted";
      return "granted" as PermissionState;
    },
    open_privacy_settings: (_args: { kind: PermissionKind }) => undefined,
    get_settings: () => this.settings,
    save_settings: ({ settings }) => {
      this.settings = clone(settings);
      try {
        localStorage.setItem(MOCK_STORAGE_KEY, JSON.stringify(this.settings));
      } catch {
        /* ignore */
      }
      return this.settings;
    },
    pick_output_dir: () => this.nextPickedDir,
    check_disk: (args) => this.diskStatus(args?.path ?? this.settings.outputDir),
    preview_filename: ({ name }) => previewFilename(this.settings.filenameTemplate, name, new Date()),
    start_recording: ({ meetingName }) => this.startRecording(meetingName),
    pause_recording: () => {
      this.assertPhase("recording", "pause_recording");
      this.closeSegment();
      this.setPhase("paused");
      return this.state;
    },
    resume_recording: () => {
      this.assertPhase("paused", "resume_recording");
      this.beginSegment();
      return this.state;
    },
    stop_recording: () => this.stopRecording(),
    cancel_recording: () => {
      this.clearTimers();
      this.state = idleRecorderState();
      this.emit("recorder://state", this.state);
      return this.state;
    },
    get_recorder_state: () => this.state,
    reveal_in_file_manager: (_args: { path: string }) => undefined,
    probe_file: ({ path }) => this.lastResult?.info ?? this.syntheticInfo(path, 0, 0),
    compress_to_budget: ({ path, targetMb }) => this.startCompress(path, targetMb),
    list_recoverable: () => this.recoverable,
    recover_session: ({ sessionId }) => {
      const session = this.recoverable.find((s) => s.sessionId === sessionId);
      if (!session) throw new Error(`mock: no recoverable session ${sessionId}`);
      this.recoverable = this.recoverable.filter((s) => s.sessionId !== sessionId);
      const result = this.makeResult(session.sessionId, session.finalOutput, session.estimatedDurationMs, session.bytes, true);
      this.lastResult = result;
      return result;
    },
    discard_session: ({ sessionId }) => {
      this.recoverable = this.recoverable.filter((s) => s.sessionId !== sessionId);
    },
    set_floating_visible: (_args: { visible: boolean }) => undefined,
    screen_thumbnail: (_args: { id: string }) => null,
    relaunch_app: () => undefined,
    drive_status: () => this.drive,
    drive_sign_in: () => {
      this.drive = { configured: this.drive.configured, email: "demo@gmail.com" };
      return this.drive;
    },
    drive_sign_out: () => {
      this.drive = { configured: this.drive.configured, email: null };
      return this.drive;
    },
    drive_upload: ({ path }) => {
      this.emit("drive://progress", { path, percent: 0, done: false, url: null, error: null, localRemoved: false });
      this.after(100, () => this.emit("drive://progress", { path, percent: 100, done: true, url: "https://drive.google.com/file/d/mock/view", error: null, localRemoved: !this.settings.saveLocal }));
    },
    open_drive_link: (_args: { url: string }) => undefined,
  };

  // ---- Recording lifecycle -----------------------------------------------------------------

  private assertPhase(expected: RecorderState["phase"], cmd: string) {
    if (this.state.phase !== expected) throw new Error(`mock: ${cmd} while ${this.state.phase}`);
  }

  private setPhase(phase: RecorderState["phase"]) {
    this.state.phase = phase;
    this.emit("recorder://state", this.state);
  }

  private startRecording(meetingName: string): RecorderState {
    this.assertPhase("idle", "start_recording");
    this.startedAt = new Date();
    this.baseElapsedMs = this.baseBytes = 0;
    this.segmentMs = this.segmentBytes = this.segmentFrames = 0;
    this.state = {
      ...idleRecorderState(),
      sessionId: `mock-${this.startedAt.getTime()}`,
      meetingName,
      cameraConnected: this.settings.camera.enabled,
      phase: this.settings.countdownSeconds > 0 ? "countdown" : "starting",
    };
    this.emit("recorder://state", this.state);

    if (this.settings.countdownSeconds > 0) {
      let remaining = this.settings.countdownSeconds;
      this.emit("countdown://tick", { remaining });
      const timer = setInterval(() => {
        remaining -= 1;
        this.emit("countdown://tick", { remaining });
        if (remaining <= 0) {
          clearInterval(timer);
          this.intervals.delete(timer);
          this.setPhase("starting");
          this.after(150, () => this.beginSegment());
        }
      }, 1000);
      this.intervals.add(timer);
    } else {
      this.after(150, () => this.beginSegment());
    }
    return this.state;
  }

  private beginSegment() {
    if (this.state.phase === "idle") return; // cancelled meanwhile
    this.segmentMs = this.segmentBytes = this.segmentFrames = 0;
    this.state.segments += 1;
    this.setPhase("recording");
    const timer = setInterval(() => this.tick(), MOCK_TICK_MS);
    this.intervals.add(timer);
  }

  private tick() {
    this.segmentMs += MOCK_TICK_MS;
    this.segmentBytes += ((MOCK_KBPS * 1000) / 8) * (MOCK_TICK_MS / 1000);
    this.segmentFrames += (this.settings.video.fps * MOCK_TICK_MS) / 1000;
    this.state.elapsedMs = this.baseElapsedMs + this.segmentMs;
    this.state.bytesWritten = Math.floor(this.baseBytes + this.segmentBytes);
    this.state.estimatedTotalBytes = this.state.bytesWritten;
    const progress: Progress = {
      frame: Math.floor(this.segmentFrames),
      fps: this.settings.video.fps,
      outTimeMs: this.segmentMs,
      totalSize: Math.floor(this.segmentBytes),
      bitrateKbps: MOCK_KBPS,
      speed: 1,
      dropFrames: 0,
      dupFrames: 0,
      finished: false,
    };
    this.emit("recorder://progress", progress);
    this.emit("recorder://state", this.state); // the real supervisor sends both at the same pace
  }

  private closeSegment() {
    for (const i of this.intervals) clearInterval(i);
    this.intervals.clear();
    this.baseElapsedMs += this.segmentMs;
    this.baseBytes += this.segmentBytes;
    this.segmentMs = this.segmentBytes = this.segmentFrames = 0;
    this.state.elapsedMs = this.baseElapsedMs;
    this.state.bytesWritten = Math.floor(this.baseBytes);
  }

  private stopRecording(): RecordingResult {
    if (this.state.phase !== "recording" && this.state.phase !== "paused") {
      throw new Error(`mock: stop_recording while ${this.state.phase}`);
    }
    if (this.state.phase === "recording") this.closeSegment();
    this.setPhase("finalizing");
    const name = previewFilename(this.settings.filenameTemplate, this.state.meetingName ?? "", this.startedAt);
    const sep = this.settings.outputDir.includes("\\") ? "\\" : "/";
    const path = `${this.settings.outputDir}${sep}${name}.mp4`;
    const bytes = Math.round(this.baseBytes * this.sizeFactor);
    const result = this.makeResult(this.state.sessionId ?? "mock", path, this.baseElapsedMs, bytes, false);
    this.lastResult = result;
    this.state = idleRecorderState();
    this.emit("recorder://state", this.state);
    this.emit("recorder://finished", result);
    return result;
  }

  private makeResult(sessionId: string, path: string, durationMs: number, bytes: number, recovered: boolean): RecordingResult {
    const tracks = this.settings.audio.tracks === "separate" ? 2 : 1;
    const b = budgetFromMbPerHour(this.settings.video.targetMbPerHour, this.settings.audio.bitrateKbps, tracks);
    return {
      sessionId,
      path,
      info: this.syntheticInfo(path, bytes, durationMs),
      budget: {
        totalKbps: b.totalKbps,
        videoKbps: b.videoKbps,
        audioKbpsTotal: b.audioKbpsTotal,
        maxrateKbps: Math.round(b.videoKbps * 1.5),
        bufsizeKbps: b.videoKbps * 2,
        expectedMbPerHour: this.settings.video.targetMbPerHour,
      },
      overBudgetRatio: overBudgetRatio(bytes, durationMs, this.settings.video.targetMbPerHour),
      recovered,
    };
  }

  private syntheticInfo(path: string, bytes: number, durationMs: number): MediaInfo {
    const capture = this.settings.capture;
    const selected =
      capture.kind === "window"
        ? null
        : (this.devices.screens.find((s) => s.id === capture.displayId) ?? this.devices.screens.find((s) => s.isDefault) ?? null);
    const size = outputSize(selected, this.settings.video.maxWidth);
    const tracks = this.settings.audio.tracks === "separate" && this.settings.audio.systemAudio.kind !== "off" ? 2 : 1;
    return {
      path,
      bytes,
      durationMs,
      width: size.width,
      height: size.height,
      fps: this.settings.video.fps,
      videoCodec: this.appInfo.encoder?.codec ?? "hevc",
      audioCodec: "aac",
      audioTracks: tracks,
      bitrateKbps: durationMs > 0 ? Math.round((bytes * 8) / durationMs) : 0,
    };
  }

  private startCompress(path: string, targetMb: number): string {
    const jobId = `job-${++this.jobCounter}`;
    const output = path.replace(/\.mp4$/i, "") + ` (${targetMb}MB).mp4`;
    let percent = 0;
    const timer = setInterval(() => {
      percent = Math.min(100, percent + 20);
      const done = percent >= 100;
      this.emit("compress://progress", { jobId, percent, done, output: done ? output : null, error: null });
      if (done) {
        clearInterval(timer);
        this.intervals.delete(timer);
      }
    }, this.compressTickMs);
    this.intervals.add(timer);
    return jobId;
  }

  private diskStatus(path: string): DiskStatus {
    const bytesPerHour = this.settings.video.targetMbPerHour * 1_000_000;
    return {
      path,
      freeBytes: this.disk.freeBytes,
      totalBytes: this.disk.totalBytes,
      hoursRecordable: this.disk.freeBytes / bytesPerHour,
      low: this.disk.freeBytes < this.settings.lowDiskWarnGb * 1_000_000_000,
    };
  }

  private after(ms: number, fn: () => void) {
    const timer = setTimeout(() => {
      this.timers.delete(timer);
      fn();
    }, ms);
    this.timers.add(timer);
  }

  private clearTimers() {
    for (const t of this.timers) clearTimeout(t);
    for (const i of this.intervals) clearInterval(i);
    this.timers.clear();
    this.intervals.clear();
  }
}

export const mock = new MockBackend();

declare global {
  interface Window {
    __flowgtMock?: MockBackend;
  }
}
if (typeof window !== "undefined") window.__flowgtMock = mock;
