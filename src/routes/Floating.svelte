<script lang="ts">
  /** The bar shown while recording: status, time, pause, stop. Nothing else. */
  import { onMount } from "svelte";
  import { recorder, cancelRecording, togglePause, stopRecording } from "../lib/stores/recorder.svelte";
  import { getSettings, loadSettings } from "../lib/stores/settings.svelte";
  import { t } from "../lib/stores/i18n.svelte";
  import { formatDuration } from "../lib/format";

  const st = $derived(recorder.state);
  const paused = $derived(st.phase === "paused");
  const canAct = $derived(!recorder.busy && (st.phase === "recording" || st.phase === "paused"));

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && st.phase === "countdown") void cancelRecording();
  }
  onMount(() => {
    void loadSettings();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

{#if st.phase === "countdown"}
  <div class="countdown" data-tauri-drag-region role="status" aria-live="assertive">
    <span class="mono num" data-testid="countdown" data-tauri-drag-region>{recorder.countdown ?? getSettings().countdownSeconds}</span>
    <button type="button" class="btn btn-sm" onclick={() => cancelRecording()}>{t("common.cancel")}</button>
  </div>
{:else}
  <div class="wrap" data-tauri-drag-region>
    <div class="bar" data-tauri-drag-region>
      <span class="status" class:paused data-tauri-drag-region><span class="dot" class:pulse={!paused}></span>{paused ? t("floating.paused") : t("floating.recording")}</span>
      <span class="mono time" data-testid="timer" data-tauri-drag-region>{formatDuration(st.elapsedMs)}</span>
      <button type="button" class="btn btn-sm" onclick={() => togglePause()} disabled={!canAct}>{paused ? t("floating.resume") : t("floating.pause")}</button>
      <button type="button" class="btn btn-sm stop" onclick={() => stopRecording()} disabled={!canAct}>{t("floating.stopLong")}</button>
    </div>
  </div>
{/if}

<style>
  .wrap { height: 100vh; display: flex; align-items: center; justify-content: center; }
  .bar { display: flex; align-items: center; gap: var(--space-3); height: 56px; padding: 0 var(--space-3) 0 var(--space-4); background: var(--color-floating-bg); border: 1px solid var(--color-border); border-radius: var(--radius-pill); color: var(--color-text); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px); white-space: nowrap; }
  .status { display: inline-flex; align-items: center; gap: var(--space-2); font-weight: 600; }
  .status.paused { color: var(--color-warn); }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--color-record); }
  .paused .dot { background: currentColor; }
  .pulse { animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.25; } }
  .time { min-width: 72px; }
  .stop { background: var(--color-record); border-color: var(--color-record); color: #fff; }
  .stop:hover:not(:disabled) { background: var(--color-record-hover); }
  .countdown { height: 100vh; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--space-3); background: var(--color-floating-bg); border-radius: var(--radius-l); color: var(--color-text); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px); }
  .num { font-size: var(--text-3xl); font-weight: 600; line-height: 1; }
</style>
