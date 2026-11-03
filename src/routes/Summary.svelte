<script lang="ts">
  import { recorder } from "../lib/stores/recorder.svelte";
  import { getSettings } from "../lib/stores/settings.svelte";
  import { invoke, subscribe } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";
  import { navigate } from "../lib/router.svelte";
  import { getPlatform } from "../lib/platform.svelte";
  import { toast } from "../lib/stores/toasts.svelte";
  import { codecLabel, formatBytes, formatDuration, formatKbps, formatPercent } from "../lib/format";

  const r = $derived(recorder.lastResult);
  const over = $derived((r?.overBudgetRatio ?? 0) > 0.1);
  let compressing = $state<{ jobId: string; percent: number } | null>(null);
  let compressed = $state<string | null>(null);

  async function reveal() {
    if (r) await invoke("reveal_in_file_manager", { path: r.path });
  }
  async function copyPath() {
    if (!r) return;
    await navigator.clipboard?.writeText(r.path);
    toast("ok", t("toast.pathCopied"), 2000);
  }
  async function compress() {
    if (!r) return;
    const target = getSettings().video.targetMbPerHour * (r.info.durationMs / 3_600_000);
    const jobId = await invoke("compress_to_budget", { path: r.path, targetMb: Math.max(1, Math.round(target)) });
    compressing = { jobId, percent: 0 };
    const un = subscribe("compress://progress", (p) => {
      if (p.jobId !== jobId) return;
      compressing = { jobId, percent: Math.round(p.percent) };
      if (p.done) {
        un();
        compressing = null;
        if (p.error) toast("error", `${t("toast.compressFailed")}: ${p.error}`);
        else {
          compressed = p.output;
          toast("ok", t("summary.compressDone"));
        }
      }
    });
  }
  const revealLabel = $derived(getPlatform() === "macos" ? t("summary.revealMac") : getPlatform() === "windows" ? t("summary.revealWin") : t("summary.revealLinux"));
</script>

<header class="topbar"><strong>{t("summary.title")}</strong><span></span></header>
<div class="page stack" style="max-width: 820px">
  {#if !r}
    <p class="muted">{t("summary.empty")}</p>
    <button type="button" class="btn btn-primary" onclick={() => navigate("/")}>{t("summary.again")}</button>
  {:else}
    <div class="card stack">
      <!-- svelte-ignore a11y_media_has_caption -->
      <video class="preview" controls src={r.path.startsWith("http") ? r.path : undefined} aria-label={t("summary.preview")}></video>
      <strong class="mono" style="word-break: break-all" data-testid="path">{r.path}</strong>
      {#if r.recovered}<span class="chip chip-warn">{t("summary.recovered")}</span>{/if}
      <div class="tiles">
        <div><span class="label">{t("summary.duration")}</span><span class="mono">{formatDuration(r.info.durationMs)}</span></div>
        <div><span class="label">{t("summary.size")}</span><span class="mono">{formatBytes(r.info.bytes)}</span></div>
        <div><span class="label">{t("summary.resolution")}</span><span class="mono">{r.info.width}×{r.info.height}</span></div>
        <div><span class="label">{t("summary.bitrate")}</span><span class="mono">{formatKbps(r.info.bitrateKbps)}</span></div>
        <div><span class="label">{t("summary.encoder")}</span><span class="mono">{codecLabel(r.info.videoCodec)} · {r.info.audioTracks}×{r.info.audioCodec}</span></div>
      </div>
      {#if over}
        <div class="over">
          <strong>{t("summary.overBudget", { percent: formatPercent(r.overBudgetRatio) })}</strong>
          {#if compressed}
            <span class="mono">{compressed}</span>
          {:else}
            <button type="button" class="btn" onclick={compress} disabled={!!compressing}>{compressing ? t("summary.compressing", { percent: compressing.percent }) : t("summary.compress", { mb: getSettings().video.targetMbPerHour })}</button>
          {/if}
        </div>
      {:else}
        <span class="chip chip-ok">{t("summary.withinBudget")}</span>
      {/if}
      <div class="row">
        <button type="button" class="btn" onclick={reveal}>{revealLabel}</button>
        <button type="button" class="btn" onclick={copyPath}>{t("summary.copyPath")}</button>
        <button type="button" class="btn btn-primary" onclick={() => navigate("/")}>{t("summary.again")}</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .preview { width: 100%; aspect-ratio: 16/9; background: var(--color-surface-3); border-radius: var(--radius-m); }
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: var(--space-3); }
  .tiles div { display: flex; flex-direction: column; gap: 2px; padding: var(--space-2) var(--space-3); background: var(--color-surface-2); border-radius: var(--radius-m); }
  .over { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-3); border-radius: var(--radius-m); background: var(--color-warn-soft); color: var(--color-warn); flex-wrap: wrap; }
</style>
