/** Recorder state mirrored from Rust events, plus the actions the screens call. */
import type { Progress, RecorderState, RecordingResult } from "../types";
import { invoke, subscribe, errorMessage, type UnlistenFn } from "../ipc";
import { idleRecorderState } from "../defaults";
import { tCode } from "./i18n.svelte";
import { toast } from "./toasts.svelte";

let state = $state<RecorderState>(idleRecorderState());
let progress = $state<Progress | null>(null);
let countdown = $state<number | null>(null);
let lastResult = $state<RecordingResult | null>(null);
let busy = $state(false);
let unlisteners: UnlistenFn[] = [];

export const recorder = {
  get state() {
    return state;
  },
  get progress() {
    return progress;
  },
  get countdown() {
    return countdown;
  },
  get lastResult() {
    return lastResult;
  },
  get busy() {
    return busy;
  },
  get isActive() {
    return state.phase !== "idle";
  },
};

export function bindRecorderEvents(onFinished?: (r: RecordingResult) => void): UnlistenFn {
  unbindRecorderEvents();
  unlisteners = [
    subscribe("recorder://state", (s) => {
      state = s;
      if (s.phase !== "countdown") countdown = null;
      if (s.phase === "idle") progress = null;
    }),
    subscribe("recorder://progress", (p) => {
      progress = p;
    }),
    subscribe("countdown://tick", ({ remaining }) => {
      countdown = remaining;
    }),
    subscribe("recorder://warning", (w) => toast("warn", tCode("warn", w.code.replace(/^warn\./, ""), w.detail))),
    subscribe("recorder://error", (e) => toast("error", tCode("error", e.code.replace(/^error\./, ""), e.detail), 8000)),
    subscribe("recorder://finished", (r) => {
      lastResult = r;
      onFinished?.(r);
    }),
  ];
  void refreshRecorderState();
  return unbindRecorderEvents;
}

export function unbindRecorderEvents(): void {
  for (const u of unlisteners) u();
  unlisteners = [];
}

export async function refreshRecorderState(): Promise<RecorderState> {
  try {
    state = await invoke("get_recorder_state");
  } catch {
    /* backend not ready yet */
  }
  return state;
}

async function act<T>(fn: () => Promise<T>): Promise<T | null> {
  if (busy) return null;
  busy = true;
  try {
    return await fn();
  } catch (e) {
    toast("error", errorMessage(e));
    return null;
  } finally {
    busy = false;
  }
}

export const startRecording = (meetingName: string) => act(async () => (state = await invoke("start_recording", { meetingName })));
export const pauseRecording = () => act(async () => (state = await invoke("pause_recording")));
export const resumeRecording = () => act(async () => (state = await invoke("resume_recording")));
export const cancelRecording = () => act(async () => (state = await invoke("cancel_recording")));
export const stopRecording = () =>
  act(async () => {
    const r = await invoke("stop_recording");
    lastResult = r;
    state = idleRecorderState();
    return r;
  });
export const togglePause = () => (state.phase === "paused" ? resumeRecording() : pauseRecording());

/** Tests only. */
export function __setRecorderState(s: RecorderState): void {
  state = s;
}
export function __setLastResult(r: RecordingResult | null): void {
  lastResult = r;
}
