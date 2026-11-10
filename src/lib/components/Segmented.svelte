<script lang="ts" generics="T extends string">
  interface Option { value: T; label: string; disabled?: boolean; hint?: string }
  let { options, value, onchange, label }: { options: Option[]; value: T; onchange: (v: T) => void; label: string } = $props();
</script>

<div class="segmented" role="group" aria-label={label}>
  {#each options as o (o.value)}
    <button type="button" aria-pressed={o.value === value} disabled={o.disabled} title={o.hint} onclick={() => onchange(o.value)}>{o.label}</button>
  {/each}
</div>
