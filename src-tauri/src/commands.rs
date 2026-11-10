//! The IPC surface. One function per entry in `Commands` (src/lib/types.ts), snake_case names,
//! camelCase JSON. Errors are `{code, detail}` with an i18n key as the code.

use crate::error::{CmdResult, CommandError};
use crate::permissions::{self, Kind};
use crate::recorder::{EventSink, StartParams};
use crate::sidecar::{self, Tools};
use crate::state::AppState;
use chrono::Local;
use recorder_core::native::{self, NativeCapture, NativeSource, WindowInfo};
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
        TauriSink {
            app,
            last_phase: std::sync::Mutex::new(Phase::Idle),
        }
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
        let _ = self.app.emit(
            "recorder://error",
            json!({ "code": code, "detail": detail }),
        );
    }
    fn finished(&self, r: &RecordingResult) {
        let _ = self.app.emit("recorder://finished", r);
        let signed_in = crate::drive::load_account(&self.app).is_some();
        let Ok(s) = load_settings(&self.app) else {
            return;
        };
        let plan = recorder_core::settings::save_plan(s.save_local, s.save_drive, signed_in);
        if plan.upload {
            spawn_upload(
                self.app.clone(),
                r.path.clone(),
                plan.remove_local_after_upload,
            );
        }
    }
}

// ───────────────────────────── settings ─────────────────────────────

pub fn default_output_dir() -> PathBuf {
    dirs::video_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("FlowGT Recorder")
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
pub async fn resolve_encoder(
    state: &AppState,
    tools: &Tools,
    pref: EncoderPreference,
) -> CmdResult<EncoderChoice> {
    let mut cache = state.encoder.lock().await;
    // The cache answers only the preference it was filled for. It used to answer "auto" with
    // whatever was there, so after one H.264 recording "auto" kept using H.264 until a restart.
    if let Some((cached_pref, e)) = cache.as_ref() {
        if *cached_pref == pref {
            return Ok(e.clone());
        }
    }
    let listing = sidecar::run(&command::list_encoders_command(&tools.ffmpeg_str())).await?;
    let mut names = encoders::parse_encoder_names(&listing.stdout);
    for _ in 0..8 {
        let choice = encoders::choose(&names, Platform::current(), pref)?;
        let test = sidecar::run(&encoders::self_test_command(&tools.ffmpeg_str(), &choice)).await?;
        if test.success {
            *cache = Some((pref, choice.clone()));
            return Ok(choice);
        }
        log::warn!(
            "encoder {} failed its test frame, trying the next one",
            choice.name
        );
        names.retain(|n| n != &choice.name);
    }
    Err(CommandError::new(
        "error.noEncoder",
        "no encoder passed its test frame",
    ))
}

pub async fn devices(app: &AppHandle, tools: &Tools) -> CmdResult<DeviceList> {
    let platform = Platform::current();
    let out = sidecar::run(&command::list_devices_command(
        &tools.ffmpeg_str(),
        platform,
    ))
    .await?;
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
        .map(|(i, m)| {
            (
                i as u32,
                m.name()
                    .cloned()
                    .unwrap_or_else(|| format!("Display {}", i + 1)),
                m.size().width,
                m.size().height,
            )
        })
        .collect();
    // "Default" is whatever the system uses right now, not the first entry of ffmpeg's list.
    mark_system_default(
        &mut list.cameras,
        permissions::default_device_name(Kind::Camera),
    );
    mark_system_default(
        &mut list.microphones,
        permissions::default_device_name(Kind::Microphone),
    );
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

fn mark_system_default(list: &mut [Device], name: Option<String>) {
    let Some(name) = name else { return };
    if list.iter().any(|d| d.name == name) {
        for d in list.iter_mut() {
            d.is_default = d.name == name;
        }
    }
}

/// The device id to record from: the one the user chose (settings keep its NAME, because
/// avfoundation indices shift when devices come and go), else the system default, else the first.
fn resolve_device(list: &[Device], wanted: Option<&str>) -> Option<String> {
    wanted
        .and_then(|w| list.iter().find(|d| d.name == w || d.id == w))
        .or_else(|| list.iter().find(|d| d.is_default))
        .or_else(|| list.first())
        .map(|d| d.id.clone())
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
            let enc = resolve_encoder(&state, t, EncoderPreference::Auto)
                .await
                .ok();
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
        system_audio_available: find_helper().is_some(),
    })
}

// ───────────────────────────── permissions ─────────────────────────────

fn kind_of(s: &str) -> CmdResult<Kind> {
    Kind::parse(s).ok_or_else(|| {
        CommandError::new(
            "error.invalidArgument",
            format!("unknown permission kind {s:?}"),
        )
    })
}

#[tauri::command]
pub async fn check_permissions() -> CmdResult<PermissionStatus> {
    Ok(permissions::check())
}

