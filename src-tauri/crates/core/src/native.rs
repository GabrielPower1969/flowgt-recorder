//! The native capture path (macOS, D27): the `sck-record` helper records the screen, camera,
//! system audio and microphone into one MP4 per segment through ScreenCaptureKit and
//! AVAssetWriter, all on one clock. ffmpeg only joins the segments and levels/mixes the audio when
//! the recording is finished, so no live timing problem can chop the sound again.

use crate::command::{LIMITER, MIC_FILTER};
use crate::{AudioTracks, FfmpegCommand, PipShape, Progress, RecordingPlan, Region};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What the helper captures. Ids are the system's, not ffmpeg's indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeSource {
    Display(u32),
    Window(u32),
    App(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCapture {
    pub helper: PathBuf,
    pub source: NativeSource,
    /// Display pixels; None = whole source.
    pub crop: Option<Region>,
    pub system_audio: bool,
    /// `Some(None)` = the system's current microphone, `Some(Some(name))` = that one.
    pub mic: Option<Option<String>>,
    /// Same shape as `mic`, for the camera.
    pub camera: Option<Option<String>>,
    pub cursor: bool,
    /// The app's own windows (floating bar, outline) stay out of the picture.
    pub exclude_pid: u32,
    /// The helper keeps a JPEG of the latest output frame here for the floating panel.
    pub preview: Option<PathBuf>,
}

/// Audio tracks of a native segment, in file order: system first, then mic. Kept in the
/// manifest so crash recovery finishes the file the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NativeAudio {
    pub system: bool,
    pub mic: bool,
}

impl NativeAudio {
    pub fn of(cap: &NativeCapture) -> NativeAudio {
        NativeAudio {
            system: cap.system_audio,
            mic: cap.mic.is_some(),
        }
    }
}

/// One window the user can pick, from the helper's `--list`. Front-most first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    /// CGWindowID; changes every time the window is opened.
    pub id: u32,
    pub title: String,
    pub app: String,
    pub bundle_id: String,
    pub pid: u32,
    /// Size in pixels on the display it sits on.
    pub width: u32,
    pub height: u32,
    /// The app's icon as a base64 PNG, when the helper had one.
    pub icon: Option<String>,
}

/// The windows from the helper's `--list` JSON, with each app's icon attached by pid.
pub fn parse_window_list(json: &str) -> crate::Result<Vec<WindowInfo>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct App {
        pid: u32,
        icon: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Win {
        id: u32,
        title: String,
        app: String,
        bundle_id: String,
        pid: u32,
        width: u32,
        height: u32,
    }
    #[derive(Deserialize)]
    struct List {
        apps: Vec<App>,
        windows: Vec<Win>,
    }
    let list: List = serde_json::from_str(json)
        .map_err(|e| crate::CoreError::Parse(format!("window list: {e}")))?;
    let icons: std::collections::HashMap<u32, String> = list
        .apps
        .into_iter()
        .filter_map(|a| a.icon.map(|i| (a.pid, i)))
        .collect();
    Ok(list
        .windows
        .into_iter()
        .filter(|w| w.width >= 2 && w.height >= 2)
        .map(|w| WindowInfo {
            icon: icons.get(&w.pid).cloned(),
            id: w.id,
            title: w.title,
            app: w.app,
            bundle_id: w.bundle_id,
            pid: w.pid,
            width: w.width,
            height: w.height,
        })
        .collect())
}

