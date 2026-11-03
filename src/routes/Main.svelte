<script lang="ts">
  import { onMount } from "svelte";
  import type { AppInfo, DeviceList, DiskStatus, Settings, CameraSettings, PipShape, Corner } from "../lib/types";
  import { invoke } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";
  import { navigate } from "../lib/router.svelte";
  import { updateSettings } from "../lib/stores/settings.svelte";
  import { recorder, startRecording } from "../lib/stores/recorder.svelte";
  import { getPlatform } from "../lib/platform.svelte";
  import { budgetFromMbPerHour, codecLabel, formatBytes, formatHours, outputSize, previewFilename, shortcutParts } from "../lib/format";
  import { openCamera, stopStream } from "../lib/media/camera";
  import { startLevelMeter, type LevelMeterHandle } from "../lib/media/level";
  import StagePreview from "../lib/components/StagePreview.svelte";
  import LevelMeter from "../lib/components/LevelMeter.svelte";
  import Segmented from "../lib/components/Segmented.svelte";

  let { settings }: { settings: Settings } = $props();
  let devices = $state<DeviceList>({ screens: [], cameras: [], microphones: [], loopbacks: [] });
  let info = $state<AppInfo | null>(null);
  let disk = $state<DiskStatus | null>(null);
  let meetingName = $state("");
  let filename = $state("");
  let level = $state(0);
  let stream = $state<MediaStream | null>(null);
  let meter: LevelMeterHandle | null = null;

  const capture = $derived(settings.capture);
  const displayId = $derived(capture.kind === "window" ? "" : capture.displayId);
  const screen = $derived(devices.screens.find((s) => s.id === displayId) ?? devices.screens.find((s) => s.isDefault) ?? devices.screens[0] ?? null);
  const out = $derived(outputSize(screen, settings.video.maxWidth));
  const budget = $derived(budgetFromMbPerHour(settings.video.targetMbPerHour, settings.audio.bitrateKbps, settings.audio.tracks === "separate" ? 2 : 1));
  const encoderName = $derived(info?.encoder?.name ?? "");
  const codec = $derived(codecLabel(info?.encoder?.codec ?? "hevc"));
  const active = $derived(recorder.isActive);
  const canRecord = $derived(!active && !!screen && !(disk && disk.hoursRecordable * 60 < 10));

  const save = (patch: (s: Settings) => void) => void updateSettings(patch);
  const patchCamera = (p: Partial<CameraSettings>) => save((s) => Object.assign(s.camera, p));

  async function refreshDevices() {
    devices = await invoke("list_devices");
  }
  async function refreshDisk() {
    disk = await invoke("check_disk", {});
  }
  async function onName(v: string) {
    meetingName = v;
    filename = await invoke("preview_filename", { name: v });
  }
  function selectScreen(id: string) {
    save((s) => {
      s.capture = s.capture.kind === "region" ? { kind: "region", displayId: id, region: s.capture.region } : { kind: "display", displayId: id };
    });
  }
  function setSourceKind(k: "display" | "region" | "window") {
    save((s) => {
      const id = s.capture.kind === "window" ? screen?.id ?? "" : s.capture.displayId;
      s.capture = k === "region" ? { kind: "region", displayId: id, region: { x: 0, y: 0, w: 1280, h: 720 } } : { kind: "display", displayId: id };
    });
  }
  async function record() {
    await startRecording(meetingName);
  }

  $effect(() => {
    // Camera preview belongs to the webview only while idle (D6).
    const enabled = settings.camera.enabled && !active;
    const id = settings.camera.deviceId;
    const label = devices.cameras.find((c) => c.id === id)?.name ?? null;
    let cancelled = false;
    stopStream(stream);
    stream = null;
    if (enabled) {
      void openCamera(label).then((s) => {
        if (cancelled) stopStream(s);
        else stream = s;
      });
    }
    return () => {
      cancelled = true;
    };
  });
  $effect(() => {
    const enabled = settings.audio.micEnabled;
    const label = devices.microphones.find((m) => m.id === settings.audio.micDeviceId)?.name ?? null;
    meter?.stop();
    meter = null;
    level = 0;
    if (enabled) void startLevelMeter(label, (v) => (level = v)).then((h) => (meter = h));
    return () => {
      meter?.stop();
      meter = null;
    };
  });
  onMount(() => {
    void refreshDevices();
    void refreshDisk();
    void invoke("get_app_info").then((i) => (info = i));
    void onName("");
    return () => stopStream(stream);
  });
  const sysOptions = $derived([
    { value: "off", label: t("main.systemAudio.off") },
    { value: "auto", label: t("main.systemAudio.auto") },
    { value: "helper", label: t("main.systemAudio.helper"), disabled: getPlatform() !== "macos" },
    ...devices.loopbacks.map((d) => ({ value: `device:${d.id}`, label: d.name })),
  ]);
  const sysValue = $derived(settings.audio.systemAudio.kind === "device" ? `device:${settings.audio.systemAudio.id}` : settings.audio.systemAudio.kind);
