//! The IPC surface. One function per entry in `Commands` (src/lib/types.ts), snake_case names,
//! camelCase JSON. Errors are `{code, detail}` with an i18n key as the code.

use crate::error::{CmdResult, CommandError};
use crate::permissions::{self, Kind};
use crate::recorder::{EventSink, StartParams};
use crate::sidecar::{self, Tools};
use crate::state::AppState;
use chrono::Local;
use recorder_core::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "settings";

// ───────────────────────────── events ─────────────────────────────

/// Turns recorder callbacks into Tauri events and drives the windows and the tray.
pub struct TauriSink {
    pub app: AppHandle,
    last_phase: std::sync::Mutex<Phase>,
}

impl TauriSink {
    pub fn new(app: AppHandle) -> Self {
        TauriSink { app, last_phase: std::sync::Mutex::new(Phase::Idle) }
    }
}

impl EventSink for TauriSink {
    fn state(&self, st: &RecorderState) {
        let _ = self.app.emit("recorder://state", st);
        let mut last = self.last_phase.lock().expect("phase lock");
        if *last != st.phase {
            let was_idle = *last == Phase::Idle;
            *last = st.phase;
            crate::tray::update(&self.app, st.phase);
            match st.phase {
                Phase::Countdown => {
                    crate::windows::hide_main(&self.app);
                    crate::windows::show_floating(&self.app, true);
                }
                Phase::Starting | Phase::Recording | Phase::Paused | Phase::Finalizing => {
                    if was_idle {
                        crate::windows::hide_main(&self.app);
                    }
                    crate::windows::show_floating(&self.app, false);
                }
                Phase::Idle => {
                    crate::windows::hide_floating(&self.app);
                    crate::windows::show_main(&self.app);
                }
            }
        }
    }
    fn progress(&self, p: &Progress) {
        let _ = self.app.emit("recorder://progress", p);
    }
    fn warning(&self, w: &Warning) {
        let _ = self.app.emit("recorder://warning", w);
    }
    fn error(&self, code: &str, detail: &str) {
        let _ = self.app.emit("recorder://error", json!({ "code": code, "detail": detail }));
    }
    fn finished(&self, r: &RecordingResult) {
        let _ = self.app.emit("recorder://finished", r);
    }
}

// ───────────────────────────── settings ─────────────────────────────

pub fn default_output_dir() -> PathBuf {
    dirs::video_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join("FlowGT Recorder")
}

pub fn effective_output_dir(s: &Settings) -> PathBuf {
    if s.output_dir.trim().is_empty() {
        default_output_dir()
    } else {
        PathBuf::from(&s.output_dir)
    }
}

pub fn load_settings(app: &AppHandle) -> CmdResult<Settings> {
    let platform = Platform::current();
    let store = app.store(STORE_FILE)?;
    let mut s = match store.get(STORE_KEY) {
        Some(v) => settings::from_json(&v.to_string(), platform).unwrap_or_else(|e| {
            log::warn!("stored settings are unusable ({e}); using defaults");
            settings::default_settings(platform)
        }),
        None => settings::default_settings(platform),
    };
    if s.output_dir.trim().is_empty() {
        s.output_dir = default_output_dir().to_string_lossy().into_owned();
    }
    Ok(s)
}

