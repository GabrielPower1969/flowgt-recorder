<script lang="ts">
  /**
   * A selector on the front panel: a lever (or, with `round`, a knob) that points at one of the
   * marks printed on the arc above it. Pressing it is the caller's business; the caller moves
   * `index` to the next mark.
   */
  interface Mark { label: string; off?: boolean }
  let { marks, index, label, round = false, pressed = undefined, disabled = false, onclick, testid = undefined }: {
    marks: Mark[];
    index: number;
    label: string;
    round?: boolean;
    pressed?: boolean;
    disabled?: boolean;
    onclick?: () => void;
    testid?: string;
  } = $props();

  const CX = 60;
  const CY = 70;
  // A knob with only two marks keeps them closer to the top, clear of its own skirt.
  const span = $derived(round ? (marks.length > 2 ? 66 : 48) : 40);
  const arcR = $derived(round ? 44 : 48);
  const textR = $derived(round ? 60 : 64);
  const at = (r: number, deg: number): [number, number] => {
    const a = (deg * Math.PI) / 180;
    return [Math.round((CX + r * Math.sin(a)) * 10) / 10, Math.round((CY - r * Math.cos(a)) * 10) / 10];
  };
  const angles = $derived(marks.map((_, i) => (marks.length < 2 ? 0 : -span + (2 * span * i) / (marks.length - 1))));
  const scale = $derived.by(() => {
    const [x0, y0] = at(arcR, -span);
    const [x1, y1] = at(arcR, span);
    const ticks = angles.map((a) => `M${at(arcR, a).join(" ")}L${at(arcR + 5, a).join(" ")}`).join("");
    return `M${x0} ${y0}A${arcR} ${arcR} 0 0 1 ${x1} ${y1}${ticks}`;
  });
  const angle = $derived(angles[Math.max(0, Math.min(index, angles.length - 1))] ?? 0);
</script>

<button type="button" class="lever" aria-label={label} aria-pressed={pressed} title={label} {disabled} {onclick} data-testid={testid}>
  <svg width="120" height="104" viewBox="0 0 120 104" aria-hidden="true">
    <path class="scale" d={scale} />
    {#each marks as m, i (i)}
      <text class:off={m.off} x={at(textR, angles[i])[0]} y={at(textR, angles[i])[1] + 4}>{m.label}</text>
    {/each}
    {#if round}
      <ellipse class="shadow" cx="60" cy="74" rx="29" ry="28" />
      <circle class="skirt" cx="60" cy="70" r="28" />
      <g class="turn" style:transform="translate(60px, 70px) rotate({angle}deg)"><circle class="cap" r="24" /><path class="capmark" d="M0 -21V-9" /></g>
    {:else}
      <ellipse class="shadow" cx="60" cy="74" rx="22" ry="21" />
      <g class="turn" style:transform="translate(60px, 70px) rotate({angle}deg)"><path class="body" d="M-5 -37Q0 -40 5 -37L19 -10A21.5 21.5 0 1 1 -19 -10Z" /><path class="mark" d="M0 -33V-17" /></g>
    {/if}
  </svg>
</button>

<style>
  .lever { display: block; width: 120px; height: 104px; padding: 0; border: 0; border-radius: var(--radius-l); background: transparent; color: var(--color-plate-ink); }
  .lever:disabled { opacity: 1; cursor: default; }
  .scale { fill: none; stroke: var(--color-plate-ink); }
  text { text-anchor: middle; font-size: var(--text-sm); fill: var(--color-plate-ink); }
  text.off { fill: var(--color-plate-off); }
  .shadow { fill: var(--color-knob-shadow); }
  .turn { transition: transform var(--duration-normal) var(--ease); }
  .body, .skirt { fill: var(--color-knob); stroke: var(--color-knob-line); }
  .mark { fill: none; stroke: var(--color-knob-mark); stroke-width: 2px; stroke-linecap: round; }
  .cap { fill: var(--color-knob-dark); }
  .capmark { fill: none; stroke: var(--color-knob-dark-mark); stroke-width: 2.5px; stroke-linecap: round; }
</style>
