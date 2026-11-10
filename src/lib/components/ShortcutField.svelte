<script lang="ts">
  import { t } from "../stores/i18n.svelte";
  import { getPlatform } from "../platform.svelte";
  import { shortcutParts } from "../format";

  let { value, label, onchange }: { value: string; label: string; onchange: (v: string) => void } = $props();
  let editing = $state(false);
  let error = $state("");

  export function accelerator(e: Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">): string | null {
    const mods: string[] = [];
    if (e.metaKey || e.ctrlKey) mods.push("CmdOrCtrl");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (["Meta", "Control", "Alt", "Shift"].includes(e.key)) return null;
    if (mods.length === 0) return "";
    let key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
    if (e.code.startsWith("Key")) key = e.code.slice(3);
    if (e.code.startsWith("Digit")) key = e.code.slice(5);
    return [...mods, key].join("+");
  }
  function onKey(e: KeyboardEvent) {
    if (!editing) return;
    e.preventDefault();
    if (e.key === "Escape") { editing = false; error = ""; return; }
    const acc = accelerator(e);
    if (acc === null) return;
    if (acc === "") { error = t("shortcut.needModifier"); return; }
    error = "";
    editing = false;
    onchange(acc);
  }
</script>

<div class="field">
  <span class="label" id="sc-{label}">{label}</span>
  <button type="button" class="btn sc" aria-labelledby="sc-{label}" aria-describedby={error ? `sc-err-${label}` : undefined} onclick={() => (editing = !editing)} onkeydown={onKey} onblur={() => (editing = false)}>
    {#if editing}
      <span class="muted">{t("shortcut.press")}</span>
    {:else}
      {#each shortcutParts(value, getPlatform()) as part, i (i)}<span class="kbd">{part}</span>{/each}
      <span class="sr-only">{t("shortcut.edit")}</span>
    {/if}
  </button>
  {#if error}<span id="sc-err-{label}" class="err">{error}</span>{/if}
  <span class="faint">{t("settings.shortcuts.hint")}</span>
</div>

<style>
  .sc { justify-content: flex-start; gap: var(--space-1); min-width: 180px; }
  .err { color: var(--color-danger); font-size: var(--text-sm); }
</style>