/// Command line for one segment. The frame size, rate, codec and bitrates come from the plan.
pub fn helper_args(cap: &NativeCapture, plan: &RecordingPlan, output: &Path) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    match &cap.source {
        NativeSource::Display(id) => a.extend(["--display".into(), id.to_string()]),
        NativeSource::Window(id) => a.extend(["--window".into(), id.to_string()]),
        NativeSource::App(bundle) => a.extend(["--app".into(), bundle.clone()]),
    }
    a.extend(["--out".into(), output.to_string_lossy().into_owned()]);
    a.extend([
        "--size".into(),
        format!("{}x{}", plan.out_width, plan.out_height),
    ]);
    a.extend(["--fps".into(), plan.fps.to_string()]);
    let codec = if plan.encoder.name.contains("264") {
        "h264"
    } else {
        "hevc"
    };
    a.extend(["--codec".into(), codec.into()]);
    a.extend(["--video-kbps".into(), plan.budget.video_kbps.to_string()]);
    a.extend(["--audio-kbps".into(), plan.audio_bitrate_kbps.to_string()]);
    if let Some(r) = &cap.crop {
        a.extend(["--crop".into(), format!("{},{},{},{}", r.x, r.y, r.w, r.h)]);
    }
    if cap.system_audio {
        a.push("--system-audio".into());
    }
    match &cap.mic {
        Some(Some(name)) => a.extend(["--mic-device".into(), name.clone()]),
        Some(None) => a.push("--mic".into()),
        None => {}
    }
    if let Some(cam) = &cap.camera {
        match cam {
            Some(name) => a.extend(["--camera-device".into(), name.clone()]),
            None => a.push("--camera".into()),
        }
        if let Some(p) = &plan.pip {
            let shape = match p.shape {
                PipShape::Square => "square",
                PipShape::Rounded => "rounded",
                PipShape::Circle => "circle",
            };
            a.extend([
                "--pip".into(),
                format!(
                    "{},{},{},{},{}",
                    p.x,
                    p.y,
                    p.size,
                    shape,
                    u8::from(p.mirror)
                ),
            ]);
        }
    }
    if !cap.cursor {
        a.push("--no-cursor".into());
    }
    a.extend(["--exclude-pid".into(), cap.exclude_pid.to_string()]);
    if let Some(p) = &cap.preview {
        a.extend(["--preview".into(), p.to_string_lossy().into_owned()]);
    }
    if let Some(s) = plan.duration_limit_s {
        a.extend(["--duration".into(), s.to_string()]);
    }
    a
}

/// One line of the helper's stdout.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NativeEvent {
    Started {
        width: u32,
        height: u32,
        #[serde(default)]
        audio_tracks: Vec<String>,
    },
    Progress {
        seconds: f64,
        bytes: u64,
        frames: u64,
        dropped: u64,
    },
    Warning {
        code: String,
        detail: String,
    },
    Stopped {
        seconds: f64,
        bytes: u64,
        frames: u64,
        dropped: u64,
    },
    Error {
        code: String,
        detail: String,
    },
}

pub fn parse_event(line: &str) -> Option<NativeEvent> {
    let t = line.trim();
    if !t.starts_with('{') {
        return None;
    }
    serde_json::from_str(t).ok()
}

/// The helper's progress in the shape the supervisor already understands.
pub fn progress_from(
    seconds: f64,
    bytes: u64,
    frames: u64,
    dropped: u64,
    finished: bool,
) -> Progress {
    let fps = if seconds > 0.0 {
        frames as f64 / seconds
    } else {
        0.0
    };
    let bitrate = if seconds > 0.0 {
        bytes as f64 * 8.0 / 1000.0 / seconds
    } else {
        0.0
    };
    Progress {
        frame: frames,
        fps,
        out_time_ms: (seconds * 1000.0).max(0.0) as u64,
        total_size: bytes,
        bitrate_kbps: bitrate,
        speed: 1.0,
        drop_frames: dropped,
        dup_frames: 0,
        finished,
    }
}

/// Helper error codes → the app's error codes.
pub fn error_code(helper_code: &str) -> &'static str {
    match helper_code {
        "permission" => "error.permission",
        // At start: the window is not on screen; while recording: it was closed.
        "sourceNotFound" => "error.windowGone",
        "outputFailed" => "error.ffmpegExit",
        "unsupportedOs" => "error.start",
        _ => "error.start",
    }
}

