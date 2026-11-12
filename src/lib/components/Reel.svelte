<script lang="ts">
  /**
   * One tape reel: a clear flange with three windows over the tape pack. `tape` is how full the
   * reel is, 0..1. The pack's radius grows with the square root of that, because tape length goes
   * with the area of the pack. The lettering on the hub stays still so it can be read while the
   * flange turns.
   */
  let { size, tape = 0, spin = "off", label = "", brand = "", value = "", caption = "" }: {
    size: number;
    tape?: number;
    spin?: "off" | "on" | "paused";
    /** What the reel stands for, for screen readers. Empty = decoration. */
    label?: string;
    brand?: string;
    value?: string;
    caption?: string;
  } = $props();

  const HUB = 60;
  const FULL = 150;
  const radius = $derived.by(() => {
    if (!(tape > 0)) return 0;
    const r = Math.sqrt(HUB * HUB + (FULL * FULL - HUB * HUB) * Math.min(1, tape));
    // A thin ring shows as soon as there is anything on the reel.
    return Math.max(r, HUB + 6);
  });
</script>

<svg class="reel" role={label ? "img" : undefined} aria-label={label || undefined} aria-hidden={label ? undefined : "true"} width={size} height={size} viewBox="-188 -188 376 376">
  {#if radius > 0}
    <circle class="tape" r={radius} />
    {#if radius > 80}<circle class="ring" r={radius - 9} />{/if}
    {#if radius > 110}<circle class="ring" r={radius - 34} />{/if}
  {/if}
  <g class="rot" class:on={spin !== "off"} class:paused={spin === "paused"}>
    <path class="flange" fill-rule="evenodd" d="M0 -186A186 186 0 1 1 0 186A186 186 0 1 1 0 -186ZM27.2 -83.7L47 -144.6Q51.3 -157.9 64.4 -153A166 166 0 0 1 164.7 20.7Q162.4 34.5 148.7 31.6L86.1 18.3Q72.4 15.4 74 1.5A74 74 0 0 0 35.7 -64.8Q22.9 -70.4 27.2 -83.7ZM58.9 65.4L101.7 113Q111.1 123.4 100.3 132.3A166 166 0 0 1 -100.3 132.3Q-111.1 123.4 -101.7 113L-58.9 65.4Q-49.5 55 -38.3 63.3A74 74 0 0 0 38.3 63.3Q49.5 55 58.9 65.4ZM-86.1 18.3L-148.7 31.6Q-162.4 34.5 -164.7 20.7A166 166 0 0 1 -64.4 -153Q-51.3 -157.9 -47 -144.6L-27.2 -83.7Q-22.9 -70.4 -35.7 -64.8A74 74 0 0 0 -74 1.5Q-72.4 15.4 -86.1 18.3Z" />
    <circle class="rim" r="187.5" />
    <circle class="hub" r={HUB} />
    <path class="tick" d="M0 -57V-52M49.4 28.5L45 26M-49.4 28.5L-45 26" />
  </g>
  <circle class="spindle" r="11" />
  <circle class="pin" r="3.5" />
  {#if brand}<text class="brand" y="-24">{brand}</text>{/if}
  {#if value}<text class="value" y="34">{value}</text>{/if}
  {#if caption}<text class="caption" y="48">{caption}</text>{/if}
</svg>

<style>
  .reel { display: block; flex: none; }
  .tape { fill: var(--color-tape); }
  .ring { fill: none; stroke: var(--color-tape-ring); vector-effect: non-scaling-stroke; }
  .rot { transform-origin: 0 0; }
  .rot.on { animation: reel-turn var(--duration-reel) linear infinite; }
  .rot.paused { animation-play-state: paused; }
  @keyframes reel-turn { to { transform: rotate(360deg); } }
  .flange { fill: var(--color-flange); stroke: var(--color-flange-line); stroke-width: 1.5px; vector-effect: non-scaling-stroke; }
  .rim { fill: none; stroke: var(--color-flange-shadow); vector-effect: non-scaling-stroke; }
  .hub { fill: var(--color-hub); stroke: var(--color-hub-line); vector-effect: non-scaling-stroke; }
  .tick { fill: none; stroke: var(--color-hub-muted); stroke-width: 1.5px; stroke-linecap: round; vector-effect: non-scaling-stroke; }
  .spindle { fill: var(--color-plate); stroke: var(--color-plate-lo); vector-effect: non-scaling-stroke; }
  .pin { fill: var(--color-plate-lo); }
  text { text-anchor: middle; fill: var(--color-hub-ink); }
  .brand { font-size: var(--text-sm); font-weight: 600; letter-spacing: 1.5px; }
  .value { font-family: var(--font-mono); font-size: 13px; font-weight: 600; }
  .caption { font-size: var(--text-xs); fill: var(--color-hub-muted); }
</style>
