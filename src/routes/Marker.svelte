<script lang="ts">
  /** Outline on the real screen, so it is obvious which one will be recorded. Clicks pass through. */
  import { t } from "../lib/stores/i18n.svelte";

  type MarkerWindow = Window & { __markerN?: number; __setMarker?: (n: number) => void };
  const w = window as MarkerWindow;
  let n = $state(w.__markerN ?? 1);
  w.__setMarker = (v) => (n = v);
</script>

<div class="frame" data-testid="marker">
  <div class="label">{t("marker.label", { n })}</div>
</div>

<style>
  .frame { position: fixed; inset: 0; display: flex; align-items: flex-start; justify-content: center; border: 8px solid var(--color-marker); pointer-events: none; }
  .label { margin-top: var(--space-6); padding: var(--space-3) var(--space-5); border-radius: var(--radius-pill); background: var(--color-marker); color: var(--color-marker-fg); font-size: 24px; font-weight: 600; }
</style>
