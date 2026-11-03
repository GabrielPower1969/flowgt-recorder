import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MOCK_KBPS, MOCK_STORAGE_KEY, MOCK_TICK_MS, mock } from "./ipc-mock";
import type { Events, Progress, RecorderState, Settings } from "./types";

type Listener<K extends keyof Events> = (p: Events[K]) => void;

async function recordNoCountdown(name = "Standup"): Promise<RecorderState> {
  mock.settings.countdownSeconds = 0;
  const s = await mock.invoke("start_recording", { meetingName: name });
  return s;
}

describe("ipc-mock: recording lifecycle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mock.reset();
  });
  afterEach(() => {
    mock.reset();
    vi.useRealTimers();
  });

  it("start_recording with countdownSeconds=0 goes starting → recording after 150ms", async () => {
    const phases: string[] = [];
    const un = await mock.listen("recorder://state", (s) => phases.push(s.phase));
    const s = await recordNoCountdown();
    expect(s.phase).toBe("starting");
    expect(s.sessionId).toMatch(/^mock-/);
    expect(s.meetingName).toBe("Standup");
    await vi.advanceTimersByTimeAsync(149);
    expect(mock.state.phase).toBe("starting");
    await vi.advanceTimersByTimeAsync(1);
    expect(mock.state.phase).toBe("recording");
    expect(mock.state.segments).toBe(1);
    expect(phases).toEqual(["starting", "recording"]);
    un();
  });

  it("start_recording with a countdown ticks countdown://tick then starts", async () => {
    mock.settings.countdownSeconds = 2;
    const ticks: number[] = [];
    const un = await mock.listen("countdown://tick", ({ remaining }) => ticks.push(remaining));
    const s = await mock.invoke("start_recording", { meetingName: "" });
    expect(s.phase).toBe("countdown");
    await vi.advanceTimersByTimeAsync(2000);
    expect(ticks).toEqual([2, 1, 0]);
    expect(mock.state.phase).toBe("starting");
    await vi.advanceTimersByTimeAsync(150);
    expect(mock.state.phase).toBe("recording");
    un();
  });

  it("progress events arrive every 500ms with totalSize growing at 889 kbps", async () => {
    const events: Progress[] = [];
    const un = await mock.listen("recorder://progress", (p) => events.push(p));
    await recordNoCountdown();
    await vi.advanceTimersByTimeAsync(150);
    await vi.advanceTimersByTimeAsync(MOCK_TICK_MS * 3);
    expect(events).toHaveLength(3);
    const perTick = ((MOCK_KBPS * 1000) / 8) * (MOCK_TICK_MS / 1000); // 55562.5 bytes
    expect(events[0].totalSize).toBe(Math.floor(perTick));
    expect(events[1].totalSize).toBe(Math.floor(perTick * 2));
    expect(events[2].totalSize).toBe(Math.floor(perTick * 3));
    expect(events[2].totalSize).toBeGreaterThan(events[0].totalSize);
    expect(events[2].bitrateKbps).toBe(889);
    expect(events[2].outTimeMs).toBe(1500);
    expect(events[2].finished).toBe(false);
    expect(mock.state.elapsedMs).toBe(1500);
    un();
  });

  it("pause freezes elapsedMs and resume increments segments", async () => {
    await recordNoCountdown();
    await vi.advanceTimersByTimeAsync(150 + MOCK_TICK_MS * 2);
    expect(mock.state.elapsedMs).toBe(1000);
    expect(mock.state.segments).toBe(1);

    const paused = await mock.invoke("pause_recording");
    expect(paused.phase).toBe("paused");
    expect(paused.elapsedMs).toBe(1000);
    await vi.advanceTimersByTimeAsync(MOCK_TICK_MS * 4);
    expect(mock.state.elapsedMs).toBe(1000); // frozen while paused
    expect(mock.state.segments).toBe(1);

    const resumed = await mock.invoke("resume_recording");
    expect(resumed.phase).toBe("recording");
    expect(resumed.segments).toBe(2);
    await vi.advanceTimersByTimeAsync(MOCK_TICK_MS);
    expect(mock.state.elapsedMs).toBe(1500);
  });

  it("pause while not recording throws", async () => {
    await expect(mock.invoke("pause_recording")).rejects.toThrow(/pause_recording while idle/);
    await expect(mock.invoke("stop_recording")).rejects.toThrow(/stop_recording while idle/);
  });

  it("stop returns a RecordingResult with an .mp4 path and the 889 kbps budget", async () => {
    const finished: unknown[] = [];
    const un = await mock.listen("recorder://finished", (r) => finished.push(r));
    await recordNoCountdown("Weekly sync");
    await vi.advanceTimersByTimeAsync(150 + MOCK_TICK_MS * 4);
    const r = await mock.invoke("stop_recording");
    expect(r.path.endsWith(".mp4")).toBe(true);
    expect(r.path.startsWith(mock.settings.outputDir)).toBe(true);
    expect(r.path).toContain("Weekly sync");
    expect(r.budget.totalKbps).toBe(889);
    expect(r.info.durationMs).toBe(2000);
    expect(r.info.bytes).toBeGreaterThan(0);
    expect(r.recovered).toBe(false);
    expect(mock.state.phase).toBe("idle");
    expect(mock.lastResult).toEqual(r);
    expect(finished).toHaveLength(1);
    un();
  });

  it("cancel returns idle and stops the timers", async () => {
    const events: Progress[] = [];
    const un = await mock.listen("recorder://progress", (p) => events.push(p));
    await recordNoCountdown();
    await vi.advanceTimersByTimeAsync(150 + MOCK_TICK_MS);
    const s = await mock.invoke("cancel_recording");
    expect(s.phase).toBe("idle");
    expect(s.sessionId).toBeNull();
    const before = events.length;
    await vi.advanceTimersByTimeAsync(MOCK_TICK_MS * 5);
    expect(events.length).toBe(before);
    un();
  });

  it("pressShortcut('toggle') starts, emits shortcut://toggle, then stops", async () => {
    mock.settings.countdownSeconds = 0;
    let toggles = 0;
    const un = await mock.listen("shortcut://toggle", () => toggles++);
    await mock.pressShortcut("toggle");
    expect(toggles).toBe(1);
    expect(mock.state.phase).toBe("starting");
    expect(mock.callsTo("start_recording")).toEqual([{ meetingName: "" }]);
    await vi.advanceTimersByTimeAsync(150 + MOCK_TICK_MS);
    await mock.pressShortcut("toggle");
    expect(toggles).toBe(2);
    expect(mock.state.phase).toBe("idle");
    expect(mock.callsTo("stop_recording")).toHaveLength(1);
    un();
  });

  it("pressShortcut('pause') toggles pause/resume", async () => {
    await recordNoCountdown();
    await vi.advanceTimersByTimeAsync(150);
    let pauses = 0;
    const un = await mock.listen("shortcut://pause", () => pauses++);
    await mock.pressShortcut("pause");
    expect(mock.state.phase).toBe("paused");
    await mock.pressShortcut("pause");
    expect(mock.state.phase).toBe("recording");
    expect(pauses).toBe(2);
    un();
  });
});

