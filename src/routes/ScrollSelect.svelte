<script lang="ts">
  /**
   * Choosing the area of a scrolling screenshot. The window covers the screen and shows a still
   * of it, dimmed; the user drags a rectangle, which shows the still at full brightness.
   */
  import { onMount } from "svelte";
  import { invoke, mediaUrl } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";

  const MIN = 48;
  let still = $state<string | null>(null);
  let from = $state<{ x: number; y: number } | null>(null);
  let to = $state<{ x: number; y: number } | null>(null);
  let dragging = $state(false);
  const rect = $derived(from && to ? { x: Math.min(from.x, to.x), y: Math.min(from.y, to.y), w: Math.abs(to.x - from.x), h: Math.abs(to.y - from.y) } : null);
  const tooSmall = $derived(!!rect && (rect.w < MIN || rect.h < MIN));

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    from = to = { x: e.clientX, y: e.clientY };
    dragging = true;
  }
  function move(e: PointerEvent) {
    if (dragging) to = { x: e.clientX, y: e.clientY };
  }
  async function up() {
    if (!dragging) return;
    dragging = false;
    if (!rect || tooSmall) return;
    try {
      await invoke("scroll_capture_start", { rect });
    } catch {
      await invoke("scroll_capture_cancel");
    }
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") void invoke("scroll_capture_cancel");
  }

  onMount(() => {
    void invoke("scroll_capture_backdrop").then(async ({ path }) => (still = await mediaUrl(path))).catch(() => {});
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="veil" data-testid="scroll-select" onpointerdown={down} onpointermove={move} onpointerup={up}>
  {#if still}<img class="still dim" src={still} alt="" draggable="false" />{/if}
  {#if rect}
    <div class="hole" class:bad={tooSmall} style:left="{rect.x}px" style:top="{rect.y}px" style:width="{rect.w}px" style:height="{rect.h}px">
      {#if still}<img class="still" src={still} alt="" draggable="false" style:left="{-rect.x - 2}px" style:top="{-rect.y - 2}px" />{/if}
    </div>
    <span class="size mono" style:left="{rect.x}px" style:top="{Math.max(4, rect.y - 30)}px">{tooSmall ? t("scroll.select.tooSmall") : t("scroll.select.size", { w: Math.round(rect.w), h: Math.round(rect.h) })}</span>
  {/if}
  {#if !dragging && !rect}<p class="hint">{t("scroll.select.hint")}</p>{/if}
</div>

<style>
  .veil { position: fixed; inset: 0; overflow: hidden; background: var(--color-desk); cursor: crosshair; user-select: none; -webkit-user-select: none; }
  .still { position: absolute; left: 0; top: 0; width: 100vw; height: 100vh; max-width: none; pointer-events: none; }
  .dim { filter: brightness(0.45); }
  .hole { position: absolute; overflow: hidden; border: 2px solid var(--color-marker); box-sizing: border-box; }
  .hole.bad { border-color: var(--color-danger); }
  .size { position: absolute; padding: 2px var(--space-2); border-radius: var(--radius-s); background: var(--color-glass); color: var(--color-glass-fg); font-size: var(--text-sm); white-space: nowrap; pointer-events: none; }
  .hint { position: absolute; left: 50%; top: 18%; transform: translateX(-50%); margin: 0; max-width: 560px; padding: var(--space-3) var(--space-5); border-radius: var(--radius-l); background: var(--color-glass); color: var(--color-glass-fg); font-size: var(--text-md); text-align: center; pointer-events: none; }
</style>