#[tauri::command]
pub async fn request_permission(kind: String) -> CmdResult<PermissionState> {
    let k = kind_of(&kind)?;
    tokio::task::spawn_blocking(move || permissions::request(k))
        .await
        .map_err(|e| CommandError::new("error.unknown", e.to_string()))
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
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
        .await
        .map_err(|e| CommandError::new("error.unknown", e.to_string()))?;
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn check_disk(app: AppHandle, path: Option<String>) -> CmdResult<DiskStatus> {
    let s = load_settings(&app)?;
    let dir = path
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| effective_output_dir(&s));
    let (free, total) = crate::disk::space(&dir)?;
    let b = budget::budget_for(s.video.target_mb_per_hour, s.audio.bitrate_kbps, 1);
    Ok(recorder_core::disk::status(
        &dir.to_string_lossy(),
        free,
        total,
        &b,
        s.low_disk_warn_gb,
    ))
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
    Ok(filename::build_stem(
        &s.filename_template,
        &name,
        fallback_name(&s),
        Local::now(),
    ))
}

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> CmdResult<()> {
    tauri_plugin_opener::reveal_item_in_dir(PathBuf::from(path))
        .map_err(|e| CommandError::io(e.to_string()))
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
            return Err(CommandError::new(
                "error.permission",
                "screen recording permission is not granted",
            ));
        }
        let cmd = FfmpegCommand {
            program: tools.ffmpeg_str(),
            args: [
                "-hide_banner",
                "-f",
                "avfoundation",
                "-framerate",
                "30",
                "-i",
                &format!("{}:none", device.id),
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        };
        // Never let a stuck device hold the countdown forever; the OS size is a fine fallback.
        if let Ok(out) = tokio::time::timeout(Duration::from_secs(8), sidecar::run(&cmd)).await {
            if let Some(size) = devices::parse_avfoundation_screen_size(&out?.stderr) {
                return Ok(size);
            }
        }
    }
    match (device.width, device.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Ok((w, h)),
        _ => Err(CommandError::new(
            "error.start",
            "cannot determine the screen size",
        )),
    }
}

pub fn find_helper() -> Option<PathBuf> {
    find_tool("sck-audio")
}

/// The native recorder (D27). None = ffmpeg path.
pub fn find_record_helper() -> Option<PathBuf> {
    if std::env::var_os("FLOWGT_FFMPEG_CAPTURE").is_some() {
        return None; // escape hatch for comparisons
    }
    find_tool("sck-record")
}

/// A macOS helper: in Contents/Helpers, next to the executable, in src-tauri/binaries, or in its
/// Swift build directory.
fn find_tool(name: &str) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            candidates.push(d.join("../Helpers").join(name));
            candidates.push(d.join(name));
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    candidates.push(
        root.join("binaries")
            .join(format!("{name}-{}", sidecar::host_triple())),
    );
    candidates.push(root.join(format!("../helpers/{name}/.build/release/{name}")));
    candidates.into_iter().find(|p| p.is_file())
}

/// Pick the system-audio input for the current settings and machine.
pub fn resolve_system_audio(
    s: &Settings,
    list: &DeviceList,
    helper: Option<&Path>,
) -> Option<InputSource> {
    let platform = Platform::current();
    let by_id = |id: &str| match platform {
        Platform::Windows => InputSource::DShow {
            video: None,
            audio: Some(id.to_string()),
        },
        _ => InputSource::AvFoundation {
            video_index: None,
            audio_index: Some(id.to_string()),
            capture_cursor: false,
        },
    };
    let pipe = || InputSource::PcmPipe {
        path: String::new(),
        sample_rate: 48_000,
        channels: 2,
    };
    match &s.audio.system_audio {
        SystemAudioSource::Off => None,
        SystemAudioSource::Device(id) => Some(by_id(id)),
        SystemAudioSource::Helper => helper.map(|_| pipe()),
        // The helper records whatever the system is playing, on any output device (D19).
        SystemAudioSource::Auto => match helper {
            Some(_) => Some(pipe()),
            None => list.loopbacks.first().map(|d| by_id(&d.id)),
        },
    }
}

