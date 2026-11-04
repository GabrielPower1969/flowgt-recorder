import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import { setTransport } from "../lib/ipc";
import { mock } from "../lib/ipc-mock";
import { __setLastResult, __setRecorderState, bindRecorderEvents, unbindRecorderEvents } from "../lib/stores/recorder.svelte";
import { __setSettings } from "../lib/stores/settings.svelte";
import { defaultSettings, idleRecorderState } from "../lib/defaults";
import { __resetDrive, bindDriveEvents, refreshDrive, signInToGoogle } from "../lib/stores/drive.svelte";
import { getToasts, clearToasts } from "../lib/stores/toasts.svelte";
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
  __resetDrive();
  clearToasts();
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
    expect(screen.getAllByRole("button")).toHaveLength(5);
    expect(screen.getByRole("button", { name: "登录 Google" })).toBeInTheDocument();
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

describe("device pickers", () => {
  it("default to the system's current camera and microphone, by name", async () => {
    render(Main, { settings: defaultSettings("macos") });
    expect(await screen.findByRole("option", { name: "跟随系统（FaceTime HD Camera）" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "跟随系统（MacBook Pro Microphone）" })).toBeInTheDocument();
    expect((screen.getByLabelText("摄像头") as HTMLSelectElement).value).toBe("");
    expect((screen.getByLabelText("麦克风") as HTMLSelectElement).value).toBe("");
  });

  it("choosing a device stores its name, not its index", async () => {
    render(Main, { settings: defaultSettings("macos") });
    await screen.findByRole("option", { name: "AirPods Pro" });
    await fireEvent.change(screen.getByLabelText("麦克风"), { target: { value: "AirPods Pro" } });
    await waitFor(() => expect(mock.settings.audio.micDeviceId).toBe("AirPods Pro"));
  });

  it("computer audio follows the system when the machine can record it, and can be turned off", async () => {
    render(Main, { settings: defaultSettings("macos") });
    const output = screen.getByLabelText("电脑声音") as HTMLSelectElement;
    await waitFor(() => expect(output).toBeEnabled());
    expect(output.value).toBe("auto");
    await fireEvent.change(output, { target: { value: "off" } });
    await waitFor(() => expect(mock.settings.audio.systemAudio).toEqual({ kind: "off" }));
  });

  it("without any way to record computer audio it says so instead of pretending", async () => {
    mock.devices.loopbacks = [];
    mock.appInfo.systemAudioAvailable = false;
    render(Main, { settings: defaultSettings("macos") });
    const output = screen.getByLabelText("电脑声音") as HTMLSelectElement;
    await waitFor(() => expect(mock.callsTo("get_app_info").length).toBe(1));
    expect(output).toBeDisabled();
    expect(output.value).toBe("off");
  });
});

describe("google drive", () => {
  it("signing in shows the account and where recordings go", async () => {
    await refreshDrive();
    render(Main, { settings: defaultSettings("macos") });
    await fireEvent.click(screen.getByRole("button", { name: "登录 Google" }));
    expect(await screen.findByTestId("account")).toHaveTextContent("demo@gmail.com");
    expect(screen.getByTestId("drive-line")).toHaveTextContent("FlowGT Recorder");
  });

  it("a build without a client tells the user instead of opening nothing", async () => {
    mock.drive = { configured: false, email: null };
    await refreshDrive();
    expect(await signInToGoogle()).toBe(false);
    expect(mock.callsTo("drive_sign_in")).toHaveLength(0);
    expect(getToasts().at(-1)?.text).toBe("这个版本还没配置 Google 登录");
  });

  it("the summary shows upload progress, then the link", async () => {
    bindDriveEvents();
    await new Promise((r) => setTimeout(r, 0));
    const path = "/a/b.mp4";
    __setLastResult({ sessionId: "s", path, info: { path, bytes: 1, durationMs: 1000, width: 1, height: 1, fps: 30, videoCodec: "hevc", audioCodec: "aac", audioTracks: 1, bitrateKbps: 1 }, budget: { totalKbps: 889, videoKbps: 793, audioKbpsTotal: 96, maxrateKbps: 1190, bufsizeKbps: 1586, expectedMbPerHour: 400 }, overBudgetRatio: 0, recovered: false });
    render(Summary);
    mock.emit("drive://progress", { path, percent: 42, done: false, url: null, error: null });
    expect(await screen.findByRole("status")).toHaveTextContent("42%");
    mock.emit("drive://progress", { path, percent: 100, done: true, url: "https://drive.google.com/file/d/x/view", error: null });
    await fireEvent.click(await screen.findByRole("button", { name: "在 Google Drive 中打开" }));
    await waitFor(() => expect(mock.callsTo("open_drive_link")).toEqual([{ url: "https://drive.google.com/file/d/x/view" }]));
  });

  it("a failed upload can be retried", async () => {
    bindDriveEvents();
    await new Promise((r) => setTimeout(r, 0));
    mock.drive = { configured: true, email: "demo@gmail.com" };
    await refreshDrive();
    const path = "/a/c.mp4";
    __setLastResult({ sessionId: "s", path, info: { path, bytes: 1, durationMs: 1000, width: 1, height: 1, fps: 30, videoCodec: "hevc", audioCodec: "aac", audioTracks: 1, bitrateKbps: 1 }, budget: { totalKbps: 889, videoKbps: 793, audioKbpsTotal: 96, maxrateKbps: 1190, bufsizeKbps: 1586, expectedMbPerHour: 400 }, overBudgetRatio: 0, recovered: false });
    render(Summary);
    mock.emit("drive://progress", { path, percent: 0, done: true, url: null, error: "boom" });
    await fireEvent.click(await screen.findByRole("button", { name: "重试" }));
    await waitFor(() => expect(mock.callsTo("drive_upload")).toEqual([{ path }]));
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
