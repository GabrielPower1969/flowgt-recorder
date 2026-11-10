import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearToasts, dismiss, getToasts, toast } from "./stores/toasts.svelte";
import { getSettings, isLoaded, loadSettings, updateSettings, applyTheme } from "./stores/settings.svelte";
import {
  bindRecorderEvents,
  recorder,
  startRecording,
  stopRecording,
  pauseRecording,
  unbindRecorderEvents,
  __setRecorderState,
} from "./stores/recorder.svelte";
import { setTransport } from "./ipc";
import { mock } from "./ipc-mock";
import { t, getLocale } from "./stores/i18n.svelte";
import { idleRecorderState } from "./defaults";
import zh from "./i18n/zh.json";

const flush = async (n = 4) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
};

describe("toasts store", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    clearToasts();
  });
  afterEach(() => {
    clearToasts();
    vi.useRealTimers();
  });

  it("toast() appends an item with an increasing id", () => {
    const a = toast("info", "one");
    const b = toast("warn", "two");
    expect(b).toBeGreaterThan(a);
    expect(getToasts().map((x) => x.text)).toEqual(["one", "two"]);
    expect(getToasts()[1].kind).toBe("warn");
  });

  it("dismiss() removes only that toast", () => {
    const a = toast("info", "one", 0);
    const b = toast("ok", "two", 0);
    dismiss(a);
    expect(getToasts().map((x) => x.id)).toEqual([b]);
    dismiss(999); // unknown id is a no-op
    expect(getToasts()).toHaveLength(1);
  });

  it("auto-dismisses after ttl (default 5000ms)", () => {
    toast("info", "short", 1000);
    toast("info", "default");
    vi.advanceTimersByTime(999);
    expect(getToasts()).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(getToasts().map((x) => x.text)).toEqual(["default"]);
    vi.advanceTimersByTime(4000);
    expect(getToasts()).toHaveLength(0);
  });

  it("ttl 0 means sticky; clearToasts empties everything", () => {
    toast("error", "sticky", 0);
    vi.advanceTimersByTime(60_000);
    expect(getToasts()).toHaveLength(1);
    clearToasts();
    expect(getToasts()).toHaveLength(0);
  });
});