/// Everything needed to start: tools, encoder, devices, sizes, paths.
pub async fn build_params(
    app: &AppHandle,
    state: &AppState,
    meeting_name: &str,
) -> CmdResult<StartParams> {
    let mut s = load_settings(app)?;
    let t = tools(state).await?;
    let list = devices(app, &t).await?;
    let platform = Platform::current();
    let encoder = resolve_encoder(state, &t, s.video.encoder_preference).await?;

    // Native path (D27): the display id and its pixel size come from CoreGraphics, no probe.
    let record_helper = find_record_helper().filter(|_| platform == Platform::MacOs);

    // A window: only the native helper can record one, and it must still be open.
    let window = match (&s.capture, &record_helper) {
        (CaptureTarget::Window { window_id }, Some(h)) => {
            let id = window_id.parse::<u32>().map_err(|_| {
                CommandError::new("error.windowGone", format!("bad window id {window_id:?}"))
            })?;
            let list = window_list(h).await?;
            Some(list.into_iter().find(|w| w.id == id).ok_or_else(|| {
                CommandError::new("error.windowGone", format!("window {id} is not on screen"))
            })?)
        }
        (CaptureTarget::Window { .. }, None) => {
            return Err(CommandError::new(
                "error.start",
                "recording one window needs the native recorder (macOS 13 or newer)",
            ))
        }
        _ => None,
    };

    // Screen: the chosen one, else the first.
    let wanted_id = match &s.capture {
        CaptureTarget::Display { display_id } | CaptureTarget::Region { display_id, .. } => {
            Some(display_id.clone())
        }
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
        CaptureTarget::Region { region, .. } => CaptureTarget::Region {
            display_id: screen.id.clone(),
            region,
        },
        CaptureTarget::Window { window_id } => CaptureTarget::Window { window_id },
        CaptureTarget::Display { .. } => CaptureTarget::Display {
            display_id: screen.id.clone(),
        },
    };
    let native_display = record_helper
        .as_ref()
        .and_then(|_| list.screens.iter().position(|d| d.id == screen.id))
        .and_then(crate::windows::display_at);
    let (sw, sh) = match (&window, native_display) {
        (Some(w), _) => (w.width, w.height),
        (None, Some((_, w, h))) if w > 0 && h > 0 => (w, h),
        _ => screen_size(&t, &screen).await?,
    };

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

    if s.camera.enabled {
        match resolve_device(&list.cameras, s.camera.device_id.as_deref()) {
            Some(id) => s.camera.device_id = Some(id),
            None => s.camera.enabled = false,
        }
    }
    let wanted_mic = s.audio.mic_device_id.is_some();
    if s.audio.mic_enabled {
        match resolve_device(&list.microphones, s.audio.mic_device_id.as_deref()) {
            Some(id) => s.audio.mic_device_id = Some(id),
            // macOS still has a "default" even when the listing came back empty.
            None if platform == Platform::MacOs => s.audio.mic_device_id = None,
            None => s.audio.mic_enabled = false,
        }
    }

    let helper = find_helper();
    // A chosen mic goes to the helper by name; none chosen follows the system.
    let chosen_mic = s
        .audio
        .mic_device_id
        .as_deref()
        .filter(|_| wanted_mic)
        .and_then(|id| list.microphones.iter().find(|d| d.id == id))
        .map(|d| d.name.clone());
    let camera_name = s
        .camera
        .device_id
        .as_deref()
        .and_then(|id| list.cameras.iter().find(|d| d.id == id))
        .map(|d| d.name.clone());
    let native = match (record_helper, native_display) {
        (Some(h), Some((display_id, _, _))) => Some(NativeCapture {
            helper: h,
            source: match &window {
                Some(w) => NativeSource::Window(w.id),
                None => NativeSource::Display(display_id),
            },
            crop: match &s.capture {
                CaptureTarget::Region { region, .. } => Some(*region),
                _ => None,
            },
            system_audio: s.audio.system_audio != SystemAudioSource::Off,
            mic: s.audio.mic_enabled.then_some(chosen_mic.clone()),
            camera: s.camera.enabled.then_some(camera_name),
            cursor: true,
            exclude_pid: std::process::id(),
            preview: None,
        }),
        _ => None,
    };
    let screen_n = list
        .screens
        .iter()
        .position(|d| d.id == screen.id)
        .map(|i| u32::try_from(i + 1).unwrap_or(u32::MAX));
    let target = match (&window, &s.capture) {
        (Some(w), _) => RecordTarget {
            kind: RecordTargetKind::Window,
            screen: None,
            app: Some(w.app.clone()),
            title: Some(w.title.clone()),
            icon: w.icon.clone(),
        },
        (None, CaptureTarget::Region { .. }) => RecordTarget {
            kind: RecordTargetKind::Region,
            screen: screen_n,
            app: None,
            title: None,
            icon: None,
        },
        (None, _) => RecordTarget {
            kind: RecordTargetKind::Screen,
            screen: screen_n,
            app: None,
            title: None,
            icon: None,
        },
    };
    let mic_helper = helper
        .clone()
        .filter(|_| native.is_none() && platform == Platform::MacOs && s.audio.mic_enabled)
        .map(|h| (h, chosen_mic));
    let system = match &native {
        // The plan needs an audio source for the budget; the helper records it itself.
        Some(n) if n.system_audio => Some(InputSource::PcmPipe {
            path: String::new(),
            sample_rate: 48_000,
            channels: 2,
        }),
        Some(_) => None,
        None => resolve_system_audio(&s, &list, helper.as_deref()),
    };
    let uses_helper = native.is_none() && matches!(system, Some(InputSource::PcmPipe { .. }));

    let out_dir = effective_output_dir(&s);
    tokio::fs::create_dir_all(&out_dir).await?;
    let plan_tmp = out_dir.join("segment.tmp");
    let mut plan = command::make_plan(&s, platform, sw, sh, encoder, &plan_tmp, system)?;
    let b = plan.budget.clone();
    plan.duration_limit_s = None;

    let (free, total) = crate::disk::space(&out_dir)?;
    let ds = recorder_core::disk::status(
        &out_dir.to_string_lossy(),
        free,
        total,
        &b,
        s.low_disk_warn_gb,
    );
    if recorder_core::disk::refuse_start(&ds) {
        return Err(CommandError::new(
            "error.noSpace",
            format!("{} MB free", free / 1_000_000),
        ));
    }

    let stem = filename::build_stem(
        &s.filename_template,
        meeting_name,
        fallback_name(&s),
        Local::now(),
    );
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
        mic_helper,
        native,
        target: Some(target),
    })
}