</script>

<header class="topbar">
  <strong>{t("app.name")}</strong>
  <span class="chip" class:chip-rec={active}>{t(`status.${recorder.state.phase}`)}</span>
  <button type="button" class="btn btn-ghost" onclick={() => navigate("/settings")}>{t("main.settings")}</button>
</header>

<div class="page grid">
  <section class="stack">
    <div class="card stack">
      <div class="row" style="justify-content: space-between">
        <h3>{t("main.source")}</h3>
        <button type="button" class="btn btn-ghost btn-sm" onclick={refreshDevices}>{t("main.devices.refresh")}</button>
      </div>
      <Segmented
        label={t("main.source")}
        value={capture.kind}
        onchange={setSourceKind}
        options={[
          { value: "display", label: t("main.source.display") },
          { value: "region", label: t("main.source.region") },
          { value: "window", label: t("main.source.window"), disabled: true, hint: t("main.source.windowSoon") },
        ]}
      />
      {#if devices.screens.length === 0}
        <p class="muted">{t("main.noScreens")}</p>
      {:else}
        <div class="screens" role="radiogroup" aria-label={t("main.source.display")}>
          {#each devices.screens as s (s.id)}
            <button type="button" class="screen" role="radio" aria-checked={screen?.id === s.id} onclick={() => selectScreen(s.id)}>
              <span>{s.name}</span>
              <span class="faint mono">{s.width ?? "?"}×{s.height ?? "?"}{s.isDefault ? ` · ${t("main.source.primary")}` : ""}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if capture.kind === "region"}
        <div class="row">
          {#each ["x", "y", "w", "h"] as k (k)}
            <label class="field" style="width: 90px">
              <span>{t(`main.source.${k}`)}</span>
              <input class="input mono" type="number" min="0" value={capture.region[k as "x" | "y" | "w" | "h"]} onchange={(e) => save((s) => { if (s.capture.kind === "region") s.capture.region[k as "x"] = Number((e.target as HTMLInputElement).value); })} />
            </label>
          {/each}
        </div>
      {/if}
    </div>

    <div class="card stack">
      <div class="row" style="justify-content: space-between">
        <h3>{t("main.camera")}</h3>
        <label class="row"><input type="checkbox" checked={settings.camera.enabled} onchange={(e) => patchCamera({ enabled: (e.target as HTMLInputElement).checked })} /> {t("main.camera.enable")}</label>
      </div>
      <StagePreview camera={settings.camera} outW={out.width} outH={out.height} {stream} onchange={patchCamera} />
      {#if settings.camera.enabled}
        <div class="row">
          <label class="field">
            <span>{t("main.camera.device")}</span>
            <select class="input" value={settings.camera.deviceId ?? ""} onchange={(e) => patchCamera({ deviceId: (e.target as HTMLSelectElement).value || null })}>
              <option value="">{t("common.default")}</option>
              {#each devices.cameras as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
            </select>
          </label>
          <div class="field">
            <span class="label">{t("main.camera.corner")}</span>
            <Segmented label={t("main.camera.corner")} value={settings.camera.corner} onchange={(v: Corner) => patchCamera({ corner: v, offsetX: 0, offsetY: 0 })} options={(["top-left", "top-right", "bottom-left", "bottom-right"] as Corner[]).map((c) => ({ value: c, label: t(`corner.${c}`) }))} />
          </div>
          <div class="field">
            <span class="label">{t("main.camera.shape")}</span>
            <Segmented label={t("main.camera.shape")} value={settings.camera.shape} onchange={(v: PipShape) => patchCamera({ shape: v })} options={(["square", "rounded", "circle"] as PipShape[]).map((s) => ({ value: s, label: t(`main.camera.shape.${s}`) }))} />
          </div>
          <label class="field" style="min-width: 160px">
            <span>{t("main.camera.size")} · {settings.camera.sizePct}%</span>
            <input type="range" min="10" max="50" value={settings.camera.sizePct} oninput={(e) => patchCamera({ sizePct: Number((e.target as HTMLInputElement).value) })} />
          </label>
          <label class="row"><input type="checkbox" checked={settings.camera.mirror} onchange={(e) => patchCamera({ mirror: (e.target as HTMLInputElement).checked })} /> {t("main.camera.mirror")}</label>
        </div>
      {/if}
    </div>

    <div class="card stack">
      <div class="row" style="justify-content: space-between">
        <h3>{t("main.mic")}</h3>
        <label class="row"><input type="checkbox" checked={settings.audio.micEnabled} onchange={(e) => save((s) => (s.audio.micEnabled = (e.target as HTMLInputElement).checked))} /> {t("main.mic.enable")}</label>
      </div>
      <div class="row">
        <label class="field" style="flex: 1">
          <span>{t("main.mic.device")}</span>
          <select class="input" value={settings.audio.micDeviceId ?? ""} disabled={!settings.audio.micEnabled} onchange={(e) => save((s) => (s.audio.micDeviceId = (e.target as HTMLSelectElement).value || null))}>
            <option value="">{t("common.default")}</option>
            {#each devices.microphones as m (m.id)}<option value={m.id}>{m.name}</option>{/each}
          </select>
        </label>
        <div class="field" style="flex: 1">
          <span class="label">{t("main.mic.level")}</span>
          <LevelMeter {level} />
        </div>
      </div>
      <div class="field">
        <span class="label">{t("main.systemAudio")}</span>
        <Segmented label={t("main.systemAudio")} value={sysValue} options={sysOptions} onchange={(v: string) => save((s) => { s.audio.systemAudio = v.startsWith("device:") ? { kind: "device", id: v.slice(7) } : ({ kind: v } as Settings["audio"]["systemAudio"]); })} />
        <span class="faint">{t(`main.systemAudio.hint.${settings.audio.systemAudio.kind}`)}</span>
      </div>
    </div>
  </section>

  <aside class="stack">
    <div class="card stack">
      <label class="field">
        <span>{t("main.meetingName")}</span>
        <input class="input" placeholder={t("main.meetingName.placeholder")} value={meetingName} oninput={(e) => onName((e.target as HTMLInputElement).value)} />
      </label>
      <span class="faint mono" data-testid="filename">{t("main.meetingName.hint", { example: `${filename || previewFilename(settings.filenameTemplate, meetingName)}.mp4` })}</span>
      <div class="field">
        <span class="label">{t("main.output")}</span>
        <span class="mono" style="font-size: var(--text-sm); word-break: break-all">{settings.outputDir}</span>
      </div>
    </div>
    <div class="card stack">
      <strong data-testid="estimate">{t("main.estimate", { mb: settings.video.targetMbPerHour })}</strong>
      <span class="faint mono">{info ? t("main.estimate.detail", { w: out.width, h: out.height, fps: settings.video.fps, codec, encoder: encoderName }) : t("main.encoderWait")} · {budget.totalKbps} kbps</span>
      {#if disk}
        <span class="faint" class:warn={disk.low}>{t("main.disk", { free: formatBytes(disk.freeBytes), hours: formatHours(disk.hoursRecordable) })}{disk.low ? ` · ${t("main.disk.low")}` : ""}</span>
      {/if}
    </div>
    <button type="button" class="btn btn-record" onclick={record} disabled={!canRecord || recorder.busy} data-testid="rec">
      <span class="dot"></span>{active ? t("main.recording") : t("main.record")}
    </button>
    <span class="faint row" style="justify-content: center">{t("main.shortcutHint")} {#each shortcutParts(settings.shortcuts.toggle, getPlatform()) as p, i (i)}<span class="kbd">{p}</span>{/each}</span>
  </aside>
</div>

<style>
  .grid { display: grid; grid-template-columns: minmax(0, 1fr) 320px; gap: var(--space-4); }
  @media (max-width: 900px) { .grid { grid-template-columns: 1fr; } }
  .screens { display: flex; gap: var(--space-2); flex-wrap: wrap; }
  .screen { display: flex; flex-direction: column; align-items: flex-start; gap: 2px; padding: var(--space-2) var(--space-3); border: 1px solid var(--color-border); border-radius: var(--radius-m); background: var(--color-surface-2); }
  .screen[aria-checked="true"] { border-color: var(--color-accent); background: var(--color-accent-soft); }
  .dot { width: 12px; height: 12px; border-radius: 50%; background: #fff; }
  .warn { color: var(--color-warn); }
</style>
