<script lang="ts">
  /**
   * The front panel. One plate for every state of the main window: two reels (disk space left,
   * this recording), the monitor with the picture that will be recorded, four selectors for the
   * inputs and five keys. Saved, history and settings are paper cards laid over the reels (`sheet`).
   */
  import { onMount, type Snippet } from "svelte";
  import type { AppInfo, Device, DeviceList, DiskStatus, PermissionKind, PermissionStatus, ScrollShot, Settings } from "../lib/types";
  import { errorMessage, invoke, mediaUrl, subscribe } from "../lib/ipc";
  import { toast } from "../lib/stores/toasts.svelte";
  import { getPlatform } from "../lib/platform.svelte";
  import { t } from "../lib/stores/i18n.svelte";
  import { getRoute, navigate } from "../lib/router.svelte";
  import { updateSettings } from "../lib/stores/settings.svelte";
  import { cancelRecording, recorder, startRecording, stopRecording, togglePause } from "../lib/stores/recorder.svelte";
  import { getCapabilities } from "../lib/stores/capabilities.svelte";
  import { openCamera, stopStream } from "../lib/media/camera";
  import { startLevelMeter } from "../lib/media/level";
  import { driveState, signInToGoogle } from "../lib/stores/drive.svelte";
  import { formatBytes, formatDuration, formatHours, outputSize, QUALITY_STEPS, qualityAvailable, qualityOf, shortcutParts, suggestedMbPerHour, type QualityKey } from "../lib/format";
  import Reel from "../lib/components/Reel.svelte";
  import Lever from "../lib/components/Lever.svelte";
  import ScreenPicker from "../lib/components/ScreenPicker.svelte";

  let { settings, sheet }: { settings: Settings; sheet?: Snippet } = $props();
  let devices = $state<DeviceList | null>(null);
  let info = $state<AppInfo | null>(null);
  let perms = $state<PermissionStatus | null>(null);
  let disk = $state<DiskStatus | null>(null);
  let preview = $state<MediaStream | null>(null);
  let thumb = $state<string | null>(null);
  let picking = $state(false);
  let level = $state(0);
  let name = $state("");
  let view = $state({ w: 1180, h: 760 });

  /** The panel is drawn at the size of the default window and scaled as one piece. */
  const STAGE_W = 1180;
  const STAGE_H = 760;
  /** The housing that shows around the plate, in drawing pixels. */
  const PLATE_INSET = 14;
  /** Height the system's window buttons need when they are drawn over the page. */
  const TITLE_BAR = 28;
  // Where the window buttons sit on the page, the plate starts below them: the housing above the
  // plate becomes that strip, and the window is moved by dragging it.
  const titleBar = $derived(getCapabilities().overlayTitleBar ? TITLE_BAR : 0);
  const scale = $derived(titleBar ? Math.min(view.w / STAGE_W, (view.h - titleBar) / (STAGE_H - PLATE_INSET)) : Math.min(view.w / STAGE_W, view.h / STAGE_H));
  const drop = $derived(titleBar ? titleBar / 2 - (PLATE_INSET / 2) * scale : 0);

  const st = $derived(recorder.state);
  const live = $derived(st.phase === "recording" || st.phase === "paused");
  const paused = $derived(st.phase === "paused");
  const busy = $derived(recorder.isActive);
  const route = $derived(getRoute());

  const camAllowed = $derived(perms?.camera === "granted" || perms?.camera === "notApplicable");
  const camDenied = $derived(perms?.camera === "denied");
  const micAllowed = $derived(perms?.microphone === "granted" || perms?.microphone === "notApplicable");
  const screenAllowed = $derived(perms?.screen === "granted" || perms?.screen === "notApplicable");
  const defaultCamera = $derived(devices?.cameras.find((c) => c.isDefault)?.name ?? devices?.cameras[0]?.name ?? null);
  const defaultMic = $derived(devices?.microphones.find((m) => m.isDefault)?.name ?? null);
  /** Settings keep the device NAME (avfoundation indices shift); null = follow the system. Older
   *  files hold an index, and a device can be unplugged: both must read as a real choice or default. */
  const chosen = (list: Device[] | undefined, stored: string | null) => list?.find((d) => d.name === stored || d.id === stored)?.name ?? "";
  const cameraChoice = $derived(chosen(devices?.cameras, settings.camera.deviceId));
  const micChoice = $derived(chosen(devices?.microphones, settings.audio.micDeviceId));
  const followLabel = (n: string | null) => (n ? t("home.followSystem", { name: n }) : t("home.followSystemPlain"));
  const canRecordOutput = $derived(!!info?.systemAudioAvailable || (devices?.loopbacks.length ?? 0) > 0);
  const outputValue = $derived(!canRecordOutput || settings.audio.systemAudio.kind === "off" ? "off" : settings.audio.systemAudio.kind === "device" ? `device:${settings.audio.systemAudio.id}` : "auto");

  /** Which screen the panel shows: the saved one, else the system's main one. */
  const screens = $derived(devices?.screens ?? []);
  const windowsOffered = $derived(getCapabilities().windowCapture);
  const savedDisplay = $derived(settings.capture.kind === "window" ? null : settings.capture.displayId);
  const screenIndex = $derived(Math.max(0, screens.findIndex((s) => s.id === savedDisplay) >= 0 ? screens.findIndex((s) => s.id === savedDisplay) : screens.findIndex((s) => s.isDefault)));
  const screenMarks = $derived([
    ...(screens.length ? screens.slice(0, 3).map((_, i) => ({ label: String(i + 1) })) : [{ label: "1" }]),
    ...(windowsOffered ? [{ label: t("face.window") }] : []),
  ]);
  const screenMark = $derived(settings.capture.kind === "window" && windowsOffered ? screenMarks.length - 1 : Math.min(screenIndex, 2));
  /** A screen named by its device (\\.\DISPLAY1, "Capture screen 0") says nothing to a person: number it, as the picker does. */
  const friendly = (d: Device | undefined, i: number) => (d && !/^(\\\\\.\\|Capture screen \d+$)/.test(d.name) ? d.name : t("picker.screen", { n: i + 1 }));
  const screenName = $derived(settings.capture.kind === "window" && windowsOffered ? t("picker.tab.window") : friendly(screens[screenIndex], screenIndex));

  /** The two quality steps on the plate; the others are in the settings. */
  const quality = $derived(qualityOf(settings.video.maxWidth));
  const outSize = $derived(outputSize(screens[screenIndex], settings.video.maxWidth));
  /** Choosing a larger picture on the plate also raises a budget that would leave it soft, and says so. */
  async function setQuality(key: QualityKey) {
    const maxWidth = QUALITY_STEPS.find((q) => q.key === key)!.maxWidth;
    const size = outputSize(screens[screenIndex], maxWidth);
    const want = Math.min(2000, suggestedMbPerHour(size.width, size.height));
    const raise = settings.video.targetMbPerHour < want * 0.8;
    await updateSettings((s) => {
      s.video.maxWidth = maxWidth;
      if (raise) s.video.targetMbPerHour = want;
    });
    if (raise) toast("ok", t("face.budgetRaised", { mb: want }));
  }

  /** A key's tooltip: what it does and, when one is set, its global shortcut. */
  const hint = (label: string, accel: string) => (accel ? `${label}  ${shortcutParts(accel, getPlatform()).join(getPlatform() === "macos" ? "" : "+")}` : label);

  // Scrolling screenshot: the key starts it; the result comes back as a card on the plate.
  let shot = $state<(ScrollShot & { url: string | null }) | null>(null);
  async function scrollShot() {
    if (busy) return;
    shot = null;
    if (route !== "/") navigate("/");
    try {
      await invoke("scroll_capture_begin");
    } catch (e) {
      toast("error", errorMessage(e));
    }
  }

  /** Drive counts only when signed in; otherwise the local file is the only copy. */
  const driveOn = $derived(settings.saveDrive && !!driveState.status.email);
  const localOn = $derived(settings.saveLocal || !driveOn);

  /** Bluetooth headsets drop to a narrow phone-call mode when their mic is used. */
  const btMic = $derived(settings.audio.micEnabled && /airpods|buds|beats|bluetooth|headset|耳机/i.test(micChoice || defaultMic || ""));

  // The reels: left is the disk, right is this recording (one full reel = one hour).
  const diskTape = $derived(disk && disk.totalBytes > 0 ? Math.max(0.02, disk.freeBytes / disk.totalBytes) : 1);
  const last = $derived(recorder.lastResult);
  const takeMs = $derived(busy ? st.elapsedMs : route === "/summary" ? (last?.info.durationMs ?? 0) : 0);
  // While recording, the count of bytes written lags until a segment closes; the estimate fills in.
  const takeBytes = $derived(busy ? Math.max(st.bytesWritten, st.estimatedTotalBytes) : route === "/summary" ? (last?.info.bytes ?? 0) : 0);
  const clock = $derived(formatDuration(takeMs).split(":"));
  const spin = $derived(st.phase === "recording" ? "on" : paused ? "paused" : "off");
  const statusKey = $derived(st.phase === "recording" ? "floating.recording" : paused ? "floating.paused" : st.phase === "finalizing" ? "face.status.saving" : busy ? "face.status.ready" : route === "/summary" && last ? "common.saved" : "face.status.idle");
  const sources = $derived([screenName, ...(settings.camera.enabled ? [t("home.camera")] : []), ...(settings.audio.micEnabled ? [t("home.mic")] : []), ...(outputValue !== "off" ? [t("home.output")] : [])].join(" ＋ "));
  const lit = $derived(Math.round(Math.max(0, Math.min(1, level)) * 8));

  /** Where the tape leaves a reel for the guide roller beside the monitor (plate coordinates). */
  function tangent(cx: number, cy: number, r: number, px: number, py: number, sign: number): [number, number] {
    const a = Math.atan2(py - cy, px - cx) + sign * Math.acos(r / Math.hypot(px - cx, py - cy));
    return [Math.round((cx + r * Math.cos(a)) * 10) / 10, Math.round((cy + r * Math.sin(a)) * 10) / 10];
  }
  const packRadius = (tape: number) => (tape > 0 ? Math.max(66, Math.sqrt(3600 + 18900 * Math.min(1, tape))) : 60);
  const tapeL = $derived(tangent(280, 216, packRadius(diskTape), 422, 350, 1));
  const tapeR = $derived(tangent(872, 216, packRadius(takeMs / 3_600_000), 730, 350, -1));

  async function toggleDrive(on: boolean) {
    if (on && !driveState.status.email && !(await signInToGoogle())) return;
    await updateSettings((s) => {
      s.saveDrive = on;
      if (!on) s.saveLocal = true;
    });
  }
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

  /** The red key. With one screen and nothing else to choose there is no dialog: it just starts. */
  async function record() {
    if (busy) return;
    if (route !== "/") navigate("/");
    const only = screens.length === 1 ? screens[0] : null;
    if (only && screenAllowed && !windowsOffered) {
      await updateSettings((s) => { s.capture = { kind: "display", displayId: only.id }; });
      await startRecording(name);
    } else {
      picking = true;
    }
  }
  const stop = () => (st.phase === "countdown" ? cancelRecording() : stopRecording());
  const toggleSheet = (path: "/history" | "/settings") => navigate(route === path ? "/" : path);

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

  // A still of the screen for the monitor. Only once the system already allows it: looking must never raise the prompt.
  $effect(() => {
    const id = screens[screenIndex]?.id;
    if (!id || !screenAllowed || busy) return;
    let cancelled = false;
    void invoke("screen_thumbnail", { id }).then((url) => {
      if (!cancelled) thumb = url;
    }).catch(() => {});
    return () => { cancelled = true; };
  });

  // The disk reel follows the output folder, and is read again after each recording.
  $effect(() => {
    void settings.outputDir;
    if (busy) return;
    void invoke("check_disk", {}).then((d) => (disk = d)).catch(() => {});
  });

  onMount(() => {
    const offShot = subscribe("scroll://finished", (s) => {
      shot = { ...s, url: null };
      void mediaUrl(s.path).then((url) => { if (shot?.path === s.path) shot = { ...shot, url }; });
    });
    const fit = () => (view = { w: window.innerWidth, h: window.innerHeight });
    fit();
    window.addEventListener("resize", fit);
    void (async () => {
      void invoke("get_app_info").then((i) => (info = i));
      [devices, perms] = await Promise.all([invoke("list_devices"), invoke("check_permissions")]);
      if (settings.camera.enabled) await ensure("camera");
      if (settings.audio.micEnabled) await ensure("microphone");
    })();
    return () => {
      offShot();
      window.removeEventListener("resize", fit);
    };
  });