/// Countdown, then start. Used by the command, the tray and the shortcut.
pub async fn begin(app: &AppHandle, meeting_name: String) -> CmdResult<RecorderState> {
    let state = app.state::<AppState>();
    if Platform::current() == Platform::MacOs
        && permissions::check().screen != PermissionState::Granted
    {
        return Err(CommandError::new(
            "error.permission",
            "screen recording permission is not granted",
        ));
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
        // Neither the outline nor the countdown may end up in the recording. The bar comes back
        // once the recording is running. The native helper leaves this app's windows out of the
        // picture by itself (D27), but only windows that exist when its filter is built: the
        // floating window stays up so the bar never appears in the video.
        crate::windows::hide_marker(&handle);
        if find_record_helper().is_none() || Platform::current() != Platform::MacOs {
            crate::windows::hide_floating(&handle);
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        // A thumbnail still being taken must finish before the stream starts (see SNAPSHOT_TURN).
        let turn = SNAPSHOT_TURN.lock().await;
        let started = match build_params(&handle, &state, &meeting_name).await {
            Ok(p) => start_with_fallbacks(&handle, &state, p).await,
            Err(e) => Err(e),
        };
        drop(turn);
        if let Err(e) = started {
            state.recorder.abort_countdown().await;
            let _ = handle.emit(
                "recorder://error",
                json!({ "code": e.code, "detail": e.detail }),
            );
        }
    });
    Ok(st)
}

