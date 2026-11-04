import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import { setTransport } from "../lib/ipc";
import { mock } from "../lib/ipc-mock";
import { __setLastResult, __setRecorderState, bindRecorderEvents, unbindRecorderEvents } from "../lib/stores/recorder.svelte";
import { __setSettings } from "../lib/stores/settings.svelte";
import { defaultSettings, idleRecorderState } from "../lib/defaults";
import Main from "./Main.svelte";
import Floating from "./Floating.svelte";
import Summary from "./Summary.svelte";

beforeEach(() => {
  mock.reset();
  setTransport(mock);
  unbindRecorderEvents();
  const settings = defaultSettings("macos");
  __setSettings(settings);
  __setRecorderState(idleRecorderState());
  __setLastResult(null);
});

const record = () => screen.getByRole("button", { name: "录制屏幕" });
const openPicker = async () => {
  render(Main, { settings: defaultSettings("macos") });
  await fireEvent.click(record());
  return screen.findByRole("dialog");
};

describe("home", () => {
  it("offers exactly one way forward: record, plus mic, camera and settings", async () => {
    render(Main, { settings: defaultSettings("macos") });
    expect(record()).toBeEnabled();
    expect(screen.getAllByRole("button")).toHaveLength(4);
    expect(screen.getByRole("button", { name: "关闭麦克风" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭摄像头" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设置" })).toBeInTheDocument();
  });

  it("asks for camera and microphone on load, the way a meeting page does", async () => {
    render(Main, { settings: defaultSettings("macos") });
    await waitFor(() => expect(mock.callsTo("request_permission")).toEqual([{ kind: "camera" }, { kind: "microphone" }]));
  });

  it("never asks for screen recording before the user wants to record", async () => {
    render(Main, { settings: defaultSettings("macos") });
    await waitFor(() => expect(mock.callsTo("request_permission").length).toBe(2));
    expect(mock.callsTo("request_permission")).not.toContainEqual({ kind: "screen" });
  });

  it("the mic toggle saves the setting", async () => {
    render(Main, { settings: defaultSettings("macos") });
    await fireEvent.click(screen.getByRole("button", { name: "关闭麦克风" }));
    await waitFor(() => expect(mock.settings.audio.micEnabled).toBe(false));
  });
});

describe("screen picker", () => {
  it("without permission it explains once and offers system settings; nothing can start", async () => {
    await openPicker();
    expect(await screen.findByTestId("perm")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始录制" })).toBeDisabled();
  });

  it("after the permission is granted it shows the screens with one preselected", async () => {
    mock.permissions.screen = "granted";
    await openPicker();
    const radios = await screen.findAllByRole("radio");
    expect(radios).toHaveLength(2);
    expect(radios.filter((r) => r.getAttribute("aria-checked") === "true")).toHaveLength(1);
    expect(screen.getByText("屏幕 1")).toBeInTheDocument();
    expect(mock.callsTo("screen_thumbnail")).toHaveLength(2);
  });

  it("start records the chosen screen and closes", async () => {
    mock.permissions.screen = "granted";
    await openPicker();
    const radios = await screen.findAllByRole("radio");
    await fireEvent.click(radios[1]);
    await fireEvent.input(screen.getByTestId("name"), { target: { value: "周会" } });
    await fireEvent.click(screen.getByRole("button", { name: "开始录制" }));
    await waitFor(() => expect(mock.callsTo("start_recording")).toEqual([{ meetingName: "周会" }]));
    expect(mock.settings.capture).toEqual({ kind: "display", displayId: "2" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("cancel closes without recording", async () => {
    mock.permissions.screen = "granted";
    await openPicker();
    await fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(mock.callsTo("start_recording")).toHaveLength(0);
  });

  it("computer audio is unavailable without a loopback device, and says why", async () => {
    mock.permissions.screen = "granted";
    mock.devices.loopbacks = [];
    await openPicker();
    await screen.findAllByRole("radio");
    expect(screen.getByRole("checkbox")).toBeDisabled();
    expect(screen.getByText(/BlackHole/)).toBeInTheDocument();
  });

  it("computer audio can be switched off when a loopback device exists", async () => {
    mock.permissions.screen = "granted";
    await openPicker();
    await waitFor(() => expect(screen.getByRole("checkbox")).toBeEnabled());
    expect(screen.getByRole("checkbox")).toBeChecked();
    await fireEvent.click(screen.getByRole("checkbox"));
    await waitFor(() => expect(mock.settings.audio.systemAudio).toEqual({ kind: "off" }));
  });
});

describe("recording bar", () => {
  const recording = { ...idleRecorderState(), phase: "recording" as const, elapsedMs: 83_000, segments: 1 };

  it("shows status, time, pause and stop and nothing else", () => {
    __setRecorderState(recording);
    render(Floating);
    expect(screen.getByText("正在录制")).toBeInTheDocument();
    expect(screen.getByTestId("timer")).toHaveTextContent("00:01:23");
    expect(screen.getAllByRole("button").map((b) => b.textContent?.trim())).toEqual(["暂停", "停止录制"]);
  });

  it("while paused the button says resume", () => {
    __setRecorderState({ ...recording, phase: "paused" });
    render(Floating);
    expect(screen.getByText("已暂停")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "继续" })).toBeEnabled();
  });

  it("during the countdown only cancel is offered", () => {
    __setRecorderState({ ...recording, phase: "countdown" });
    render(Floating);
    expect(screen.getByTestId("countdown")).toHaveTextContent("3");
    expect(screen.getAllByRole("button").map((b) => b.textContent?.trim())).toEqual(["取消"]);
  });
});

describe("summary", () => {
  const result = (ratio: number) => ({
    sessionId: "s",
    path: "/Users/demo/Movies/FlowGT Recorder/2026-11-04 1430 周会.mp4",
    info: { path: "x", bytes: 13_300_000, durationMs: 120_000, width: 1920, height: 1200, fps: 30, videoCodec: "hevc", audioCodec: "aac", audioTracks: 1, bitrateKbps: 889 },
    budget: { totalKbps: 889, videoKbps: 793, audioKbpsTotal: 96, maxrateKbps: 1190, bufsizeKbps: 1586, expectedMbPerHour: 400 },
    overBudgetRatio: ratio,
    recovered: false,
  });

  it("shows the file name and the three facts that matter", () => {
    __setLastResult(result(0));
    render(Summary);
    expect(screen.getByText("录像已保存")).toBeInTheDocument();
    expect(screen.getByTestId("path")).toHaveTextContent("2026-11-04 1430 周会.mp4");
    expect(screen.getByTestId("facts")).toHaveTextContent("00:02:00 · 13.3 MB · 1920×1200");
    expect(screen.queryByRole("button", { name: /压到/ })).toBeNull();
  });

  it("offers compression only when over budget", () => {
    __setLastResult(result(0.5));
    render(Summary);
    expect(screen.getByRole("button", { name: /压到/ })).toBeInTheDocument();
  });

  it("finishing a recording through events reaches the store", async () => {
    let finished = false;
    bindRecorderEvents(() => (finished = true));
    await new Promise((r) => setTimeout(r, 0));
    mock.emit("recorder://finished", result(0));
    expect(finished).toBe(true);
  });
});
