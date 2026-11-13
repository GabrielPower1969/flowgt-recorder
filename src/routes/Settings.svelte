<script lang="ts">
  import { onMount } from "svelte";
  import type { AppInfo, AutostartStatus, CameraSettings, Corner, Device, PipShape, RecoverableSession, Settings, ShortcutName, ShortcutState, Theme, Language } from "../lib/types";
  import { getCapabilities } from "../lib/stores/capabilities.svelte";
  import { defaultSettings } from "../lib/defaults";
  import { errorMessage } from "../lib/ipc";
  import { invoke } from "../lib/ipc";
  import { t } from "../lib/stores/i18n.svelte";
  import { navigate } from "../lib/router.svelte";
  import { getPlatform } from "../lib/platform.svelte";
  import { getSettings, updateSettings } from "../lib/stores/settings.svelte";
  import { toast } from "../lib/stores/toasts.svelte";
  import { budgetFromMbPerHour, formatBytes, formatMbps, outputSize, previewFilename, QUALITY_STEPS, qualityAvailable, qualityOf, suggestedMbPerHour, type QualityKey } from "../lib/format";
  import { __setLastResult } from "../lib/stores/recorder.svelte";
  import logo from "../assets/brand/logo-256.png";
  import Segmented from "../lib/components/Segmented.svelte";
  import ShortcutField from "../lib/components/ShortcutField.svelte";
  import StagePreview from "../lib/components/StagePreview.svelte";
  import { openCamera, stopStream } from "../lib/media/camera";
  import { driveState, signInToGoogle, signOutOfGoogle } from "../lib/stores/drive.svelte";

  type Tab = "general" | "camera" | "video" | "audio" | "shortcuts" | "recovery" | "about";
  const tabs: Tab[] = ["general", "camera", "audio", "video", "shortcuts", "recovery", "about"];
  let tab = $state<Tab>("general");
  let info = $state<AppInfo | null>(null);
  let recoverable = $state<RecoverableSession[]>([]);
  let stream = $state<MediaStream | null>(null);
  const s = $derived(getSettings());
  const budget = $derived(budgetFromMbPerHour(s.video.targetMbPerHour, s.audio.bitrateKbps, s.audio.tracks === "separate" ? 2 : 1));
  const save = (patch: (x: Settings) => void) => void updateSettings(patch);
  const num = (e: Event) => Number((e.target as HTMLInputElement).value);

  const caps = $derived(getCapabilities());

  // Quality: what the chosen screen has, what the file will have, and whether the budget fits.
  let screens = $state<Device[]>([]);
  const screen = $derived(screens.find((d) => s.capture.kind !== "window" && d.id === s.capture.displayId) ?? screens.find((d) => d.isDefault) ?? screens[0] ?? null);
  const out = $derived(outputSize(screen, s.video.maxWidth));
  const quality = $derived(qualityOf(s.video.maxWidth));
  const wantMb = $derived(suggestedMbPerHour(out.width, out.height));
  const setQuality = (key: QualityKey) => save((x) => (x.video.maxWidth = QUALITY_STEPS.find((q) => q.key === key)!.maxWidth));

  // Start at login: the system holds the answer, so it is asked, never remembered.
  let autostart = $state<AutostartStatus>("unsupported");
  async function setAutostart(on: boolean) {
    try {
      autostart = await invoke("autostart_set", { enabled: on });
    } catch (e) {
      toast("error", errorMessage(e));
      autostart = await invoke("autostart_status");
    }
  }

  // Shortcuts: which ones the system really gave us.
  const SHORTCUTS: { name: ShortcutName; optional: boolean; offered: () => boolean }[] = [
    { name: "toggle", optional: false, offered: () => true },
    { name: "pause", optional: false, offered: () => true },
    { name: "scrollCapture", optional: true, offered: () => caps.scrollCapture },
    { name: "showWindow", optional: true, offered: () => true },
  ];
  let shortcutState = $state<Partial<Record<ShortcutName, ShortcutState>>>({});
  const refreshShortcuts = () => invoke("shortcut_status").then((r) => (shortcutState = r)).catch(() => {});
  async function setShortcut(name: ShortcutName, accel: string) {
    const other = SHORTCUTS.find((o) => o.name !== name && accel && s.shortcuts[o.name].toLowerCase() === accel.toLowerCase());
    if (other) {
      toast("error", t("settings.shortcuts.duplicate", { other: t(`settings.shortcuts.${other.name}`) }));
      return;
    }
    await updateSettings((x) => (x.shortcuts[name] = accel));
    await refreshShortcuts();
  }
  async function resetShortcuts() {
    await updateSettings((x) => (x.shortcuts = defaultSettings(getPlatform()).shortcuts));
    await refreshShortcuts();
  }
  const shortcutProblem = (name: ShortcutName) => (shortcutState[name] === "taken" ? t("settings.shortcuts.taken") : shortcutState[name] === "invalid" ? t("settings.shortcuts.invalid") : "");

  const patchCamera = (p: Partial<CameraSettings>) => save((x) => Object.assign(x.camera, p));
  // The camera only while its tab is open.
  $effect(() => {
    if (tab !== "camera") return;
    const label = s.camera.deviceId;
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
    void loadRecoverable();
    void invoke("list_devices").then((d) => (screens = d.screens)).catch(() => {});
    void invoke("autostart_status").then((a) => (autostart = a)).catch(() => {});
    void refreshShortcuts();
  });
