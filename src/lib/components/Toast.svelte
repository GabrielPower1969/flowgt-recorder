<script lang="ts">
  import { getToasts, dismiss } from "../stores/toasts.svelte";
  import { t } from "../stores/i18n.svelte";
</script>

<div class="toasts" role="region" aria-label={t("a11y.toasts")} aria-live="polite">
  {#each getToasts() as item (item.id)}
    <div class="toast toast-{item.kind}" role={item.kind === "error" ? "alert" : "status"}>
      <span>{item.text}</span>
      <button type="button" class="btn btn-ghost btn-sm" aria-label={t("a11y.dismiss")} onclick={() => dismiss(item.id)}>×</button>
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: var(--space-4); bottom: var(--space-4); display: flex; flex-direction: column; gap: var(--space-2); z-index: 50; max-width: min(420px, calc(100vw - 32px)); }
  .toast { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); padding: var(--space-2) var(--space-3); border-radius: var(--radius-m); background: var(--color-surface); border: 1px solid var(--color-border); box-shadow: var(--shadow-2); font-size: var(--text-sm); }
  .toast-warn { border-color: var(--color-warn); background: var(--color-warn-soft); }
  .toast-error { border-color: var(--color-danger); background: var(--color-record-soft); }
  .toast-ok { border-color: var(--color-ok); background: var(--color-ok-soft); }
</style>