</script>

<div class="room" data-tauri-drag-region={titleBar ? "" : undefined}>
  {#if titleBar}<div class="titlebar" data-testid="titlebar" data-tauri-drag-region style:height="{titleBar}px"></div>{/if}
  <main class="stage" style:transform="translateY({drop}px) scale({scale})">
    <div class="plate">
      {#each [[14, 14], [570, 9], [1126, 14], [14, 360], [1126, 360], [14, 706], [570, 711], [1126, 706]] as [x, y], i (i)}
        <span class="screw" aria-hidden="true" style:left="{x}px" style:top="{y}px" style:transform="rotate({i * 25}deg)"></span>
      {/each}

      <svg class="path" aria-hidden="true" width="1152" height="510" viewBox="0 0 1152 510">
        <path d="M{tapeL[0]} {tapeL[1]}L422 350H730L{tapeR[0]} {tapeR[1]}" />
        <circle cx="422" cy="356" r="6" /><circle cx="730" cy="356" r="6" />
      </svg>

      <div class="reel-l"><Reel size={376} tape={diskTape} {spin} label={t("face.reel.disk", { free: disk ? formatBytes(disk.freeBytes) : "" })} brand={t("face.brand")} value={disk ? formatBytes(disk.freeBytes) : ""} caption={t("face.diskFree")} /></div>
      <div class="reel-r"><Reel size={376} tape={takeMs / 3_600_000} {spin} label={t("face.reel.take", { size: formatBytes(takeBytes) })} brand={t("face.brand")} value={formatBytes(takeBytes)} caption={t("face.thisTake")} /></div>

      <label class="name">
        <input bind:value={name} placeholder={t("main.meetingName.placeholder")} data-testid="home-name" disabled={busy} />
        <span>{t("picker.name")}</span>
      </label>

      <div class="status glass" role="status"><span class="lamp" class:on={st.phase === "recording"} class:blink={paused}></span>{t(statusKey)}</div>

      <div class="monitor glass">
        <div class="screen">
          {#if thumb}
            <img class="shot" src={thumb} alt="" />
          {:else}
            <div class="desk" aria-hidden="true"><i></i><i></i></div>
          {/if}
          {#if settings.camera.enabled}
            <div class="pip {settings.camera.corner} {settings.camera.shape}" style:width="{Math.max(20, settings.camera.sizePct)}%">
              {#if preview}
                <video class:mirror={settings.camera.mirror} autoplay muted playsinline use:attach={preview}></video>
              {:else if !camDenied}
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" role="img" aria-label={t("main.camera.noPreview")}><circle cx="12" cy="8.5" r="3.5" /><path d="M5 19.5c1.2-3.2 3.8-4.8 7-4.8s5.8 1.6 7 4.8" /></svg>
              {/if}
            </div>
            {#if camDenied}
              <div class="denied">
                <span>{t("home.camDenied")}</span>
                <button type="button" class="btn btn-sm" onclick={() => invoke("open_privacy_settings", { kind: "camera" })}>{t("home.openSettings")}</button>
              </div>
            {/if}
          {/if}
          {#if st.phase === "countdown"}
            <div class="count mono" aria-live="assertive" data-testid="home-countdown">{recorder.countdown ?? settings.countdownSeconds}</div>
          {/if}
        </div>
      </div>
      <p class="cap monitor-cap">{t("face.monitor")} · {sources}</p>

      <div class="vu glass" role="meter" aria-label={t("a11y.meter")} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(level * 100)}>
        <div class="col">{#each [0, 1, 2, 3, 4, 5, 6, 7] as i (i)}<i class:lit={settings.audio.micEnabled && 8 - i <= lit} class:hot={i < 2}></i>{/each}<span>{t("face.vu.mic")}</span></div>
        <div class="col">{#each [0, 1, 2, 3, 4, 5, 6, 7] as i (i)}<i></i>{/each}<span>{t("face.vu.sys")}</span></div>
      </div>
      {#if disk?.low}<p class="tag lowdisk" data-testid="low-disk">{t("face.lowDisk", { free: formatBytes(disk.freeBytes), hours: formatHours(disk.hoursRecordable) })}</p>{/if}

      <div class="counter glass mono" role="timer" aria-label={t("face.timer")} data-testid="home-timer"><span>{clock[0]}</span>:<span>{clock[1]}</span>:<span>{clock[2]}</span></div>
      <p class="cap counter-cap">{t("face.timer")}</p>

      <div class="ctl" style:left="44px">
        <Lever marks={screenMarks} index={screenMark} label={t("face.chooseScreen")} disabled={busy} onclick={() => (picking = true)} testid="screen-lever" />
        <div class="eng"><span>{t("face.screen")}</span><em title={screenName}>{screenName}</em></div>
      </div>
      <div class="ctl" style:left="184px">
        <Lever marks={[{ label: t("common.off") }, { label: t("common.on") }]} index={settings.camera.enabled ? 1 : 0} label={settings.camera.enabled ? t("home.camOn") : t("home.camOff")} pressed={settings.camera.enabled} disabled={busy} onclick={toggleCamera} />
        <label class="eng">
          <span>{t("home.camera")}</span>
          <select disabled={!settings.camera.enabled || busy} onchange={(e) => { const v = (e.target as HTMLSelectElement).value || null; void updateSettings((s) => { s.camera.deviceId = v; }); }}>
            <option value="">{followLabel(defaultCamera)}</option>
            {#each devices?.cameras ?? [] as c (c.id)}<option value={c.name} selected={c.name === cameraChoice}>{c.name}</option>{/each}
          </select>
        </label>
      </div>

      <div class="well keys-l" class:two={getCapabilities().scrollCapture}>
        {#if getCapabilities().scrollCapture}<button type="button" class="key" aria-label={t("scroll.key")} title={hint(t("scroll.key"), settings.shortcuts.scrollCapture)} data-testid="scroll-shot" disabled={busy} onclick={scrollShot}></button>{/if}
        <button type="button" class="key" aria-label={t("history.title")} aria-pressed={route === "/history"} data-testid="open-history" disabled={busy} onclick={() => toggleSheet("/history")}></button>
      </div>
      <div class="well keys-m">
        <button type="button" class="key" aria-label={paused ? t("floating.resume") : t("floating.pause")} title={hint(paused ? t("floating.resume") : t("floating.pause"), settings.shortcuts.pause)} aria-pressed={paused} disabled={!live || recorder.busy} onclick={() => togglePause()}></button>
        <button type="button" class="key" aria-label={t("home.record")} title={hint(t("home.record"), settings.shortcuts.toggle)} aria-pressed={busy} data-testid="rec" disabled={busy} onclick={record}><span class="key-dot"></span></button>
        <button type="button" class="key" aria-label={st.phase === "countdown" ? t("common.cancel") : t("floating.stopLong")} disabled={!(live || st.phase === "countdown") || recorder.busy} onclick={stop}></button>
      </div>
      <div class="well keys-r"><button type="button" class="key" aria-label={t("common.settings")} aria-pressed={route === "/settings"} disabled={busy} onclick={() => toggleSheet("/settings")}></button></div>
      {#if getCapabilities().scrollCapture}<span class="klabel" class:off={busy} aria-hidden="true" style:left="322px" style:width="60px"><svg width="9" height="11" viewBox="0 0 9 11"><path d="M1 0h7v1H1zM4 2h1v5.3l2-2 .7.7L4.5 9.2 1.3 6l.7-.7 2 2zM1 10h7v1H1z" fill="currentColor" /></svg>{t("face.key.scroll")}</span>{/if}
      <span class="klabel" class:off={busy} aria-hidden="true" style:left={getCapabilities().scrollCapture ? "384px" : "374px"} style:width={getCapabilities().scrollCapture ? "60px" : undefined}><svg width="13" height="9" viewBox="0 0 13 9"><path d="M6 0v9L0 4.5zM13 0v9L7 4.5z" fill="currentColor" /></svg>{t("face.key.history")}</span>
      <span class="klabel" class:off={!live} aria-hidden="true" style:left="474px"><svg width="8" height="9" viewBox="0 0 8 9">{#if paused}<path d="M0 0v9l8-4.5z" fill="currentColor" />{:else}<path d="M0 0h3v9H0zM5 0h3v9H5z" fill="currentColor" />{/if}</svg>{paused ? t("floating.resume") : t("floating.pause")}</span>
      <span class="klabel" aria-hidden="true" style:left="536px">{t("face.key.record")}</span>
      <span class="klabel" class:off={!(live || st.phase === "countdown")} aria-hidden="true" style:left="598px"><svg width="9" height="9" viewBox="0 0 9 9"><path d="M0 0h9v9H0z" fill="currentColor" /></svg>{st.phase === "countdown" ? t("common.cancel") : t("floating.stop")}</span>
      <span class="klabel" class:off={busy} aria-hidden="true" style:left="698px">{t("common.settings")}</span>

      <div class="quality glass" role="group" aria-label={t("face.quality")} data-testid="quality">
        {#each ["1080p", "4k"] as const as key (key)}
          <button type="button" aria-pressed={quality === key} disabled={busy || !qualityAvailable(key, screens[screenIndex]?.width)} onclick={() => setQuality(key)}>{t(`quality.${key}`)}</button>
        {/each}
      </div>
      <p class="cap quality-cap" data-testid="quality-cap">{t("face.quality")} · {quality === "1080p" || quality === "4k" ? "" : `${t(`quality.${quality}`)} · `}{outSize.width} × {outSize.height}</p>

      <div class="ctl" style:left="836px">
        <Lever round marks={[{ label: t("common.off") }, { label: t("common.on") }]} index={settings.audio.micEnabled ? 1 : 0} label={settings.audio.micEnabled ? t("home.micOn") : t("home.micOff")} pressed={settings.audio.micEnabled} disabled={busy} onclick={toggleMic} />
        <label class="eng">
          <span>{t("home.mic")}</span>
          <select disabled={!settings.audio.micEnabled || busy} onchange={(e) => { const v = (e.target as HTMLSelectElement).value || null; void updateSettings((s) => { s.audio.micDeviceId = v; }); }}>
            <option value="">{followLabel(defaultMic)}</option>
            {#each devices?.microphones ?? [] as m (m.id)}<option value={m.name} selected={m.name === micChoice}>{m.name}</option>{/each}
          </select>
        </label>
      </div>
      <div class="ctl" style:left="976px">
        <Lever marks={[{ label: t("common.off") }, { label: t("common.on"), off: !canRecordOutput }]} index={outputValue === "off" ? 0 : 1} label={outputValue === "off" ? t("home.outputOff") : t("home.outputOn")} pressed={outputValue !== "off"} disabled={!canRecordOutput || busy} onclick={() => setOutput(outputValue === "off" ? "auto" : "off")} />
        <label class="eng">
          <span>{t("home.output")}</span>
          <select disabled={!canRecordOutput || busy} onchange={(e) => setOutput((e.target as HTMLSelectElement).value)}>
            {#if canRecordOutput}<option value="auto" selected={outputValue === "auto"}>{t("home.followSystemPlain")}</option>{/if}
            <option value="off" selected={outputValue === "off"}>{canRecordOutput ? t("home.noOutput") : t("face.outputUnavailable")}</option>
            {#each devices?.loopbacks ?? [] as d (d.id)}<option value="device:{d.id}" selected={outputValue === `device:${d.id}`}>{d.name}</option>{/each}
          </select>
        </label>
      </div>

      <div class="foot">
        <div class="foot-l">
          <fieldset class="save" data-testid="save-to">
            <legend>{t("home.saveTo")}</legend>
            <label><input type="checkbox" checked={localOn} disabled={!driveOn} onchange={(e) => void updateSettings((s) => (s.saveLocal = (e.target as HTMLInputElement).checked))} /> {t("home.saveLocal")}</label>
            <label><input type="checkbox" checked={driveOn} disabled={driveState.signingIn} onchange={(e) => { const el = e.target as HTMLInputElement; const on = el.checked; void toggleDrive(on).finally(() => (el.checked = driveOn)); }} /> {t("home.saveDrive")}</label>
          </fieldset>
          {#if btMic}
            <p class="tag" data-testid="bt-mic">{t("home.btMic")}</p>
          {:else}
            <p class="hint">{#if localOn}{t("home.hint", { mb: settings.video.targetMbPerHour, dir: settings.outputDir })}{:else}{t("home.hintCloud", { mb: settings.video.targetMbPerHour })}{/if}{#if driveOn} · <span data-testid="drive-line">{t(localOn ? "home.driveLine" : "home.driveOnlyLine", { folder: "FlowGT Recorder" })}</span>{/if}</p>
          {/if}
        </div>
        <div class="foot-r">
          {#if driveState.status.email}
            <span data-testid="account">{driveState.status.email}</span>
          {:else}
            <button type="button" class="link" onclick={signInToGoogle} disabled={driveState.signingIn}>{driveState.signingIn ? t("home.signingIn") : t("home.signIn")}</button>
          {/if}
        </div>
      </div>

      {#if sheet}
        <section class="sheet">{@render sheet()}</section>
      {:else if shot}
        <section class="sheet shot-card" data-testid="scroll-result">
          <header class="shot-head">
            <div>
              <h1>{t("scroll.result.title")}</h1>
              <p class="muted">{t("scroll.result.meta", { w: shot.width, h: shot.height })}{#if shot.copied} · {t("scroll.result.copied")}{/if}</p>
              <p class="faint mono shot-path">{shot.path}</p>
            </div>
            <div class="shot-actions">
              <button type="button" class="btn" onclick={() => shot && invoke("reveal_in_file_manager", { path: shot.path })}>{t("scroll.result.reveal")}</button>
              <button type="button" class="btn btn-primary" onclick={() => (shot = null)}>{t("common.back")}</button>
            </div>
          </header>
          <div class="shot-view">{#if shot.url}<img src={shot.url} alt="" />{/if}</div>
        </section>
      {/if}
    </div>
  </main>
</div>

{#if picking}
  <ScreenPicker initialName={name} onclose={() => (picking = false)} />
{/if}

<style>
  .room { position: fixed; inset: 0; overflow: hidden; background: var(--color-bg); }
  /* Centred by its own middle, so it stays centred when the window is smaller than the drawing. */
  .stage { position: absolute; left: 50%; top: 50%; width: 1180px; height: 760px; margin: -380px 0 0 -590px; color: var(--color-plate-ink); font-size: 13px; line-height: 1.4; }
  .titlebar { position: absolute; z-index: 1; left: 0; right: 0; top: 0; }
  .plate { position: absolute; left: 14px; top: 14px; width: 1152px; height: 732px; border-radius: var(--radius-l); background: var(--color-plate); box-shadow: inset 0 1px 0 var(--color-plate-hi), inset 0 0 0 1px var(--color-plate-lo), var(--shadow-1); }
  .plate > * { position: absolute; }
  .screw { width: 12px; height: 12px; border-radius: 50%; background: var(--color-plate-hi); box-shadow: inset 0 0 0 1px var(--color-plate-lo); }
  .screw::before, .screw::after { content: ""; position: absolute; left: 5.25px; top: 2px; width: 1.5px; height: 8px; background: var(--color-plate-lo); }
  .screw::after { transform: rotate(90deg); }
  .path { left: 0; top: 0; pointer-events: none; }
  .path path { fill: none; stroke: var(--color-tape); stroke-width: 1.5px; }
  .path circle { fill: var(--color-plate-hi); stroke: var(--color-plate-lo); }
  .reel-l { left: 92px; top: 28px; }
  .reel-r { left: 684px; top: 28px; }

  .name { left: 436px; top: 30px; width: 280px; display: flex; flex-direction: column; align-items: center; gap: 3px; font-size: var(--text-sm); color: var(--color-plate-muted); }
  .name input { width: 100%; height: 44px; padding: 0 var(--space-3); text-align: center; border: 1px solid var(--color-knob-line); border-radius: var(--radius-s); background: var(--color-surface); color: var(--color-text); font-size: var(--text-md); }
  .status { left: 506px; top: 198px; width: 140px; height: 36px; display: flex; align-items: center; justify-content: center; gap: var(--space-2); font-weight: 600; }

  .monitor { left: 432px; top: 334px; width: 288px; height: 162px; padding: 5px; border-radius: var(--radius-l); }
  .screen { position: relative; width: 100%; height: 100%; overflow: hidden; border-radius: var(--radius-s); background: var(--color-desk); }
  .shot { display: block; width: 100%; height: 100%; object-fit: contain; }
  .desk { position: absolute; inset: 0; display: flex; gap: var(--space-2); padding: 18px 10px 16px; border-top: 9px solid var(--color-desk-pane); border-bottom: 8px solid var(--color-desk-pane); }
  .desk i { flex: 3; border: 1px solid var(--color-desk-line); border-radius: var(--radius-s); background: var(--color-desk-pane); }
  .desk i + i { flex: 2; }
  .pip { position: absolute; aspect-ratio: 1 / 1; display: flex; align-items: center; justify-content: center; overflow: hidden; background: var(--color-glass); color: var(--color-glass-fg); box-shadow: 0 0 0 2px var(--color-desk-pane); }
  .pip.top-left { left: 4%; top: 7%; } .pip.top-right { right: 4%; top: 7%; }
  .pip.bottom-left { left: 4%; bottom: 7%; } .pip.bottom-right { right: 4%; bottom: 7%; }
  .pip.rounded { border-radius: 16%; } .pip.circle { border-radius: 50%; }
  .pip video { width: 100%; height: 100%; object-fit: cover; }
  .mirror { transform: scaleX(-1); }
  .denied { position: absolute; left: 0; right: 0; bottom: 0; display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); padding: var(--space-1) var(--space-2); background: var(--color-glass); color: var(--color-glass-fg); font-size: var(--text-sm); }
  .count { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; background: var(--color-overlay); color: var(--color-glass-fg); font-size: 96px; font-weight: 600; line-height: 1; }
  .cap { font-size: var(--text-sm); color: var(--color-plate-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .monitor-cap { left: 336px; top: 499px; width: 480px; text-align: center; }

  .vu { left: 56px; top: 400px; width: 84px; height: 106px; padding: 9px 14px 6px; display: flex; justify-content: space-between; }
  .col { display: flex; flex-direction: column; align-items: center; gap: 3px; }
  .col i { width: 18px; height: 6px; border-radius: var(--radius-s); background: var(--color-glass-dim); }
  .col i.lit { background: var(--color-glass-fg); }
  .col i.lit.hot { background: var(--color-record); }
  .col span { margin-top: 1px; font-size: var(--text-xs); color: var(--color-glass-muted); }
  .lowdisk { left: 152px; top: 470px; max-width: 264px; }
  .counter { left: 928px; top: 420px; width: 168px; height: 46px; display: flex; align-items: center; justify-content: center; gap: var(--space-1); font-size: 21px; font-weight: 600; }
  .counter span { min-width: 2ch; padding: 3px 6px; border-radius: var(--radius-s); background: var(--color-glass-dim); text-align: center; }
  .counter-cap { left: 928px; top: 472px; width: 168px; text-align: right; }

  .ctl { top: 518px; width: 132px; display: flex; flex-direction: column; align-items: center; }
  .eng { width: 100%; display: flex; flex-direction: column; align-items: center; gap: 1px; }
  .eng span { font-weight: 600; }
  .eng em, .eng select { max-width: 100%; font-style: normal; font-size: var(--text-sm); color: var(--color-plate-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .eng select { width: 100%; height: 22px; padding: 0 var(--space-1); border: 0; border-radius: var(--radius-s); background: transparent; text-align: center; text-align-last: center; cursor: pointer; }
  .eng select:hover:not(:disabled) { background: var(--color-plate-hi); }
  .eng select:disabled { color: var(--color-plate-off); cursor: default; }
  .eng select option { background: var(--color-surface); color: var(--color-text); }

  .well { top: 556px; }
  .keys-l { left: 382px; } .keys-l.two { left: 320px; } .keys-m { left: 482px; } .keys-r { left: 706px; }
  .quality { left: 744px; top: 420px; width: 168px; height: 46px; padding: 5px; display: flex; gap: 4px; }
  .quality button { flex: 1; border: 0; border-radius: var(--radius-s); background: var(--color-glass-dim); color: var(--color-glass-muted); font-size: var(--text-md); font-weight: 600; cursor: pointer; }
  .quality button[aria-pressed="true"] { background: var(--color-glass-fg); color: var(--color-glass); }
  .quality button:disabled { opacity: 0.4; cursor: default; }
  .quality-cap { left: 728px; top: 472px; width: 200px; text-align: center; }
  .shot-card { display: flex; flex-direction: column; gap: var(--space-3); }
  .shot-head { display: flex; justify-content: space-between; align-items: flex-start; gap: var(--space-4); }
  .shot-head h1 { margin: 0 0 var(--space-1); font-size: var(--text-lg); }
  .shot-head p { margin: 0; }
  .shot-path { word-break: break-all; font-size: var(--text-sm); }
  .shot-actions { flex: none; display: flex; gap: var(--space-2); }
  .shot-view { flex: 1; min-height: 0; overflow: auto; border: 1px solid var(--color-border); border-radius: var(--radius-s); background: var(--color-desk); }
  .shot-view img { display: block; width: 100%; }
  .klabel { top: 628px; width: 80px; display: flex; align-items: center; justify-content: center; gap: 5px; font-size: var(--text-sm); white-space: nowrap; }
  .klabel.off { color: var(--color-plate-off); }

  .foot { left: 40px; right: 40px; top: 672px; height: 52px; display: flex; justify-content: space-between; align-items: flex-start; gap: var(--space-5); font-size: var(--text-sm); color: var(--color-plate-muted); }
  .foot-l { flex: 1; min-width: 0; max-width: 500px; display: flex; flex-direction: column; gap: 2px; }
  .foot-r { flex: none; max-width: 420px; text-align: right; }
  .save { display: flex; align-items: center; gap: var(--space-4); border: 0; padding: 0; margin: 0; }
  .save legend { float: left; margin-right: var(--space-3); padding: 0; }
  .save label { display: inline-flex; align-items: center; gap: var(--space-1); }
  .save input { accent-color: var(--color-accent); }
  .hint { overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; word-break: break-all; }
  .link { padding: var(--space-1) var(--space-2); border: 0; border-radius: var(--radius-s); background: transparent; color: var(--color-plate-ink); text-decoration: underline; }

  .sheet { left: 226px; top: 20px; width: 700px; height: 496px; z-index: 5; overflow: auto; padding: var(--space-5); border: 1px solid var(--color-border); border-radius: var(--radius-l); background: var(--color-surface); color: var(--color-text); box-shadow: var(--shadow-floating); font-size: var(--text-md); line-height: var(--leading-normal); }
</style>
