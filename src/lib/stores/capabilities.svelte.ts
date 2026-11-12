/** What the platform can do, as the backend reports it (S-2). Components read this, never the platform's name. */
import type { Capabilities } from "../types";
import { invoke, isTauri } from "../ipc";
import { mockCapabilities } from "../ipc-mock";

const NONE: Capabilities = { windowCapture: false, systemAudio: false, perAppAudio: false, camera: false, pictureInPicture: false, floatingBar: false, tray: false, globalShortcuts: false, pauseResume: false, livePreview: false, overlayTitleBar: false };

// Inside the app nothing optional is offered until the backend has answered; the browser preview
// and the tests start from what the mock reports.
let caps = $state<Capabilities>(isTauri() ? NONE : mockCapabilities());
let loaded = false;

export function getCapabilities(): Capabilities {
  return caps;
}

/** For tests: pretend to be a platform with these capabilities. */
export function setCapabilities(c: Partial<Capabilities>): void {
  caps = { ...caps, ...c };
  loaded = true;
}

export async function loadCapabilities(): Promise<Capabilities> {
  if (loaded) return caps;
  try {
    caps = await invoke("get_capabilities");
    loaded = true;
  } catch {
    /* an older backend: keep what we have */
  }
  return caps;
}
