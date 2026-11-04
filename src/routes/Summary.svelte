<script lang="ts">
  /** After recording: watch it, find it, record another. Over budget offers the one fix. */
  import { recorder } from "../lib/stores/recorder.svelte";
  import { getSettings } from "../lib/stores/settings.svelte";
  import { invoke, mediaUrl, subscribe } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";
  import { navigate } from "../lib/router.svelte";
  import { getPlatform } from "../lib/platform.svelte";
  import { toast } from "../lib/stores/toasts.svelte";
  import { formatBytes, formatDuration, formatPercent } from "../lib/format";

  const r = $derived(recorder.lastResult);
  const over = $derived((r?.overBudgetRatio ?? 0) > 0.1);
  const fileName = $derived(r ? (r.path.split(/[\\/]/).pop() ?? r.path) : "");
  let src = $state<string | null>(null);
  let compressing = $state<{ jobId: string; percent: number } | null>(null);
  let compressed = $state<string | null>(null);

  $effect(() => {
    const path = r?.path;
    src = null;
    if (path) void mediaUrl(path).then((u) => (src = u));
  });

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
  const revealLabel = $derived(getPlatform() === "windows" ? t("summary.revealWin") : getPlatform() === "macos" ? t("summary.revealMac") : t("summary.revealLinux"));
</script>

<main class="wrap">
  {#if !r}
    <p class="muted">{t("summary.empty")}</p>
    <button type="button" class="btn btn-primary" onclick={() => navigate("/")}>{t("summary.again")}</button>
  {:else}
    <h1>{t("summary.saved")}</h1>
    {#if src}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video class="preview" controls {src} aria-label={t("summary.preview")}></video>
    {/if}
    <strong class="name" data-testid="path" title={r.path}>{fileName}</strong>
    <span class="muted mono" data-testid="facts">{formatDuration(r.info.durationMs)} · {formatBytes(r.info.bytes)} · {r.info.width}×{r.info.height}</span>
    {#if r.recovered}<span class="chip chip-warn">{t("summary.recovered")}</span>{/if}
    {#if over}
      <div class="over">
        <span>{t("summary.overBudget", { percent: formatPercent(r.overBudgetRatio) })}</span>
        {#if compressed}
          <span class="mono">{compressed}</span>
        {:else}
          <button type="button" class="btn btn-sm" onclick={compress} disabled={!!compressing}>{compressing ? t("summary.compressing", { percent: compressing.percent }) : t("summary.compress", { mb: getSettings().video.targetMbPerHour })}</button>
        {/if}
      </div>
    {/if}
    <div class="row" style="justify-content: center">
      <button type="button" class="btn" onclick={() => r && invoke("reveal_in_file_manager", { path: r.path })}>{revealLabel}</button>
      <button type="button" class="btn btn-primary" onclick={() => navigate("/")}>{t("summary.again")}</button>
    </div>
  {/if}
</main>

<style>
  .wrap { min-height: 100vh; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--space-3); padding: var(--space-5) var(--space-4); text-align: center; }
  .preview { width: min(720px, 100%); max-height: 60vh; border-radius: var(--radius-l); background: var(--color-surface-3); }
  .name { word-break: break-all; }
  .over { display: flex; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border-radius: var(--radius-m); background: var(--color-warn-soft); color: var(--color-warn); flex-wrap: wrap; }
</style>
