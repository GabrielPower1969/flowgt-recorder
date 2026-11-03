<script lang="ts">
  import { onMount } from "svelte";
  import { recorder, cancelRecording, togglePause, stopRecording } from "../lib/stores/recorder.svelte";
  import { getSettings, loadSettings } from "../lib/stores/settings.svelte";
  import { t } from "../lib/stores/i18n.svelte";
  import { formatBytes, formatDuration } from "../lib/format";
  import { startLevelMeter, type LevelMeterHandle } from "../lib/media/level";
  import LevelMeter from "../lib/components/LevelMeter.svelte";

  let level = $state(0);
  let meter: LevelMeterHandle | null = null;
  const st = $derived(recorder.state);
  const paused = $derived(st.phase === "paused");
  const countdown = $derived(st.phase === "countdown" ? recorder.countdown : null);

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && st.phase === "countdown") void cancelRecording();
  }
  onMount(() => {
    void loadSettings();
    window.addEventListener("keydown", onKey);
    void startLevelMeter(null, (v) => (level = v)).then((h) => (meter = h));
    return () => {
      window.removeEventListener("keydown", onKey);
      meter?.stop();
    };
  });
</script>

{#if st.phase === "countdown"}
  <div class="countdown" data-tauri-drag-region role="status" aria-live="assertive">
    <div class="ring"><span class="mono num" data-testid="countdown">{countdown ?? getSettings().countdownSeconds}</span></div>
    <div class="stack" style="gap: 4px">
      <strong>{t("floating.countdownLabel")}</strong>
      <span class="faint">{st.meetingName || t("app.name")}</span>
      <span class="faint">{t("floating.cancelHint")}</span>
    </div>
    <button type="button" class="btn btn-sm" onclick={() => cancelRecording()}>{t("common.cancel")}</button>
  </div>
{:else}
  <div class="bar" data-tauri-drag-region>
    <span class="drag" aria-label={t("floating.drag")} data-tauri-drag-region>⋮⋮</span>
    <span class="rec" class:paused><span class="recdot" class:pulse={!paused}></span>{paused ? t("status.paused") : t("floating.rec")}</span>
    <span class="mono time" data-testid="timer">{formatDuration(st.elapsedMs)}</span>
    <span class="mono faint">{t("floating.estimate", { size: formatBytes(st.bytesWritten) })}</span>
    <LevelMeter {level} compact />
    <button type="button" class="btn btn-ghost btn-icon" aria-label={paused ? t("floating.resume") : t("floating.pause")} onclick={() => togglePause()} disabled={recorder.busy || (st.phase !== "recording" && st.phase !== "paused")}>{paused ? "▶" : "❚❚"}</button>
    <button type="button" class="btn btn-icon stop" aria-label={t("floating.stop")} onclick={() => stopRecording()} disabled={recorder.busy || (st.phase !== "recording" && st.phase !== "paused")}>■</button>
  </div>
{/if}

<style>
  .bar, .countdown { display: flex; align-items: center; gap: var(--space-3); height: 100vh; padding: 0 var(--space-3); background: var(--color-floating-bg); border: 1px solid var(--color-border); border-radius: var(--radius-pill); box-shadow: var(--shadow-floating); color: var(--color-text); backdrop-filter: blur(16px); }
  .countdown { border-radius: var(--radius-l); padding: var(--space-4); gap: var(--space-4); }
  .drag { color: var(--color-text-faint); cursor: grab; letter-spacing: -2px; }
  .rec { display: inline-flex; align-items: center; gap: 6px; font-weight: 700; color: var(--color-record); font-size: var(--text-sm); }
  .rec.paused { color: var(--color-warn); }
  .recdot { width: 10px; height: 10px; border-radius: 50%; background: currentColor; }
  .pulse { animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.25; } }
  .time { font-size: var(--text-lg); min-width: 84px; }
  .stop { background: var(--color-record); border-color: var(--color-record); color: #fff; }
  .ring { width: 72px; height: 72px; border-radius: 50%; border: 4px solid var(--color-record); display: flex; align-items: center; justify-content: center; }
  .num { font-size: var(--text-2xl); font-weight: 600; }
</style>
