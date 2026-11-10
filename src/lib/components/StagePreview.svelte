<script lang="ts">
  /** Screen thumbnail + draggable, resizable PiP. Geometry is kept in output pixels (settings), the stage scales it. */
  import type { CameraSettings, Corner } from "../types";
  import { t } from "../stores/i18n.svelte";

  let {
    camera,
    outW = 1920,
    outH = 1080,
    stream = null,
    onchange,
  }: { camera: CameraSettings; outW?: number; outH?: number; stream?: MediaStream | null; onchange: (patch: Partial<CameraSettings>) => void } = $props();

  let stageW = $state(640);
  let stageEl = $state<HTMLDivElement | null>(null);
  const scale = $derived(stageW / outW);
  const stageH = $derived(Math.round(outH * scale));

  /** Same arithmetic as pip::layout in the core. */
  export function layout(c: CameraSettings, w: number, h: number) {
    const pct = Math.min(50, Math.max(10, c.sizePct));
    const size = Math.max(2, Math.min(Math.floor((h * pct) / 100) & ~1, Math.min(w, h) & ~1));
    const m = c.margin;
    const right = w - size - m, bottom = h - size - m;
    const anchor: Record<Corner, [number, number]> = { "top-left": [m, m], "top-right": [right, m], "bottom-left": [m, bottom], "bottom-right": [right, bottom] };
    const [ax, ay] = anchor[c.corner];
    const x = Math.min(Math.max(ax + c.offsetX, 0), w - size);
    const y = Math.min(Math.max(ay + c.offsetY, 0), h - size);
    return { size, x, y };
  }
  const geo = $derived(layout(camera, outW, outH));

  let drag: { startX: number; startY: number; ox: number; oy: number } | null = null;
  function onPointerDown(e: PointerEvent) {
    drag = { startX: e.clientX, startY: e.clientY, ox: camera.offsetX, oy: camera.offsetY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    onchange({ offsetX: Math.round(drag.ox + (e.clientX - drag.startX) / scale), offsetY: Math.round(drag.oy + (e.clientY - drag.startY) / scale) });
  }
  function onPointerUp() {
    drag = null;
  }
  function onKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 0 : 16;
    const grow = e.shiftKey ? 2 : 0;
    const map: Record<string, Partial<CameraSettings>> = {
      ArrowLeft: e.shiftKey ? { sizePct: camera.sizePct - grow } : { offsetX: camera.offsetX - step },
      ArrowRight: e.shiftKey ? { sizePct: camera.sizePct + grow } : { offsetX: camera.offsetX + step },
      ArrowUp: e.shiftKey ? { sizePct: camera.sizePct + grow } : { offsetY: camera.offsetY - step },
      ArrowDown: e.shiftKey ? { sizePct: camera.sizePct - grow } : { offsetY: camera.offsetY + step },
    };
    const patch = map[e.key];
    if (!patch) return;
    e.preventDefault();
    if (patch.sizePct !== undefined) patch.sizePct = Math.min(50, Math.max(10, patch.sizePct));
    onchange(patch);
  }
  function attach(node: HTMLVideoElement, s: MediaStream | null) {
    node.srcObject = s;
    return { update(next: MediaStream | null) { node.srcObject = next; } };
  }
  $effect(() => {
    if (!stageEl || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => { if (stageEl) stageW = stageEl.clientWidth || stageW; });
    ro.observe(stageEl);
    stageW = stageEl.clientWidth || stageW;
    return () => ro.disconnect();
  });
</script>

<div class="stage" bind:this={stageEl} style:height="{stageH}px" role="img" aria-label={t("a11y.stage")}>
  <div class="grid"></div>
  {#if camera.enabled}
    <div
      class="pip shape-{camera.shape}"
      class:mirror={camera.mirror}
      role="slider"
      tabindex="0"
      aria-label={t("a11y.pip")}
      aria-valuenow={camera.sizePct}
      aria-valuemin="10"
      aria-valuemax="50"
      data-x={geo.x}
      data-y={geo.y}
      data-size={geo.size}
      style:left="{geo.x * scale}px"
      style:top="{geo.y * scale}px"
      style:width="{geo.size * scale}px"
      style:height="{geo.size * scale}px"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onkeydown={onKey}
    >
      {#if stream}
        <video autoplay muted playsinline use:attach={stream}></video>
      {:else}
        <span class="faint">{t("main.camera.noPreview")}</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .stage { position: relative; width: 100%; background: var(--color-surface-3); border-radius: var(--radius-m); overflow: hidden; border: 1px solid var(--color-border); }
  .grid { position: absolute; inset: 0; background-image: linear-gradient(var(--color-border) 1px, transparent 1px), linear-gradient(90deg, var(--color-border) 1px, transparent 1px); background-size: 12.5% 12.5%; opacity: 0.6; }
  .pip { position: absolute; display: flex; align-items: center; justify-content: center; overflow: hidden; background: var(--color-surface); outline: 2px solid var(--color-accent); cursor: grab; touch-action: none; user-select: none; }
  .pip:focus-visible { outline: 3px solid var(--color-focus); }
  .shape-rounded { border-radius: 12.5%; }
  .shape-circle { border-radius: 50%; }
  .pip video { width: 100%; height: 100%; object-fit: cover; }
  .mirror video { transform: scaleX(-1); }
</style>
