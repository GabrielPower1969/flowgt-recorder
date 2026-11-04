<script lang="ts">
  import { onMount } from "svelte";
  import type { AppInfo, CameraSettings, Corner, DeviceList, PipShape, RecoverableSession, Settings, Theme, Language } from "../lib/types";
  import { invoke } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";
  import { navigate } from "../lib/router.svelte";
  import { getSettings, updateSettings } from "../lib/stores/settings.svelte";
  import { toast } from "../lib/stores/toasts.svelte";
  import { budgetFromMbPerHour, formatBytes, formatMbps, previewFilename } from "../lib/format";
  import { __setLastResult } from "../lib/stores/recorder.svelte";
  import Segmented from "../lib/components/Segmented.svelte";
  import ShortcutField from "../lib/components/ShortcutField.svelte";
  import StagePreview from "../lib/components/StagePreview.svelte";
  import LevelMeter from "../lib/components/LevelMeter.svelte";
  import { openCamera, stopStream } from "../lib/media/camera";
  import { startLevelMeter } from "../lib/media/level";

  type Tab = "general" | "camera" | "video" | "audio" | "shortcuts" | "recovery" | "about";
  const tabs: Tab[] = ["general", "camera", "audio", "video", "shortcuts", "recovery", "about"];
  let tab = $state<Tab>("general");
  let info = $state<AppInfo | null>(null);
  let recoverable = $state<RecoverableSession[]>([]);
  let devices = $state<DeviceList>({ screens: [], cameras: [], microphones: [], loopbacks: [] });
  let stream = $state<MediaStream | null>(null);
  let level = $state(0);
  const s = $derived(getSettings());
  const budget = $derived(budgetFromMbPerHour(s.video.targetMbPerHour, s.audio.bitrateKbps, s.audio.tracks === "separate" ? 2 : 1));
  const save = (patch: (x: Settings) => void) => void updateSettings(patch);
  const num = (e: Event) => Number((e.target as HTMLInputElement).value);

  const patchCamera = (p: Partial<CameraSettings>) => save((x) => Object.assign(x.camera, p));
  const sysValue = $derived(s.audio.systemAudio.kind === "device" ? `device:${s.audio.systemAudio.id}` : s.audio.systemAudio.kind);
  const sysOptions = $derived([
    { value: "off", label: t("main.systemAudio.off") },
    { value: "auto", label: t("main.systemAudio.auto") },
    ...devices.loopbacks.map((d) => ({ value: `device:${d.id}`, label: d.name })),
  ]);

  // Live devices only while their tab is open.
  $effect(() => {
    if (tab !== "camera") return;
    const label = devices.cameras.find((c) => c.id === s.camera.deviceId)?.name ?? null;
    let cancelled = false;
    let opened: MediaStream | null = null;
    void openCamera(label).then((st) => {
      if (cancelled) return stopStream(st);
      opened = st;
      stream = st;
    });
    return () => {
      cancelled = true;
      stopStream(opened);
      stream = null;
    };
  });
  $effect(() => {
    if (tab !== "audio" || !s.audio.micEnabled) return;
    const label = devices.microphones.find((m) => m.id === s.audio.micDeviceId)?.name ?? null;
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

  async function pickDir() {
    const dir = await invoke("pick_output_dir");
    if (dir) save((x) => (x.outputDir = dir));
  }
  async function loadRecoverable() {
    recoverable = await invoke("list_recoverable");
  }
  async function recover(id: string) {
    const r = await invoke("recover_session", { sessionId: id });
    __setLastResult(r);
    toast("ok", t("toast.recovered"));
    navigate("/summary");
  }
  async function discard(id: string) {
    await invoke("discard_session", { sessionId: id });
    toast("ok", t("toast.discarded"));
    await loadRecoverable();
  }
  onMount(() => {
    void invoke("get_app_info").then((i) => (info = i));
    void invoke("list_devices").then((d) => (devices = d));
    void loadRecoverable();
  });
</script>

<header class="topbar">
  <button type="button" class="btn btn-ghost" onclick={() => navigate("/")}>‹ {t("common.back")}</button>
  <strong>{t("settings.title")}</strong>
  <span></span>
</header>
<div class="page layout">
  <nav class="nav" aria-label={t("a11y.nav")}>
    {#each tabs as k (k)}
      <button type="button" class="navbtn" aria-current={tab === k ? "page" : undefined} onclick={() => (tab = k)}>{t(`settings.nav.${k}`)}{k === "recovery" && recoverable.length ? ` (${recoverable.length})` : ""}</button>
    {/each}
  </nav>
  <section class="card stack" style="gap: var(--space-4)">
    {#if tab === "general"}
      <div class="field"><span class="label">{t("settings.general.language")}</span>
        <Segmented label={t("settings.general.language")} value={s.language} onchange={(v: Language) => save((x) => (x.language = v))} options={(["auto", "zh", "en"] as Language[]).map((l) => ({ value: l, label: t(`lang.${l}`) }))} /></div>
      <div class="field"><span class="label">{t("settings.general.theme")}</span>
        <Segmented label={t("settings.general.theme")} value={s.theme} onchange={(v: Theme) => save((x) => (x.theme = v))} options={(["system", "light", "dark"] as Theme[]).map((v) => ({ value: v, label: t(`theme.${v}`) }))} /></div>
      <div class="field"><span class="label">{t("settings.general.outputDir")}</span>
        <div class="row"><span class="mono" style="font-size: var(--text-sm)">{s.outputDir}</span><button type="button" class="btn btn-sm" onclick={pickDir}>{t("common.change")}</button></div></div>
      <label class="field"><span>{t("settings.general.template")}</span>
        <input class="input mono" value={s.filenameTemplate} onchange={(e) => save((x) => (x.filenameTemplate = (e.target as HTMLInputElement).value))} />
        <span class="faint">{t("settings.general.templateHint", { tokens: "{date} {time} {name} {datetime} {yyyy} {mm} {dd} {hh} {min}", example: previewFilename(s.filenameTemplate, "周会") })}</span></label>
      <label class="field"><span>{t("settings.general.countdown")} · {s.countdownSeconds === 0 ? t("settings.general.countdown.off") : t("common.seconds", { n: s.countdownSeconds })}</span>
        <input type="range" min="0" max="10" value={s.countdownSeconds} onchange={(e) => save((x) => (x.countdownSeconds = num(e)))} /></label>
      <label class="field"><span>{t("settings.general.lowDisk")} · {t("settings.general.lowDiskValue", { gb: s.lowDiskWarnGb })}</span>
        <input type="range" min="1" max="50" value={s.lowDiskWarnGb} onchange={(e) => save((x) => (x.lowDiskWarnGb = num(e)))} /></label>
    {:else if tab === "camera"}
      <StagePreview camera={s.camera} {stream} onchange={patchCamera} />
      <label class="field"><span>{t("main.camera.device")}</span>
        <select class="input" value={s.camera.deviceId ?? ""} onchange={(e) => patchCamera({ deviceId: (e.target as HTMLSelectElement).value || null })}>
          <option value="">{t("common.default")}</option>
          {#each devices.cameras as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
        </select></label>
      <div class="field"><span class="label">{t("main.camera.corner")}</span>
        <Segmented label={t("main.camera.corner")} value={s.camera.corner} onchange={(v: Corner) => patchCamera({ corner: v, offsetX: 0, offsetY: 0 })} options={(["top-left", "top-right", "bottom-left", "bottom-right"] as Corner[]).map((c) => ({ value: c, label: t(`corner.${c}`) }))} /></div>
      <div class="field"><span class="label">{t("main.camera.shape")}</span>
        <Segmented label={t("main.camera.shape")} value={s.camera.shape} onchange={(v: PipShape) => patchCamera({ shape: v })} options={(["square", "rounded", "circle"] as PipShape[]).map((v) => ({ value: v, label: t(`main.camera.shape.${v}`) }))} /></div>
      <label class="field"><span>{t("main.camera.size")} · {s.camera.sizePct}%</span>
        <input type="range" min="10" max="50" value={s.camera.sizePct} oninput={(e) => patchCamera({ sizePct: num(e) })} /></label>
      <label class="row"><input type="checkbox" checked={s.camera.mirror} onchange={(e) => patchCamera({ mirror: (e.target as HTMLInputElement).checked })} /> {t("main.camera.mirror")}</label>
    {:else if tab === "video"}
      <label class="field"><span>{t("settings.video.target")} · {t("settings.video.targetValue", { mb: s.video.targetMbPerHour })}</span>
        <input type="range" min="100" max="2000" step="50" value={s.video.targetMbPerHour} onchange={(e) => save((x) => (x.video.targetMbPerHour = num(e)))} />
        <span class="faint mono" data-testid="target-live">{t("settings.video.targetLive", { mbps: formatMbps(budget.totalKbps).replace(" Mbps", ""), video: budget.videoKbps })}</span></label>
      <div class="field"><span class="label">{t("settings.video.fps")}</span>
        <Segmented label={t("settings.video.fps")} value={String(s.video.fps)} onchange={(v: string) => save((x) => (x.video.fps = Number(v)))} options={["24", "30", "60"].map((v) => ({ value: v, label: v }))} /></div>
      <div class="field"><span class="label">{t("settings.video.maxWidth")}</span>
        <Segmented label={t("settings.video.maxWidth")} value={String(s.video.maxWidth)} onchange={(v: string) => save((x) => (x.video.maxWidth = Number(v)))} options={["1280", "1920", "2560"].map((v) => ({ value: v, label: v }))} /></div>
      <div class="field"><span class="label">{t("settings.video.encoder")}</span>
        <Segmented label={t("settings.video.encoder")} value={s.video.encoderPreference} onchange={(v: Settings["video"]["encoderPreference"]) => save((x) => (x.video.encoderPreference = v))} options={(["auto", "hevc", "h264"] as const).map((v) => ({ value: v, label: t(`settings.video.encoder.${v}`) }))} /></div>
    {:else if tab === "audio"}
      <label class="field"><span>{t("main.mic.device")}</span>
        <select class="input" value={s.audio.micDeviceId ?? ""} onchange={(e) => save((x) => (x.audio.micDeviceId = (e.target as HTMLSelectElement).value || null))}>
          <option value="">{t("common.default")}</option>
          {#each devices.microphones as m (m.id)}<option value={m.id}>{m.name}</option>{/each}
        </select></label>
      <div class="field"><span class="label">{t("main.mic.level")}</span><LevelMeter {level} /></div>
      <div class="field"><span class="label">{t("settings.audio.tracks")}</span>
        <Segmented label={t("settings.audio.tracks")} value={s.audio.tracks} onchange={(v: Settings["audio"]["tracks"]) => save((x) => (x.audio.tracks = v))} options={(["mixed", "separate"] as const).map((v) => ({ value: v, label: t(`settings.audio.tracks.${v}`) }))} /></div>
      <div class="field"><span class="label">{t("settings.audio.bitrate")}</span>
        <Segmented label={t("settings.audio.bitrate")} value={String(s.audio.bitrateKbps)} onchange={(v: string) => save((x) => (x.audio.bitrateKbps = Number(v)))} options={["64", "96", "128"].map((v) => ({ value: v, label: `${v} kbps` }))} /></div>
      <div class="field"><span class="label">{t("settings.audio.system")}</span>
        <Segmented label={t("settings.audio.system")} value={sysValue} options={sysOptions} onchange={(v: string) => save((x) => { x.audio.systemAudio = v.startsWith("device:") ? { kind: "device", id: v.slice(7) } : ({ kind: v } as Settings["audio"]["systemAudio"]); })} />
        <span class="faint">{t(devices.loopbacks.length ? `main.systemAudio.hint.${s.audio.systemAudio.kind}` : "picker.systemAudioMissing")}</span></div>
    {:else if tab === "shortcuts"}
      <ShortcutField label={t("settings.shortcuts.toggle")} value={s.shortcuts.toggle} onchange={(v) => save((x) => (x.shortcuts.toggle = v))} />
      <ShortcutField label={t("settings.shortcuts.pause")} value={s.shortcuts.pause} onchange={(v) => save((x) => (x.shortcuts.pause = v))} />
    {:else if tab === "recovery"}
      {#if recoverable.length === 0}
        <p class="muted">{t("settings.recovery.empty")}</p>
      {:else}
        {#each recoverable as r (r.sessionId)}
          <div class="row rec-row">
            <div class="stack" style="gap: 2px; flex: 1">
              <strong>{r.meetingName || t("app.name")}</strong>
              <span class="faint mono">{r.createdAt} · {t("settings.recovery.segments", { n: r.segments })} · {formatBytes(r.bytes)}</span>
            </div>
            <button type="button" class="btn btn-primary btn-sm" onclick={() => recover(r.sessionId)}>{t("common.recover")}</button>
            <button type="button" class="btn btn-sm" onclick={() => discard(r.sessionId)}>{t("common.discard")}</button>
          </div>
        {/each}
      {/if}
    {:else}
      <dl class="about">
        <dt>{t("settings.about.version")}</dt><dd class="mono">{info?.version ?? "…"}</dd>
        <dt>{t("settings.about.platform")}</dt><dd>{info ? t(`platform.${info.platform}`) : "…"}</dd>
        <dt>{t("settings.about.ffmpeg")}</dt><dd class="mono">{info ? `${info.ffmpegVersion} · ${info.ffmpegBundled ? t("settings.about.bundled") : t("settings.about.systemFfmpeg")} · ${info.ffmpegPath}` : "…"}</dd>
        <dt>{t("settings.about.encoder")}</dt><dd class="mono">{info?.encoder ? `${info.encoder.name} · ${info.encoder.hardware ? t("settings.about.hardware") : t("settings.about.software")}` : t("settings.about.noEncoder")}</dd>
      </dl>
      <p class="faint">{t("settings.about.lgpl")}</p>
    {/if}
  </section>
</div>

<style>
  .layout { display: grid; grid-template-columns: 180px minmax(0, 1fr); gap: var(--space-4); }
  @media (max-width: 700px) { .layout { grid-template-columns: 1fr; } .nav { flex-direction: row; flex-wrap: wrap; } }
  .nav { display: flex; flex-direction: column; gap: 2px; }
  .navbtn { text-align: left; padding: var(--space-2) var(--space-3); border: 0; border-radius: var(--radius-s); background: transparent; color: var(--color-text-muted); min-height: 36px; }
  .navbtn[aria-current="page"] { background: var(--color-accent-soft); color: var(--color-accent); font-weight: 600; }
  .rec-row { padding: var(--space-2) var(--space-3); border-radius: var(--radius-m); background: var(--color-warn-soft); }
  .about { display: grid; grid-template-columns: 120px 1fr; gap: var(--space-2) var(--space-3); margin: 0; }
  .about dt { color: var(--color-text-muted); } .about dd { margin: 0; word-break: break-all; }
</style>