fn store_settings(app: &AppHandle, s: &Settings) -> CmdResult<()> {
    let store = app.store(STORE_FILE)?;
    store.set(STORE_KEY, serde_json::to_value(s)?);
    store.save()?;
    Ok(())
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> CmdResult<Settings> {
    load_settings(&app)
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> CmdResult<Settings> {
    // Round-trip through the core so ranges are validated and defaults normalised.
    let json = settings::to_json(&settings)?;
    let clean = recorder_core::settings::from_json(&json, Platform::current())?;
    store_settings(&app, &clean)?;
    crate::shortcuts::register(&app, &clean);
    Ok(clean)
}

// ───────────────────────────── tools, devices, encoder ─────────────────────────────

pub async fn tools(state: &AppState) -> CmdResult<Tools> {
    let mut guard = state.tools.lock().await;
    if guard.is_none() {
        *guard = Some(Tools::discover()?);
    }
    Ok(guard.clone().expect("just set"))
}

/// First encoder of the fallback chain that really encodes one test frame (cached).
pub async fn resolve_encoder(state: &AppState, tools: &Tools, pref: EncoderPreference) -> CmdResult<EncoderChoice> {
    let mut cache = state.encoder.lock().await;
    if let Some(e) = cache.as_ref() {
        let wanted_codec = match pref {
            EncoderPreference::Auto => true,
            EncoderPreference::Hevc => e.codec == VideoCodec::Hevc,
            EncoderPreference::H264 => e.codec == VideoCodec::H264,
        };
        if wanted_codec {
            return Ok(e.clone());
        }
    }
    let listing = sidecar::run(&command::list_encoders_command(&tools.ffmpeg_str())).await?;
    let mut names = encoders::parse_encoder_names(&listing.stdout);
    for _ in 0..8 {
        let choice = encoders::choose(&names, Platform::current(), pref)?;
        let test = sidecar::run(&encoders::self_test_command(&tools.ffmpeg_str(), &choice)).await?;
        if test.success {
            *cache = Some(choice.clone());
            return Ok(choice);
        }
        log::warn!("encoder {} failed its test frame, trying the next one", choice.name);
        names.retain(|n| n != &choice.name);
    }
    Err(CommandError::new("error.noEncoder", "no encoder passed its test frame"))
}

pub async fn devices(app: &AppHandle, tools: &Tools) -> CmdResult<DeviceList> {
    let platform = Platform::current();
    let out = sidecar::run(&command::list_devices_command(&tools.ffmpeg_str(), platform)).await?;
    // Device listings go to stderr and ffmpeg exits non-zero by design.
    let mut list = match platform {
        Platform::MacOs => devices::parse_avfoundation(&out.stderr),
        Platform::Windows => devices::parse_dshow(&out.stderr),
        Platform::Linux => DeviceList::default(),
    };
    let monitors: Vec<(u32, String, u32, u32)> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, m)| (i as u32, m.name().cloned().unwrap_or_else(|| format!("Display {}", i + 1)), m.size().width, m.size().height))
        .collect();
    if platform == Platform::Windows {
        list.screens = devices::windows_screens(&monitors);
    } else {
        // Sizes from the OS; the exact capture size is probed when a recording starts.
        for (i, d) in list.screens.iter_mut().enumerate() {
            if let Some((_, _, w, h)) = monitors.get(i) {
                d.width = Some(*w);
                d.height = Some(*h);
            }
        }
    }
    Ok(list)
}

#[tauri::command]
pub async fn list_devices(app: AppHandle, state: State<'_, AppState>) -> CmdResult<DeviceList> {
    let t = tools(&state).await?;
    devices(&app, &t).await
}

#[tauri::command]
pub async fn get_app_info(state: State<'_, AppState>) -> CmdResult<AppInfo> {
    let t = tools(&state).await;
    let (path, version, bundled, encoder) = match &t {
        Ok(t) => {
            let enc = resolve_encoder(&state, t, EncoderPreference::Auto).await.ok();
            (t.ffmpeg_str(), t.version().await, t.bundled, enc)
        }
        Err(_) => (String::new(), String::new(), false, None),
    };
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: Platform::current(),
        ffmpeg_path: path,
        ffmpeg_version: version,
        ffmpeg_bundled: bundled,
        encoder,
        system_audio_available: cfg!(target_os = "macos") || cfg!(target_os = "windows"),
    })
}

// ───────────────────────────── permissions ─────────────────────────────

fn kind_of(s: &str) -> CmdResult<Kind> {
    Kind::parse(s).ok_or_else(|| CommandError::new("error.invalidArgument", format!("unknown permission kind {s:?}")))
}

#[tauri::command]
pub async fn check_permissions() -> CmdResult<PermissionStatus> {
    Ok(permissions::check())
}

#[tauri::command]
pub async fn request_permission(kind: String) -> CmdResult<PermissionState> {
    let k = kind_of(&kind)?;
    tokio::task::spawn_blocking(move || permissions::request(k)).await.map_err(|e| CommandError::new("error.unknown", e.to_string()))
}

#[tauri::command]
pub async fn open_privacy_settings(kind: String) -> CmdResult<()> {
    permissions::open_settings(kind_of(&kind)?)?;
    Ok(())
}

// ───────────────────────────── files, disk ─────────────────────────────

