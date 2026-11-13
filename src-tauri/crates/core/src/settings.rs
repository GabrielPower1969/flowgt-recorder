//! Persisted Settings: defaults, validation, migration.
//!
//! Loading is lenient on shape and strict on values: missing or unknown keys never fail and old
//! versions are migrated, but [`validate`] rejects anything the recorder could not honour.
use crate::filename::ILLEGAL_FILENAME_CHARS;
use crate::types::*;
use crate::CoreError;
use serde_json::{Map, Value};
use std::ops::RangeInclusive;

pub const ALLOWED_FPS: [u32; 3] = [24, 30, 60];
pub const ALLOWED_AUDIO_KBPS: [u32; 3] = [64, 96, 128];
pub const MAX_WIDTH_RANGE: RangeInclusive<u32> = 640..=7680;
pub const TARGET_MB_RANGE: RangeInclusive<u32> = 100..=2000;
pub const PIP_SIZE_PCT_RANGE: RangeInclusive<u32> = 10..=50;
pub const MAX_PIP_MARGIN: u32 = 400;
pub const COUNTDOWN_RANGE: RangeInclusive<u32> = 0..=10;
/// A template must contain one of these, or every recording would get the same name.
pub const UNIQUE_TEMPLATE_TOKENS: [&str; 4] = ["{name}", "{date}", "{datetime}", "{time}"];

/// Defaults for a platform (output dir left empty = platform default resolved by the app).
pub fn default_settings(platform: Platform) -> Settings {
    // The defaults are the same everywhere except the two recording shortcuts on Windows (below).
    let (toggle, pause) = default_record_shortcuts(platform);
    Settings {
        version: SETTINGS_VERSION,
        language: Language::Auto,
        theme: Theme::System,
        output_dir: String::new(),
        filename_template: DEFAULT_FILENAME_TEMPLATE.to_string(),
        video: VideoSettings {
            fps: 30,
            max_width: 1920,
            target_mb_per_hour: DEFAULT_TARGET_MB_PER_HOUR,
            encoder_preference: EncoderPreference::Auto,
        },
        audio: AudioSettings {
            mic_device_id: None,
            mic_enabled: true,
            system_audio: SystemAudioSource::Auto,
            tracks: AudioTracks::Mixed,
            bitrate_kbps: 96,
        },
        camera: CameraSettings {
            enabled: true,
            device_id: None,
            corner: Corner::BottomRight,
            margin: 24,
            size_pct: 20,
            shape: PipShape::Rounded,
            mirror: false,
            offset_x: 0,
            offset_y: 0,
        },
        capture: CaptureTarget::Display { display_id: "0".to_string() },
        shortcuts: ShortcutSettings {
            toggle: toggle.to_string(),
            pause: pause.to_string(),
            scroll_capture: "Alt+Shift+S".to_string(),
            show_window: String::new(),
        },
        countdown_seconds: 3,
        low_disk_warn_gb: 2,
        onboarding_done: false,
        save_local: true,
        save_drive: true,
        start_hidden_on_login: true,
    }
}

/// What to do with a finished recording. It is always written locally first, so a failed or
/// interrupted upload can never lose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavePlan {
    pub upload: bool,
    /// Remove the local copy, but only after Drive has confirmed every byte.
    pub remove_local_after_upload: bool,
}

/// Signed out means the local file is the only copy, whatever the checkboxes say.
pub fn save_plan(save_local: bool, save_drive: bool, signed_in: bool) -> SavePlan {
    let upload = save_drive && signed_in;
    SavePlan { upload, remove_local_after_upload: upload && !save_local }
}

