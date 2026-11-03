/**
 * Pure formatting helpers. Numbers here must agree with the Rust core
 * (budget.rs, filename.rs); the tests pin the shared cases.
 */
import type { Platform } from "./types";

const MB = 1_000_000; // decimal, like the 400 MB/h promise
const GB = 1_000_000_000;

/** "83.4 MB" below 1 GB, "1.2 GB" above; one decimal. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) bytes = 0;
  if (bytes >= GB) return `${(bytes / GB).toFixed(1)} GB`;
  return `${(bytes / MB).toFixed(1)} MB`;
}

/** Milliseconds → "HH:MM:SS" (hours grow past 99 if needed). */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) ms = 0;
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

/** "889 kbps" */
export function formatKbps(kbps: number): string {
  return `${Math.round(kbps)} kbps`;
}

/** "0.89 Mbps" (two decimals) */
export function formatMbps(kbps: number): string {
  return `${(kbps / 1000).toFixed(2)} Mbps`;
}

/** Extrapolate a measured sample to one hour, in decimal MB. */
export function estimateMbPerHour(bytes: number, ms: number): number {
  if (ms <= 0) return 0;
  return (bytes / (ms / 1000)) * 3600 / MB;
}

export interface Budget {
  totalKbps: number;
  videoKbps: number;
  audioKbpsTotal: number;
}

/**
 * total = round(mb × 8 × 1000 / 3600); video = total − audio.
 * 400 MB/h, 96 kbps, 1 track → 889 / 793 (matches budget.rs).
 */
export function budgetFromMbPerHour(mb: number, audioKbps: number, tracks: number): Budget {
  const totalKbps = Math.round((mb * 8 * 1000) / 3600);
  const audioKbpsTotal = audioKbps * Math.max(1, tracks);
  const videoKbps = Math.max(0, totalKbps - audioKbpsTotal);
  return { totalKbps, videoKbps, audioKbpsTotal };
}

/** bytes ÷ (hours × target MB) − 1; positive = over budget. */
export function overBudgetRatio(bytes: number, ms: number, targetMbPerHour: number): number {
  if (ms <= 0 || targetMbPerHour <= 0) return 0;
  const hours = ms / 3_600_000;
  return bytes / (hours * targetMbPerHour * MB) - 1;
}

/** Bytes expected at `ms` for a budget of `totalKbps`. */
export function expectedBytes(totalKbps: number, ms: number): number {
  return Math.round((totalKbps * 1000 / 8) * (ms / 1000));
}

/** Mirror of filename::sanitize — strips / \ : * ? " < > |, trims, collapses spaces, caps at 120. */
export function sanitizeFilename(name: string, fallback: string): string {
  const cleaned = name
    .replace(/[/\\:*?"<>|]/g, "")
    .replace(/[\u0000-\u001f]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 120)
    .trim();
  return cleaned.length > 0 ? cleaned : fallback;
}

const pad2 = (n: number) => String(n).padStart(2, "0");

/**
 * Render the file-name template with the same tokens as filename.rs:
 * {date}=YYYY-MM-DD {time}=HHmm {datetime}=YYYY-MM-DD_HHmm {name} {yyyy} {mm} {dd} {hh} {min}.
 * Unknown tokens are left verbatim. No extension.
 */
export function previewFilename(template: string, name: string, date: Date = new Date(), fallbackName = "录像"): string {
  const yyyy = String(date.getFullYear());
  const mm = pad2(date.getMonth() + 1);
  const dd = pad2(date.getDate());
  const hh = pad2(date.getHours());
  const min = pad2(date.getMinutes());
  const tokens: Record<string, string> = {
    date: `${yyyy}-${mm}-${dd}`,
    time: `${hh}${min}`,
    datetime: `${yyyy}-${mm}-${dd}_${hh}${min}`,
    name: sanitizeFilename(name, fallbackName),
    yyyy,
    mm,
    dd,
    hh,
    min,
  };
  return template.replace(/\{(\w+)\}/g, (whole, key: string) => (key in tokens ? tokens[key] : whole));
}

/** Output frame size: width capped at maxWidth, height follows the aspect ratio, rounded to even. */
export function outputSize(
  source: { width: number | null; height: number | null } | null | undefined,
  maxWidth: number,
): { width: number; height: number } {
  const srcW = source?.width && source.width > 0 ? source.width : 1920;
  const srcH = source?.height && source.height > 0 ? source.height : 1080;
  const width = Math.min(srcW, maxWidth);
  const scale = width / srcW;
  const height = Math.max(2, Math.round((srcH * scale) / 2) * 2);
  return { width: Math.round(width / 2) * 2, height };
}

export const CODEC_LABELS: Record<string, string> = { hevc: "HEVC", h264: "H.264", mpeg4: "MPEG-4" };

export function codecLabel(codec: string): string {
  return CODEC_LABELS[codec] ?? codec.toUpperCase();
}

/** Tauri accelerator string → display chips, e.g. "CmdOrCtrl+Shift+R" → ["⌘","⇧","R"] on macOS. */
export function shortcutParts(shortcut: string, platform: Platform): string[] {
  const mac = platform === "macos";
  const map: Record<string, string> = mac
    ? {
        cmdorctrl: "⌘",
        commandorcontrol: "⌘",
        cmd: "⌘",
        command: "⌘",
        super: "⌘",
        meta: "⌘",
        ctrl: "⌃",
        control: "⌃",
        alt: "⌥",
        option: "⌥",
        shift: "⇧",
      }
    : {
        cmdorctrl: "Ctrl",
        commandorcontrol: "Ctrl",
        cmd: "Win",
        command: "Win",
        super: "Win",
        meta: "Win",
        ctrl: "Ctrl",
        control: "Ctrl",
        alt: "Alt",
        option: "Alt",
        shift: "Shift",
      };
  return shortcut
    .split("+")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => map[p.toLowerCase()] ?? (p.length === 1 ? p.toUpperCase() : p));
}

/** Percent with no decimals, e.g. 0.123 → "12". */
export function formatPercent(ratio: number): string {
  return String(Math.round(ratio * 100));
}

/** Hours with one decimal unless ≥ 100. */
export function formatHours(hours: number): string {
  if (!Number.isFinite(hours) || hours < 0) return "0";
  return hours >= 100 ? String(Math.round(hours)) : hours.toFixed(1);
}
