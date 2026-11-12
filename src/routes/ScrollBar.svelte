<script lang="ts">
  /** The control bar of a scrolling screenshot: the picture so far, how tall it is, Done and Cancel. */
  import { onMount } from "svelte";
  import type { ScrollProgress, ScrollWarning } from "../lib/types";
  import { invoke, subscribe } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";

  let progress = $state<ScrollProgress | null>(null);
  let warning = $state<ScrollWarning>(null);
  let saving = $state(false);
  let strip = $state<HTMLDivElement | null>(null);

  async function done() {
    if (saving) return;
    saving = true;
    try {
      await invoke("scroll_capture_finish");
    } catch {
      await invoke("scroll_capture_cancel");
    }
  }
  const cancel = () => void invoke("scroll_capture_cancel");

  onMount(() => {
    const offs = [
      subscribe("scroll://progress", (p) => {
        progress = p;
        // Keep the newest part of the picture in view.
        queueMicrotask(() => { if (strip) strip.scrollTop = Math.max(0, strip.scrollHeight * p.newestAt - 40); });
      }),
      subscribe("scroll://warning", (w) => (warning = w)),
    ];
    const key = (e: KeyboardEvent) => {
      if (e.key === "Enter") void done();
      if (e.key === "Escape") cancel();
    };
    window.addEventListener("keydown", key);
    return () => {
      offs.forEach((off) => off());
      window.removeEventListener("keydown", key);
    };
  });
</script>

<div class="bar" data-testid="scroll-bar" data-tauri-drag-region>
  <div class="strip" bind:this={strip}>
    {#if progress?.thumbnail}<img src={progress.thumbnail} alt="" draggable="false" />{/if}
  </div>
  <div class="side">
    <strong>{t("scroll.bar.title")}</strong>
    <p class="count mono" data-testid="scroll-count">{progress && progress.frames > 0 ? t("scroll.bar.height", { h: progress.height, n: progress.frames }) : t("scroll.bar.waiting")}</p>
    {#if warning}<p class="warn" role="alert" data-testid="scroll-warning">{t(`scroll.bar.${warning}`)}</p>{/if}
    <div class="actions">
      <button type="button" class="btn btn-primary" data-testid="scroll-done" disabled={saving} onclick={done}>{t("scroll.bar.done")}</button>
      <button type="button" class="btn" disabled={saving} onclick={cancel}>{t("common.cancel")}</button>
    </div>
  </div>
</div>

<style>
  .bar { position: fixed; inset: 0; display: flex; gap: var(--space-3); padding: var(--space-3); background: var(--color-plate); color: var(--color-plate-ink); font-size: var(--text-sm); user-select: none; -webkit-user-select: none; }
  .strip { flex: none; width: 96px; overflow: hidden auto; border-radius: var(--radius-s); background: var(--color-desk); scrollbar-width: none; }
  .strip img { display: block; width: 100%; }
  .side { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: var(--space-2); }
  .side p { margin: 0; }
  .count { color: var(--color-plate-muted); }
  .warn { color: var(--color-danger); }
  .actions { margin-top: auto; display: flex; flex-direction: column; gap: var(--space-2); }
  .actions .btn { width: 100%; justify-content: center; }
</style>