/// A camera that will not open or a silent audio helper must not cost the recording: try again
/// with less, and say what was dropped.
async fn start_with_fallbacks(
    app: &AppHandle,
    state: &AppState,
    full: StartParams,
) -> CmdResult<RecorderState> {
    let has_camera = full.plan.camera.is_some();
    // The native helper drops a failing camera by itself; only the ffmpeg path retries without the helper.
    let has_helper = full.native.is_none() && (full.helper.is_some() || full.mic_helper.is_some());
    let without = |camera: bool, helper: bool| {
        let mut q = full.clone();
        if camera {
            q.plan.camera = None;
            q.plan.pip = None;
            if let Some(n) = q.native.as_mut() {
                n.camera = None;
            }
        }
        if helper {
            q.plan.system_audio = None;
            q.helper = None;
            q.mic_helper = None;
        }
        q
    };
    let mut attempts = vec![(full.clone(), None)];
    if has_helper {
        attempts.push((without(false, true), Some("warn.systemAudioOff")));
    }
    if has_camera {
        attempts.push((without(true, false), Some("warn.cameraLost")));
    }
    if has_camera && has_helper {
        attempts.push((without(true, true), Some("warn.cameraLost")));
    }
    let mut last = None;
    for (params, dropped) in attempts {
        match state.recorder.start(params).await {
            Ok(st) => {
                if let Some(code) = dropped {
                    let _ = app.emit("recorder://warning", json!({ "code": code, "detail": "" }));
                }
                return Ok(st);
            }
            Err(e) if e.code == "error.start" => {
                log::warn!("start failed, trying with less: {}", e.detail);
                last = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| CommandError::new("error.start", "no attempt was made")))
}

pub async fn toggle_recording(app: &AppHandle) {
    let state = app.state::<AppState>();
    match state.recorder.phase().await {
        Phase::Idle => {
            let name = state.last_meeting.lock().expect("meeting lock").clone();
            if let Err(e) = begin(app, name).await {
                crate::windows::show_main(app);
                let _ = app.emit(
                    "recorder://error",
                    json!({ "code": e.code, "detail": e.detail }),
                );
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

/// The helper's `--list`, parsed to the windows a user can pick.
async fn window_list(helper: &Path) -> CmdResult<Vec<WindowInfo>> {
    let mut c = tokio::process::Command::new(helper);
    c.arg("--list")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(8), c.output())
        .await
        .map_err(|_| CommandError::new("error.start", "listing windows took too long"))?
        .map_err(|e| {
            CommandError::new(
                "error.start",
                format!("cannot run the recorder helper: {e}"),
            )
        })?;
    if !out.status.success() {
        return Err(CommandError::new(
            "error.start",
            format!("listing windows failed ({})", out.status),
        ));
    }
    Ok(native::parse_window_list(&String::from_utf8_lossy(
        &out.stdout,
    ))?)
}

/// The windows on screen that can be recorded on their own (macOS with the native helper;
/// empty elsewhere and while the screen permission is missing).
#[tauri::command]
pub async fn list_windows() -> CmdResult<Vec<WindowInfo>> {
    let Some(helper) = find_record_helper().filter(|_| Platform::current() == Platform::MacOs)
    else {
        return Ok(Vec::new());
    };
    if permissions::check().screen != PermissionState::Granted {
        return Ok(Vec::new());
    }
    window_list(&helper).await
}

/// A small JPEG of one window as a data URL, for the picker. `None` when it cannot be taken
/// (no helper, no permission, the window closed, macOS 13).
/// One snapshot at a time: several SCScreenshotManager clients at once leave the capture daemon
/// (replayd) reconnecting forever and none of them finishes. A recording start takes it too, so
/// no snapshot runs beside a starting SCStream (a thumbnail still loading when "Start" was
/// pressed stalled the stream and ended the recording after a second, 2026-11-05).
static SNAPSHOT_TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tauri::command]
pub async fn window_thumbnail(state: State<'_, AppState>, id: u32) -> CmdResult<Option<String>> {
    use base64::Engine;
    if state.recorder.phase().await != Phase::Idle {
        return Ok(None);
    }
    let _turn = SNAPSHOT_TURN.lock().await;
    let Some(helper) = find_record_helper().filter(|_| Platform::current() == Platform::MacOs)
    else {
        return Ok(None);
    };
    if permissions::check().screen != PermissionState::Granted {
        return Ok(None);
    }
    let file = std::env::temp_dir().join(format!(
        "flowgt-thumb-{}-{}.jpg",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let mut c = tokio::process::Command::new(&helper);
    c.args(["--snapshot", "--window", &id.to_string(), "--out"])
        .arg(&file)
        .args(["--max-width", "480"]);
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let ok = match tokio::time::timeout(Duration::from_secs(5), c.output()).await {
        Ok(Ok(out)) if out.status.success() => true,
        Ok(Ok(out)) => {
            log::warn!(
                "window thumbnail {id} failed: {} {}",
                String::from_utf8_lossy(&out.stdout).trim(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
            false
        }
        Ok(Err(e)) => {
            log::warn!("window thumbnail {id}: cannot run the helper: {e}");
            false
        }
        Err(_) => {
            log::warn!("window thumbnail {id}: the helper took more than 5 s");
            false
        }
    };
    let bytes = if ok {
        tokio::fs::read(&file).await.ok()
    } else {
        None
    };
    let _ = tokio::fs::remove_file(&file).await;
    Ok(bytes.filter(|b| !b.is_empty()).map(|b| {
        format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b)
        )
    }))
}

/// A small JPEG of one screen as a data URL, for the screen picker. `None` when the
/// screen-recording permission is missing (asking ffmpeg would only fail) or on other platforms.
#[tauri::command]
pub async fn screen_thumbnail(state: State<'_, AppState>, id: String) -> CmdResult<Option<String>> {
    use base64::Engine;
    if Platform::current() != Platform::MacOs
        || permissions::check().screen != PermissionState::Granted
    {
        return Ok(None);
    }
    let t = tools(&state).await?;
    let mut c = tokio::process::Command::new(&t.ffmpeg);
    c.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "avfoundation",
        "-framerate",
        "30",
        "-capture_cursor",
        "0",
        "-i",
    ])
    .arg(format!("{id}:none"))
    // The first frames from a screen input can be black; take a later one.
    .args([
        "-vf",
        "select=gte(n\\,5),scale=480:-2",
        "-frames:v",
        "1",
        "-c:v",
        "mjpeg",
        "-q:v",
        "5",
        "-f",
        "image2pipe",
        "-",
    ])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .kill_on_drop(true);
    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(5), c.output()).await else {
        return Ok(None);
    };
    if !out.status.success() || out.stdout.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&out.stdout)
    )))
}

/// Recordings on this computer and in Drive, merged by file name. A Drive failure must not hide
/// the local list, so it is reported alongside it.
#[tauri::command]
pub async fn list_recordings(app: AppHandle) -> CmdResult<RecordingHistory> {
    let s = load_settings(&app)?;
    let mut entries = local_recordings(&effective_output_dir(&s));
    let mut drive_error = None;
    if let (Some(account), Some(client)) = (
        crate::drive::load_account(&app),
        crate::drive::oauth_client(),
    ) {
        match crate::drive::list_recordings(
            &crate::drive::Endpoints::google(),
            Some(&client),
            &account.refresh_token,
            None,
        )
        .await
        {
            Ok(remote) => merge_recordings(&mut entries, remote),
            Err(e) => drive_error = Some(e),
        }
    }
    Ok(RecordingHistory {
        entries,
        drive_error,
    })
}

