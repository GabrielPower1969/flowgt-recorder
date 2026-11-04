<script lang="ts">
  /** Home: self-view, mic and camera toggles, one button. Everything else lives in Settings. */
  import { onMount } from "svelte";
  import type { AppInfo, Device, DeviceList, PermissionKind, PermissionStatus, Settings } from "../lib/types";
  import { invoke } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";
  import { navigate } from "../lib/router.svelte";
  import { updateSettings } from "../lib/stores/settings.svelte";
  import { recorder } from "../lib/stores/recorder.svelte";
  import { openCamera, stopStream } from "../lib/media/camera";
  import { startLevelMeter } from "../lib/media/level";
  import { driveState, signInToGoogle } from "../lib/stores/drive.svelte";
  import LevelMeter from "../lib/components/LevelMeter.svelte";
  import ScreenPicker from "../lib/components/ScreenPicker.svelte";

  let { settings }: { settings: Settings } = $props();
  let devices = $state<DeviceList | null>(null);
  let info = $state<AppInfo | null>(null);
  let perms = $state<PermissionStatus | null>(null);
  let preview = $state<MediaStream | null>(null);
  let picking = $state(false);
  let level = $state(0);

  const camAllowed = $derived(perms?.camera === "granted" || perms?.camera === "notApplicable");
  const camDenied = $derived(perms?.camera === "denied");
  const micAllowed = $derived(perms?.microphone === "granted" || perms?.microphone === "notApplicable");
  const defaultCamera = $derived(devices?.cameras.find((c) => c.isDefault)?.name ?? devices?.cameras[0]?.name ?? null);
  const defaultMic = $derived(devices?.microphones.find((m) => m.isDefault)?.name ?? null);
  /** Settings keep the device NAME (avfoundation indices shift); null = follow the system. Older
   *  files hold an index, and a device can be unplugged: both must read as a real choice or default. */
  const chosen = (list: Device[] | undefined, stored: string | null) => list?.find((d) => d.name === stored || d.id === stored)?.name ?? "";
  const cameraChoice = $derived(chosen(devices?.cameras, settings.camera.deviceId));
  const micChoice = $derived(chosen(devices?.microphones, settings.audio.micDeviceId));
  const followLabel = (name: string | null) => (name ? t("home.followSystem", { name }) : t("home.followSystemPlain"));
  const canRecordOutput = $derived(!!info?.systemAudioAvailable || (devices?.loopbacks.length ?? 0) > 0);
  const outputValue = $derived(!canRecordOutput || settings.audio.systemAudio.kind === "off" ? "off" : settings.audio.systemAudio.kind === "device" ? `device:${settings.audio.systemAudio.id}` : "auto");

  function setOutput(v: string) {
    void updateSettings((s) => {
      s.audio.systemAudio = v.startsWith("device:") ? { kind: "device", id: v.slice(7) } : v === "off" ? { kind: "off" } : { kind: "auto" };
    });
  }

  /** Ask the first time something is switched on, the way a meeting page does. */
  async function ensure(kind: PermissionKind) {
    if (perms && perms[kind] === "notDetermined") {
      const r = await invoke("request_permission", { kind });
      perms = { ...perms, [kind]: r };
    }
  }
  async function toggleCamera() {
    const on = !settings.camera.enabled;
    await updateSettings((s) => { s.camera.enabled = on; });
    if (on) await ensure("camera");
  }
  async function toggleMic() {
    const on = !settings.audio.micEnabled;
    await updateSettings((s) => { s.audio.micEnabled = on; });
    if (on) await ensure("microphone");
  }

  function attach(node: HTMLVideoElement, s: MediaStream | null) {
    node.srcObject = s;
    return { update(next: MediaStream | null) { node.srcObject = next; } };
  }

  // The camera preview belongs to the webview only while idle; ffmpeg needs the camera to record (D6).
  $effect(() => {
    const wanted = settings.camera.enabled && camAllowed && !recorder.isActive;
    const label = cameraChoice || defaultCamera;
    if (!wanted) return;
    let cancelled = false;
    let opened: MediaStream | null = null;
    void openCamera(label).then((s) => {
      if (cancelled) return stopStream(s);
      opened = s;
      preview = s;
    });
    return () => {
      cancelled = true;
      stopStream(opened);
      preview = null;
    };
  });

  // The level meter shows that the chosen microphone really hears you.
  $effect(() => {
    if (!settings.audio.micEnabled || !micAllowed || recorder.isActive) return;
    const label = micChoice || defaultMic;
    let cancelled = false;
    let handle: { stop(): void } | null = null;
    void startLevelMeter(label, (v) => (level = v)).then((h) => {
      if (cancelled) h?.stop();
      else handle = h;
    });
    return () => {
      cancelled = true;
      handle?.stop();
      level = 0;
    };
  });

  onMount(() => {
    void (async () => {
      void invoke("get_app_info").then((i) => (info = i));
      [devices, perms] = await Promise.all([invoke("list_devices"), invoke("check_permissions")]);
      if (settings.camera.enabled) await ensure("camera");
      if (settings.audio.micEnabled) await ensure("microphone");
    })();
  });
</script>

