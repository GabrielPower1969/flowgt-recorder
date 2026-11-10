/** Platform detection: @tauri-apps/plugin-os inside Tauri, a user-agent guess (or the mock) elsewhere. */
import type { Platform } from "./types";
import { isTauri } from "./ipc";

function guessPlatform(): Platform {
  if (typeof navigator === "undefined") return "macos";
  const ua = `${navigator.userAgent} ${navigator.platform ?? ""}`.toLowerCase();
  // "mac" and "darwin" first: "darwin" contains "win" (jsdom reports "darwin").
  if (ua.includes("mac") || ua.includes("darwin")) return "macos";
  if (ua.includes("win")) return "windows";
  if (ua.includes("linux")) return "linux";
  return "macos";
}

let current = $state<Platform>(guessPlatform());
let resolved = false;

export function getPlatform(): Platform {
  return current;
}

/** For tests and for the mock: override the detected platform. */
export function setPlatform(p: Platform): void {
  current = p;
  resolved = true;
}

export async function initPlatform(): Promise<Platform> {
  if (resolved) return current;
  if (isTauri()) {
    try {
      const os = await import("@tauri-apps/plugin-os");
      const p = os.platform();
      current = p === "windows" ? "windows" : p === "macos" ? "macos" : "linux";
    } catch {
      current = guessPlatform();
    }
  } else {
    current = "macos"; // the mock is a Mac
  }
  resolved = true;
  return current;
}

export const isMac = () => current === "macos";