#[tauri::command]
pub async fn pick_output_dir(app: AppHandle) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().blocking_pick_folder()).await.map_err(|e| CommandError::new("error.unknown", e.to_string()))?;
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn check_disk(app: AppHandle, path: Option<String>) -> CmdResult<DiskStatus> {
    let s = load_settings(&app)?;
    let dir = path.filter(|p| !p.is_empty()).map(PathBuf::from).unwrap_or_else(|| effective_output_dir(&s));
    let (free, total) = crate::disk::space(&dir)?;
    let b = budget::budget_for(s.video.target_mb_per_hour, s.audio.bitrate_kbps, 1);
    Ok(recorder_core::disk::status(&dir.to_string_lossy(), free, total, &b, s.low_disk_warn_gb))
}

fn fallback_name(s: &Settings) -> &'static str {
    if s.language == Language::En {
        "Recording"
    } else {
        "录像"
    }
}

#[tauri::command]
pub async fn preview_filename(app: AppHandle, name: String) -> CmdResult<String> {
    let s = load_settings(&app)?;
    Ok(filename::build_stem(&s.filename_template, &name, fallback_name(&s), Local::now()))
}

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> CmdResult<()> {
    tauri_plugin_opener::reveal_item_in_dir(PathBuf::from(path)).map_err(|e| CommandError::io(e.to_string()))
}

#[tauri::command]
pub async fn probe_file(state: State<'_, AppState>, path: String) -> CmdResult<MediaInfo> {
    let t = tools(&state).await?;
    sidecar::probe(&t, Path::new(&path)).await
}

// ───────────────────────────── recording ─────────────────────────────

/// Resolve the macOS/Windows capture size of one screen. macOS: ask ffmpeg for one frame (needs
/// the screen-recording permission); anything else falls back to the OS monitor size.
async fn screen_size(tools: &Tools, device: &Device) -> CmdResult<(u32, u32)> {
    if Platform::current() == Platform::MacOs {
        if permissions::check().screen != PermissionState::Granted {
            return Err(CommandError::new("error.permission", "screen recording permission is not granted"));
        }
        let cmd = FfmpegCommand {
            program: tools.ffmpeg_str(),
            args: ["-hide_banner", "-f", "avfoundation", "-framerate", "30", "-i", &format!("{}:none", device.id), "-frames:v", "1", "-f", "null", "-"].iter().map(|s| s.to_string()).collect(),
        };
        let out = sidecar::run(&cmd).await?;
        if let Some(size) = devices::parse_avfoundation_screen_size(&out.stderr) {
            return Ok(size);
        }
    }
    match (device.width, device.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Ok((w, h)),
        _ => Err(CommandError::new("error.start", "cannot determine the screen size")),
    }
}

fn find_helper() -> Option<PathBuf> {
    let name = if cfg!(windows) { "sck-audio.exe" } else { "sck-audio" };
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            candidates.push(d.join(name));
        }
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../helpers/sck-audio/.build/release").join(name));
    candidates.into_iter().find(|p| p.is_file())
}

/// Pick the system-audio input for the current settings and machine.
pub fn resolve_system_audio(s: &Settings, list: &DeviceList, helper: Option<&Path>) -> Option<InputSource> {
    let platform = Platform::current();
    let by_id = |id: &str| match platform {
        Platform::Windows => InputSource::DShow { video: None, audio: Some(id.to_string()) },
        _ => InputSource::AvFoundation { video_index: None, audio_index: Some(id.to_string()), capture_cursor: false },
    };
    let pipe = || InputSource::PcmPipe { path: String::new(), sample_rate: 48_000, channels: 2 };
    match &s.audio.system_audio {
        SystemAudioSource::Off => None,
        SystemAudioSource::Device(id) => Some(by_id(id)),
        SystemAudioSource::Helper => helper.map(|_| pipe()),
        // The helper is experimental (D7), so only an explicit choice uses it: a helper that never
        // writes would leave ffmpeg waiting on the FIFO and the recording would fail to start.
        SystemAudioSource::Auto => list.loopbacks.first().map(|d| by_id(&d.id)),
    }
}

