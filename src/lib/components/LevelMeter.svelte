<script lang="ts">
  import { t } from "../stores/i18n.svelte";
  let { level = 0, compact = false }: { level?: number; compact?: boolean } = $props();
  const pct = $derived(Math.round(Math.max(0, Math.min(1, level)) * 100));
  const zone = $derived(pct > 85 ? "hot" : "ok");
</script>

<div class="meter" class:compact role="meter" aria-label={t("a11y.meter")} aria-valuemin="0" aria-valuemax="100" aria-valuenow={pct}>
  <div class="fill {zone}" style:width="{pct}%"></div>
</div>

<style>
  .meter { height: 8px; width: 100%; background: var(--color-surface-3); border-radius: var(--radius-pill); overflow: hidden; }
  .compact { height: 6px; width: 48px; }
  .fill { height: 100%; background: var(--color-text); transition: width 60ms linear; }
  .fill.hot { background: var(--color-record); }
</style>