fn local_recordings(dir: &Path) -> Vec<RecordingEntry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    read.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !name.to_lowercase().ends_with(".mp4") {
                return None;
            }
            let meta = e.metadata().ok().filter(std::fs::Metadata::is_file)?;
            let modified_ms = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis() as u64;
            Some(RecordingEntry {
                name,
                path: Some(e.path().to_string_lossy().into_owned()),
                bytes: meta.len(),
                modified_ms: Some(modified_ms),
                ..Default::default()
            })
        })
        .collect()
}

fn merge_recordings(entries: &mut Vec<RecordingEntry>, remote: Vec<RecordingEntry>) {
    for r in remote {
        match entries.iter_mut().find(|e| e.name == r.name) {
            Some(e) => {
                e.drive_url = r.drive_url;
                e.drive_created = r.drive_created;
            }
            None => entries.push(r),
        }
    }
}

/// `async` on purpose: the first call creates the marker window, and on Windows a synchronous
/// command runs on the main thread, where building a webview window deadlocks the whole app.
#[tauri::command]
pub async fn show_screen_marker(app: AppHandle, index: usize) -> CmdResult<()> {
    crate::windows::show_marker(&app, index).map_err(|e| CommandError::io(e.to_string()))
}

#[tauri::command]
pub fn hide_screen_marker(app: AppHandle) {
    crate::windows::hide_marker(&app);
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

/// One frame of the live preview the native helper writes, as a data URL. `since` is the
/// `mtimeMs` of the frame the caller already has; the same frame again returns None.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFrame {
    pub data_url: String,
    pub mtime_ms: u64,
}

#[tauri::command]
pub async fn preview_frame(
    state: State<'_, AppState>,
    since: Option<u64>,
) -> CmdResult<Option<PreviewFrame>> {
    use base64::Engine;
    let Some(path) = state.recorder.preview_path().await else {
        return Ok(None);
    };
    let Ok(meta) = tokio::fs::metadata(&path).await else {
        return Ok(None);
    };
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    if since == Some(mtime_ms) {
        return Ok(None);
    }
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return Ok(None);
    };
    if bytes.len() < 4 {
        return Ok(None);
    }
    Ok(Some(PreviewFrame {
        data_url: format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ),
        mtime_ms,
    }))
}

/// The floating window as the full panel (bar + live preview) or the bar alone.
#[tauri::command]
pub async fn set_floating_expanded(app: AppHandle, expanded: bool) -> CmdResult<()> {
    crate::windows::set_floating_expanded(&app, expanded);
    Ok(())
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

// ───────────────────────────── Google Drive ─────────────────────────────

fn drive_status_now(app: &AppHandle) -> DriveStatus {
    DriveStatus {
        configured: crate::drive::oauth_client().is_some(),
        email: crate::drive::load_account(app).map(|a| a.email),
    }
}

#[tauri::command]
pub async fn drive_status(app: AppHandle) -> CmdResult<DriveStatus> {
    Ok(drive_status_now(&app))
}

#[tauri::command]
pub async fn drive_sign_in(app: AppHandle) -> CmdResult<DriveStatus> {
    let client = crate::drive::oauth_client().ok_or_else(|| {
        CommandError::new(
            "error.driveNotConfigured",
            "this build has no Google OAuth client",
        )
    })?;
    let account = crate::drive::sign_in(&crate::drive::Endpoints::google(), &client, |url| {
        if let Err(e) = tauri_plugin_opener::open_url(url, None::<&str>) {
            log::warn!("cannot open the browser: {e}");
        }
    })
    .await?;
    crate::drive::save_account(&app, &account)?;
    crate::windows::show_main(&app);
    Ok(drive_status_now(&app))
}

#[tauri::command]
pub async fn drive_sign_out(app: AppHandle) -> CmdResult<DriveStatus> {
    crate::drive::forget_account(&app);
    Ok(drive_status_now(&app))
}

#[cfg(test)]
mod save_tests {
    use super::*;
    use crate::drive::Uploaded;

    #[test]
    fn history_merges_by_name_and_skips_everything_but_recordings() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.mp4"), b"12345").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.path().join(".hidden.mp4"), b"x").unwrap();
        std::fs::create_dir(dir.path().join("sub.mp4")).unwrap();
        let mut e = local_recordings(dir.path());
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].bytes, 5);
        let cloud = |name: &str| RecordingEntry {
            name: name.into(),
            bytes: 9,
            drive_url: Some("u".into()),
            ..Default::default()
        };
        merge_recordings(&mut e, vec![cloud("a.mp4"), cloud("b.mp4")]);
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].bytes, e[0].drive_url.as_deref()), (5, Some("u")));
        assert!(e[0].path.is_some() && e[1].path.is_none());
    }

    #[test]
    fn local_copy_goes_only_when_drive_confirms_its_size() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("rec.mp4");
        std::fs::write(&p, vec![7u8; 10]).unwrap();
        let up = |size| Uploaded {
            link: String::new(),
            size,
        };
        assert!(!remove_if_confirmed(&p, &up(Some(9))));
        assert!(!remove_if_confirmed(&p, &up(None)));
        assert!(p.exists());
        assert!(remove_if_confirmed(&p, &up(Some(10))));
        assert!(!p.exists());
    }
}

