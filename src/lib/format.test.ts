import { describe, expect, it } from "vitest";
import {
  budgetFromMbPerHour,
  codecLabel,
  estimateMbPerHour,
  expectedBytes,
  formatBytes,
  formatDuration,
  formatHours,
  formatKbps,
  formatMbps,
  formatPercent,
  outputSize,
  overBudgetRatio,
  previewFilename,
  sanitizeFilename,
  shortcutParts,
} from "./format";

describe("formatBytes", () => {
  it("uses decimal MB below 1 GB", () => {
    expect(formatBytes(0)).toBe("0.0 MB");
    expect(formatBytes(83_400_000)).toBe("83.4 MB");
  });
  it("switches to GB", () => {
    expect(formatBytes(1_200_000_000)).toBe("1.2 GB");
  });
  it("treats junk as zero", () => {
    expect(formatBytes(-5)).toBe("0.0 MB");
    expect(formatBytes(Number.NaN)).toBe("0.0 MB");
  });
});

describe("formatDuration", () => {
  it("pads and rolls over", () => {
    expect(formatDuration(0)).toBe("00:00:00");
    expect(formatDuration(61_000)).toBe("00:01:01");
    expect(formatDuration(3_661_999)).toBe("01:01:01");
    expect(formatDuration(100 * 3_600_000)).toBe("100:00:00");
    expect(formatDuration(-1)).toBe("00:00:00");
  });
});

describe("rates", () => {
  it("formats kbps and Mbps", () => {
    expect(formatKbps(888.9)).toBe("889 kbps");
    expect(formatMbps(889)).toBe("0.89 Mbps");
  });
  it("budget 400 MB/h with one 96 kbps track is 889 / 793 (same as budget.rs)", () => {
    expect(budgetFromMbPerHour(400, 96, 1)).toEqual({ totalKbps: 889, videoKbps: 793, audioKbpsTotal: 96 });
  });
  it("two tracks take two audio shares", () => {
    expect(budgetFromMbPerHour(400, 96, 2).videoKbps).toBe(697);
  });
  it("extrapolates a sample to one hour", () => {
    expect(estimateMbPerHour(111_125 * 10, 10_000)).toBeCloseTo(400.05, 2);
    expect(estimateMbPerHour(1, 0)).toBe(0);
  });
  it("over-budget ratio", () => {
    expect(overBudgetRatio(450_000_000, 3_600_000, 400)).toBeCloseTo(0.125, 5);
    expect(overBudgetRatio(1, 0, 400)).toBe(0);
  });
  it("expected bytes at the budget", () => {
    expect(expectedBytes(889, 3_600_000)).toBe(400_050_000);
  });
});

describe("file names", () => {
  const at = new Date(2026, 10, 4, 14, 30);
  it("renders the default template as 2026-11-04 1430 周会", () => {
    expect(previewFilename("{date} {time} {name}", "周会", at)).toBe("2026-11-04 1430 周会");
  });
  it("supports datetime and parts, keeps unknown tokens", () => {
    expect(previewFilename("{datetime}-{x}", "a", at)).toBe("2026-11-04_1430-{x}");
    expect(previewFilename("{yyyy}{mm}{dd}{hh}{min}", "a", at)).toBe("202611041430");
  });
  it("sanitises illegal characters and falls back when empty", () => {
    expect(sanitizeFilename('a/b:c*d?"e<f>g|h', "x")).toBe("abcdefgh");
    expect(sanitizeFilename("   ", "录像")).toBe("录像");
    expect(previewFilename("{name}", "///", at)).toBe("录像");
  });
});

describe("outputSize", () => {
  it("scales a Retina screen to 1920", () => {
    expect(outputSize({ width: 2880, height: 1800 }, 1920)).toEqual({ width: 1920, height: 1200 });
  });
  it("never scales up and defaults to 1080p when unknown", () => {
    expect(outputSize({ width: 1280, height: 720 }, 1920)).toEqual({ width: 1280, height: 720 });
    expect(outputSize(null, 1920)).toEqual({ width: 1920, height: 1080 });
  });
});

describe("misc", () => {
  it("shortcut chips per platform", () => {
    expect(shortcutParts("CmdOrCtrl+Shift+R", "macos")).toEqual(["⌘", "⇧", "R"]);
    expect(shortcutParts("CmdOrCtrl+Shift+R", "windows")).toEqual(["Ctrl", "Shift", "R"]);
  });
  it("labels and rounding helpers", () => {
    expect(codecLabel("hevc")).toBe("HEVC");
    expect(codecLabel("vp9")).toBe("VP9");
    expect(formatPercent(0.123)).toBe("12");
    expect(formatHours(12.34)).toBe("12.3");
    expect(formatHours(250.4)).toBe("250");
  });
});