/// Native segments → the final file. The video is copied; the audio is levelled (mic) and mixed
/// (one track) or kept apart (two tracks). `list` is a concat list for several segments.
pub fn finish_command(
    ffmpeg: &str,
    input: &Path,
    concat: bool,
    output: &Path,
    audio: NativeAudio,
    tracks: AudioTracks,
    audio_kbps: u32,
) -> FfmpegCommand {
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into()];
    if concat {
        args.extend(["-f".into(), "concat".into(), "-safe".into(), "0".into()]);
    }
    args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
    args.extend(["-map".into(), "0:v:0".into(), "-c:v".into(), "copy".into()]);
    let kbps = format!("{audio_kbps}k");
    match (audio.system, audio.mic) {
        (false, false) => {}
        (true, false) => args.extend(["-map".into(), "0:a:0".into(), "-c:a".into(), "copy".into()]),
        (false, true) => {
            args.extend([
                "-filter_complex".into(),
                format!("[0:a:0]{MIC_FILTER},{LIMITER}[aout]"),
                "-map".into(),
                "[aout]".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                kbps,
            ]);
        }
        (true, true) if tracks == AudioTracks::Separate => {
            // Output order matches the ffmpeg path: mic first, computer audio second.
            args.extend([
                "-filter_complex".into(),
                format!("[0:a:1]{MIC_FILTER},{LIMITER}[am]"),
                "-map".into(),
                "[am]".into(),
                "-map".into(),
                "0:a:0".into(),
            ]);
            args.extend([
                "-c:a:0".into(),
                "aac".into(),
                "-b:a:0".into(),
                kbps,
                "-c:a:1".into(),
                "copy".into(),
            ]);
        }
        (true, true) => {
            args.extend([
                "-filter_complex".into(),
                format!("[0:a:1]{MIC_FILTER}[am];[am][0:a:0]amix=inputs=2:duration=longest:normalize=0,{LIMITER}[aout]"),
                "-map".into(),
                "[aout]".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                kbps,
            ]);
        }
    }
    args.extend([
        "-movflags".into(),
        "+faststart".into(),
        "-y".into(),
        output.to_string_lossy().into_owned(),
    ]);
    FfmpegCommand {
        program: ffmpeg.to_string(),
        args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::sample_plan;
    use pretty_assertions::assert_eq;

    fn cap() -> NativeCapture {
        NativeCapture {
            helper: PathBuf::from("/h/sck-record"),
            source: NativeSource::Display(1),
            crop: None,
            system_audio: true,
            mic: Some(Some("USB Mic".into())),
            camera: Some(None),
            cursor: true,
            exclude_pid: 4242,
            preview: Some(PathBuf::from("/w/preview.jpg")),
        }
    }

    #[test]
    fn args_cover_the_plan() {
        let mut plan = sample_plan();
        plan.pip = Some(crate::PipLayout {
            size: 300,
            x: 1560,
            y: 740,
            shape: PipShape::Circle,
            mirror: true,
        });
        let a = helper_args(&cap(), &plan, Path::new("/w/seg-000.mp4"));
        let s = a.join(" ");
        assert!(
            s.starts_with("--display 1 --out /w/seg-000.mp4 --size "),
            "{s}"
        );
        assert!(s.contains(" --fps 30 "), "{s}");
        assert!(s.contains("--system-audio --mic-device USB Mic --camera --pip 1560,740,300,circle,1 --exclude-pid 4242 --preview /w/preview.jpg"), "{s}");
        assert!(!s.contains("--no-cursor"));
        assert!(!s.contains("--crop"));
    }

    #[test]
    fn args_for_a_window_without_extras() {
        let mut c = cap();
        c.source = NativeSource::Window(77);
        c.system_audio = false;
        c.mic = Some(None);
        c.camera = None;
        c.cursor = false;
        c.crop = Some(Region {
            x: 1,
            y: 2,
            w: 30,
            h: 40,
        });
        c.preview = None;
        let s = helper_args(&c, &sample_plan(), Path::new("o.mp4")).join(" ");
        assert!(s.starts_with("--window 77 --out o.mp4"), "{s}");
        assert!(
            s.contains("--crop 1,2,30,40 --mic --no-cursor --exclude-pid 4242"),
            "{s}"
        );
        assert!(!s.contains("--pip"));
        assert!(!s.contains("--preview"));
    }

    #[test]
    fn events_parse() {
        assert_eq!(
            parse_event(
                r#"{"audioTracks":["system","mic"],"event":"started","height":1080,"width":1920}"#
            ),
            Some(NativeEvent::Started {
                width: 1920,
                height: 1080,
                audio_tracks: vec!["system".into(), "mic".into()]
            })
        );
        assert_eq!(
            parse_event(
                r#"{"bytes":10,"dropped":0,"event":"progress","frames":3,"seconds":1.235}"#
            ),
            Some(NativeEvent::Progress {
                seconds: 1.235,
                bytes: 10,
                frames: 3,
                dropped: 0
            })
        );
        assert_eq!(
            parse_event(r#"{"code":"warn.cameraLost","detail":"x","event":"warning"}"#),
            Some(NativeEvent::Warning {
                code: "warn.cameraLost".into(),
                detail: "x".into()
            })
        );
        assert_eq!(parse_event("sck-record: stopping"), None);
        assert_eq!(parse_event("{not json"), None);
    }

    #[test]
    fn progress_shape() {
        let p = progress_from(2.0, 500_000, 60, 1, false);
        assert_eq!(p.out_time_ms, 2000);
        assert_eq!(p.frame, 60);
        assert_eq!(p.fps, 30.0);
        assert_eq!(p.bitrate_kbps, 2000.0);
        assert_eq!(p.drop_frames, 1);
    }

    #[test]
    fn finish_mixes_levels_and_copies() {
        let mixed = finish_command(
            "ffmpeg",
            Path::new("seg.mp4"),
            false,
            Path::new("out.mp4"),
            NativeAudio {
                system: true,
                mic: true,
            },
            AudioTracks::Mixed,
            128,
        );
        let s = mixed.args.join(" ");
        assert!(
            s.contains("-i seg.mp4 -map 0:v:0 -c:v copy -filter_complex [0:a:1]highpass"),
            "{s}"
        );
        assert!(
            s.contains("[am][0:a:0]amix=inputs=2:duration=longest:normalize=0,alimiter"),
            "{s}"
        );
        assert!(
            s.contains("-map [aout] -c:a aac -b:a 128k -movflags +faststart -y out.mp4"),
            "{s}"
        );
        assert!(!s.contains("concat"));

        let sep = finish_command(
            "ffmpeg",
            Path::new("l.txt"),
            true,
            Path::new("o.mp4"),
            NativeAudio {
                system: true,
                mic: true,
            },
            AudioTracks::Separate,
            96,
        );
        let s = sep.args.join(" ");
        assert!(s.contains("-f concat -safe 0 -i l.txt"), "{s}");
        assert!(
            s.contains("-map [am] -map 0:a:0 -c:a:0 aac -b:a:0 96k -c:a:1 copy"),
            "{s}"
        );

        let sys = finish_command(
            "ffmpeg",
            Path::new("s.mp4"),
            false,
            Path::new("o.mp4"),
            NativeAudio {
                system: true,
                mic: false,
            },
            AudioTracks::Mixed,
            128,
        );
        assert!(sys
            .args
            .join(" ")
            .contains("-map 0:a:0 -c:a copy -movflags"));

        let mic = finish_command(
            "ffmpeg",
            Path::new("s.mp4"),
            false,
            Path::new("o.mp4"),
            NativeAudio {
                system: false,
                mic: true,
            },
            AudioTracks::Mixed,
            128,
        );
        assert!(mic
            .args
            .join(" ")
            .contains("-filter_complex [0:a:0]highpass"));

        let none = finish_command(
            "ffmpeg",
            Path::new("s.mp4"),
            false,
            Path::new("o.mp4"),
            NativeAudio::default(),
            AudioTracks::Mixed,
            128,
        );
        assert_eq!(none.args.join(" "), "-hide_banner -loglevel error -i s.mp4 -map 0:v:0 -c:v copy -movflags +faststart -y o.mp4");
    }

    #[test]
    fn window_list_attaches_icons_and_drops_empty_windows() {
        let json = r#"{"displays":[],"cameras":[],"microphones":[],
            "apps":[{"bundleId":"com.a","name":"A","pid":7,"icon":"AAAA"},{"bundleId":"com.b","name":"B","pid":8}],
            "windows":[{"id":12,"title":"Doc","app":"A","bundleId":"com.a","pid":7,"x":0,"y":0,"pointWidth":10,"pointHeight":5,"width":20,"height":10},
                       {"id":13,"title":"","app":"B","bundleId":"com.b","pid":8,"x":0,"y":0,"pointWidth":1,"pointHeight":1,"width":1,"height":1}]}"#;
        let w = parse_window_list(json).unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].id, 12);
        assert_eq!(w[0].icon.as_deref(), Some("AAAA"));
        assert_eq!((w[0].width, w[0].height), (20, 10));
        assert!(parse_window_list("nope").is_err());
    }

    #[test]
    fn error_codes() {
        assert_eq!(error_code("permission"), "error.permission");
        assert_eq!(error_code("sourceNotFound"), "error.windowGone");
        assert_eq!(error_code("whatever"), "error.start");
    }
}