/// Everything needed to start: tools, encoder, devices, sizes, paths.
pub async fn build_params(app: &AppHandle, state: &AppState, meeting_name: &str) -> CmdResult<StartParams> {
    let mut s = load_settings(app)?;
    let t = tools(state).await?;
    let list = devices(app, &t).await?;
    let platform = Platform::current();
    let encoder = resolve_encoder(state, &t, s.video.encoder_preference).await?;

    // Screen: the chosen one, else the first.
    let wanted_id = match &s.capture {
        CaptureTarget::Display { display_id } | CaptureTarget::Region { display_id, .. } => Some(display_id.clone()),
        CaptureTarget::Window { .. } => None,
    };
    let screen = list
        .screens
        .iter()
        .find(|d| Some(&d.id) == wanted_id.as_ref())
        .or_else(|| list.screens.first())
        .cloned()
        .ok_or_else(|| CommandError::new("error.start", "no screen to record"))?;
    s.capture = match s.capture.clone() {
        CaptureTarget::Region { region, .. } => CaptureTarget::Region { display_id: screen.id.clone(), region },
        CaptureTarget::Window { window_id } => CaptureTarget::Window { window_id },
        CaptureTarget::Display { .. } => CaptureTarget::Display { display_id: screen.id.clone() },
    };
    let (sw, sh) = screen_size(&t, &screen).await?;

    // A camera or microphone the user has not allowed must not stop the recording: ffmpeg would
    // exit at once. Record what is allowed instead.
    if platform == Platform::MacOs {
        let perms = permissions::check();
        if perms.camera != PermissionState::Granted {
            s.camera.enabled = false;
        }
        if perms.microphone != PermissionState::Granted {
            s.audio.mic_enabled = false;
        }
    }

    // Camera: default means the first camera everywhere; no camera at all means no PiP.
    if s.camera.enabled {
        match (&s.camera.device_id, list.cameras.first()) {
            (Some(id), _) if list.cameras.iter().any(|d| &d.id == id) => {}
            (_, Some(first)) => s.camera.device_id = Some(first.id.clone()),
            (_, None) => s.camera.enabled = false,
        }
    }
    // Windows microphones are addressed by name; macOS has a real "default".
    if platform == Platform::Windows && s.audio.mic_enabled && s.audio.mic_device_id.is_none() {
        match list.microphones.first() {
            Some(m) => s.audio.mic_device_id = Some(m.id.clone()),
            None => s.audio.mic_enabled = false,
        }
    }

    let helper = find_helper();
    let system = resolve_system_audio(&s, &list, helper.as_deref());
    let uses_helper = matches!(system, Some(InputSource::PcmPipe { .. }));

    let out_dir = effective_output_dir(&s);
    tokio::fs::create_dir_all(&out_dir).await?;
    let plan_tmp = out_dir.join("segment.tmp");
    let mut plan = command::make_plan(&s, platform, sw, sh, encoder, &plan_tmp, system)?;
    let b = plan.budget.clone();
    plan.duration_limit_s = None;

    let (free, total) = crate::disk::space(&out_dir)?;
    let ds = recorder_core::disk::status(&out_dir.to_string_lossy(), free, total, &b, s.low_disk_warn_gb);
    if recorder_core::disk::refuse_start(&ds) {
        return Err(CommandError::new("error.noSpace", format!("{} MB free", free / 1_000_000)));
    }

    let stem = filename::build_stem(&s.filename_template, meeting_name, fallback_name(&s), Local::now());
    let final_output = filename::unique_path(&out_dir, &stem, "mp4", &|p| p.exists());
    let session_id = uuid::Uuid::new_v4().to_string();
    let work_dir = out_dir.join(".flowgt-recorder").join(&session_id);
    Ok(StartParams {
        plan,
        session_id,
        meeting_name: meeting_name.to_string(),
        final_output,
        work_dir,
        target_mb_per_hour: s.video.target_mb_per_hour,
        low_disk_warn_gb: s.low_disk_warn_gb,
        tools: t,
        helper: if uses_helper { helper } else { None },
    })
}

