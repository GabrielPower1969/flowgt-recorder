<script lang="ts">
  /** The "choose what to share" dialog, modelled on Google Meet / Chrome's screen picker. */
  import { onMount } from "svelte";
  import type { Device, PermissionState } from "../types";
  import { invoke } from "../ipc";
  import { t } from "../stores/i18n.svelte";
  import { getSettings, updateSettings } from "../stores/settings.svelte";
  import { startRecording } from "../stores/recorder.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let screens = $state<Device[]>([]);
  let loopbacks = $state<Device[]>([]);
  let thumbs = $state<Record<string, string | null>>({});
  let selected = $state<string | null>(null);
  let permission = $state<PermissionState | null>(null);
  let asked = $state(false);
  let name = $state("");
  let starting = $state(false);

  const allowed = $derived(permission === "granted" || permission === "notApplicable");
  const hasLoopback = $derived(loopbacks.length > 0);
  const systemAudio = $derived(hasLoopback && getSettings().audio.systemAudio.kind !== "off");

  async function loadThumbs() {
    for (const s of screens) {
      void invoke("screen_thumbnail", { id: s.id }).then((url) => {
        thumbs = { ...thumbs, [s.id]: url };
      });
    }
  }
  async function allow() {
    const r = await invoke("request_permission", { kind: "screen" });
    permission = r;
    if (r === "granted") return void loadThumbs();
    await invoke("open_privacy_settings", { kind: "screen" });
    asked = true;
  }
  function toggleSystemAudio(on: boolean) {
    void updateSettings((s) => {
      s.audio.systemAudio = { kind: on ? "auto" : "off" };
    });
  }
  async function start() {
    if (!selected || starting) return;
    starting = true;
    const id = selected;
    await updateSettings((s) => {
      s.capture = { kind: "display", displayId: id };
    });
    const st = await startRecording(name);
    starting = false;
    if (st) onclose();
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    void (async () => {
      const [list, perms] = await Promise.all([invoke("list_devices"), invoke("check_permissions")]);
      screens = list.screens;
      loopbacks = list.loopbacks;
      permission = perms.screen;
      const capture = getSettings().capture;
      const saved = capture.kind === "window" ? null : capture.displayId;
      selected = screens.find((s) => s.id === saved)?.id ?? screens.find((s) => s.isDefault)?.id ?? screens[0]?.id ?? null;
      if (perms.screen === "granted" || perms.screen === "notApplicable") void loadThumbs();
    })();
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

<div class="backdrop">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="picker-title">
    <h2 id="picker-title">{t("picker.title")}</h2>

    {#if permission !== null && !allowed}
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
      <div class="grid" role="radiogroup" aria-labelledby="picker-title">
        {#each screens as s, i (s.id)}
          <button type="button" class="tile" role="radio" aria-checked={selected === s.id} onclick={() => (selected = s.id)} ondblclick={start}>
            <span class="thumb">
              {#if thumbs[s.id]}<img src={thumbs[s.id]} alt="" />{/if}
            </span>
            <span>{t("picker.screen", { n: i + 1 })}</span>
          </button>
        {/each}
      </div>
      <label class="field">
        <span>{t("picker.name")}</span>
        <input class="input" bind:value={name} data-testid="name" />
      </label>
      <label class="row sys">
        <input type="checkbox" checked={systemAudio} disabled={!hasLoopback} onchange={(e) => toggleSystemAudio((e.target as HTMLInputElement).checked)} />
        {t("picker.systemAudio")}
        {#if !hasLoopback}<span class="faint">{t("picker.systemAudioMissing")}</span>{/if}
      </label>
    {/if}

    <div class="foot">
      <button type="button" class="btn" onclick={onclose}>{t("common.cancel")}</button>
      <button type="button" class="btn btn-primary" onclick={start} disabled={!allowed || !selected || starting}>{t("picker.start")}</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; display: flex; align-items: center; justify-content: center; padding: var(--space-4); background: var(--color-overlay); z-index: 20; }
  .dialog { width: min(640px, 100%); max-height: 100%; overflow: auto; display: flex; flex-direction: column; gap: var(--space-4); padding: var(--space-5); background: var(--color-surface); border-radius: var(--radius-l); box-shadow: var(--shadow-floating); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: var(--space-3); }
  .tile { display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-2); border: 2px solid transparent; border-radius: var(--radius-m); background: var(--color-surface-2); text-align: left; }
  .tile[aria-checked="true"] { border-color: var(--color-accent); background: var(--color-accent-soft); }
  .thumb { display: block; width: 100%; aspect-ratio: 16 / 10; border-radius: var(--radius-s); background: var(--color-surface-3); overflow: hidden; }
  .thumb img { display: block; width: 100%; height: 100%; object-fit: cover; }
  .perm { display: flex; flex-direction: column; gap: var(--space-3); padding: var(--space-4); border-radius: var(--radius-m); background: var(--color-surface-2); }
  .sys { gap: var(--space-2); }
  .foot { display: flex; justify-content: flex-end; gap: var(--space-2); }
</style>