#[tauri::command]
pub async fn drive_upload(app: AppHandle, path: String) -> CmdResult<()> {
    if crate::drive::load_account(&app).is_none() {
        return Err(CommandError::new(
            "error.driveSignedOut",
            "not signed in to Google",
        ));
    }
    // A retry or a manual upload follows the same rule as the automatic one.
    let s = load_settings(&app)?;
    let plan = recorder_core::settings::save_plan(s.save_local, s.save_drive, true);
    spawn_upload(app, path, plan.remove_local_after_upload);
    Ok(())
}

/// Only Drive links: the webview must not be able to open arbitrary URLs through this.
#[tauri::command]
pub async fn open_drive_link(url: String) -> CmdResult<()> {
    if !url.starts_with("https://drive.google.com/") {
        return Err(CommandError::new(
            "error.invalidArgument",
            "not a Google Drive link",
        ));
    }
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| CommandError::io(e.to_string()))
}

fn spawn_upload(app: AppHandle, path: String, remove_local: bool) {
    tauri::async_runtime::spawn(async move {
        let send = |percent: f64,
                    done: bool,
                    url: Option<String>,
                    error: Option<String>,
                    local_removed: bool| {
            let _ = app.emit(
                "drive://progress",
                DriveProgress {
                    path: path.clone(),
                    percent,
                    done,
                    url,
                    error,
                    local_removed,
                },
            );
        };
        let emit = |percent: f64, done: bool, url: Option<String>, error: Option<String>| {
            send(percent, done, url, error, false)
        };
        emit(0.0, false, None, None);
        let outcome = async {
            let client = crate::drive::oauth_client()
                .ok_or_else(|| "this build has no Google OAuth client".to_string())?;
            let account =
                crate::drive::load_account(&app).ok_or_else(|| "not signed in".to_string())?;
            crate::drive::upload_recording(
                &crate::drive::Endpoints::google(),
                Some(&client),
                &account.refresh_token,
                None,
                Path::new(&path),
                |pct| emit(pct, false, None, None),
            )
            .await
        }
        .await;
        match outcome {
            Ok(up) => {
                let removed = remove_local && remove_if_confirmed(Path::new(&path), &up);
                send(100.0, true, Some(up.link), None, removed);
            }
            Err(e) => emit(0.0, true, None, Some(e)),
        }
    });
}

/// Remove the local copy only when Drive reports exactly the bytes on disk. Anything else keeps it.
fn remove_if_confirmed(path: &Path, up: &crate::drive::Uploaded) -> bool {
    match std::fs::metadata(path) {
        Ok(m) if up.confirms(m.len()) => std::fs::remove_file(path).is_ok(),
        _ => false,
    }
}

// ───────────────────────────── compress ─────────────────────────────

