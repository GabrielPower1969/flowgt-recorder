<script lang="ts">
  import type { PermissionKind, PermissionStatus } from "../lib/types";
  import { invoke } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";
  import { navigate } from "../lib/router.svelte";
  import { updateSettings } from "../lib/stores/settings.svelte";
  import { getPlatform } from "../lib/platform.svelte";
  import { onMount } from "svelte";

  const steps: PermissionKind[] = ["screen", "camera", "microphone"];
  let current = $state(0);
  let status = $state<PermissionStatus | null>(null);
  let checking = $state(false);
  const kind = $derived(steps[current]);
  const permState = $derived(status?.[kind] ?? "notDetermined");

  async function recheck() {
    checking = true;
    try {
      status = await invoke("check_permissions");
    } finally {
      checking = false;
    }
  }
  async function request() {
    checking = true;
    try {
      const r = await invoke("request_permission", { kind });
      status = { ...(status ?? { screen: "notDetermined", camera: "notDetermined", microphone: "notDetermined" }), [kind]: r };
    } finally {
      checking = false;
    }
  }
  async function next() {
    if (current < steps.length - 1) current += 1;
    else await finish();
  }
  async function finish() {
    await updateSettings((s) => { s.onboardingDone = true; });
    navigate("/");
  }
  onMount(() => { void recheck(); });
</script>

<div class="page stack" style="max-width: 760px">
  <h1>{t("onboarding.title")}</h1>
  <p class="muted">{t("onboarding.subtitle")}</p>
  <ol class="steps" aria-label={t("a11y.steps")}>
    {#each steps as s, i (s)}
      <li class:active={i === current} class:done={status?.[s] === "granted"} aria-current={i === current ? "step" : undefined}>
        <span class="n">{i + 1}</span>{t(`onboarding.step.${s}`)}
      </li>
    {/each}
  </ol>
  <div class="card stack">
    <div class="row" style="justify-content: space-between">
      <h2>{t(`onboarding.step.${kind}`)}</h2>
      <span class="chip" class:chip-ok={permState === "granted"} class:chip-warn={permState === "denied"} data-testid="perm-state">{t(`permission.${permState}`)}</span>
    </div>
    <p class="muted">{t(`onboarding.desc.${kind}`)}</p>
    <p class="faint">{t("onboarding.stepOf", { current: current + 1, total: steps.length })}</p>
    <div class="row">
      {#if permState !== "granted"}
        <button type="button" class="btn btn-primary" onclick={request} disabled={checking}>{t(`onboarding.step.${kind}`)} · {t("common.ok")}</button>
        <button type="button" class="btn" onclick={() => invoke("open_privacy_settings", { kind })}>{t("onboarding.openSettings")}</button>
        <button type="button" class="btn" onclick={recheck} disabled={checking}>{t("onboarding.recheck")}</button>
        <button type="button" class="btn btn-ghost" onclick={next}>{t("common.skip")}</button>
      {:else}
        <button type="button" class="btn btn-primary" onclick={next}>{current === steps.length - 1 ? t("onboarding.finish") : t("common.next")}</button>
      {/if}
    </div>
  </div>
  {#if getPlatform() === "windows"}<p class="faint">{t("onboarding.windowsNote")}</p>{/if}
</div>

<style>
  .steps { display: flex; gap: var(--space-4); list-style: none; padding: 0; margin: 0; }
  .steps li { display: flex; align-items: center; gap: var(--space-2); color: var(--color-text-muted); }
  .steps li.active { color: var(--color-text); font-weight: 600; }
  .n { width: 24px; height: 24px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; background: var(--color-surface-3); font-size: var(--text-xs); }
  .active .n { background: var(--color-accent); color: var(--color-accent-fg); }
  .done .n { background: var(--color-ok); color: #fff; }
</style>
