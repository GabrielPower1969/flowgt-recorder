<script lang="ts">
  /**
   * The recording window: the bar, a small copy of the front panel (two reels, pause, the
   * read-out, stop), and, under it, the live preview of what is going into the file, labelled
   * with the screen or window being recorded. The preview is the helper's own output frame, so it
   * shows exactly what is recorded, camera square included. A recorded window that leaves the
   * screen (minimised, other Space) is reported by the helper as `warn.windowHidden`; the panel
   * dims and says so until it is back.
   */
  import { onMount } from "svelte";
  import { recorder, cancelRecording, togglePause, stopRecording } from "../lib/stores/recorder.svelte";
  import { getSettings, loadSettings } from "../lib/stores/settings.svelte";
  import { t } from "../lib/stores/i18n.svelte";
  import { formatBytes, formatDuration } from "../lib/format";
  import { invoke } from "../lib/ipc";
  import { getCapabilities } from "../lib/stores/capabilities.svelte";
  import Reel from "../lib/components/Reel.svelte";

  const POLL_MS = 150;

  const st = $derived(recorder.state);
  const paused = $derived(st.phase === "paused");
  const canAct = $derived(!recorder.busy && (st.phase === "recording" || st.phase === "paused"));
  const live = $derived(st.phase === "recording" || st.phase === "paused");
  const spin = $derived(paused ? "paused" : "on");

  let expanded = $state(true);
  // Only a native capture engine writes preview frames; without one the panel would stay empty.
  const previewOffered = $derived(getCapabilities().livePreview);
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
    if (!live || !expanded || !previewOffered) return;
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
    <div class="reels" data-tauri-drag-region>
      <Reel size={64} tape={1} />
      <div class="glass digit" data-tauri-drag-region>
        <span class="mono num" data-testid="countdown" data-tauri-drag-region>{recorder.countdown ?? getSettings().countdownSeconds}</span>
      </div>
      <Reel size={64} />
    </div>
    <button type="button" class="btn btn-sm" onclick={() => cancelRecording()}>{t("common.cancel")}</button>
  </div>
{:else}
  <div class="wrap" data-tauri-drag-region>
    <div class="bar" data-tauri-drag-region>
      <Reel size={40} tape={1} {spin} />
      <div class="well">
        <button type="button" class="key sm" aria-label={paused ? t("floating.resume") : t("floating.pause")} title={paused ? t("floating.resume") : t("floating.pause")} aria-pressed={paused} onclick={() => togglePause()} disabled={!canAct}>
          <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">{#if paused}<path d="M3 1v12l10-6z" fill="currentColor" />{:else}<path d="M2 1h3.5v12H2zM8.5 1H12v12H8.5z" fill="currentColor" />{/if}</svg>
        </button>
      </div>
      <div class="glass readout" data-tauri-drag-region>
        <span class="lamp" class:on={!paused} class:blink={paused}></span>
        <span class="read" data-tauri-drag-region>
          <span class="state" data-tauri-drag-region>{paused ? t("floating.paused") : t("floating.recording")}</span>
          <span class="mono time" data-testid="timer" data-tauri-drag-region>{formatDuration(st.elapsedMs)}</span>
        </span>
        <span class="mono size" data-testid="size" data-tauri-drag-region>{formatBytes(st.bytesWritten)}</span>
      </div>
      <div class="well">
        <button type="button" class="key sm" aria-label={t("floating.stopLong")} title={t("floating.stopLong")} onclick={() => stopRecording()} disabled={!canAct}>
          <svg width="13" height="13" viewBox="0 0 13 13" aria-hidden="true"><path d="M1 1h11v11H1z" fill="currentColor" /></svg>
        </button>
      </div>
      {#if previewOffered}<button type="button" class="chev" aria-label={expanded ? t("floating.collapse") : t("floating.expand")} title={expanded ? t("floating.collapse") : t("floating.expand")} aria-expanded={expanded} onclick={toggleExpanded}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{#if expanded}<path d="M6 15l6-6 6 6" />{:else}<path d="M6 9l6 6 6-6" />{/if}</svg>
      </button>{/if}
      <Reel size={40} tape={st.elapsedMs / 3_600_000} {spin} />
    </div>
    {#if expanded && previewOffered}
      <div class="panel" data-tauri-drag-region>
        <div class="target" data-tauri-drag-region>
          {#if st.target?.icon}<img class="icon" src={`data:image/png;base64,${st.target.icon}`} alt="" />{/if}
          <span class="tname" data-testid="target" title={targetLabel} data-tauri-drag-region>{targetLabel}</span>
          {#if st.cameraConnected}<span class="chip">{t("floating.withCamera")}</span>{/if}
          <span class="chip live"><span class="lamp" class:on={!stale}></span>{t("floating.live")}</span>
        </div>
        <div class="view" class:stale data-tauri-drag-region>
          {#if frame}
            <img class="frame" src={frame} alt={t("floating.live")} />
          {:else}
            <span class="waiting">{t("floating.waiting")}</span>
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
  .wrap { height: 100vh; display: flex; flex-direction: column; align-items: center; gap: var(--space-2); padding-top: 6px; box-sizing: border-box; }
  .bar { display: flex; align-items: center; gap: 6px; width: 408px; height: 60px; padding: 0 var(--space-2); border-radius: var(--radius-l); background: var(--color-plate); color: var(--color-plate-ink); box-shadow: inset 0 1px 0 var(--color-plate-hi), inset 0 0 0 1px var(--color-plate-lo); white-space: nowrap; }
  .well { flex: none; }
  .key.sm { width: 44px; height: 44px; }
  .readout { flex: 1; min-width: 0; height: 48px; display: flex; align-items: center; gap: var(--space-2); padding: 0 var(--space-2); }
  .read { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.15; }
  .state { font-size: var(--text-xs); color: var(--color-glass-muted); }
  .time { font-size: 15px; font-weight: 600; }
  .size { font-size: var(--text-xs); color: var(--color-glass-muted); }
  .chev { flex: none; display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; border: 0; border-radius: var(--radius-m); background: transparent; color: var(--color-plate-muted); cursor: pointer; }
  .chev:hover { background: var(--color-plate-hi); color: var(--color-plate-ink); }
  .panel { width: 408px; display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-2); border-radius: var(--radius-l); background: var(--color-plate); color: var(--color-plate-ink); box-shadow: inset 0 1px 0 var(--color-plate-hi), inset 0 0 0 1px var(--color-plate-lo); box-sizing: border-box; }
  .target { display: flex; align-items: center; gap: var(--space-2); height: 24px; font-size: var(--text-sm); }
  .icon { width: 18px; height: 18px; border-radius: var(--radius-s); }
  .tname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .chip { background: var(--color-plate-hi); color: var(--color-plate-ink); }
  .chip.live { display: inline-flex; align-items: center; gap: var(--space-1); }
  .view { position: relative; width: 392px; height: 220px; display: flex; align-items: center; justify-content: center; background: var(--color-glass); border-radius: var(--radius-m); overflow: hidden; }
  .frame { max-width: 100%; max-height: 100%; object-fit: contain; display: block; }
  .view.stale .frame { opacity: 0.35; }
  .waiting { font-size: var(--text-sm); color: var(--color-glass-muted); }
  .notice { position: absolute; left: var(--space-3); right: var(--space-3); bottom: var(--space-3); padding: var(--space-2) var(--space-3); border-radius: var(--radius-m); background: var(--color-warn); color: var(--color-warn-fg); font-size: var(--text-sm); font-weight: 600; text-align: center; }
  .countdown { height: 100vh; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--space-3); border-radius: var(--radius-l); background: var(--color-plate); color: var(--color-plate-ink); box-shadow: inset 0 1px 0 var(--color-plate-hi), inset 0 0 0 1px var(--color-plate-lo); }
  .reels { display: flex; align-items: center; gap: var(--space-5); }
  .digit { width: 120px; height: 96px; display: flex; align-items: center; justify-content: center; }
  .num { font-size: var(--text-3xl); font-weight: 600; line-height: 1; }
</style>
