/** Settings store: loaded from the backend, saved back, theme + language applied to the document. */
import type { Settings, Theme } from "../types";
import { invoke, errorMessage } from "../ipc";
import { defaultSettings } from "../defaults";
import { setLanguage } from "./i18n.svelte";
import { toast } from "./toasts.svelte";

let settings = $state<Settings>(defaultSettings("macos"));
let loaded = $state(false);
let saving = $state(false);

export function getSettings(): Settings {
  return settings;
}
export function isLoaded(): boolean {
  return loaded;
}
export function isSaving(): boolean {
  return saving;
}

export function applyTheme(theme: Theme): void {
  if (typeof document === "undefined") return;
  if (theme === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.setAttribute("data-theme", theme);
}

function applyAppearance(s: Settings) {
  applyTheme(s.theme);
  setLanguage(s.language);
}

export async function loadSettings(): Promise<Settings> {
  settings = await invoke("get_settings");
  applyAppearance(settings);
  loaded = true;
  return settings;
}

/** Patch locally (instant UI), then persist. Returns the backend's normalised copy. */
export async function updateSettings(patch: (s: Settings) => void): Promise<Settings> {
  const draft: Settings = JSON.parse(JSON.stringify(settings));
  patch(draft);
  settings = draft;
  applyAppearance(settings);
  saving = true;
  try {
    settings = await invoke("save_settings", { settings: draft });
    applyAppearance(settings);
    return settings;
  } catch (e) {
    toast("error", errorMessage(e));
    throw e;
  } finally {
    saving = false;
  }
}

/** Tests only. */
export function __setSettings(s: Settings): void {
  settings = s;
  loaded = true;
}
