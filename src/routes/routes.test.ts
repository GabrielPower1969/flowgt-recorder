import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/svelte";
import { setTransport } from "../lib/ipc";
import { mock } from "../lib/ipc-mock";
import { __setLastResult, __setRecorderState, bindRecorderEvents, unbindRecorderEvents } from "../lib/stores/recorder.svelte";
import { __setSettings } from "../lib/stores/settings.svelte";
import { setPlatform } from "../lib/platform.svelte";
import { defaultSettings, idleRecorderState } from "../lib/defaults";
import { __resetDrive, bindDriveEvents, refreshDrive, signInToGoogle } from "../lib/stores/drive.svelte";
import { getToasts, clearToasts } from "../lib/stores/toasts.svelte";
import Main from "./Main.svelte";
import History from "./History.svelte";
import Floating from "./Floating.svelte";
import Summary from "./Summary.svelte";

beforeEach(() => {
  mock.reset();
  setTransport(mock);
  unbindRecorderEvents();
  const settings = defaultSettings("macos");
  setPlatform("macos");
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
  it("offers exactly one way forward: record, plus mic, camera, history and settings", async () => {
    render(Main, { settings: defaultSettings("macos") });
    expect(record()).toBeEnabled();
    expect(screen.getAllByRole("button")).toHaveLength(6);
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

describe("which screen will be recorded", () => {
  it("the picker outlines the chosen screen and removes the outline on cancel", async () => {
    mock.permissions.screen = "granted";
    const dialog = await openPicker();
    await waitFor(() => expect(mock.callsTo("show_screen_marker").length).toBeGreaterThan(0));
    expect(await screen.findByText("将录制")).toBeInTheDocument();
    await fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    await waitFor(() => expect(mock.callsTo("hide_screen_marker")).toHaveLength(1));
  });
});

describe("history", () => {
  it("lists recordings newest first and offers what each one can do", async () => {
    mock.history = { driveError: null, entries: [
      { name: "old.mp4", path: "/m/old.mp4", bytes: 10, modifiedMs: 1000, driveUrl: null, driveCreated: null },
      { name: "new.mp4", path: null, bytes: 20, modifiedMs: null, driveUrl: "https://drive.google.com/file/d/n/view", driveCreated: "2026-11-05T01:00:00Z" },
    ] };
    render(History);
    const rows = await screen.findAllByTestId("history-row");
    expect(rows[0]).toHaveTextContent("new.mp4");
    expect(within(rows[0]).queryByRole("button", { name: "播放" })).toBeNull();
    await fireEvent.click(within(rows[0]).getByRole("button", { name: "在 Google Drive 中打开" }));
    await waitFor(() => expect(mock.callsTo("open_drive_link")).toHaveLength(1));
    expect(within(rows[1]).getByRole("button", { name: "播放" })).toBeInTheDocument();
  });

  it("a Drive failure still shows the local list and says why", async () => {
    mock.history = { driveError: "boom", entries: [{ name: "a.mp4", path: "/m/a.mp4", bytes: 1, modifiedMs: 1, driveUrl: null, driveCreated: null }] };
    render(History);
    expect(await screen.findByTestId("history-error")).toHaveTextContent("boom");
    expect(screen.getAllByTestId("history-row")).toHaveLength(1);
  });
});

describe("bluetooth microphones", () => {
  it("warns only when the mic in use is a Bluetooth headset", async () => {
    const base = defaultSettings("macos");
    const air = render(Main, { settings: { ...base, audio: { ...base.audio, micDeviceId: "AirPods Pro" } } });
    expect(await screen.findByTestId("bt-mic")).toBeInTheDocument();
    air.unmount();
    render(Main, { settings: base });
    await waitFor(() => expect(mock.callsTo("list_devices").length).toBeGreaterThan(1));
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByTestId("bt-mic")).toBeNull();
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

  it("an index stored by an older version shows as that device, and an unplugged one as the default", async () => {
    const settings = defaultSettings("macos");
    settings.camera.deviceId = "0";
    settings.audio.micDeviceId = "Unplugged Mic";
    render(Main, { settings });
    await screen.findByRole("option", { name: "FaceTime HD Camera" });
    await waitFor(() => expect((screen.getByLabelText("摄像头") as HTMLSelectElement).value).toBe("FaceTime HD Camera"));
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
    mock.emit("drive://progress", { path, percent: 42, done: false, url: null, error: null, localRemoved: false });
    expect(await screen.findByRole("status")).toHaveTextContent("42%");
    mock.emit("drive://progress", { path, percent: 100, done: true, url: "https://drive.google.com/file/d/x/view", error: null, localRemoved: false });
    await fireEvent.click(await screen.findByRole("button", { name: "在 Google Drive 中打开" }));
    await waitFor(() => expect(mock.callsTo("open_drive_link")).toEqual([{ url: "https://drive.google.com/file/d/x/view" }]));
  });

  it("saving only to Drive: the summary drops the preview and says where the recording is", async () => {
    bindDriveEvents();
    await new Promise((r) => setTimeout(r, 0));
    const path = "/a/d.mp4";
    __setLastResult({ sessionId: "s", path, info: { path, bytes: 1, durationMs: 1000, width: 1, height: 1, fps: 30, videoCodec: "hevc", audioCodec: "aac", audioTracks: 1, bitrateKbps: 1 }, budget: { totalKbps: 889, videoKbps: 793, audioKbpsTotal: 96, maxrateKbps: 1190, bufsizeKbps: 1586, expectedMbPerHour: 400 }, overBudgetRatio: 0, recovered: false });
    render(Summary);
    mock.emit("drive://progress", { path, percent: 100, done: true, url: "https://drive.google.com/file/d/x/view", error: null, localRemoved: true });
    expect(await screen.findByTestId("drive-only")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Finder|访达|文件夹/ })).toBeNull();
  });

  it("the home page offers local, Drive or both, and never neither", async () => {
    mock.drive = { configured: true, email: "demo@gmail.com" };
    await refreshDrive();
    const lastSaved = () => mock.callsTo("save_settings").at(-1)?.settings;
    const first = render(Main, { settings: defaultSettings("macos") });
    const local = (await screen.findByRole("checkbox", { name: "本机" })) as HTMLInputElement;
    expect(local.checked && (screen.getByRole("checkbox", { name: "Google Drive" }) as HTMLInputElement).checked).toBe(true);
    await fireEvent.click(local);
    await waitFor(() => expect(lastSaved()?.saveLocal).toBe(false));
    first.unmount();

    const second = render(Main, { settings: { ...defaultSettings("macos"), saveLocal: false } });
    expect(await screen.findByTestId("drive-line")).toHaveTextContent("只存");
    await fireEvent.click(screen.getByRole("checkbox", { name: "Google Drive" }));
    await waitFor(() => expect(lastSaved()).toMatchObject({ saveDrive: false, saveLocal: true }));
    second.unmount();

    render(Main, { settings: { ...defaultSettings("macos"), saveDrive: false } });
    expect(await screen.findByRole("checkbox", { name: "本机" })).toBeDisabled();
  });

  it("a failed upload can be retried", async () => {
    bindDriveEvents();
    await new Promise((r) => setTimeout(r, 0));
    mock.drive = { configured: true, email: "demo@gmail.com" };
    await refreshDrive();
    const path = "/a/c.mp4";
    __setLastResult({ sessionId: "s", path, info: { path, bytes: 1, durationMs: 1000, width: 1, height: 1, fps: 30, videoCodec: "hevc", audioCodec: "aac", audioTracks: 1, bitrateKbps: 1 }, budget: { totalKbps: 889, videoKbps: 793, audioKbpsTotal: 96, maxrateKbps: 1190, bufsizeKbps: 1586, expectedMbPerHour: 400 }, overBudgetRatio: 0, recovered: false });
    render(Summary);
    mock.emit("drive://progress", { path, percent: 0, done: true, url: null, error: "boom", localRemoved: false });
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

  it("the window tab lists the windows by app, drops the outline, and records the chosen one", async () => {
    mock.permissions.screen = "granted";
    await openPicker();
    await screen.findAllByRole("radio");
    await fireEvent.click(screen.getByRole("button", { name: "单个 App 窗口" }));
    await waitFor(() => expect(mock.callsTo("list_windows")).toHaveLength(1));
    const radios = await screen.findAllByRole("radio");
    expect(radios).toHaveLength(3);
    expect(screen.getByText("zoom.us")).toBeInTheDocument();
    expect(screen.getByText("Weekly sync - Zoom")).toBeInTheDocument();
    expect(screen.getByText("（无标题）")).toBeInTheDocument();
    expect(screen.queryByTestId("marker-hint")).toBeNull();
    await waitFor(() => expect(mock.callsTo("hide_screen_marker").length).toBeGreaterThan(0));
    await waitFor(() => expect(mock.callsTo("window_thumbnail")).toHaveLength(3));
    await fireEvent.click(radios[1]);
    await fireEvent.click(screen.getByRole("button", { name: "开始录制" }));
    await waitFor(() => expect(mock.callsTo("start_recording")).toHaveLength(1));
    expect(mock.settings.capture).toEqual({ kind: "window", windowId: "102" });
  });

  it("with no window on screen the window tab says so and cannot start", async () => {
    mock.permissions.screen = "granted";
    mock.windows = [];
    await openPicker();
    await screen.findAllByRole("radio");
    await fireEvent.click(screen.getByRole("button", { name: "单个 App 窗口" }));
    expect(await screen.findByTestId("no-windows")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始录制" })).toBeDisabled();
  });

  it("on Windows there is no window tab", async () => {
    mock.permissions.screen = "granted";
    setPlatform("windows");
    await openPicker();
    await screen.findAllByRole("radio");
    expect(screen.queryByRole("button", { name: "单个 App 窗口" })).toBeNull();
    setPlatform("macos");
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
