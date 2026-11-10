<script lang="ts">
  /**
   * The recording window: the bar (status, time, pause, stop) and, under it, the live preview
   * of what is going into the file, labelled with the screen or window being recorded. The
   * preview is the helper's own output frame, so it shows exactly what is recorded, camera
   * square included. A recorded window that leaves the screen (minimised, other Space) is
   * reported by the helper as `warn.windowHidden`; the panel dims and says so until it is back.
   */
  import { onMount } from "svelte";
  import { recorder, cancelRecording, togglePause, stopRecording } from "../lib/stores/recorder.svelte";
  import { getSettings, loadSettings } from "../lib/stores/settings.svelte";
  import { t } from "../lib/stores/i18n.svelte";
  import { formatDuration } from "../lib/format";
  import { invoke } from "../lib/ipc";

  const POLL_MS = 150;

  const st = $derived(recorder.state);
  const paused = $derived(st.phase === "paused");
  const canAct = $derived(!recorder.busy && (st.phase === "recording" || st.phase === "paused"));
  const live = $derived(st.phase === "recording" || st.phase === "paused");

  let expanded = $state(true);
  let frame = $state<string | null>(null);
  let frameMtime = $state<number | null>(null);
  const stale = $derived(st.warnings.some((w) => w.code === "warn.windowHidden"));

  const targetLabel = $derived.by(() => {
    const g = st.target;
    if (!g) return "";
    if (g.kind === "window") return [g.app, g.title].filter((x) => x && x.trim()).join(" — ") || t("floating.target.window");
    return t(g.kind === "region" ? "floating.target.region" : "floating.target.screen", { n: g.screen ?? 1 });
  });

  async function poll() {
    try {
      const f = await invoke("preview_frame", { since: frameMtime });
      if (f) {
        frame = f.dataUrl;
        frameMtime = f.mtimeMs;
      }
    } catch {
      /* the backend is between segments */
    }
  }

  async function toggleExpanded() {
    expanded = !expanded;
    try {
      await invoke("set_floating_expanded", { expanded });
    } catch {
      /* no window to resize in tests */
    }
  }

  $effect(() => {
    if (!live || !expanded) return;
    const timer = setInterval(() => void poll(), POLL_MS);
    void poll();
    return () => clearInterval(timer);
  });
  $effect(() => {
    if (st.phase === "idle") {
      frame = null;
      frameMtime = null;
    }
  });

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
      <button type="button" class="chev" aria-label={expanded ? t("floating.collapse") : t("floating.expand")} title={expanded ? t("floating.collapse") : t("floating.expand")} aria-expanded={expanded} onclick={toggleExpanded}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{#if expanded}<path d="M6 15l6-6 6 6" />{:else}<path d="M6 9l6 6 6-6" />{/if}</svg>
      </button>
    </div>
    {#if expanded}
      <div class="panel" data-tauri-drag-region>
        <div class="target" data-tauri-drag-region>
          {#if st.target?.icon}<img class="icon" src={`data:image/png;base64,${st.target.icon}`} alt="" />{/if}
          <span class="tname" data-testid="target" title={targetLabel} data-tauri-drag-region>{targetLabel}</span>
          {#if st.cameraConnected}<span class="chip">{t("floating.withCamera")}</span>{/if}
          <span class="chip live" class:off={stale}><span class="dot small"></span>{t("floating.live")}</span>
        </div>
        <div class="view" class:stale data-tauri-drag-region>
          {#if frame}
            <img class="frame" src={frame} alt={t("floating.live")} />
          {:else}
            <span class="muted waiting">{t("floating.waiting")}</span>
          {/if}
          {#if stale}
            <div class="notice" role="status">{t("floating.stale")}</div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .wrap { height: 100vh; display: flex; flex-direction: column; align-items: center; gap: var(--space-2); padding-top: 8px; box-sizing: border-box; }
  .bar { display: flex; align-items: center; gap: var(--space-3); height: 56px; padding: 0 var(--space-2) 0 var(--space-4); background: var(--color-floating-bg); border: 1px solid var(--color-border); border-radius: var(--radius-pill); color: var(--color-text); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px); white-space: nowrap; }
  .status { display: inline-flex; align-items: center; gap: var(--space-2); font-weight: 600; }
  .status.paused { color: var(--color-warn); }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--color-record); }
  .dot.small { width: 7px; height: 7px; }
  .paused .dot { background: currentColor; }
  .pulse { animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.25; } }
  .time { min-width: 72px; }
  .stop { background: var(--color-record); border-color: var(--color-record); color: #fff; }
  .stop:hover:not(:disabled) { background: var(--color-record-hover); }
  .chev { display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; border: 0; border-radius: 50%; background: transparent; color: var(--color-text-muted); cursor: pointer; }
  .chev:hover { background: var(--color-surface-3); color: var(--color-text); }
  .panel { width: 404px; display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-2); background: var(--color-floating-bg); border: 1px solid var(--color-border); border-radius: var(--radius-l); color: var(--color-text); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px); box-sizing: border-box; }
  .target { display: flex; align-items: center; gap: var(--space-2); height: 24px; font-size: var(--text-sm); }
  .icon { width: 18px; height: 18px; border-radius: 4px; }
  .tname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .chip.live { color: var(--color-record); display: inline-flex; align-items: center; gap: 4px; }
  .chip.live.off { color: var(--color-warn); }
  .chip.live.off .dot { background: currentColor; }
  .view { position: relative; width: 388px; height: 218px; display: flex; align-items: center; justify-content: center; background: #000; border-radius: var(--radius-m); overflow: hidden; }
  .frame { max-width: 100%; max-height: 100%; object-fit: contain; display: block; }
  .view.stale .frame { opacity: 0.35; }
  .waiting { font-size: var(--text-sm); }
  .notice { position: absolute; left: var(--space-3); right: var(--space-3); bottom: var(--space-3); padding: var(--space-2) var(--space-3); border-radius: var(--radius-m); background: var(--color-warn); color: #000; font-size: var(--text-sm); font-weight: 600; text-align: center; }
  .countdown { height: 100vh; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--space-3); background: var(--color-floating-bg); border-radius: var(--radius-l); color: var(--color-text); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px); }
  .num { font-size: var(--text-3xl); font-weight: 600; line-height: 1; }
</style>