/// Countdown, then start. Used by the command, the tray and the shortcut.
pub async fn begin(app: &AppHandle, meeting_name: String) -> CmdResult<RecorderState> {
    let state = app.state::<AppState>();
    if Platform::current() == Platform::MacOs && permissions::check().screen != PermissionState::Granted {
        return Err(CommandError::new("error.permission", "screen recording permission is not granted"));
    }
    let st = state.recorder.begin_countdown().await?;
    *state.last_meeting.lock().expect("meeting lock") = meeting_name.clone();
    let s = load_settings(app)?;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        for remaining in (1..=s.countdown_seconds).rev() {
            let _ = handle.emit("countdown://tick", json!({ "remaining": remaining }));
            tokio::time::sleep(Duration::from_secs(1)).await;
            if state.recorder.phase().await != Phase::Countdown {
                return; // cancelled
            }
        }
        if state.recorder.phase().await != Phase::Countdown {
            return;
        }
        let started = match build_params(&handle, &state, &meeting_name).await {
            Ok(p) => {
                let without_camera = p.plan.camera.is_some().then(|| {
                    let mut q = p.clone();
                    q.plan.camera = None;
                    q.plan.pip = None;
                    q
                });
                match (state.recorder.start(p).await, without_camera) {
                    // A camera that will not open (busy, unsupported mode) must not cost the recording.
                    (Err(e), Some(q)) if e.code == "error.start" => {
                        log::warn!("start with the camera failed, retrying without it: {}", e.detail);
                        let retried = state.recorder.start(q).await;
                        if retried.is_ok() {
                            let _ = handle.emit("recorder://warning", json!({ "code": "warn.cameraLost", "detail": e.detail }));
                        }
                        retried
                    }
                    (result, _) => result,
                }
            }
            Err(e) => Err(e),
        };
        if let Err(e) = started {
            state.recorder.abort_countdown().await;
            let _ = handle.emit("recorder://error", json!({ "code": e.code, "detail": e.detail }));
        }
    });
    Ok(st)
}

pub async fn toggle_recording(app: &AppHandle) {
    let state = app.state::<AppState>();
    match state.recorder.phase().await {
        Phase::Idle => {
            let name = state.last_meeting.lock().expect("meeting lock").clone();
            if let Err(e) = begin(app, name).await {
                crate::windows::show_main(app);
                let _ = app.emit("recorder://error", json!({ "code": e.code, "detail": e.detail }));
            }
        }
        Phase::Countdown => {
            let _ = state.recorder.cancel().await;
        }
        Phase::Recording | Phase::Paused => {
            let _ = state.recorder.stop().await;
        }
        _ => {}
    }
}

pub async fn toggle_pause(app: &AppHandle) {
    let state = app.state::<AppState>();
    match state.recorder.phase().await {
        Phase::Recording => {
            let _ = state.recorder.pause().await;
        }
        Phase::Paused => {
            let _ = state.recorder.resume().await;
        }
        _ => {}
    }
}

/// A small JPEG of one screen as a data URL, for the screen picker. `None` when the
/// screen-recording permission is missing (asking ffmpeg would only fail) or on other platforms.
#[tauri::command]
pub async fn screen_thumbnail(state: State<'_, AppState>, id: String) -> CmdResult<Option<String>> {
    use base64::Engine;
    if Platform::current() != Platform::MacOs || permissions::check().screen != PermissionState::Granted {
        return Ok(None);
    }
    let t = tools(&state).await?;
    let mut c = tokio::process::Command::new(&t.ffmpeg);
    c.args(["-hide_banner", "-loglevel", "error", "-f", "avfoundation", "-framerate", "30", "-capture_cursor", "0", "-i"])
        .arg(format!("{id}:none"))
        // The first frames from a screen input can be black; take a later one.
        .args(["-vf", "select=gte(n\\,5),scale=480:-2", "-frames:v", "1", "-c:v", "mjpeg", "-q:v", "5", "-f", "image2pipe", "-"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(5), c.output()).await else {
        return Ok(None);
    };
    if !out.status.success() || out.stdout.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&out.stdout))))
}

/// macOS applies a newly granted screen-recording permission only after a restart.
#[tauri::command]
pub fn relaunch_app(app: AppHandle) {
    app.restart();
}

#[tauri::command]
pub async fn start_recording(app: AppHandle, meeting_name: String) -> CmdResult<RecorderState> {
    begin(&app, meeting_name).await
}

#[tauri::command]
pub async fn pause_recording(state: State<'_, AppState>) -> CmdResult<RecorderState> {
    state.recorder.pause().await
}

