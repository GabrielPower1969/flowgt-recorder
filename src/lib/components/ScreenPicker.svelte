<script lang="ts">
  /** The "choose what to share" dialog, modelled on Google Meet / Chrome's screen picker. */
  import { onMount } from "svelte";
  import type { Device, PermissionState, WindowInfo } from "../types";
  import { invoke } from "../ipc";
  import { t } from "../stores/i18n.svelte";
  import { getSettings, updateSettings } from "../stores/settings.svelte";
  import { startRecording } from "../stores/recorder.svelte";
  import { getPlatform } from "../platform.svelte";
  import Segmented from "./Segmented.svelte";

  let { onclose }: { onclose: () => void } = $props();

  type Tab = "screen" | "window";
  let tab = $state<Tab>("screen");
  let screens = $state<Device[]>([]);
  let thumbs = $state<Record<string, string | null>>({});
  let selected = $state<string | null>(null);
  let windows = $state<WindowInfo[] | null>(null);
  let windowThumbs = $state<Record<number, string | null>>({});
  let selectedWindow = $state<number | null>(null);
  let permission = $state<PermissionState | null>(null);
  let asked = $state(false);
  let name = $state("");
  let starting = $state(false);

  const allowed = $derived(permission === "granted" || permission === "notApplicable");
  // Recording one window needs the native helper, which only macOS has.
  const windowsOffered = $derived(getPlatform() === "macos");
  const canStart = $derived(allowed && !starting && (tab === "screen" ? selected !== null : selectedWindow !== null));

  // Outline the real screen while choosing. The backend removes it before capture starts.
  let keepMarker = false;
  $effect(() => {
    const index = screens.findIndex((s) => s.id === selected);
    if (!allowed) return;
    if (tab === "screen" && index >= 0) void invoke("show_screen_marker", { index });
    else void invoke("hide_screen_marker");
  });

  async function loadThumbs() {
    for (const s of screens) {
      void invoke("screen_thumbnail", { id: s.id }).then((url) => {
        thumbs = { ...thumbs, [s.id]: url };
      });
    }
  }
  async function loadWindows() {
    windows = null;
    const list = await invoke("list_windows");
    windows = list;
    if (!list.some((w) => w.id === selectedWindow)) selectedWindow = list[0]?.id ?? null;
    // One after the other: each thumbnail is one helper run, and the backend serialises them anyway.
    for (const w of list) {
      const url = await invoke("window_thumbnail", { id: w.id });
      windowThumbs = { ...windowThumbs, [w.id]: url };
    }
  }
  function switchTab(next: Tab) {
    tab = next;
    if (next === "window" && windows === null) void loadWindows();
  }
  async function allow() {
    const r = await invoke("request_permission", { kind: "screen" });
    permission = r;
    if (r === "granted") return void loadThumbs();
    await invoke("open_privacy_settings", { kind: "screen" });
    asked = true;
  }
  async function start() {
    if (!canStart) return;
    starting = true;
    keepMarker = true;
    const id = selected;
    const win = selectedWindow;
    await updateSettings((s) => {
      s.capture = tab === "window" && win !== null ? { kind: "window", windowId: String(win) } : { kind: "display", displayId: id ?? "" };
    });
    const st = await startRecording(name);
    starting = false;
    if (st) onclose();
    else keepMarker = false;
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
  function label(w: WindowInfo): string {
    return w.title.trim() === "" ? t("picker.untitled") : w.title;
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    void (async () => {
      const [list, perms] = await Promise.all([invoke("list_devices"), invoke("check_permissions")]);
      screens = list.screens;
      permission = perms.screen;
      const capture = getSettings().capture;
      const saved = capture.kind === "window" ? null : capture.displayId;
      selected = screens.find((s) => s.id === saved)?.id ?? screens.find((s) => s.isDefault)?.id ?? screens[0]?.id ?? null;
      if (perms.screen === "granted" || perms.screen === "notApplicable") {
        void loadThumbs();
        // Last time was a window: open that tab again (its id is new, so the first one is preselected).
        if (capture.kind === "window" && windowsOffered) switchTab("window");
      }
    })();
    return () => {
      window.removeEventListener("keydown", onKey);
      if (!keepMarker) void invoke("hide_screen_marker");
    };
  });
</script>

