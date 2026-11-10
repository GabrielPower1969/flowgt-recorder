import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import LevelMeter from "./components/LevelMeter.svelte";
import Segmented from "./components/Segmented.svelte";
import Toast from "./components/Toast.svelte";
import StagePreview from "./components/StagePreview.svelte";
import ShortcutField from "./components/ShortcutField.svelte";
import { setLanguage } from "./stores/i18n.svelte";
import { setPlatform } from "./platform.svelte";
import { clearToasts, toast } from "./stores/toasts.svelte";
import { defaultSettings } from "./defaults";
import type { CameraSettings } from "./types";
import en from "./i18n/en.json";

beforeAll(() => {
  setLanguage("en");
  setPlatform("macos");
});
afterEach(() => cleanup());

describe("LevelMeter", () => {
  it("renders a meter with aria-valuenow clamped to 0..100", () => {
    const { rerender } = render(LevelMeter, { level: 0.5 });
    const meter = screen.getByRole("meter");
    expect(meter).toHaveAttribute("aria-valuenow", "50");
    expect(meter).toHaveAttribute("aria-valuemin", "0");
    expect(meter).toHaveAttribute("aria-valuemax", "100");
    expect(meter).toHaveAttribute("aria-label", en["a11y.meter"]);
    cleanup();
    render(LevelMeter, { level: -3 });
    expect(screen.getByRole("meter")).toHaveAttribute("aria-valuenow", "0");
    cleanup();
    render(LevelMeter, { level: 7 });
    expect(screen.getByRole("meter")).toHaveAttribute("aria-valuenow", "100");
    void rerender;
  });

  it("switches the fill to the hot zone above 0.85", () => {
    const { container } = render(LevelMeter, { level: 0.9 });
    const fill = container.querySelector(".fill") as HTMLElement;
    expect(fill.classList.contains("hot")).toBe(true);
    expect(fill.style.width).toBe("90%");
    cleanup();
    const low = render(LevelMeter, { level: 0.85 }).container.querySelector(".fill") as HTMLElement;
    expect(low.classList.contains("hot")).toBe(false);
    expect(low.classList.contains("ok")).toBe(true);
  });

  it("compact adds the compact class", () => {
    const { container } = render(LevelMeter, { level: 0, compact: true });
    expect(container.querySelector(".meter")!.classList.contains("compact")).toBe(true);
  });
});

