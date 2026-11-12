<script lang="ts">
  /** The outline around the area of a scrolling screenshot, where the system does not draw it natively. Clicks pass through. */
</script>

<div class="frame" data-testid="scroll-frame"></div>

<style>
  .frame { position: fixed; inset: 0; border: 6px solid var(--color-marker); pointer-events: none; }
</style>