describe("ipc-mock: commands", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mock.reset();
  });
  afterEach(() => {
    mock.reset();
    vi.useRealTimers();
  });

  it("request_permission flips the kind to granted", async () => {
    expect((await mock.invoke("check_permissions")).camera).toBe("notDetermined");
    expect(await mock.invoke("request_permission", { kind: "camera" })).toBe("granted");
    const p = await mock.invoke("check_permissions");
    expect(p.camera).toBe("granted");
    expect(p.screen).toBe("notDetermined");
  });

  it("save_settings persists to localStorage and round-trips through get_settings", async () => {
    const s: Settings = JSON.parse(JSON.stringify(mock.settings));
    s.theme = "dark";
    s.video.fps = 60;
    const saved = await mock.invoke("save_settings", { settings: s });
    expect(saved.theme).toBe("dark");
    expect(saved).not.toBe(s); // cloned
    const raw = localStorage.getItem(MOCK_STORAGE_KEY);
    expect(raw).not.toBeNull();
    expect(JSON.parse(raw!).video.fps).toBe(60);
    expect((await mock.invoke("get_settings")).theme).toBe("dark");
    mock.reset();
    expect(localStorage.getItem(MOCK_STORAGE_KEY)).toBeNull();
  });

  it("compress_to_budget emits compress://progress until done", async () => {
    const events: Events["compress://progress"][] = [];
    const un = await mock.listen("compress://progress", (p) => events.push(p));
    const jobId = await mock.invoke("compress_to_budget", { path: "/tmp/a.mp4", targetMb: 300 });
    expect(jobId).toMatch(/^job-/);
    await vi.advanceTimersByTimeAsync(mock.compressTickMs * 5);
    expect(events.map((e) => e.percent)).toEqual([20, 40, 60, 80, 100]);
    expect(events.slice(0, 4).every((e) => !e.done && e.output === null)).toBe(true);
    const last = events[4];
    expect(last.done).toBe(true);
    expect(last.jobId).toBe(jobId);
    expect(last.output).toBe("/tmp/a (300MB).mp4");
    await vi.advanceTimersByTimeAsync(mock.compressTickMs * 3);
    expect(events).toHaveLength(5);
    un();
  });

  it("listen/unlisten keep accurate listener counts", async () => {
    expect(mock.listenerCount("recorder://warning")).toBe(0);
    const a: Listener<"recorder://warning"> = () => {};
    const b: Listener<"recorder://warning"> = () => {};
    const unA = await mock.listen("recorder://warning", a);
    const unB = await mock.listen("recorder://warning", b);
    expect(mock.listenerCount("recorder://warning")).toBe(2);
    unA();
    expect(mock.listenerCount("recorder://warning")).toBe(1);
    unA(); // idempotent
    expect(mock.listenerCount("recorder://warning")).toBe(1);
    unB();
    expect(mock.listenerCount("recorder://warning")).toBe(0);
  });

  it("emit delivers a cloned payload only to listeners of that event", async () => {
    const got: unknown[] = [];
    const un = await mock.listen("recorder://warning", (w) => got.push(w));
    const payload = { code: "warn.lowDisk", detail: "" };
    mock.emit("recorder://warning", payload);
    mock.emit("recorder://error", { code: "error.io", detail: "" });
    expect(got).toEqual([payload]);
    expect(got[0]).not.toBe(payload);
    un();
  });

  it("records every call and answers unknown commands with an error", async () => {
    await mock.invoke("get_app_info");
    await mock.invoke("list_devices");
    expect(mock.calls.map((c) => c.cmd)).toEqual(["get_app_info", "list_devices"]);
    await expect(mock.invoke("nope" as never)).rejects.toThrow(/unknown command/);
  });

  it("check_disk reports low when free space is under the threshold", async () => {
    const ok = await mock.invoke("check_disk", {});
    expect(ok.low).toBe(false);
    expect(ok.hoursRecordable).toBeCloseTo(120_000_000_000 / 400_000_000);
    mock.disk.freeBytes = 2_000_000_000;
    expect((await mock.invoke("check_disk", { path: "/x" })).low).toBe(true);
  });

  it("recover_session returns a recovered result and removes it from the list", async () => {
    mock.recoverable = [
      { sessionId: "s1", meetingName: "Lost", createdAt: "2026-01-01", segments: 2, estimatedDurationMs: 60_000, bytes: 6_000_000, finalOutput: "/tmp/lost.mp4" },
    ];
    const r = await mock.invoke("recover_session", { sessionId: "s1" });
    expect(r.recovered).toBe(true);
    expect(r.path).toBe("/tmp/lost.mp4");
    expect(await mock.invoke("list_recoverable")).toEqual([]);
    await expect(mock.invoke("recover_session", { sessionId: "s1" })).rejects.toThrow();
  });
});