describe("Segmented", () => {
  const options = [
    { value: "a", label: "Alpha" },
    { value: "b", label: "Beta" },
    { value: "c", label: "Gamma", disabled: true, hint: "nope" },
  ];

  it("marks the current value with aria-pressed", () => {
    render(Segmented, { options, value: "b", onchange: () => {}, label: "Pick" });
    expect(screen.getByRole("group", { name: "Pick" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Alpha" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: "Beta" })).toHaveAttribute("aria-pressed", "true");
  });

  it("calls onchange with the clicked value and updates aria-pressed on rerender", async () => {
    const onchange = vi.fn();
    const { rerender } = render(Segmented, { options, value: "a", onchange, label: "Pick" });
    await fireEvent.click(screen.getByRole("button", { name: "Beta" }));
    expect(onchange).toHaveBeenCalledWith("b");
    await rerender({ options, value: "b", onchange, label: "Pick" });
    expect(screen.getByRole("button", { name: "Beta" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Alpha" })).toHaveAttribute("aria-pressed", "false");
  });

  it("a disabled option is not clickable and shows its hint", async () => {
    const onchange = vi.fn();
    render(Segmented, { options, value: "a", onchange, label: "Pick" });
    const gamma = screen.getByRole("button", { name: "Gamma" });
    expect(gamma).toBeDisabled();
    expect(gamma).toHaveAttribute("title", "nope");
    // user-event honours `disabled` like a browser does (fireEvent would dispatch regardless).
    await userEvent.click(gamma);
    expect(onchange).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Beta" }));
    expect(onchange).toHaveBeenCalledWith("b");
  });
});

describe("Toast", () => {
  beforeEach(() => clearToasts());
  afterEach(() => clearToasts());

  it("renders the current toasts with role by kind", () => {
    toast("info", "hello", 0);
    toast("error", "boom", 0);
    render(Toast);
    expect(screen.getByRole("region", { name: en["a11y.toasts"] })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("hello");
    expect(screen.getByRole("alert")).toHaveTextContent("boom");
  });

  it("reacts to toasts added after mount and the dismiss button removes one", async () => {
    render(Toast);
    expect(screen.queryAllByRole("status")).toHaveLength(0);
    toast("ok", "saved", 0);
    toast("warn", "careful", 0);
    flushSync();
    expect(screen.getAllByRole("status")).toHaveLength(2);
    const buttons = screen.getAllByRole("button", { name: en["a11y.dismiss"] });
    await fireEvent.click(buttons[0]);
    flushSync();
    const left = screen.getAllByRole("status");
    expect(left).toHaveLength(1);
    expect(left[0]).toHaveTextContent("careful");
  });
});

describe("StagePreview", () => {
  const camera = (patch: Partial<CameraSettings> = {}): CameraSettings => ({
    ...defaultSettings("macos").camera,
    corner: "bottom-right",
    margin: 24,
    sizePct: 22,
    offsetX: 0,
    offsetY: 0,
    ...patch,
  });

  it("renders a slider whose data-* match the core layout (1920×1080, bottom-right, margin 24, 22%)", () => {
    render(StagePreview, { camera: camera(), outW: 1920, outH: 1080, onchange: () => {} });
    const pip = screen.getByRole("slider");
    expect(pip).toHaveAttribute("data-size", "236");
    expect(pip).toHaveAttribute("data-x", "1660");
    expect(pip).toHaveAttribute("data-y", "820");
    expect(pip).toHaveAttribute("aria-valuenow", "22");
    expect(pip).toHaveAttribute("aria-valuemin", "10");
    expect(pip).toHaveAttribute("aria-valuemax", "50");
    expect(screen.getByRole("img", { name: en["a11y.stage"] })).toBeInTheDocument();
  });

  it("other corners and offsets are clamped inside the frame", () => {
    render(StagePreview, { camera: camera({ corner: "top-left", offsetX: -500, offsetY: 5000 }), outW: 1920, outH: 1080, onchange: () => {} });
    const pip = screen.getByRole("slider");
    expect(pip).toHaveAttribute("data-x", "0");
    expect(pip).toHaveAttribute("data-y", String(1080 - 236));
  });

  it("ArrowLeft nudges offsetX by −16", async () => {
    const onchange = vi.fn();
    render(StagePreview, { camera: camera(), onchange });
    await fireEvent.keyDown(screen.getByRole("slider"), { key: "ArrowLeft" });
    expect(onchange).toHaveBeenCalledWith({ offsetX: -16 });
    await fireEvent.keyDown(screen.getByRole("slider"), { key: "ArrowDown" });
    expect(onchange).toHaveBeenLastCalledWith({ offsetY: 16 });
  });

  it("Shift+ArrowUp grows sizePct by 2, clamped to 50", async () => {
    const onchange = vi.fn();
    const { rerender } = render(StagePreview, { camera: camera({ sizePct: 30 }), onchange });
    await fireEvent.keyDown(screen.getByRole("slider"), { key: "ArrowUp", shiftKey: true });
    expect(onchange).toHaveBeenLastCalledWith({ sizePct: 32 });
    await rerender({ camera: camera({ sizePct: 49 }), onchange });
    await fireEvent.keyDown(screen.getByRole("slider"), { key: "ArrowUp", shiftKey: true });
    expect(onchange).toHaveBeenLastCalledWith({ sizePct: 50 });
    await rerender({ camera: camera({ sizePct: 11 }), onchange });
    await fireEvent.keyDown(screen.getByRole("slider"), { key: "ArrowDown", shiftKey: true });
    expect(onchange).toHaveBeenLastCalledWith({ sizePct: 10 });
  });

  it("ignores unrelated keys", async () => {
    const onchange = vi.fn();
    render(StagePreview, { camera: camera(), onchange });
    await fireEvent.keyDown(screen.getByRole("slider"), { key: "Enter" });
    expect(onchange).not.toHaveBeenCalled();
  });

  it("renders no slider when the camera is disabled", () => {
    render(StagePreview, { camera: camera({ enabled: false }), onchange: () => {} });
    expect(screen.queryByRole("slider")).toBeNull();
    expect(screen.getByRole("img")).toBeInTheDocument();
  });
});

describe("ShortcutField", () => {
  const props = (onchange = vi.fn()) => ({ value: "CmdOrCtrl+Shift+R", label: "Toggle", onchange });

  it("shows the shortcut chips and a press hint after click", async () => {
    render(ShortcutField, props());
    const button = screen.getByRole("button", { name: /Toggle/ });
    expect(button.textContent).toContain("⌘");
    expect(button.textContent).toContain("⇧");
    expect(button.textContent).toContain("R");
    await fireEvent.click(button);
    expect(button).toHaveTextContent(en["shortcut.press"]);
  });

  it("keydown Meta+Shift+R reports CmdOrCtrl+Shift+R and leaves edit mode", async () => {
    const onchange = vi.fn();
    render(ShortcutField, props(onchange));
    const button = screen.getByRole("button", { name: /Toggle/ });
    await fireEvent.click(button);
    await fireEvent.keyDown(button, { key: "r", code: "KeyR", metaKey: true, shiftKey: true });
    expect(onchange).toHaveBeenCalledWith("CmdOrCtrl+Shift+R");
    expect(button).not.toHaveTextContent(en["shortcut.press"]);
  });

  it("a modifier key alone is ignored while editing", async () => {
    const onchange = vi.fn();
    render(ShortcutField, props(onchange));
    const button = screen.getByRole("button", { name: /Toggle/ });
    await fireEvent.click(button);
    await fireEvent.keyDown(button, { key: "Shift", code: "ShiftLeft", shiftKey: true });
    expect(onchange).not.toHaveBeenCalled();
    expect(button).toHaveTextContent(en["shortcut.press"]);
  });

  it("a plain key shows the needModifier error", async () => {
    const onchange = vi.fn();
    render(ShortcutField, props(onchange));
    const button = screen.getByRole("button", { name: /Toggle/ });
    await fireEvent.click(button);
    await fireEvent.keyDown(button, { key: "r", code: "KeyR" });
    expect(onchange).not.toHaveBeenCalled();
    expect(screen.getByText(en["shortcut.needModifier"])).toBeInTheDocument();
    expect(button).toHaveAttribute("aria-describedby", "sc-err-Toggle");
  });

  it("Escape cancels editing and clears the error", async () => {
    const onchange = vi.fn();
    render(ShortcutField, props(onchange));
    const button = screen.getByRole("button", { name: /Toggle/ });
    await fireEvent.click(button);
    await fireEvent.keyDown(button, { key: "r", code: "KeyR" });
    expect(screen.queryByText(en["shortcut.needModifier"])).not.toBeNull();
    await fireEvent.keyDown(button, { key: "Escape", code: "Escape" });
    expect(screen.queryByText(en["shortcut.needModifier"])).toBeNull();
    expect(button).not.toHaveTextContent(en["shortcut.press"]);
    expect(onchange).not.toHaveBeenCalled();
  });

  it("keys do nothing when not editing", async () => {
    const onchange = vi.fn();
    render(ShortcutField, props(onchange));
    await fireEvent.keyDown(screen.getByRole("button", { name: /Toggle/ }), { key: "p", code: "KeyP", ctrlKey: true });
    expect(onchange).not.toHaveBeenCalled();
  });
});