#[tauri::command]
pub async fn resume_recording(state: State<'_, AppState>) -> CmdResult<RecorderState> {
    state.recorder.resume().await
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, AppState>) -> CmdResult<RecordingResult> {
    state.recorder.stop().await
}

#[tauri::command]
pub async fn cancel_recording(state: State<'_, AppState>) -> CmdResult<RecorderState> {
    state.recorder.cancel().await
}

#[tauri::command]
pub async fn get_recorder_state(state: State<'_, AppState>) -> CmdResult<RecorderState> {
    Ok(state.recorder.state().await)
}

#[tauri::command]
pub async fn set_floating_visible(app: AppHandle, visible: bool) -> CmdResult<()> {
    if visible {
        crate::windows::show_floating(&app, false);
    } else {
        crate::windows::hide_floating(&app);
    }
    Ok(())
}

// ───────────────────────────── compress ─────────────────────────────

#[tauri::command]
pub async fn compress_to_budget(app: AppHandle, state: State<'_, AppState>, path: String, target_mb: f64) -> CmdResult<String> {
    let t = tools(&state).await?;
    let s = load_settings(&app)?;
    let encoder = resolve_encoder(&state, &t, s.video.encoder_preference).await?;
    let input = PathBuf::from(&path);
    let info = sidecar::probe(&t, &input).await?;
    let dir = input.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let stem = input.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| "recording".into());
    let output = filename::unique_path(&dir, &format!("{stem} (compressed)"), "mp4", &|p| p.exists());
    let cmd = command::compress_command(&t.ffmpeg_str(), &input, &output, info.duration_ms, target_mb, &encoder, s.audio.bitrate_kbps);
    let job_id = uuid::Uuid::new_v4().to_string();
    let id = job_id.clone();
    let duration = info.duration_ms.max(1);
    tauri::async_runtime::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut c = tokio::process::Command::new(&cmd.program);
        c.args(&cmd.args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        let spawned = c.spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("compress://progress", json!({ "jobId": id, "percent": 0, "done": true, "output": null, "error": e.to_string() }));
                return;
            }
        };
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let err_task = tokio::spawn(async move {
            let mut s = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.read_to_string(&mut s).await;
            }
            s
        });
        let mut parser = progress::ProgressParser::new();
        let mut buf = [0u8; 4096];
        if let Some(out) = stdout.as_mut() {
            while let Ok(n) = out.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                for p in parser.feed(&String::from_utf8_lossy(&buf[..n])) {
                    let pct = (p.out_time_ms as f64 / duration as f64 * 100.0).clamp(0.0, 99.0);
                    let _ = app.emit("compress://progress", json!({ "jobId": id, "percent": pct, "done": false, "output": null, "error": null }));
                }
            }
        }
        let status = child.wait().await;
        let err_text = err_task.await.unwrap_or_default();
        let ok = status.is_ok_and(|s| s.success());
        let payload = if ok {
            json!({ "jobId": id, "percent": 100, "done": true, "output": output.to_string_lossy(), "error": null })
        } else {
            json!({ "jobId": id, "percent": 0, "done": true, "output": null, "error": err_text.trim() })
        };
        let _ = app.emit("compress://progress", payload);
    });
    Ok(job_id)
}

// ───────────────────────────── recovery ─────────────────────────────

fn work_root(s: &Settings) -> PathBuf {
    effective_output_dir(s).join(".flowgt-recorder")
}

/// Manifests under the work root, skipping the session being recorded right now.
async fn scan(app: &AppHandle, state: &AppState) -> CmdResult<Vec<SessionManifest>> {
    let s = load_settings(app)?;
    let active = state.recorder.state().await.session_id;
    let mut out = Vec::new();
    let Ok(mut rd) = tokio::fs::read_dir(work_root(&s)).await else { return Ok(out) };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let mp = recovery::manifest_path(&entry.path());
        let Ok(text) = tokio::fs::read_to_string(&mp).await else { continue };
        let Ok(m) = recovery::manifest_from_json(&text) else { continue };
        if Some(&m.session_id) == active.as_ref() {
            continue;
        }
        out.push(m);
    }
    Ok(out)
}

fn sizes(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().map(|m| m.len())
}