/// Validate ranges (fps ∈ {24,30,60}, size_pct 10–50, template renders, …).
///
/// Every problem is reported, joined with "; ", each prefixed with the camelCase field path the
/// frontend uses (`video.fps: …`), so the UI can highlight the fields.
pub fn validate(settings: &Settings) -> crate::Result<()> {
    let mut problems: Vec<String> = Vec::new();
    let s = settings;

    if s.version != SETTINGS_VERSION {
        problems.push(format!("version: expected {SETTINGS_VERSION}, got {}", s.version));
    }
    if !ALLOWED_FPS.contains(&s.video.fps) {
        problems.push(format!("video.fps: must be one of 24, 30 or 60, got {}", s.video.fps));
    }
    if s.video.max_width == 0 {
        // No cap: the screen is recorded at its own size.
    } else if !MAX_WIDTH_RANGE.contains(&s.video.max_width) {
        problems.push(format!(
            "video.maxWidth: must be 0 (the screen's own size) or between {} and {}, got {}",
            MAX_WIDTH_RANGE.start(),
            MAX_WIDTH_RANGE.end(),
            s.video.max_width
        ));
    } else if !s.video.max_width.is_multiple_of(2) {
        problems.push(format!("video.maxWidth: must be even, got {}", s.video.max_width));
    }
    if !TARGET_MB_RANGE.contains(&s.video.target_mb_per_hour) {
        problems.push(format!(
            "video.targetMbPerHour: must be between {} and {}, got {}",
            TARGET_MB_RANGE.start(),
            TARGET_MB_RANGE.end(),
            s.video.target_mb_per_hour
        ));
    }
    if !ALLOWED_AUDIO_KBPS.contains(&s.audio.bitrate_kbps) {
        problems.push(format!("audio.bitrateKbps: must be one of 64, 96 or 128, got {}", s.audio.bitrate_kbps));
    }
    if !PIP_SIZE_PCT_RANGE.contains(&s.camera.size_pct) {
        problems.push(format!(
            "camera.sizePct: must be between {} and {}, got {}",
            PIP_SIZE_PCT_RANGE.start(),
            PIP_SIZE_PCT_RANGE.end(),
            s.camera.size_pct
        ));
    }
    if s.camera.margin > MAX_PIP_MARGIN {
        problems.push(format!("camera.margin: must be at most {MAX_PIP_MARGIN}, got {}", s.camera.margin));
    }
    if !COUNTDOWN_RANGE.contains(&s.countdown_seconds) {
        problems.push(format!(
            "countdownSeconds: must be between {} and {}, got {}",
            COUNTDOWN_RANGE.start(),
            COUNTDOWN_RANGE.end(),
            s.countdown_seconds
        ));
    }

    if !s.save_local && !s.save_drive {
        problems.push("saveLocal: choose at least one place to save to".to_string());
    }

    let template = &s.filename_template;
    if template.trim().is_empty() {
        problems.push("filenameTemplate: must not be empty".to_string());
    } else {
        if !UNIQUE_TEMPLATE_TOKENS.iter().any(|token| template.contains(token)) {
            problems.push(
                "filenameTemplate: must contain {name} or a time token ({date}, {datetime} or {time}), \
                 otherwise every recording gets the same name"
                    .to_string(),
            );
        }
        if let Some(c) = template.chars().find(|c| ILLEGAL_FILENAME_CHARS.contains(c) || c.is_control()) {
            problems.push(format!("filenameTemplate: must not contain {c:?}"));
        }
    }

    let toggle = s.shortcuts.toggle.trim();
    let pause = s.shortcuts.pause.trim();
    if toggle.is_empty() {
        problems.push("shortcuts.toggle: must not be empty".to_string());
    }
    if pause.is_empty() {
        problems.push("shortcuts.pause: must not be empty".to_string());
    }
    if !toggle.is_empty() && toggle.eq_ignore_ascii_case(pause) {
        problems.push("shortcuts: toggle and pause must differ".to_string());
    }
    // The two optional shortcuts may be empty; a set one must not repeat another.
    let all = [
        ("toggle", toggle),
        ("pause", pause),
        ("scrollCapture", s.shortcuts.scroll_capture.trim()),
        ("showWindow", s.shortcuts.show_window.trim()),
    ];
    for (i, (name, accel)) in all.iter().enumerate().skip(2) {
        if accel.is_empty() {
            continue;
        }
        if let Some((other, _)) = all[..i].iter().find(|(_, a)| a.eq_ignore_ascii_case(accel)) {
            problems.push(format!("shortcuts: {name} and {other} must differ"));
        }
    }

    if !s.output_dir.is_empty() && !looks_absolute(&s.output_dir) {
        problems.push(format!(
            "outputDir: must be an absolute path (or empty for the default folder), got {:?}",
            s.output_dir
        ));
    }

    match &s.capture {
        CaptureTarget::Display { display_id } => {
            if display_id.trim().is_empty() {
                problems.push("capture.displayId: must not be empty".to_string());
            }
        }
        CaptureTarget::Region { display_id, region } => {
            if display_id.trim().is_empty() {
                problems.push("capture.displayId: must not be empty".to_string());
            }
            if region.w < 2 || region.h < 2 {
                problems.push(format!(
                    "capture.region: width and height must be at least 2 pixels, got {}×{}",
                    region.w, region.h
                ));
            }
        }
        CaptureTarget::Window { window_id } => {
            if window_id.trim().is_empty() {
                problems.push("capture.windowId: must not be empty".to_string());
            }
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(CoreError::InvalidSettings(problems.join("; ")))
    }
}

/// Absolute on either platform, judged textually so tests run anywhere: `/…`, `~…`,
/// `C:\…`, `C:/…` or a UNC `\\server\…`.
fn looks_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with('~')
        || path.starts_with("\\\\")
        || (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/'))
}

/// Start/stop and pause. On Windows, Ctrl+Shift+R and Ctrl+Shift+P are Edge's "reload without the
/// cache" and "print", and VS Code's command palette; a global shortcut takes them from those
/// programs without either side noticing. Windows uses Alt+Shift instead (Gabriel, 2026-11-13).
pub fn default_record_shortcuts(platform: Platform) -> (&'static str, &'static str) {
    match platform {
        Platform::Windows => ("Alt+Shift+R", "Alt+Shift+P"),
        _ => ("CmdOrCtrl+Shift+R", "CmdOrCtrl+Shift+P"),
    }
}

/// Load from JSON text: unknown/missing fields get defaults, `version` migrated to SETTINGS_VERSION.
///
/// Steps: parse → [`migrate`] the raw document → deep-merge over [`default_settings`] → pin the
/// version → deserialise → [`validate`]. Only broken JSON, a wrong-typed field or an invalid value
/// fails, always as [`CoreError::InvalidSettings`].
pub fn from_json(json: &str, platform: Platform) -> crate::Result<Settings> {
    let mut raw: Value = serde_json::from_str(json)
        .map_err(|e| CoreError::InvalidSettings(format!("settings are not valid JSON: {e}")))?;
    if !raw.is_object() {
        return Err(CoreError::InvalidSettings(format!(
            "settings must be a JSON object, found {}",
            json_kind(&raw)
        )));
    }
    let from_version = raw
        .get("version")
        .and_then(Value::as_u64)
        .map_or(0, |v| u32::try_from(v).unwrap_or(u32::MAX));
    if from_version < SETTINGS_VERSION {
        migrate(&mut raw, from_version);
    }

    let mut merged = serde_json::to_value(default_settings(platform))?;
    deep_merge(&mut merged, raw);
    if let Some(fields) = merged.as_object_mut() {
        // Files from a newer build are read as far as we understand them and re-stamped.
        fields.insert("version".to_string(), Value::from(SETTINGS_VERSION));
    }

    let mut settings: Settings = serde_json::from_value(merged)
        .map_err(|e| CoreError::InvalidSettings(format!("settings field has the wrong type: {e}")))?;
    // A Windows file that still holds the old shared defaults gets the Windows ones. A user who
    // chose exactly those keys by hand is moved too; no released build had the old defaults.
    if platform == Platform::Windows {
        let (toggle, pause) = default_record_shortcuts(platform);
        if settings.shortcuts.toggle == "CmdOrCtrl+Shift+R" {
            settings.shortcuts.toggle = toggle.to_string();
        }
        if settings.shortcuts.pause == "CmdOrCtrl+Shift+P" {
            settings.shortcuts.pause = pause.to_string();
        }
    }
    validate(&settings)?;
    Ok(settings)
}

pub fn to_json(settings: &Settings) -> crate::Result<String> {
    Ok(serde_json::to_string_pretty(settings)?)
}

/// Upgrade a raw settings document written by schema `from_version` to [`SETTINGS_VERSION`],
/// in place. One step per schema bump, oldest first; each step only rewrites the keys that
/// changed in that bump (missing keys are filled from the defaults afterwards).
pub fn migrate(value: &mut Value, from_version: u32) {
    let Some(fields) = value.as_object_mut() else {
        return;
    };
    let mut version = from_version;
    while version < SETTINGS_VERSION {
        #[allow(clippy::single_match)] // grows a new arm with every schema bump
        match version {
            // 0 → 1: the first persisted schema. Files written before the version key existed
            // already have the v1 layout, so nothing is renamed. Later bumps go here as
            // `1 => rename_xyz(fields)`.
            0 => {}
            // 1 → 2: new keys only (shortcuts.scrollCapture, shortcuts.showWindow,
            // startHiddenOnLogin); the defaults fill them. video.maxWidth gained the value 0.
            1 => {}
            _ => {}
        }
        version += 1;
    }
    fields.insert("version".to_string(), Value::from(SETTINGS_VERSION));
}

/// Overlay `overlay` onto `base`: objects merge key by key, everything else replaces. `null`
/// means "not set" and keeps the default. Two tagged objects (`{"kind": …}`) of different kinds
/// do not merge, because the old variant's fields would be nonsense for the new one.
fn deep_merge(base: &mut Value, overlay: Value) {
    match overlay {
        Value::Null => {}
        Value::Object(fields) => match base {
            Value::Object(existing) if same_kind(existing, &fields) => {
                for (key, value) in fields {
                    match existing.get_mut(&key) {
                        Some(slot) => deep_merge(slot, value),
                        None => {
                            existing.insert(key, value);
                        }
                    }
                }
            }
            _ => *base = Value::Object(fields),
        },
        other => *base = other,
    }
}

fn same_kind(base: &Map<String, Value>, overlay: &Map<String, Value>) -> bool {
    match (base.get("kind"), overlay.get("kind")) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn defaults() -> Settings {
        default_settings(Platform::MacOs)
    }

    fn error_text(result: crate::Result<Settings>) -> String {
        match result {
            Err(CoreError::InvalidSettings(msg)) => msg,
            other => panic!("expected InvalidSettings, got {other:?}"),
        }
    }

    fn validation_error(settings: &Settings) -> String {
        match validate(settings) {
            Err(CoreError::InvalidSettings(msg)) => msg,
            other => panic!("expected InvalidSettings, got {other:?}"),
        }
    }

    #[test]
    fn windows_records_with_alt_shift_and_old_files_follow() {
        let w = default_settings(Platform::Windows);
        assert_eq!((w.shortcuts.toggle.as_str(), w.shortcuts.pause.as_str()), ("Alt+Shift+R", "Alt+Shift+P"));
        let m = default_settings(Platform::MacOs);
        assert_eq!(m.shortcuts.toggle, "CmdOrCtrl+Shift+R");
        // A Windows file with the old defaults moves; a chosen key stays.
        let old = r#"{"version":2,"shortcuts":{"toggle":"CmdOrCtrl+Shift+R","pause":"Ctrl+Alt+9"}}"#;
        let s = from_json(old, Platform::Windows).unwrap();
        assert_eq!(s.shortcuts.toggle, "Alt+Shift+R");
        assert_eq!(s.shortcuts.pause, "Ctrl+Alt+9");
        // The same file on a Mac is left alone.
        let s = from_json(old, Platform::MacOs).unwrap();
        assert_eq!(s.shortcuts.toggle, "CmdOrCtrl+Shift+R");
    }

    #[test]
    fn defaults_match_the_spec() {
        let s = defaults();
        assert_eq!(s.version, SETTINGS_VERSION);
        assert_eq!(s.language, Language::Auto);
        assert_eq!(s.theme, Theme::System);
        assert_eq!(s.output_dir, "");
        assert_eq!(s.filename_template, "{date} {time} {name}");
        assert_eq!(s.video.fps, 30);
        assert_eq!(s.video.max_width, 1920);
        assert_eq!(s.video.target_mb_per_hour, 400);
        assert_eq!(s.video.encoder_preference, EncoderPreference::Auto);
        assert_eq!(s.audio.mic_enabled, true);
        assert_eq!(s.audio.mic_device_id, None);
        assert_eq!(s.audio.system_audio, SystemAudioSource::Auto);
        assert_eq!(s.audio.tracks, AudioTracks::Mixed);
        assert_eq!(s.audio.bitrate_kbps, 96);
        assert_eq!(s.camera.enabled, true);
        assert_eq!(s.camera.device_id, None);
        assert_eq!(s.camera.corner, Corner::BottomRight);
        assert_eq!(s.camera.margin, 24);
        assert_eq!(s.camera.size_pct, 20);
        assert_eq!(s.camera.shape, PipShape::Rounded);
        assert_eq!(s.camera.mirror, false);
        assert_eq!((s.camera.offset_x, s.camera.offset_y), (0, 0));
        assert_eq!(s.capture, CaptureTarget::Display { display_id: "0".to_string() });
        assert_eq!(s.shortcuts.toggle, "CmdOrCtrl+Shift+R");
        assert_eq!(s.shortcuts.pause, "CmdOrCtrl+Shift+P");
        assert_eq!(s.countdown_seconds, 3);
        assert_eq!(s.low_disk_warn_gb, 2);
        assert_eq!(s.onboarding_done, false);
    }

    #[test]
    fn defaults_are_valid_on_every_platform() {
        for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
            validate(&default_settings(platform)).unwrap();
        }
    }

    #[test]
    fn json_round_trip_keeps_every_field() {
        let mut s = defaults();
        s.language = Language::Zh;
        s.theme = Theme::Dark;
        s.output_dir = "/Users/g/Movies/FlowGT".to_string();
        s.filename_template = "{datetime}_{name}".to_string();
        s.video.fps = 60;
        s.video.max_width = 2560;
        s.video.target_mb_per_hour = 800;
        s.video.encoder_preference = EncoderPreference::H264;
        s.audio.mic_device_id = Some("1".to_string());
        s.audio.system_audio = SystemAudioSource::Device("BlackHole 2ch".to_string());
        s.audio.tracks = AudioTracks::Separate;
        s.audio.bitrate_kbps = 128;
        s.camera.corner = Corner::TopLeft;
        s.camera.shape = PipShape::Circle;
        s.camera.mirror = true;
        s.camera.offset_x = -12;
        s.capture = CaptureTarget::Region {
            display_id: "1".to_string(),
            region: Region { x: 10, y: 20, w: 1280, h: 720 },
        };
        s.countdown_seconds = 0;
        s.onboarding_done = true;

        let json = to_json(&s).unwrap();
        assert!(json.contains("\"filenameTemplate\""), "camelCase on the wire");
        assert!(json.contains("\"maxWidth\": 2560"), "pretty-printed");
        assert!(json.contains("\"kind\": \"region\""));
        let back = from_json(&json, Platform::MacOs).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn empty_object_gives_the_defaults() {
        assert_eq!(from_json("{}", Platform::Windows).unwrap(), default_settings(Platform::Windows));
    }

    #[test]
    fn missing_fields_are_filled_from_defaults() {
        let s = from_json(r#"{"video": {"fps": 60}, "countdownSeconds": 5}"#, Platform::MacOs).unwrap();
        assert_eq!(s.video.fps, 60);
        assert_eq!(s.video.max_width, 1920, "sibling inside the same object kept its default");
        assert_eq!(s.video.target_mb_per_hour, 400);
        assert_eq!(s.countdown_seconds, 5);
        assert_eq!(s.audio, defaults().audio);
        assert_eq!(s.camera, defaults().camera);
        assert_eq!(s.version, SETTINGS_VERSION);
    }

    #[test]
    fn unknown_fields_are_ignored_at_every_level() {
        let json = json!({
            "futureTopLevel": true,
            "video": {"fps": 24, "futureNested": "x"},
            "camera": {"deep": {"deeper": [1, 2, 3]}},
            "shortcuts": {"toggle": "Alt+R", "stop": "Alt+S"}
        })
        .to_string();
        let s = from_json(&json, Platform::MacOs).unwrap();
        assert_eq!(s.video.fps, 24);
        assert_eq!(s.shortcuts.toggle, "Alt+R");
        assert_eq!(s.shortcuts.pause, "CmdOrCtrl+Shift+P");
    }

    #[test]
    fn null_means_not_set() {
        let s = from_json(r#"{"video": {"fps": null}, "audio": null, "camera": {"deviceId": null}}"#, Platform::MacOs).unwrap();
        assert_eq!(s.video.fps, 30);
        assert_eq!(s.audio, defaults().audio);
        assert_eq!(s.camera.device_id, None);
    }

    #[test]
    fn tagged_enums_of_a_different_kind_replace_instead_of_merging() {
        let json = json!({"capture": {"kind": "region", "displayId": "2", "region": {"x": 0, "y": 0, "w": 800, "h": 600}}}).to_string();
        let s = from_json(&json, Platform::MacOs).unwrap();
        assert_eq!(
            s.capture,
            CaptureTarget::Region { display_id: "2".to_string(), region: Region { x: 0, y: 0, w: 800, h: 600 } }
        );

        let json = json!({"capture": {"kind": "window", "windowId": "w1"}}).to_string();
        let s = from_json(&json, Platform::MacOs).unwrap();
        assert_eq!(s.capture, CaptureTarget::Window { window_id: "w1".to_string() });

        let json = json!({"audio": {"systemAudio": {"kind": "device", "id": "BlackHole 2ch"}}}).to_string();
        let s = from_json(&json, Platform::MacOs).unwrap();
        assert_eq!(s.audio.system_audio, SystemAudioSource::Device("BlackHole 2ch".to_string()));

        let json = json!({"audio": {"systemAudio": {"kind": "off"}}}).to_string();
        let s = from_json(&json, Platform::MacOs).unwrap();
        assert_eq!(s.audio.system_audio, SystemAudioSource::Off);

        // Same kind, missing detail: the default detail fills in.
        let json = json!({"capture": {"kind": "display"}}).to_string();
        let s = from_json(&json, Platform::MacOs).unwrap();
        assert_eq!(s.capture, CaptureTarget::Display { display_id: "0".to_string() });
    }

    #[test]
    fn bad_fps_is_rejected() {
        let msg = error_text(from_json(r#"{"video": {"fps": 25}}"#, Platform::MacOs));
        assert!(msg.contains("video.fps"), "{msg}");
        assert!(msg.contains("25"), "{msg}");
    }

    #[test]
    fn wrong_type_is_rejected_with_context() {
        let msg = error_text(from_json(r#"{"video": {"fps": "30"}}"#, Platform::MacOs));
        assert!(msg.contains("wrong type"), "{msg}");
        let msg = error_text(from_json(r#"{"video": 5}"#, Platform::MacOs));
        assert!(msg.contains("wrong type"), "{msg}");
    }

    #[test]
    fn save_plan_never_removes_the_only_copy() {
        let p = |local, drive, signed_in| save_plan(local, drive, signed_in);
        assert_eq!(p(true, true, true), SavePlan { upload: true, remove_local_after_upload: false });
        assert_eq!(p(true, false, true), SavePlan { upload: false, remove_local_after_upload: false });
        assert_eq!(p(false, true, true), SavePlan { upload: true, remove_local_after_upload: true });
        // Signed out: nothing can be uploaded, so nothing may be removed.
        assert_eq!(p(false, true, false), SavePlan { upload: false, remove_local_after_upload: false });
        assert_eq!(p(true, true, false), SavePlan { upload: false, remove_local_after_upload: false });
    }

    #[test]
    fn at_least_one_save_target_is_required_and_old_files_get_both() {
        let mut s = from_json(r#"{"version": 0}"#, Platform::MacOs).unwrap();
        assert!(s.save_local && s.save_drive);
        s.save_local = false;
        assert!(validate(&s).is_ok());
        s.save_drive = false;
        assert!(validate(&s).unwrap_err().to_string().contains("saveLocal"));
    }

    #[test]
    fn old_version_is_migrated_and_values_kept() {
        let s = from_json(r#"{"version": 0, "video": {"fps": 60}, "onboardingDone": true}"#, Platform::MacOs).unwrap();
        assert_eq!(s.version, SETTINGS_VERSION);
        assert_eq!(s.video.fps, 60);
        assert!(s.onboarding_done);
        // No version key at all is treated as the oldest schema.
        let s = from_json(r#"{"theme": "dark"}"#, Platform::MacOs).unwrap();
        assert_eq!(s.version, SETTINGS_VERSION);
        assert_eq!(s.theme, Theme::Dark);
    }

    #[test]
    fn newer_version_is_read_and_restamped() {
        let s = from_json(r#"{"version": 99, "video": {"fps": 24}}"#, Platform::MacOs).unwrap();
        assert_eq!(s.version, SETTINGS_VERSION);
        assert_eq!(s.video.fps, 24);
        let s = from_json(r#"{"version": -3}"#, Platform::MacOs).unwrap();
        assert_eq!(s.version, SETTINGS_VERSION, "a negative version is nonsense, treated as oldest");
    }

    #[test]
    fn migrate_hook_stamps_the_version_and_touches_nothing_else() {
        let mut v = json!({"version": 0, "video": {"fps": 60}, "weird": [1]});
        migrate(&mut v, 0);
        assert_eq!(v, json!({"version": SETTINGS_VERSION, "video": {"fps": 60}, "weird": [1]}));

        let mut v = json!({"theme": "light"});
        migrate(&mut v, 0);
        assert_eq!(v["version"], json!(SETTINGS_VERSION));
        assert_eq!(v["theme"], json!("light"));

        // A non-object is left alone rather than panicking.
        let mut v = json!("nope");
        migrate(&mut v, 0);
        assert_eq!(v, json!("nope"));
    }

    #[test]
    fn garbage_input_is_an_invalid_settings_error() {
        for bad in ["not json", "", "[1, 2]", "42", "\"string\"", "null", "{\"video\": {"] {
            let msg = error_text(from_json(bad, Platform::MacOs));
            assert!(!msg.is_empty(), "{bad:?}");
        }
        assert!(error_text(from_json("[1, 2]", Platform::MacOs)).contains("an array"));
        assert!(error_text(from_json("not json", Platform::MacOs)).contains("not valid JSON"));
    }

    #[test]
    fn validate_video_rules() {
        let mut s = defaults();
        s.video.fps = 50;
        assert!(validation_error(&s).contains("video.fps"));
        s = defaults();
        s.video.max_width = 641;
        assert!(validation_error(&s).contains("even"));
        s.video.max_width = 638;
        assert!(validation_error(&s).contains("video.maxWidth"));
        s.video.max_width = 7682;
        assert!(validation_error(&s).contains("video.maxWidth"));
        s.video.max_width = 7680;
        validate(&s).unwrap();
        s.video.max_width = 640;
        validate(&s).unwrap();
        s = defaults();
        s.video.target_mb_per_hour = 99;
        assert!(validation_error(&s).contains("video.targetMbPerHour"));
        s.video.target_mb_per_hour = 2001;
        assert!(validation_error(&s).contains("video.targetMbPerHour"));
        s.video.target_mb_per_hour = 2000;
        validate(&s).unwrap();
    }

    #[test]
    fn validate_audio_camera_countdown_rules() {
        let mut s = defaults();
        s.audio.bitrate_kbps = 80;
        assert!(validation_error(&s).contains("audio.bitrateKbps"));
        for ok in ALLOWED_AUDIO_KBPS {
            s.audio.bitrate_kbps = ok;
            validate(&s).unwrap();
        }
        s = defaults();
        s.camera.size_pct = 9;
        assert!(validation_error(&s).contains("camera.sizePct"));
        s.camera.size_pct = 51;
        assert!(validation_error(&s).contains("camera.sizePct"));
        s.camera.size_pct = 50;
        validate(&s).unwrap();
        s = defaults();
        s.camera.margin = 401;
        assert!(validation_error(&s).contains("camera.margin"));
        s.camera.margin = 400;
        validate(&s).unwrap();
        s = defaults();
        s.countdown_seconds = 11;
        assert!(validation_error(&s).contains("countdownSeconds"));
        s.countdown_seconds = 0;
        validate(&s).unwrap();
        s.countdown_seconds = 10;
        validate(&s).unwrap();
    }

    #[test]
    fn validate_template_rules() {
        let mut s = defaults();
        for bad in ["meeting", "{yyyy}-{mm}", "", "   ", "{Name}"] {
            s.filename_template = bad.to_string();
            assert!(validation_error(&s).contains("filenameTemplate"), "{bad:?}");
        }
        for bad in ["{name}/{date}", "{date}\\{name}", "{hh}:{min} {name}", "{name}?", "{date}\t{name}"] {
            s.filename_template = bad.to_string();
            let msg = validation_error(&s);
            assert!(msg.contains("must not contain"), "{bad:?} → {msg}");
        }
        for ok in ["{name}", "{date}", "{time}", "{datetime}", "{yyyy}-{mm}-{dd} {time} {name}", "rec {date}"] {
            s.filename_template = ok.to_string();
            validate(&s).unwrap_or_else(|e| panic!("{ok:?}: {e}"));
        }
    }

    #[test]
    fn optional_shortcuts_may_be_empty_but_not_repeat() {
        let mut s = default_settings(Platform::MacOs);
        assert_eq!(s.shortcuts.scroll_capture, "Alt+Shift+S");
        assert_eq!(s.shortcuts.show_window, "");
        s.shortcuts.scroll_capture = String::new();
        assert!(validate(&s).is_ok(), "both optional shortcuts empty");
        s.shortcuts.scroll_capture = "cmdorctrl+shift+r".to_string();
        assert!(validation_error(&s).contains("scrollCapture and toggle"));
        s.shortcuts.scroll_capture = "Alt+Shift+S".to_string();
        s.shortcuts.show_window = "Alt+Shift+S".to_string();
        assert!(validation_error(&s).contains("showWindow and scrollCapture"));
    }

    #[test]
    fn version_1_file_keeps_its_values_and_gains_the_new_keys() {
        let s = from_json(
            r#"{"version": 1, "video": {"maxWidth": 2560}, "shortcuts": {"toggle": "Alt+R", "pause": "Alt+P"}}"#,
            Platform::MacOs,
        )
        .unwrap();
        assert_eq!(s.version, SETTINGS_VERSION);
        assert_eq!(s.video.max_width, 2560);
        assert_eq!(s.shortcuts.toggle, "Alt+R");
        assert_eq!(s.shortcuts.scroll_capture, "Alt+Shift+S");
        assert!(s.start_hidden_on_login);
    }

    #[test]
    fn max_width_zero_means_the_screens_own_size() {
        let mut s = default_settings(Platform::MacOs);
        s.video.max_width = 0;
        assert!(validate(&s).is_ok());
        s.video.max_width = 3840;
        assert!(validate(&s).is_ok());
    }

    #[test]
    fn validate_shortcut_rules() {
        let mut s = defaults();
        s.shortcuts.toggle = "  ".to_string();
        assert!(validation_error(&s).contains("shortcuts.toggle"));
        s = defaults();
        s.shortcuts.pause = String::new();
        assert!(validation_error(&s).contains("shortcuts.pause"));
        s = defaults();
        s.shortcuts.pause = "cmdorctrl+shift+r".to_string();
        assert!(validation_error(&s).contains("must differ"));
    }

    #[test]
    fn validate_output_dir_rules() {
        let mut s = defaults();
        for ok in ["", "/Users/g/Movies", "~/Movies", "C:\\Users\\g\\Videos", "D:/Videos", "\\\\nas\\share\\rec"] {
            s.output_dir = ok.to_string();
            validate(&s).unwrap_or_else(|e| panic!("{ok:?}: {e}"));
        }
        for bad in ["videos", "./out", "C:", "c:videos"] {
            s.output_dir = bad.to_string();
            assert!(validation_error(&s).contains("outputDir"), "{bad:?}");
        }
    }

    #[test]
    fn validate_capture_and_version_rules() {
        let mut s = defaults();
        s.capture = CaptureTarget::Display { display_id: " ".to_string() };
        assert!(validation_error(&s).contains("capture.displayId"));
        s.capture = CaptureTarget::Region { display_id: "0".to_string(), region: Region { x: 0, y: 0, w: 0, h: 600 } };
        assert!(validation_error(&s).contains("capture.region"));
        s.capture = CaptureTarget::Region { display_id: "0".to_string(), region: Region { x: 0, y: 0, w: 800, h: 600 } };
        validate(&s).unwrap();
        s.capture = CaptureTarget::Window { window_id: String::new() };
        assert!(validation_error(&s).contains("capture.windowId"));
        s.capture = CaptureTarget::Window { window_id: "123".to_string() };
        validate(&s).unwrap();
        s = defaults();
        s.version = 0;
        assert!(validation_error(&s).contains("version"));
    }

    #[test]
    fn validate_reports_every_problem_at_once() {
        let mut s = defaults();
        s.video.fps = 1;
        s.audio.bitrate_kbps = 1;
        s.countdown_seconds = 99;
        let msg = validation_error(&s);
        assert!(msg.contains("video.fps") && msg.contains("audio.bitrateKbps") && msg.contains("countdownSeconds"), "{msg}");
        assert_eq!(msg.matches("; ").count(), 2);
    }
}
