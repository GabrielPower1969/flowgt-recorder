/** Google account and Drive uploads, mirrored from Rust. */
import type { DriveProgress, DriveStatus } from "../types";
import { errorMessage, invoke, subscribe, type UnlistenFn } from "../ipc";
import { tCode } from "./i18n.svelte";
import { toast } from "./toasts.svelte";

let status = $state<DriveStatus>({ configured: false, email: null });
let uploads = $state<Record<string, DriveProgress>>({});
let signingIn = $state(false);

export const driveState = {
  get status() {
    return status;
  },
  get uploads() {
    return uploads;
  },
  get signingIn() {
    return signingIn;
  },
};

export function bindDriveEvents(): UnlistenFn {
  return subscribe("drive://progress", (p) => {
    uploads = { ...uploads, [p.path]: p };
  });
}

export async function refreshDrive(): Promise<DriveStatus> {
  status = await invoke("drive_status");
  return status;
}

/** Opens the browser; true once signed in. */
export async function signInToGoogle(): Promise<boolean> {
  if (!status.configured) {
    toast("error", tCode("error", "driveNotConfigured"));
    return false;
  }
  signingIn = true;
  try {
    status = await invoke("drive_sign_in");
    return !!status.email;
  } catch (e) {
    toast("error", errorMessage(e));
    return false;
  } finally {
    signingIn = false;
  }
}

export async function signOutOfGoogle(): Promise<void> {
  status = await invoke("drive_sign_out");
}

export async function uploadToDrive(path: string): Promise<void> {
  const { [path]: _previous, ...rest } = uploads;
  uploads = rest;
  try {
    await invoke("drive_upload", { path });
  } catch (e) {
    toast("error", errorMessage(e));
  }
}

/** Tests only. */
export function __resetDrive(): void {
  status = { configured: false, email: null };
  uploads = {};
  signingIn = false;
}