describe("settings store", () => {
  beforeEach(() => {
    mock.reset();
    setTransport(mock);
    document.documentElement.removeAttribute("data-theme");
  });
  afterEach(() => {
    setTransport(null);
    mock.reset();
  });

  it("loadSettings pulls from the transport and marks loaded", async () => {
    mock.settings.theme = "dark";
    const s = await loadSettings();
    expect(s.theme).toBe("dark");
    expect(getSettings().theme).toBe("dark");
    expect(isLoaded()).toBe(true);
    expect(mock.callsTo("get_settings")).toHaveLength(1);
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("updateSettings patches theme, sets data-theme and persists", async () => {
    await loadSettings();
    await updateSettings((s) => (s.theme = "light"));
    expect(getSettings().theme).toBe("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(mock.settings.theme).toBe("light");
    expect(mock.callsTo("save_settings")).toHaveLength(1);

    await updateSettings((s) => (s.theme = "system"));
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });

  it("applyTheme toggles the attribute directly", () => {
    applyTheme("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    applyTheme("system");
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });

  it("language zh/en switches document lang", async () => {
    await loadSettings();
    await updateSettings((s) => (s.language = "en"));
    expect(document.documentElement.lang).toBe("en");
    expect(getLocale()).toBe("en");
    await updateSettings((s) => (s.language = "zh"));
    expect(document.documentElement.lang).toBe("zh-CN");
    expect(getLocale()).toBe("zh");
  });

  it("a failing save surfaces an error toast and rethrows", async () => {
    clearToasts();
    await loadSettings();
    const broken = {
      invoke: vi.fn(async () => {
        throw new Error("disk exploded");
      }),
      listen: mock.listen.bind(mock),
    };
    setTransport(broken as never);
    await expect(updateSettings((s) => (s.theme = "dark"))).rejects.toThrow("disk exploded");
    expect(getToasts().some((x) => x.kind === "error" && x.text === "disk exploded")).toBe(true);
    clearToasts();
  });
});

describe("recorder store", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    mock.reset();
    setTransport(mock);
    clearToasts();
    await loadSettings(); // language → zh (navigator zh-CN)
    __setRecorderState(idleRecorderState());
  });
  afterEach(() => {
    unbindRecorderEvents();
    setTransport(null);
    mock.reset();
    clearToasts();
    vi.useRealTimers();
  });

  it("bindRecorderEvents turns recorder://warning into a warn toast", async () => {
    bindRecorderEvents();
    await flush();
    expect(mock.listenerCount("recorder://warning")).toBe(1);
    mock.emit("recorder://warning", { code: "warn.lowDisk", detail: "" });
    expect(getToasts()).toHaveLength(1);
    expect(getToasts()[0].kind).toBe("warn");
    expect(getToasts()[0].text).toBe(zh["warn.lowDisk"]);
    expect(getToasts()[0].text).toBe(t("warn.lowDisk"));
  });

  it("recorder://error with an unknown code falls back to detail", async () => {
    bindRecorderEvents();
    await flush();
    mock.emit("recorder://error", { code: "error.weird", detail: "raw" });
    expect(getToasts()[0]).toMatchObject({ kind: "error", text: "raw" });
  });

  it("recorder://state updates recorder.state and clears progress on idle", async () => {
    bindRecorderEvents();
    await flush();
    mock.emit("recorder://progress", { frame: 1, fps: 30, outTimeMs: 500, totalSize: 10, bitrateKbps: 889, speed: 1, dropFrames: 0, dupFrames: 0, finished: false });
    expect(recorder.progress?.totalSize).toBe(10);
    mock.emit("recorder://state", { ...idleRecorderState(), phase: "recording", sessionId: "s", segments: 1 });
    expect(recorder.state.phase).toBe("recording");
    expect(recorder.isActive).toBe(true);
    mock.emit("countdown://tick", { remaining: 2 });
    expect(recorder.countdown).toBe(2);
    mock.emit("recorder://state", idleRecorderState());
    expect(recorder.state.phase).toBe("idle");
    expect(recorder.isActive).toBe(false);
    expect(recorder.progress).toBeNull();
    expect(recorder.countdown).toBeNull();
  });

  it("unbind removes the listeners", async () => {
    const unbind = bindRecorderEvents();
    await flush();
    expect(mock.listenerCount("recorder://state")).toBe(1);
    unbind();
    expect(mock.listenerCount("recorder://state")).toBe(0);
    mock.emit("recorder://warning", { code: "warn.lowDisk", detail: "" });
    expect(getToasts()).toHaveLength(0);
  });

  it("startRecording / stopRecording drive the mock and set lastResult", async () => {
    mock.settings.countdownSeconds = 0;
    const onFinished = vi.fn();
    bindRecorderEvents(onFinished);
    await flush();
    await startRecording("Demo");
    expect(recorder.state.phase).toBe("starting");
    await vi.advanceTimersByTimeAsync(150 + 1000);
    expect(recorder.state.phase).toBe("recording");
    await pauseRecording();
    expect(recorder.state.phase).toBe("paused");
    const r = await stopRecording();
    expect(r?.path.endsWith(".mp4")).toBe(true);
    expect(recorder.lastResult).toEqual(r);
    expect(recorder.state.phase).toBe("idle");
    expect(recorder.busy).toBe(false);
    expect(onFinished).toHaveBeenCalledTimes(1);
  });

  it("a failing action toasts an error and returns null", async () => {
    bindRecorderEvents();
    await flush();
    const r = await stopRecording(); // idle → mock throws
    expect(r).toBeNull();
    expect(getToasts().some((x) => x.kind === "error" && /stop_recording while idle/.test(x.text))).toBe(true);
  });
});