<div class="top">
  {#if driveState.status.email}
    <span class="faint" data-testid="account">{driveState.status.email}</span>
  {:else}
    <button type="button" class="btn btn-ghost btn-sm" onclick={signInToGoogle} disabled={driveState.signingIn}>{driveState.signingIn ? t("home.signingIn") : t("home.signIn")}</button>
  {/if}
  <button type="button" class="btn btn-ghost btn-icon" aria-label={t("common.settings")} onclick={() => navigate("/settings")}>
  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.8 1.2V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-2.9-1.1l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0-1.2-2.8H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.1-2.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h.1a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 2.9 1.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v.1a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></svg>
</button>
</div>

<main class="home">
  <div class="tile">
    {#if settings.camera.enabled && preview}
      <video class:mirror={settings.camera.mirror} autoplay muted playsinline use:attach={preview}></video>
    {:else if settings.camera.enabled && camDenied}
      <div class="stack" style="align-items: center">
        <span>{t("home.camDenied")}</span>
        <button type="button" class="btn btn-sm" onclick={() => invoke("open_privacy_settings", { kind: "camera" })}>{t("home.openSettings")}</button>
      </div>
    {:else if settings.camera.enabled}
      <span>{t("main.camera.noPreview")}</span>
    {:else}
      <span>{t("home.camIsOff")}</span>
    {/if}
  </div>

  <div class="devices">
    <label class="field">
      <span>{t("home.camera")}</span>
      <select class="input" disabled={!settings.camera.enabled} onchange={(e) => { const v = (e.target as HTMLSelectElement).value || null; void updateSettings((s) => { s.camera.deviceId = v; }); }}>
        <option value="">{followLabel(defaultCamera)}</option>
        {#each devices?.cameras ?? [] as c (c.id)}<option value={c.name} selected={c.name === cameraChoice}>{c.name}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>{t("home.mic")}</span>
      <select class="input" disabled={!settings.audio.micEnabled} onchange={(e) => { const v = (e.target as HTMLSelectElement).value || null; void updateSettings((s) => { s.audio.micDeviceId = v; }); }}>
        <option value="">{followLabel(defaultMic)}</option>
        {#each devices?.microphones ?? [] as m (m.id)}<option value={m.name} selected={m.name === micChoice}>{m.name}</option>{/each}
      </select>
      <LevelMeter {level} />
    </label>
    <label class="field">
      <span>{t("home.output")}</span>
      <select class="input" disabled={!canRecordOutput} onchange={(e) => setOutput((e.target as HTMLSelectElement).value)}>
        {#if canRecordOutput}<option value="auto" selected={outputValue === "auto"}>{t("home.followSystemPlain")}</option>{/if}
        <option value="off" selected={outputValue === "off"}>{t("home.noOutput")}</option>
        {#each devices?.loopbacks ?? [] as d (d.id)}<option value="device:{d.id}" selected={outputValue === `device:${d.id}`}>{d.name}</option>{/each}
      </select>
    </label>
  </div>

  <div class="controls">
    <button type="button" class="round" class:off={!settings.audio.micEnabled} aria-pressed={settings.audio.micEnabled} aria-label={settings.audio.micEnabled ? t("home.micOn") : t("home.micOff")} title={settings.audio.micEnabled ? t("home.micOn") : t("home.micOff")} onclick={toggleMic}>
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3" />{#if !settings.audio.micEnabled}<path d="M4 4l16 16" />{/if}</svg>
    </button>
    <button type="button" class="round" class:off={!settings.camera.enabled} aria-pressed={settings.camera.enabled} aria-label={settings.camera.enabled ? t("home.camOn") : t("home.camOff")} title={settings.camera.enabled ? t("home.camOn") : t("home.camOff")} onclick={toggleCamera}>
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="6" width="13" height="12" rx="2" /><path d="M16 10l5-3v10l-5-3" />{#if !settings.camera.enabled}<path d="M4 4l16 16" />{/if}</svg>
    </button>
    <button type="button" class="btn btn-record" data-testid="rec" disabled={recorder.isActive} onclick={() => (picking = true)}>
      <span class="dot"></span>{t("home.record")}
    </button>
  </div>
  <p class="faint hint">{t("home.hint", { mb: settings.video.targetMbPerHour, dir: settings.outputDir })}{#if driveState.status.email}<br /><span data-testid="drive-line">{t("home.driveLine", { folder: "FlowGT Recorder" })}</span>{/if}</p>
</main>

{#if picking}
  <ScreenPicker onclose={() => (picking = false)} />
{/if}

<style>
  .top { position: fixed; top: var(--space-3); right: var(--space-3); z-index: 5; display: flex; align-items: center; gap: var(--space-2); }
  .devices { width: min(720px, 100%); display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--space-3); align-items: start; }
  @media (max-width: 600px) { .devices { grid-template-columns: 1fr; } }
  .home { min-height: 100vh; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--space-4); padding: var(--space-6) var(--space-4); }
  .tile { width: min(720px, 100%, calc((100vh - 300px) * 16 / 9)); aspect-ratio: 16 / 9; display: flex; align-items: center; justify-content: center; overflow: hidden; border-radius: var(--radius-l); background: var(--color-surface-3); color: var(--color-text-muted); }
  .tile video { width: 100%; height: 100%; object-fit: cover; }
  .mirror { transform: scaleX(-1); }
  .controls { display: flex; align-items: center; gap: var(--space-3); }
  .round { width: 56px; height: 56px; display: inline-flex; align-items: center; justify-content: center; border: 1px solid var(--color-border-strong); border-radius: 50%; background: var(--color-surface); color: var(--color-text); }
  .round:hover { background: var(--color-surface-2); }
  .round.off { background: var(--color-surface-3); color: var(--color-text-muted); border-color: transparent; }
  .btn-record { padding: 0 var(--space-6); border-radius: var(--radius-pill); }
  .dot { width: 12px; height: 12px; border-radius: 50%; background: currentColor; }
  .hint { max-width: min(720px, 100%); text-align: center; word-break: break-all; }
</style>