<div class="backdrop">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="picker-title">
    <h2 id="picker-title">{t("picker.title")}</h2>

    {#if permission === null}
      <div class="thumb loading" aria-busy="true"></div>
    {:else if !allowed}
      <div class="perm" data-testid="perm">
        <strong>{t("picker.perm.title")}</strong>
        <p class="muted">{t("picker.perm.body")}</p>
        <div class="row">
          <button type="button" class="btn btn-primary" onclick={allow}>{t("picker.perm.open")}</button>
          {#if asked}
            <button type="button" class="btn" onclick={() => invoke("relaunch_app")}>{t("picker.perm.relaunch")}</button>
          {/if}
        </div>
      </div>
    {:else}
      {#if windowsOffered}
        <Segmented label={t("picker.title")} value={tab} onchange={switchTab} options={[{ value: "screen", label: t("picker.tab.screen") }, { value: "window", label: t("picker.tab.window") }]} />
      {/if}

      {#if tab === "screen"}
        <div class="grid" role="radiogroup" aria-labelledby="picker-title">
          {#each screens as s, i (s.id)}
            <button type="button" class="tile" role="radio" aria-checked={selected === s.id} onclick={() => (selected = s.id)} ondblclick={start}>
              <span class="thumb">
                {#if thumbs[s.id]}<img src={thumbs[s.id]} alt="" />{/if}
              </span>
              <span class="caption"><span>{t("picker.screen", { n: i + 1 })}</span>{#if selected === s.id}<span class="chip chip-ok">{t("picker.willRecord")}</span>{/if}</span>
            </button>
          {/each}
        </div>
        <p class="faint hint" data-testid="marker-hint">{t("picker.markerHint")}</p>
      {:else if windows === null}
        <div class="thumb loading" aria-busy="true"></div>
      {:else if windows.length === 0}
        <div class="perm" data-testid="no-windows">
          <p class="muted">{t("picker.noWindows")}</p>
          <div class="row"><button type="button" class="btn" onclick={loadWindows}>{t("picker.refresh")}</button></div>
        </div>
      {:else}
        <div class="grid windows" role="radiogroup" aria-labelledby="picker-title">
          {#each windows as w (w.id)}
            <button type="button" class="tile" role="radio" aria-checked={selectedWindow === w.id} onclick={() => (selectedWindow = w.id)} ondblclick={start} title={`${w.app} — ${label(w)}`}>
              <span class="thumb">
                {#if windowThumbs[w.id]}<img src={windowThumbs[w.id]} alt="" />{/if}
              </span>
              <span class="caption">
                <span class="who">
                  {#if w.icon}<img class="icon" src={`data:image/png;base64,${w.icon}`} alt="" />{/if}
                  <span class="names"><strong>{w.app}</strong><span class="faint title">{label(w)}</span></span>
                </span>
                {#if selectedWindow === w.id}<span class="chip chip-ok">{t("picker.willRecord")}</span>{/if}
              </span>
            </button>
          {/each}
        </div>
        <p class="faint hint row-between"><span>{t("picker.windowHint")}</span><button type="button" class="btn btn-small" onclick={loadWindows}>{t("picker.refresh")}</button></p>
      {/if}
      <label class="field">
        <span>{t("picker.name")}</span>
        <input class="input" bind:value={name} data-testid="name" />
      </label>
    {/if}

    <div class="foot">
      <button type="button" class="btn" onclick={onclose}>{t("common.cancel")}</button>
      <button type="button" class="btn btn-primary" onclick={start} disabled={!canStart}>{t("picker.start")}</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; display: flex; align-items: center; justify-content: center; padding: var(--space-4); background: var(--color-overlay); z-index: 20; }
  .dialog { width: min(640px, 100%); max-height: 100%; overflow: auto; display: flex; flex-direction: column; gap: var(--space-4); padding: var(--space-5); background: var(--color-surface); border-radius: var(--radius-l); box-shadow: var(--shadow-floating); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: var(--space-3); }
  .grid.windows { grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); max-height: 46vh; overflow: auto; padding: 2px; }
  .tile { display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-2); border: 2px solid transparent; border-radius: var(--radius-m); background: var(--color-surface-2); text-align: left; }
  .tile[aria-checked="true"] { border-color: var(--color-accent); background: var(--color-accent-soft); }
  .tile[aria-checked="true"] { border-width: 4px; }
  .caption { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); min-width: 0; }
  .who { display: flex; align-items: center; gap: var(--space-2); min-width: 0; }
  .icon { width: 24px; height: 24px; flex: none; }
  .names { display: flex; flex-direction: column; min-width: 0; line-height: 1.2; }
  .names > * { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title { font-size: 0.85em; }
  .hint { margin: 0; }
  .row-between { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
  .btn-small { padding: 2px var(--space-2); font-size: 0.85em; flex: none; }
  .thumb { display: block; width: 100%; aspect-ratio: 16 / 10; border-radius: var(--radius-s); background: var(--color-surface-3); overflow: hidden; }
  .thumb img { display: block; width: 100%; height: 100%; object-fit: contain; }
  .loading { height: 200px; aspect-ratio: auto; }
  .perm { display: flex; flex-direction: column; gap: var(--space-3); padding: var(--space-4); border-radius: var(--radius-m); background: var(--color-surface-2); }
  .foot { display: flex; justify-content: flex-end; gap: var(--space-2); }
</style>