#[tauri::command]
pub async fn compress_to_budget(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    target_mb: f64,
) -> CmdResult<String> {
    let t = tools(&state).await?;
    let s = load_settings(&app)?;
    let encoder = resolve_encoder(&state, &t, s.video.encoder_preference).await?;
    let input = PathBuf::from(&path);
    let info = sidecar::probe(&t, &input).await?;
    let dir = input
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = input
        .file_stem()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_else(|| "recording".into());
    let output = filename::unique_path(&dir, &format!("{stem} (compressed)"), "mp4", &|p| {
        p.exists()
    });
    let cmd = command::compress_command(
        &t.ffmpeg_str(),
        &input,
        &output,
        info.duration_ms,
        target_mb,
        &encoder,
        s.audio.bitrate_kbps,
    );
    let job_id = uuid::Uuid::new_v4().to_string();
    let id = job_id.clone();
    let duration = info.duration_ms.max(1);
    tauri::async_runtime::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut c = tokio::process::Command::new(&cmd.program);
        c.args(&cmd.args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
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
    let Ok(mut rd) = tokio::fs::read_dir(work_root(&s)).await else {
        return Ok(out);
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let mp = recovery::manifest_path(&entry.path());
        let Ok(text) = tokio::fs::read_to_string(&mp).await else {
            continue;
        };
        let Ok(m) = recovery::manifest_from_json(&text) else {
            continue;
        };
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
pub async fn list_recoverable(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Vec<RecoverableSession>> {
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
pub async fn recover_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> CmdResult<RecordingResult> {
    let t = tools(&state).await?;
    let s = load_settings(&app)?;
    let m = scan(&app, &state)
        .await?
        .into_iter()
        .find(|m| m.session_id == session_id)
        .ok_or_else(|| CommandError::new("error.recovery", "no such session"))?;
    let out_dir = m
        .final_output
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| effective_output_dir(&s));
    let stem = m
        .final_output
        .file_stem()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_else(|| "recovered".into());
    let mut m2 = m.clone();
    m2.final_output =
        filename::unique_path(&out_dir, &format!("{stem} (recovered)"), "mp4", &|p| {
            p.exists()
        });
    let res = crate::recorder::assemble(
        &m2,
        &t,
        s.video.target_mb_per_hour,
        m2.plan.budget.clone(),
        true,
    )
    .await?;
    let _ = tokio::fs::remove_dir_all(&m.work_dir).await;
    Ok(res)
}

#[tauri::command]
pub async fn discard_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> CmdResult<()> {
    if let Some(m) = scan(&app, &state)
        .await?
        .into_iter()
        .find(|m| m.session_id == session_id)
    {
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
        list_windows,
        window_thumbnail,
        preview_frame,
        set_floating_expanded,
        relaunch_app,
        list_recordings,
        show_screen_marker,
        hide_screen_marker,
        drive_status,
        drive_sign_in,
        drive_sign_out,
        drive_upload,
        open_drive_link,
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
        let d = Device {
            id: "5".into(),
            name: "BlackHole 2ch".into(),
            kind: DeviceKind::Microphone,
            is_default: false,
            width: None,
            height: None,
        };
        DeviceList {
            loopbacks: vec![d],
            ..Default::default()
        }
    }

    #[test]
    fn system_audio_choices() {
        let mut s = settings::default_settings(Platform::current());
        s.audio.system_audio = SystemAudioSource::Off;
        assert!(resolve_system_audio(&s, &loopbacks(), None).is_none());
        s.audio.system_audio = SystemAudioSource::Auto;
        assert!(
            matches!(
                resolve_system_audio(&s, &loopbacks(), Some(Path::new("x"))),
                Some(InputSource::PcmPipe { .. })
            ),
            "the helper wins"
        );
        // No helper: falls back to the loopback device, or nothing when there is none.
        assert!(resolve_system_audio(&s, &loopbacks(), None).is_some());
        assert!(resolve_system_audio(&s, &DeviceList::default(), None).is_none());
        s.audio.system_audio = SystemAudioSource::Device("7".into());
        assert!(resolve_system_audio(&s, &DeviceList::default(), None).is_some());
        s.audio.system_audio = SystemAudioSource::Helper;
        assert!(resolve_system_audio(&s, &DeviceList::default(), None).is_none());
        assert!(matches!(
            resolve_system_audio(&s, &DeviceList::default(), Some(Path::new("x"))),
            Some(InputSource::PcmPipe { .. })
        ));
    }

    fn dev(id: &str, name: &str, is_default: bool) -> Device {
        Device {
            id: id.into(),
            name: name.into(),
            kind: DeviceKind::Camera,
            is_default,
            width: None,
            height: None,
        }
    }

    #[test]
    fn device_choice_follows_the_system_unless_one_was_picked() {
        let list = vec![
            dev("0", "Virtual", false),
            dev("1", "FaceTime", true),
            dev("2", "Logi", false),
        ];
        assert_eq!(
            resolve_device(&list, None),
            Some("1".into()),
            "system default, not the first entry"
        );
        assert_eq!(
            resolve_device(&list, Some("Logi")),
            Some("2".into()),
            "chosen by name"
        );
        assert_eq!(
            resolve_device(&list, Some("2")),
            Some("2".into()),
            "an old stored id still works"
        );
        assert_eq!(
            resolve_device(&list, Some("Unplugged")),
            Some("1".into()),
            "a missing device falls back to the default"
        );
        assert_eq!(
            resolve_device(&[dev("0", "Only", false)], None),
            Some("0".into())
        );
        assert_eq!(resolve_device(&[], None), None);
    }

    #[test]
    fn system_default_is_marked_by_name() {
        let mut list = vec![dev("0", "A", true), dev("1", "B", false)];
        mark_system_default(&mut list, Some("B".into()));
        assert!(!list[0].is_default && list[1].is_default);
        mark_system_default(&mut list, Some("Gone".into()));
        assert!(list[1].is_default, "an unknown name changes nothing");
        mark_system_default(&mut list, None);
        assert!(list[1].is_default);
    }

    #[test]
    fn permission_kinds_reject_junk() {
        assert!(kind_of("screen").is_ok());
        assert!(kind_of("gps").is_err());
    }
}