#[tauri::command]
pub async fn list_recoverable(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<RecoverableSession>> {
    let mut list = Vec::new();
    for m in scan(&app, &state).await? {
        match recovery::decide(&m, &sizes) {
            recovery::RecoveryAction::Concat { .. } => list.push(recovery::summarize(&m, &sizes)),
            recovery::RecoveryAction::Discard => {
                let _ = tokio::fs::remove_dir_all(&m.work_dir).await; // empty or complete: nothing to offer
            }
        }
    }
    Ok(list)
}

#[tauri::command]
pub async fn recover_session(app: AppHandle, state: State<'_, AppState>, session_id: String) -> CmdResult<RecordingResult> {
    let t = tools(&state).await?;
    let s = load_settings(&app)?;
    let m = scan(&app, &state).await?.into_iter().find(|m| m.session_id == session_id).ok_or_else(|| CommandError::new("error.recovery", "no such session"))?;
    let out_dir = m.final_output.parent().map(Path::to_path_buf).unwrap_or_else(|| effective_output_dir(&s));
    let stem = m.final_output.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| "recovered".into());
    let mut m2 = m.clone();
    m2.final_output = filename::unique_path(&out_dir, &format!("{stem} (recovered)"), "mp4", &|p| p.exists());
    let res = crate::recorder::assemble(&m2, &t, s.video.target_mb_per_hour, m2.plan.budget.clone(), true).await?;
    let _ = tokio::fs::remove_dir_all(&m.work_dir).await;
    Ok(res)
}

#[tauri::command]
pub async fn discard_session(app: AppHandle, state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    if let Some(m) = scan(&app, &state).await?.into_iter().find(|m| m.session_id == session_id) {
        tokio::fs::remove_dir_all(&m.work_dir).await?;
    }
    Ok(())
}

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        get_app_info,
        list_devices,
        check_permissions,
        request_permission,
        open_privacy_settings,
        get_settings,
        save_settings,
        pick_output_dir,
        check_disk,
        preview_filename,
        start_recording,
        pause_recording,
        resume_recording,
        stop_recording,
        cancel_recording,
        get_recorder_state,
        reveal_in_file_manager,
        probe_file,
        compress_to_budget,
        list_recoverable,
        recover_session,
        discard_session,
        set_floating_visible,
        screen_thumbnail,
        relaunch_app,
    ]
}

#[allow(dead_code)]
fn _assert_sink_is_send(_: Arc<dyn EventSink>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir_is_under_videos_or_home() {
        let d = default_output_dir();
        assert!(d.ends_with("FlowGT Recorder"));
    }

    #[test]
    fn empty_output_dir_means_the_default() {
        let mut s = settings::default_settings(Platform::MacOs);
        assert_eq!(effective_output_dir(&s), default_output_dir());
        s.output_dir = "/somewhere".into();
        assert_eq!(effective_output_dir(&s), PathBuf::from("/somewhere"));
    }

    fn loopbacks() -> DeviceList {
        let d = Device { id: "5".into(), name: "BlackHole 2ch".into(), kind: DeviceKind::Microphone, is_default: false, width: None, height: None };
        DeviceList { loopbacks: vec![d], ..Default::default() }
    }

    #[test]
    fn system_audio_choices() {
        let mut s = settings::default_settings(Platform::current());
        s.audio.system_audio = SystemAudioSource::Off;
        assert!(resolve_system_audio(&s, &loopbacks(), None).is_none());
        s.audio.system_audio = SystemAudioSource::Auto;
        // No helper: falls back to the loopback device, or nothing when there is none.
        assert!(resolve_system_audio(&s, &loopbacks(), None).is_some());
        assert!(resolve_system_audio(&s, &DeviceList::default(), None).is_none());
        s.audio.system_audio = SystemAudioSource::Device("7".into());
        assert!(resolve_system_audio(&s, &DeviceList::default(), None).is_some());
        s.audio.system_audio = SystemAudioSource::Helper;
        assert!(resolve_system_audio(&s, &DeviceList::default(), None).is_none());
        assert!(matches!(resolve_system_audio(&s, &DeviceList::default(), Some(Path::new("x"))), Some(InputSource::PcmPipe { .. })));
    }

    #[test]
    fn permission_kinds_reject_junk() {
        assert!(kind_of("screen").is_ok());
        assert!(kind_of("gps").is_err());
    }
}