</script>

<header class="head">
  <h1>{t("settings.title")}</h1>
  <button type="button" class="btn" onclick={() => navigate("/")}>{t("common.back")}</button>
</header>
<div class="layout">
  <nav class="nav" aria-label={t("a11y.nav")}>
    {#each tabs as k (k)}
      <button type="button" class="navbtn" aria-current={tab === k ? "page" : undefined} onclick={() => (tab = k)}>{t(`settings.nav.${k}`)}{k === "recovery" && recoverable.length ? ` (${recoverable.length})` : ""}</button>
    {/each}
  </nav>
  <section class="stack" style="gap: var(--space-4)">
    {#if tab === "general"}
      <div class="field"><span class="label">{t("settings.general.account")}</span>
        <div class="row">
          <span data-testid="account">{driveState.status.email ?? t("settings.general.notSignedIn")}</span>
          {#if driveState.status.email}
            <button type="button" class="btn btn-sm" onclick={signOutOfGoogle}>{t("settings.general.signOut")}</button>
          {:else}
            <button type="button" class="btn btn-sm" onclick={signInToGoogle} disabled={driveState.signingIn}>{driveState.signingIn ? t("home.signingIn") : t("home.signIn")}</button>
          {/if}
        </div></div>
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
      {#if caps.autostart}
        <div class="field">
          <label class="row"><input type="checkbox" data-testid="autostart" checked={autostart === "on" || autostart === "needsApproval"} onchange={(e) => setAutostart((e.target as HTMLInputElement).checked)} /> {t("settings.general.autostart")}</label>
          {#if autostart === "needsApproval"}
            <div class="row"><span class="err" data-testid="autostart-approval">{t(getPlatform() === "windows" ? "settings.general.autostartApprovalWin" : "settings.general.autostartApproval")}</span><button type="button" class="btn btn-sm" onclick={() => invoke("open_login_items_settings")}>{t(getPlatform() === "windows" ? "settings.general.autostartOpenWin" : "settings.general.autostartOpen")}</button></div>
          {/if}
          <label class="row faint"><input type="checkbox" checked={s.startHiddenOnLogin} disabled={autostart === "off"} onchange={(e) => save((x) => (x.startHiddenOnLogin = (e.target as HTMLInputElement).checked))} /> {t(getPlatform() === "windows" ? "settings.general.autostartHiddenWin" : "settings.general.autostartHidden")}</label>
        </div>
      {/if}
    {:else if tab === "camera"}
      <StagePreview camera={s.camera} {stream} onchange={patchCamera} />
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
      <div class="field"><span class="label">{t("settings.video.quality")}</span>
        <Segmented label={t("settings.video.quality")} value={quality} onchange={setQuality} options={QUALITY_STEPS.map((q) => ({ value: q.key, label: t(`quality.${q.key}`), disabled: !qualityAvailable(q.key, screen?.width), hint: !qualityAvailable(q.key, screen?.width) && screen ? t("settings.video.qualityTooBig", { w: screen.width ?? 0, h: screen.height ?? 0 }) : undefined }))} />
        {#if screen?.width}<span class="faint mono" data-testid="quality-out">{t("settings.video.qualityHint", { w: screen.width, h: screen.height ?? 0, ow: out.width, oh: out.height })}</span>{/if}
        {#if s.video.targetMbPerHour < wantMb * 0.8}
          <div class="row"><span class="err" data-testid="budget-low">{t("settings.video.budgetLow", { mb: wantMb })}</span><button type="button" class="btn btn-sm" onclick={() => save((x) => (x.video.targetMbPerHour = Math.min(2000, wantMb)))}>{t("settings.video.budgetRaise", { mb: Math.min(2000, wantMb) })}</button></div>
        {/if}
        {#if out.width > 2560 && s.video.fps >= 60 && info?.encoder && !info.encoder.hardware}<span class="err">{t("settings.video.heavy")}</span>{/if}
      </div>
      <div class="field"><span class="label">{t("settings.video.encoder")}</span>
        <Segmented label={t("settings.video.encoder")} value={s.video.encoderPreference} onchange={(v: Settings["video"]["encoderPreference"]) => save((x) => (x.video.encoderPreference = v))} options={(["auto", "hevc", "h264"] as const).map((v) => ({ value: v, label: t(`settings.video.encoder.${v}`) }))} /></div>
    {:else if tab === "audio"}
      <div class="field"><span class="label">{t("settings.audio.tracks")}</span>
        <Segmented label={t("settings.audio.tracks")} value={s.audio.tracks} onchange={(v: Settings["audio"]["tracks"]) => save((x) => (x.audio.tracks = v))} options={(["mixed", "separate"] as const).map((v) => ({ value: v, label: t(`settings.audio.tracks.${v}`) }))} /></div>
      <div class="field"><span class="label">{t("settings.audio.bitrate")}</span>
        <Segmented label={t("settings.audio.bitrate")} value={String(s.audio.bitrateKbps)} onchange={(v: string) => save((x) => (x.audio.bitrateKbps = Number(v)))} options={["64", "96", "128"].map((v) => ({ value: v, label: `${v} kbps` }))} /></div>
    {:else if tab === "shortcuts"}
      {#each SHORTCUTS.filter((o) => o.offered()) as o (o.name)}
        <ShortcutField label={t(`settings.shortcuts.${o.name}`)} value={s.shortcuts[o.name]} optional={o.optional} problem={shortcutProblem(o.name)} onchange={(v) => setShortcut(o.name, v)} />
      {/each}
      <p class="faint">{t("settings.shortcuts.hint")}</p>
      <div><button type="button" class="btn btn-sm" data-testid="shortcuts-reset" onclick={resetShortcuts}>{t("settings.shortcuts.reset")}</button></div>
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
      <div class="brand"><img src={logo} alt="" width="72" height="72" /><strong>{t("app.name")}</strong></div>
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
  .brand { display: flex; align-items: center; gap: var(--space-3); font-size: var(--text-lg); }
  .err { color: var(--color-danger); font-size: var(--text-sm); }
  .head { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); margin-bottom: var(--space-3); }
  .layout { display: flex; flex-direction: column; gap: var(--space-4); }
  .nav { display: flex; flex-wrap: wrap; gap: var(--space-1) var(--space-4); border-bottom: 1px solid var(--color-border); }
  .navbtn { padding: var(--space-2) 0; border: 0; border-bottom: 2px solid transparent; margin-bottom: -1px; background: transparent; color: var(--color-text-muted); min-height: 36px; }
  .navbtn[aria-current="page"] { border-bottom-color: var(--color-text); color: var(--color-text); font-weight: 600; }
  .rec-row { padding: var(--space-2) var(--space-3); border-radius: var(--radius-m); background: var(--color-warn-soft); }
  .about { display: grid; grid-template-columns: 120px 1fr; gap: var(--space-2) var(--space-3); margin: 0; }
  .about dt { color: var(--color-text-muted); } .about dd { margin: 0; word-break: break-all; }
</style>
