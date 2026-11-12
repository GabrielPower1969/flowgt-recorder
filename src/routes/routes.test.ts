import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/svelte";
import { setTransport } from "../lib/ipc";
import { mock } from "../lib/ipc-mock";
import { __setLastResult, __setRecorderState, bindRecorderEvents, unbindRecorderEvents } from "../lib/stores/recorder.svelte";
import { __setSettings } from "../lib/stores/settings.svelte";
import { setPlatform } from "../lib/platform.svelte";
import { setCapabilities } from "../lib/stores/capabilities.svelte";
import { mockCapabilities } from "../lib/ipc-mock";
import { defaultSettings, idleRecorderState } from "../lib/defaults";
import { __resetDrive, bindDriveEvents, refreshDrive, signInToGoogle } from "../lib/stores/drive.svelte";
import { getToasts, clearToasts } from "../lib/stores/toasts.svelte";
import Main from "./Main.svelte";
import History from "./History.svelte";
import Floating from "./Floating.svelte";
import Summary from "./Summary.svelte";
import Settings from "./Settings.svelte";
import ScrollBar from "./ScrollBar.svelte";
import ScrollSelect from "./ScrollSelect.svelte";

beforeEach(() => {
  mock.reset();
  setTransport(mock);
  unbindRecorderEvents();
  const settings = defaultSettings("macos");
  setPlatform("macos");
  setCapabilities(mockCapabilities());
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
  it("the front panel: six keys, the quality switch, four selectors and the sign-in, nothing else to press", async () => {
    render(Main, { settings: defaultSettings("macos") });
    expect(record()).toBeEnabled();
    // The computer audio lever reads "on" once the app knows the machine can record it.
    await screen.findByRole("button", { name: "关闭电脑声音" });
    const names = screen.getAllByRole("button").map((b) => b.getAttribute("aria-label") ?? b.textContent?.trim());
    expect(names.sort()).toEqual(["1080p", "4K", "停止录制", "关闭摄像头", "关闭电脑声音", "关闭麦克风", "录制历史", "录制屏幕", "暂停", "滚动截图", "登录 Google", "设置", "选择要录的屏幕"].sort());
    // Pause and stop are there, but there is nothing to pause or stop yet.
    expect(screen.getByRole("button", { name: "暂停" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止录制" })).toBeDisabled();
  });

  it("the monitor does not ask the system for a picture before screen recording is allowed", async () => {
    render(Main, { settings: defaultSettings("macos") });
    await waitFor(() => expect(mock.callsTo("request_permission").length).toBe(2));
    await new Promise((r) => setTimeout(r, 20));
    expect(mock.callsTo("screen_thumbnail")).toHaveLength(0);
  });

  it("once it is allowed the monitor shows the screen that will be recorded", async () => {
    mock.permissions.screen = "granted";
    render(Main, { settings: defaultSettings("macos") });
    await waitFor(() => expect(mock.callsTo("screen_thumbnail")).toEqual([{ id: "1" }]));
  });

  it("the left reel is the disk: it says how much is free, and warns when that is little", async () => {
    render(Main, { settings: defaultSettings("macos") });
    expect(await screen.findByRole("img", { name: /左盘：磁盘还剩 .*GB/ })).toBeInTheDocument();
    expect(screen.queryByTestId("low-disk")).toBeNull();
  });

  it("with little room left on the disk it says how long that will last", async () => {
    mock.disk = { freeBytes: 1_600_000_000, totalBytes: 1_000_000_000_000 };
    render(Main, { settings: defaultSettings("macos") });
    expect(await screen.findByTestId("low-disk")).toHaveTextContent(/大约还能录 .* 小时/);
  });

  it("the computer audio lever switches it off and on again", async () => {
    render(Main, { settings: defaultSettings("macos") });
    const lever = await screen.findByRole("button", { name: "关闭电脑声音" });
    await waitFor(() => expect(lever).toBeEnabled());
    await fireEvent.click(lever);
    await waitFor(() => expect(mock.settings.audio.systemAudio).toEqual({ kind: "off" }));
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

describe("a window whose buttons the system draws over the page", () => {
  it("keeps a strip clear for them and moves the window from it", () => {
    render(Main, { settings: defaultSettings("macos") });
    expect(screen.getByTestId("titlebar")).toHaveAttribute("data-tauri-drag-region");
  });

  it("a window with its own title bar has no strip", () => {
    setCapabilities({ overlayTitleBar: false });
    render(Main, { settings: defaultSettings("macos") });
    expect(screen.queryByTestId("titlebar")).toBeNull();
  });

  it("the panel numbers a screen the system only names by its device, as the picker does", async () => {
    mock.devices.screens = [{ ...mock.devices.screens[0], name: "Capture screen 0" }];
    render(Main, { settings: defaultSettings("macos") });
    await waitFor(() => expect(screen.getByTitle("屏幕 1")).toBeInTheDocument());
    expect(screen.queryByText(/Capture screen/)).toBeNull();
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
    // One still per screen in the picker (the monitor on the panel asks for its own).
    expect(new Set(mock.callsTo("screen_thumbnail").map((c) => c.id))).toEqual(new Set(["1", "2"]));
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

  it("without window capture there is no window tab, whatever the platform is called", async () => {
    mock.permissions.screen = "granted";
    setCapabilities({ windowCapture: false });
    await openPicker();
    await screen.findAllByRole("radio");
    expect(screen.queryByRole("button", { name: "单个 App 窗口" })).toBeNull();
  });

  it("the window tab follows the capability, not the platform name", async () => {
    mock.permissions.screen = "granted";
    setPlatform("windows");
    await openPicker();
    await screen.findAllByRole("radio");
    expect(screen.getByRole("button", { name: "单个 App 窗口" })).toBeInTheDocument();
  });
});

describe("one screen, nothing to choose", () => {
  it("the red key starts at once, with the name typed on the panel, and no dialog", async () => {
    mock.permissions.screen = "granted";
    mock.devices.screens = mock.devices.screens.slice(0, 1);
    setCapabilities({ windowCapture: false });
    render(Main, { settings: defaultSettings("macos") });
    await waitFor(() => expect(mock.callsTo("screen_thumbnail")).toHaveLength(1));
    await fireEvent.input(screen.getByTestId("home-name"), { target: { value: "周会" } });
    await fireEvent.click(record());
    await waitFor(() => expect(mock.callsTo("start_recording")).toEqual([{ meetingName: "周会" }]));
    expect(mock.settings.capture).toEqual({ kind: "display", displayId: "1" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("without the screen permission the dialog still opens, because it explains what to do", async () => {
    mock.devices.screens = mock.devices.screens.slice(0, 1);
    setCapabilities({ windowCapture: false });
    render(Main, { settings: defaultSettings("macos") });
    await waitFor(() => expect(mock.callsTo("list_devices").length).toBeGreaterThan(0));
    await fireEvent.click(record());
    expect(await screen.findByTestId("perm")).toBeInTheDocument();
    expect(mock.callsTo("start_recording")).toHaveLength(0);
  });

  it("the name typed on the panel is already in the dialog when there is a choice to make", async () => {
    mock.permissions.screen = "granted";
    render(Main, { settings: defaultSettings("macos") });
    await fireEvent.input(screen.getByTestId("home-name"), { target: { value: "面试" } });
    await fireEvent.click(record());
    expect(((await screen.findByTestId("name")) as HTMLInputElement).value).toBe("面试");
  });
});

describe("recording bar", () => {
  const recording = { ...idleRecorderState(), phase: "recording" as const, elapsedMs: 83_000, segments: 1 };

  it("without a live preview there is only the bar: no empty panel, nothing to expand", async () => {
    setCapabilities({ livePreview: false });
    __setRecorderState(recording);
    render(Floating);
    expect(screen.getByTestId("timer")).toBeInTheDocument();
    expect(screen.queryByTestId("target")).toBeNull();
    expect(screen.queryByRole("button", { name: "收起画面" })).toBeNull();
    await new Promise((r) => setTimeout(r, 200));
    expect(mock.callsTo("preview_frame")).toHaveLength(0);
  });

  it("shows status, time, pause, stop and the preview toggle, nothing else", () => {
    __setRecorderState(recording);
    render(Floating);
    expect(screen.getByText("正在录制")).toBeInTheDocument();
    expect(screen.getByTestId("timer")).toHaveTextContent("00:01:23");
    expect(screen.getByTestId("size")).toHaveTextContent(/\d/);
    expect(screen.getAllByRole("button").map((b) => b.textContent?.trim() || b.getAttribute("aria-label"))).toEqual(["暂停", "停止录制", "收起画面"]);
  });

  it("names the window being recorded and waits for the first frame", () => {
    __setRecorderState({ ...recording, target: { kind: "window", screen: null, app: "Keynote", title: "季度回顾", icon: null }, cameraConnected: true });
    render(Floating);
    expect(screen.getByTestId("target")).toHaveTextContent("Keynote — 季度回顾");
    expect(screen.getByText("带摄像头")).toBeInTheDocument();
    expect(screen.getByText("等待第一帧…")).toBeInTheDocument();
  });

  it("says so when the recorded window has left the screen", () => {
    __setRecorderState({ ...recording, target: { kind: "window", screen: null, app: "Keynote", title: "", icon: null }, warnings: [{ code: "warn.windowHidden", detail: "" }] });
    render(Floating);
    expect(screen.getByRole("status")).toHaveTextContent("被录的窗口已最小化");
  });

  it("names the screen being recorded", () => {
    __setRecorderState({ ...recording, target: { kind: "screen", screen: 2, app: null, title: null, icon: null } });
    render(Floating);
    expect(screen.getByTestId("target")).toHaveTextContent("屏幕 2");
  });

  it("collapsing hides the preview and tells the backend", async () => {
    const spy = vi.spyOn(mock, "invoke");
    __setRecorderState({ ...recording, target: { kind: "screen", screen: 1, app: null, title: null, icon: null } });
    render(Floating);
    await fireEvent.click(screen.getByRole("button", { name: "收起画面" }));
    expect(screen.queryByTestId("target")).toBeNull();
    expect(spy).toHaveBeenCalledWith("set_floating_expanded", { expanded: false });
    spy.mockRestore();
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

describe("quality", () => {
  it("the switch on the plate stores 4K and says what the file will be", async () => {
    render(Main, { settings: defaultSettings("macos") });
    const cap = await screen.findByTestId("quality-cap");
    await waitFor(() => expect(cap.textContent).toContain("1920 × 1080"));
    expect(screen.getByRole("button", { name: "1080p" })).toHaveAttribute("aria-pressed", "true");
    await fireEvent.click(screen.getByRole("button", { name: "4K" }));
    await waitFor(() => expect(mock.settings.video.maxWidth).toBe(3840));
    // The budget that was fine for 1080p would leave 3840 × 2160 soft: it is raised, and said.
    expect(mock.settings.video.targetMbPerHour).toBe(1150);
    expect(getToasts().some((x) => x.text.includes("1150"))).toBe(true);
  });

  it("4K is not offered for a screen that has no more pixels than 1440p", async () => {
    mock.devices.screens = [{ ...mock.devices.screens[0], width: 2560, height: 1440 }];
    render(Main, { settings: defaultSettings("macos") });
    await waitFor(() => expect(screen.getByRole("button", { name: "4K" })).toBeDisabled());
    expect(screen.getByRole("button", { name: "1080p" })).toBeEnabled();
  });

  it("the settings offer five steps and warn when the budget is too small for the picture", async () => {
    const s = defaultSettings("macos");
    s.video.maxWidth = 3840;
    __setSettings(s);
    mock.settings = s;
    render(Settings);
    await fireEvent.click(screen.getByRole("button", { name: "视频" }));
    const group = screen.getByRole("group", { name: "清晰度" });
    expect(within(group).getAllByRole("button").map((b) => b.textContent)).toEqual(["720p", "1080p", "1440p", "4K", "原始"]);
    await waitFor(() => expect(screen.getByTestId("quality-out").textContent).toContain("3840 × 2160"));
    expect(screen.getByTestId("budget-low")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /调到/ }));
    await waitFor(() => expect(mock.settings.video.targetMbPerHour).toBeGreaterThanOrEqual(1000));
  });
});

describe("start at login", () => {
  it("the switch asks the system and shows what it answered", async () => {
    render(Settings);
    const box = await screen.findByTestId("autostart");
    expect(box).not.toBeChecked();
    await fireEvent.click(box);
    await waitFor(() => expect(mock.autostart).toBe("on"));
    expect(mock.callsTo("autostart_set")[0]).toEqual({ enabled: true });
  });

  it("says so when the system still wants the user's approval", async () => {
    mock.autostart = "needsApproval";
    render(Settings);
    expect(await screen.findByTestId("autostart-approval")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "打开登录项设置" }));
    expect(mock.callsTo("open_login_items_settings")).toHaveLength(1);
  });

  it("is not shown where the platform cannot do it", async () => {
    setCapabilities({ autostart: false });
    render(Settings);
    await screen.findByText("保存位置");
    expect(screen.queryByTestId("autostart")).toBeNull();
  });
});

describe("shortcuts", () => {
  it("lists four, and says which one another program already holds", async () => {
    mock.takenShortcuts.add("cmdorctrl+shift+r");
    render(Settings);
    await fireEvent.click(screen.getByRole("button", { name: "快捷键" }));
    for (const name of ["开始 / 停止", "暂停 / 继续", "滚动截图", "显示／隐藏主窗口"]) expect(screen.getByText(name)).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByTestId("sc-problem")).toHaveLength(1));
    expect(screen.getByTestId("sc-problem").textContent).toContain("占用");
  });

  it("an optional one can be cleared, and reset brings the defaults back", async () => {
    render(Settings);
    await fireEvent.click(screen.getByRole("button", { name: "快捷键" }));
    await fireEvent.click(screen.getByRole("button", { name: "清除" }));
    await waitFor(() => expect(mock.settings.shortcuts.scrollCapture).toBe(""));
    await fireEvent.click(screen.getByTestId("shortcuts-reset"));
    await waitFor(() => expect(mock.settings.shortcuts.scrollCapture).toBe("Alt+Shift+S"));
  });
});

describe("scrolling screenshot", () => {
  it("the key starts choosing the area; the saved picture comes back as a card", async () => {
    render(Main, { settings: defaultSettings("macos") });
    await fireEvent.click(screen.getByRole("button", { name: "滚动截图" }));
    await waitFor(() => expect(mock.scroll).toBe("selecting"));
    await mock.invoke("scroll_capture_start", { rect: { x: 0, y: 0, w: 400, h: 300 } });
    await mock.invoke("scroll_capture_finish");
    const card = await screen.findByTestId("scroll-result");
    expect(card.textContent).toContain("800 × 4200");
    expect(card.textContent).toContain("已放进剪贴板");
  });

  it("the key is not there where the platform cannot do it", () => {
    setCapabilities({ scrollCapture: false });
    render(Main, { settings: defaultSettings("macos") });
    expect(screen.queryByRole("button", { name: "滚动截图" })).toBeNull();
  });

  it("the bar shows the height so far, a warning when frames do not join, and saves on Done", async () => {
    render(ScrollBar);
    expect(screen.getByTestId("scroll-count").textContent).toContain("现在滚动页面");
    await new Promise((r) => setTimeout(r, 0));
    mock.emit("scroll://progress", { width: 800, height: 2400, frames: 7, thumbnail: null, newestAt: 0.8 });
    await waitFor(() => expect(screen.getByTestId("scroll-count").textContent).toContain("2400"));
    mock.emit("scroll://warning", "noOverlap");
    expect((await screen.findByTestId("scroll-warning")).textContent).toContain("往回滚");
    mock.emit("scroll://warning", null);
    await waitFor(() => expect(screen.queryByTestId("scroll-warning")).toBeNull());
    await fireEvent.click(screen.getByTestId("scroll-done"));
    await waitFor(() => expect(mock.callsTo("scroll_capture_finish")).toHaveLength(1));
  });

  it("choosing: a drag gives the rectangle, a tiny one is refused, Esc backs out", async () => {
    render(ScrollSelect);
    const veil = screen.getByTestId("scroll-select");
    veil.setPointerCapture = () => {};
    const drag = async (x1: number, y1: number, x2: number, y2: number) => {
      await fireEvent.pointerDown(veil, { button: 0, clientX: x1, clientY: y1, pointerId: 1 });
      await fireEvent.pointerMove(veil, { clientX: x2, clientY: y2, pointerId: 1 });
      await fireEvent.pointerUp(veil, { pointerId: 1 });
    };
    await drag(10, 10, 30, 30);
    expect(mock.callsTo("scroll_capture_start")).toHaveLength(0);
    await drag(100, 120, 500, 420);
    await waitFor(() => expect(mock.callsTo("scroll_capture_start")).toHaveLength(1));
    expect(mock.callsTo("scroll_capture_start")[0]).toEqual({ rect: { x: 100, y: 120, w: 400, h: 300 } });
    await fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(mock.callsTo("scroll_capture_cancel")).toHaveLength(1));
  });
});
