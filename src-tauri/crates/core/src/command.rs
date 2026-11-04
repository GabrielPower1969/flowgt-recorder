//! RecordingPlan → ffmpeg argv. Pure. Tested with lavfi inputs in integration tests.
use crate::types::*;
use std::path::Path;

fn s(v: impl Into<String>) -> String {
    v.into()
}

fn push(args: &mut Vec<String>, items: &[&str]) {
    args.extend(items.iter().map(|x| (*x).to_string()));
}

fn even_round(v: f64) -> u32 {
    (((v / 2.0).round() as u32) * 2).max(2)
}

/// Output size for a captured screen: scale to max_width, keep aspect, force even dimensions.
/// Never scales up.
pub fn output_size(screen_w: u32, screen_h: u32, max_width: u32) -> (u32, u32) {
    if screen_w == 0 || screen_h == 0 {
        return (2, 2);
    }
    let w = screen_w.min(max_width.max(2));
    if w == screen_w {
        return ((screen_w & !1).max(2), (screen_h & !1).max(2));
    }
    let out_w = w & !1;
    let out_h = even_round(f64::from(screen_h) * f64::from(out_w) / f64::from(screen_w));
    (out_w.max(2), out_h)
}

/// Assemble the full plan from Settings + probed context. `output` is the segment path.
///
/// The app crate resolves "default device" to a concrete id on Windows (dshow has no default),
/// so a missing id there is an error; on macOS `None` becomes avfoundation's `default`.
pub fn make_plan(settings: &Settings, platform: Platform, screen_w: u32, screen_h: u32, encoder: EncoderChoice, output: &Path, system_audio: Option<InputSource>) -> crate::Result<RecordingPlan> {
    let (screen, crop, src_w, src_h) = match &settings.capture {
        CaptureTarget::Display { display_id } => (screen_input(platform, display_id)?, None, screen_w, screen_h),
        CaptureTarget::Region { display_id, region } => (screen_input(platform, display_id)?, Some(*region), region.w, region.h),
        CaptureTarget::Window { .. } => {
            return Err(crate::CoreError::Unsupported("window capture needs the native capture path (D9)".into()));
        }
    };
    let (out_width, out_height) = output_size(src_w, src_h, settings.video.max_width);

    let device_input = |kind: DeviceKind, id: &Option<String>| -> crate::Result<InputSource> {
        match platform {
            Platform::MacOs => {
                let id = id.clone().unwrap_or_else(|| "default".to_string());
                Ok(match kind {
                    DeviceKind::Camera => InputSource::AvFoundation { video_index: Some(id), audio_index: None, capture_cursor: false },
                    _ => InputSource::AvFoundation { video_index: None, audio_index: Some(id), capture_cursor: false },
                })
            }
            Platform::Windows => {
                let id = id.clone().ok_or_else(|| crate::CoreError::InvalidSettings("dshow needs an explicit device name".into()))?;
                Ok(match kind {
                    DeviceKind::Camera => InputSource::DShow { video: Some(id), audio: None },
                    _ => InputSource::DShow { video: None, audio: Some(id) },
                })
            }
            Platform::Linux => Err(crate::CoreError::Unsupported("recording is not supported on Linux".into())),
        }
    };

    let camera = if settings.camera.enabled { Some(device_input(DeviceKind::Camera, &settings.camera.device_id)?) } else { None };
    let mic = if settings.audio.mic_enabled { Some(device_input(DeviceKind::Microphone, &settings.audio.mic_device_id)?) } else { None };
    let pip = camera.as_ref().map(|_| crate::pip::layout(&settings.camera, out_width, out_height));

    let sources = u32::from(mic.is_some()) + u32::from(system_audio.is_some());
    let audio_tracks_in_file = match (settings.audio.tracks, sources) {
        (_, 0) => 0,
        (AudioTracks::Separate, 2) => 2,
        _ => 1,
    };
    let budget = crate::budget::budget_for(settings.video.target_mb_per_hour, settings.audio.bitrate_kbps, audio_tracks_in_file);

    Ok(RecordingPlan {
        platform,
        screen,
        screen_width: src_w,
        screen_height: src_h,
        crop,
        camera,
        mic,
        system_audio,
        fps: settings.video.fps,
        out_width,
        out_height,
        encoder,
        budget,
        audio_tracks: settings.audio.tracks,
        audio_bitrate_kbps: settings.audio.bitrate_kbps,
        pip,
        output: output.to_path_buf(),
        duration_limit_s: None,
        fragmented: true,
    })
}

fn screen_input(platform: Platform, display_id: &str) -> crate::Result<InputSource> {
    match platform {
        Platform::MacOs => Ok(InputSource::AvFoundation { video_index: Some(display_id.to_string()), audio_index: None, capture_cursor: true }),
        Platform::Windows => {
            let idx = display_id.parse().map_err(|_| crate::CoreError::InvalidSettings(format!("display id {display_id:?} is not a monitor index")))?;
            Ok(InputSource::DdaGrab { output_idx: idx })
        }
        Platform::Linux => Err(crate::CoreError::Unsupported("recording is not supported on Linux".into())),
    }
}

/// Mic levelling: cut rumble, bring speech up to a steady level. Runs far faster than real time.
const MIC_FILTER: &str = "highpass=f=80,speechnorm=e=8:t=0.01:r=0.0005:l=1";
const LIMITER: &str = "alimiter=limit=0.89:level=false";

/// Input arguments for one source (everything from `-thread_queue_size` through `-i …`).
fn input_args(src: &InputSource, fps: u32, is_camera: bool) -> Vec<String> {
    let mut a = Vec::new();
    push(&mut a, &["-thread_queue_size", "1024"]);
    match src {
        InputSource::AvFoundation { video_index, audio_index, capture_cursor } => {
            push(&mut a, &["-f", "avfoundation"]);
            if video_index.is_some() {
                // Cameras only offer 15/30 fps modes.
                let rate = if is_camera { fps.min(30) } else { fps };
                push(&mut a, &["-framerate", &rate.to_string()]);
                if !is_camera {
                    push(&mut a, &["-capture_cursor", if *capture_cursor { "1" } else { "0" }]);
                }
            }
            let v = video_index.clone().unwrap_or_else(|| "none".into());
            let au = audio_index.clone().unwrap_or_else(|| "none".into());
            push(&mut a, &["-i", &format!("{v}:{au}")]);
        }
        InputSource::DdaGrab { output_idx } => {
            push(&mut a, &["-init_hw_device", "d3d11va", "-f", "lavfi", "-i", &format!("ddagrab=output_idx={output_idx}:framerate={fps}:draw_mouse=1")]);
        }
        InputSource::GdiGrab { offset } => {
            push(&mut a, &["-f", "gdigrab", "-framerate", &fps.to_string(), "-draw_mouse", "1"]);
            if let Some(r) = offset {
                push(&mut a, &["-offset_x", &r.x.to_string(), "-offset_y", &r.y.to_string(), "-video_size", &format!("{}x{}", r.w, r.h)]);
            }
            push(&mut a, &["-i", "desktop"]);
        }
        InputSource::DShow { video, audio } => {
            push(&mut a, &["-f", "dshow"]);
            let mut target = Vec::new();
            if let Some(v) = video {
                push(&mut a, &["-rtbufsize", "256M"]);
                if is_camera {
                    push(&mut a, &["-framerate", &fps.min(30).to_string()]);
                }
                target.push(format!("video={v}"));
            }
            if let Some(au) = audio {
                target.push(format!("audio={au}"));
            }
            push(&mut a, &["-i", &target.join(":")]);
        }
        InputSource::PcmPipe { path, sample_rate, channels } => {
            push(&mut a, &["-use_wallclock_as_timestamps", "1", "-f", "s16le", "-ar", &sample_rate.to_string(), "-ac", &channels.to_string(), "-i", path]);
        }
        InputSource::Lavfi { graph } => {
            push(&mut a, &["-f", "lavfi", "-i", graph]);
        }
    }
    a
}

/// The recording command for one segment:
/// inputs (screen, camera, mic, system) → filter_complex (scale/crop, pip overlay, amix or two tracks)
/// → -c:v enc -b:v -maxrate -bufsize -g fps*2 -pix_fmt yuv420p → -c:a aac -b:a → -movflags +frag_keyframe+empty_moov+default_base_moof
/// → -progress pipe:1 -nostats -y output. `-t` when duration_limit_s.
pub fn recording_command(ffmpeg: &str, plan: &RecordingPlan) -> FfmpegCommand {
    let mut args = Vec::new();
    push(&mut args, &["-hide_banner", "-loglevel", "warning"]);

    // ---- inputs, in a fixed order: screen, camera, mic, system audio
    let screen_idx = 0usize;
    args.extend(input_args(&plan.screen, plan.fps, false));
    let mut next = 1usize;
    let mut idx = [None::<usize>; 3];
    for (slot, (src, is_camera)) in idx.iter_mut().zip([(&plan.camera, true), (&plan.mic, false), (&plan.system_audio, false)]) {
        if let Some(src) = src {
            args.extend(input_args(src, plan.fps, is_camera));
            *slot = Some(next);
            next += 1;
        }
    }
    let [cam_idx, mic_idx, sys_idx] = idx;

    // ---- video graph
    let mut graph: Vec<String> = Vec::new();
    let mut chain = format!("[{screen_idx}:v]");
    let mut steps: Vec<String> = Vec::new();
    if matches!(plan.screen, InputSource::DdaGrab { .. }) {
        // Frames arrive as D3D11 surfaces; bring them to system memory for scale / overlay.
        steps.push("hwdownload".into());
        steps.push("format=bgra".into());
    }
    if let Some(c) = &plan.crop {
        steps.push(format!("crop={}:{}:{}:{}", c.w, c.h, c.x, c.y));
    }
    steps.push(format!("scale={}:{}:flags=bicubic", plan.out_width, plan.out_height));
    chain.push_str(&steps.join(","));
    match (&plan.pip, cam_idx) {
        (Some(layout), Some(ci)) => {
            chain.push_str("[scr]");
            graph.push(chain);
            graph.push(crate::pip::pip_filter(layout, &format!("{ci}:v"), "pip"));
            graph.push(crate::pip::overlay_filter(layout, "scr", "pip", "vout"));
        }
        _ => {
            chain.push_str("[vout]");
            graph.push(chain);
        }
    }

    // ---- audio graph
    let mut audio_maps: Vec<String> = Vec::new();
    // The mic gets speech levelling; computer audio is already mastered and stays as it is.
    let mic_chain = |idx: usize, out: &str| format!("[{idx}:a]aresample=48000:async=1,{MIC_FILTER},{LIMITER}[{out}]");
    let plain_chain = |idx: usize, out: &str| format!("[{idx}:a]aresample=48000:async=1[{out}]");
    match (mic_idx, sys_idx) {
        (Some(m), Some(sy)) if plan.audio_tracks == AudioTracks::Separate => {
            graph.push(mic_chain(m, "a0"));
            graph.push(plain_chain(sy, "a1"));
            audio_maps.extend(["[a0]".to_string(), "[a1]".to_string()]);
        }
        (Some(m), Some(sy)) => {
            graph.push(format!("[{m}:a]aresample=48000:async=1,{MIC_FILTER}[am];[{sy}:a]aresample=48000:async=1[as];[am][as]amix=inputs=2:duration=longest:normalize=0,{LIMITER}[aout]"));
            audio_maps.push("[aout]".into());
        }
        (Some(m), None) => {
            graph.push(mic_chain(m, "aout"));
            audio_maps.push("[aout]".into());
        }
        (None, Some(sy)) => {
            graph.push(plain_chain(sy, "aout"));
            audio_maps.push("[aout]".into());
        }
        (None, None) => {}
    }

    push(&mut args, &["-filter_complex", &graph.join(";")]);
    push(&mut args, &["-map", "[vout]"]);
    for m in &audio_maps {
        push(&mut args, &["-map", m]);
    }

    // ---- video encode
    let b = &plan.budget;
    // VideoToolbox enforces maxrate by DROPPING frames: at 1.5x every keyframe stalled the picture
    // for 0.3 s, every 2 s (D20, measured on real capture). A 4x ceiling only stops runaway sizes.
    let (maxrate, bufsize) = if plan.encoder.name.contains("videotoolbox") { (b.video_kbps * 4, b.video_kbps * 4) } else { (b.maxrate_kbps, b.bufsize_kbps) };
    push(&mut args, &["-c:v", &plan.encoder.name]);
    args.extend(plan.encoder.extra_args.iter().cloned());
    push(
        &mut args,
        &[
            "-b:v", &format!("{}k", b.video_kbps),
            "-maxrate", &format!("{maxrate}k"),
            "-bufsize", &format!("{bufsize}k"),
            "-g", &(plan.fps * 2).to_string(),
            "-pix_fmt", "yuv420p",
            "-r", &plan.fps.to_string(),
            "-fps_mode", "cfr",
        ],
    );

    // ---- audio encode
    if audio_maps.is_empty() {
        args.push(s("-an"));
    } else {
        push(&mut args, &["-c:a", "aac", "-b:a", &format!("{}k", plan.audio_bitrate_kbps), "-ar", "48000", "-ac", "2"]);
        if audio_maps.len() == 2 {
            push(&mut args, &["-metadata:s:a:0", "title=Microphone", "-metadata:s:a:1", "title=System audio"]);
        }
    }

    // ---- container, progress, limits
    if plan.fragmented {
        push(&mut args, &["-movflags", "+frag_keyframe+empty_moov+default_base_moof", "-frag_duration", "2000000"]);
    } else {
        push(&mut args, &["-movflags", "+faststart"]);
    }
    push(&mut args, &["-progress", "pipe:1", "-nostats"]);
    if let Some(t) = plan.duration_limit_s {
        push(&mut args, &["-t", &t.to_string()]);
    }
    push(&mut args, &["-y"]);
    args.push(plan.output.to_string_lossy().into_owned());
    FfmpegCommand { program: ffmpeg.to_string(), args }
}

/// Make synthetic (`lavfi`) inputs run at wall-clock speed by inserting `-re` before each of them.
/// Real devices are already real time; tests and the app's `--selftest` need this so that pausing
/// and timing behave as they would with a screen.
pub fn realtime_lavfi(cmd: &mut FfmpegCommand) {
    let mut out = Vec::with_capacity(cmd.args.len() + 4);
    let mut i = 0;
    while i < cmd.args.len() {
        if cmd.args[i] == "-f" && cmd.args.get(i + 1).map(String::as_str) == Some("lavfi") && cmd.args.get(i + 2).map(String::as_str) == Some("-i") {
            out.push(s("-re"));
        }
        out.push(cmd.args[i].clone());
        i += 1;
    }
    cmd.args = out;
}

/// Concat segments (stream copy, fast): -f concat -safe 0 -i list.txt -c copy -movflags +faststart out.mp4
pub fn concat_command(ffmpeg: &str, list_file: &Path, output: &Path) -> FfmpegCommand {
    let mut args = Vec::new();
    push(&mut args, &["-hide_banner", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i"]);
    args.push(list_file.to_string_lossy().into_owned());
    push(&mut args, &["-map", "0", "-c", "copy", "-movflags", "+faststart", "-y"]);
    args.push(output.to_string_lossy().into_owned());
    FfmpegCommand { program: ffmpeg.to_string(), args }
}

/// Single segment → final: remux with faststart (-c copy).
pub fn remux_command(ffmpeg: &str, input: &Path, output: &Path) -> FfmpegCommand {
    let mut args = Vec::new();
    push(&mut args, &["-hide_banner", "-loglevel", "error", "-i"]);
    args.push(input.to_string_lossy().into_owned());
    push(&mut args, &["-map", "0", "-c", "copy", "-movflags", "+faststart", "-y"]);
    args.push(output.to_string_lossy().into_owned());
    FfmpegCommand { program: ffmpeg.to_string(), args }
}

/// Re-encode to hit a target size: two-pass not needed; CBR-ish at computed bitrate for the known duration.
///
/// total kbps = target_mb × 8000 ÷ duration_s; video = total − audio (never below 200 kbps).
pub fn compress_command(ffmpeg: &str, input: &Path, output: &Path, duration_ms: u64, target_mb: f64, encoder: &EncoderChoice, audio_kbps: u32) -> FfmpegCommand {
    let secs = (duration_ms as f64 / 1000.0).max(1.0);
    let total_kbps = (target_mb * 8000.0 / secs).floor() as u32;
    let video_kbps = total_kbps.saturating_sub(audio_kbps).max(crate::budget::MIN_VIDEO_KBPS);
    let mut args = Vec::new();
    push(&mut args, &["-hide_banner", "-loglevel", "error", "-i"]);
    args.push(input.to_string_lossy().into_owned());
    push(&mut args, &["-map", "0", "-c:v", &encoder.name]);
    args.extend(encoder.extra_args.iter().cloned());
    push(
        &mut args,
        &[
            "-b:v", &format!("{video_kbps}k"),
            "-maxrate", &format!("{}k", video_kbps.saturating_mul(3).div_ceil(2)),
            "-bufsize", &format!("{}k", video_kbps.saturating_mul(2)),
            "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", &format!("{audio_kbps}k"),
            "-movflags", "+faststart",
            "-progress", "pipe:1", "-nostats", "-y",
        ],
    );
    args.push(output.to_string_lossy().into_owned());
    FfmpegCommand { program: ffmpeg.to_string(), args }
}

/// ffprobe JSON: -v error -print_format json -show_format -show_streams input
pub fn probe_command(ffprobe: &str, input: &Path) -> FfmpegCommand {
    let mut args = Vec::new();
    push(&mut args, &["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]);
    args.push(input.to_string_lossy().into_owned());
    FfmpegCommand { program: ffprobe.to_string(), args }
}

fn parse_rate(r: &str) -> Option<f64> {
    let (n, d) = r.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (d != 0.0 && n > 0.0).then(|| n / d)
}

/// Parse the ffprobe JSON into MediaInfo.
pub fn parse_probe(json: &str, path: &str) -> crate::Result<MediaInfo> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| crate::CoreError::Parse(format!("ffprobe output is not JSON: {e}")))?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let video = streams.iter().find(|x| x["codec_type"] == "video").ok_or_else(|| crate::CoreError::Parse("no video stream in the file".into()))?;
    let audio: Vec<&serde_json::Value> = streams.iter().filter(|x| x["codec_type"] == "audio").collect();

    let num = |val: &serde_json::Value| -> Option<f64> { val.as_f64().or_else(|| val.as_str().and_then(|t| t.parse().ok())) };
    let duration_s = num(&v["format"]["duration"]).or_else(|| num(&video["duration"])).unwrap_or(0.0);
    let bytes = num(&v["format"]["size"]).unwrap_or(0.0) as u64;
    let bitrate_kbps = match num(&v["format"]["bit_rate"]) {
        Some(bps) => bps / 1000.0,
        None if duration_s > 0.0 => bytes as f64 * 8.0 / duration_s / 1000.0,
        None => 0.0,
    };
    let fps = video["avg_frame_rate"].as_str().and_then(parse_rate).or_else(|| video["r_frame_rate"].as_str().and_then(parse_rate)).unwrap_or(0.0);

    Ok(MediaInfo {
        path: path.to_string(),
        bytes,
        duration_ms: (duration_s * 1000.0).round() as u64,
        width: video["width"].as_u64().unwrap_or(0) as u32,
        height: video["height"].as_u64().unwrap_or(0) as u32,
        fps,
        video_codec: video["codec_name"].as_str().unwrap_or("").to_string(),
        audio_codec: audio.first().and_then(|a| a["codec_name"].as_str()).unwrap_or("").to_string(),
        audio_tracks: audio.len() as u32,
        bitrate_kbps,
    })
}

/// SSIM/PSNR comparison: -i ref -i dist -lavfi "[0:v][1:v]ssim=stats_file=-;[0:v][1:v]psnr" -f null -
///
/// The summary lines `SSIM Y:… All:…` and `PSNR y:… average:…` appear on stderr. SSIM takes the
/// distorted stream first.
pub fn quality_command(ffmpeg: &str, reference: &Path, distorted: &Path) -> FfmpegCommand {
    let mut args = Vec::new();
    push(&mut args, &["-hide_banner", "-i"]);
    args.push(reference.to_string_lossy().into_owned());
    args.push(s("-i"));
    args.push(distorted.to_string_lossy().into_owned());
    push(&mut args, &["-lavfi", "[1:v][0:v]ssim;[1:v][0:v]psnr", "-f", "null", "-"]);
    FfmpegCommand { program: ffmpeg.to_string(), args }
}

/// Device-listing commands per platform.
pub fn list_devices_command(ffmpeg: &str, platform: Platform) -> FfmpegCommand {
    let args: &[&str] = match platform {
        Platform::MacOs => &["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""],
        Platform::Windows => &["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"],
        Platform::Linux => &["-hide_banner", "-devices"],
    };
    FfmpegCommand { program: ffmpeg.to_string(), args: args.iter().map(|x| (*x).to_string()).collect() }
}

pub fn list_encoders_command(ffmpeg: &str) -> FfmpegCommand {
    FfmpegCommand { program: ffmpeg.to_string(), args: vec![s("-hide_banner"), s("-encoders")] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::sample_plan;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    fn has_pair(args: &[String], a: &str, b: &str) -> bool {
        args.windows(2).any(|w| w[0] == a && w[1] == b)
    }

    fn value_of<'a>(args: &'a [String], key: &str) -> &'a str {
        let i = args.iter().position(|x| x == key).unwrap_or_else(|| panic!("{key} missing in {args:?}"));
        &args[i + 1]
    }

    #[test]
    fn output_size_examples() {
        assert_eq!(output_size(2880, 1800, 1920), (1920, 1200));
        assert_eq!(output_size(3024, 1964, 1920), (1920, 1246));
        assert_eq!(output_size(1920, 1080, 1920), (1920, 1080));
        assert_eq!(output_size(1366, 768, 1920), (1366, 768), "never scales up");
        assert_eq!(output_size(1367, 769, 1920), (1366, 768), "odd sizes are made even");
        assert_eq!(output_size(3840, 2160, 1280), (1280, 720));
        assert_eq!(output_size(0, 0, 1920), (2, 2));
    }

    #[test]
    fn make_plan_from_default_settings() {
        let settings = crate::settings::default_settings(Platform::MacOs);
        let enc = sample_plan().encoder;
        let plan = make_plan(&settings, Platform::MacOs, 2880, 1800, enc, Path::new("seg.mp4"), None).unwrap();
        assert_eq!((plan.out_width, plan.out_height), (1920, 1200));
        assert!(plan.fragmented && plan.duration_limit_s.is_none());
        assert_eq!(plan.fps, settings.video.fps);
        assert_eq!(plan.budget, crate::budget::budget_for(400, settings.audio.bitrate_kbps, 1));
        assert_eq!(plan.camera.is_some(), settings.camera.enabled);
        assert_eq!(plan.pip.is_some(), settings.camera.enabled);
    }

    #[test]
    fn make_plan_region_window_and_windows_rules() {
        let mut settings = crate::settings::default_settings(Platform::MacOs);
        settings.capture = CaptureTarget::Region { display_id: "3".into(), region: Region { x: 10, y: 20, w: 1280, h: 720 } };
        let plan = make_plan(&settings, Platform::MacOs, 2880, 1800, sample_plan().encoder, Path::new("s.mp4"), None).unwrap();
        assert_eq!(plan.crop, Some(Region { x: 10, y: 20, w: 1280, h: 720 }));
        assert_eq!((plan.out_width, plan.out_height), (1280, 720));

        settings.capture = CaptureTarget::Window { window_id: "w".into() };
        assert!(matches!(make_plan(&settings, Platform::MacOs, 1, 1, sample_plan().encoder, Path::new("s.mp4"), None), Err(crate::CoreError::Unsupported(_))));

        let mut win = crate::settings::default_settings(Platform::Windows);
        win.capture = CaptureTarget::Display { display_id: "0".into() };
        win.camera.enabled = true;
        win.camera.device_id = None;
        assert!(make_plan(&win, Platform::Windows, 1920, 1080, sample_plan().encoder, Path::new("s.mp4"), None).is_err());
        win.camera.device_id = Some("Integrated Camera".into());
        win.audio.mic_enabled = false;
        let plan = make_plan(&win, Platform::Windows, 1920, 1080, sample_plan().encoder, Path::new("s.mp4"), None).unwrap();
        assert_eq!(plan.screen, InputSource::DdaGrab { output_idx: 0 });
        assert_eq!(plan.camera, Some(InputSource::DShow { video: Some("Integrated Camera".into()), audio: None }));
        assert_eq!(plan.budget.audio_kbps_total, 0, "no audio source, no audio budget");
    }

    #[test]
    fn macos_recording_command_has_every_documented_flag() {
        let cmd = recording_command("ffmpeg", &sample_plan());
        let a = &cmd.args;
        assert_eq!(cmd.program, "ffmpeg");
        // three avfoundation inputs (screen, camera, mic), each with a queue size
        assert_eq!(a.iter().filter(|x| *x == "-thread_queue_size").count(), 3);
        assert!(a.contains(&"3:none".to_string()) && a.contains(&"0:none".to_string()) && a.contains(&"none:1".to_string()));
        assert_eq!(value_of(a, "-c:v"), "hevc_videotoolbox");
        assert!(has_pair(a, "-tag:v", "hvc1"));
        assert_eq!(value_of(a, "-b:v"), "793k");
        // VideoToolbox: a loose ceiling, or it drops frames at every keyframe.
        assert_eq!(value_of(a, "-maxrate"), "3172k");
        assert_eq!(value_of(a, "-bufsize"), "3172k");
        assert_eq!(value_of(a, "-g"), "60");
        assert_eq!(value_of(a, "-pix_fmt"), "yuv420p");
        assert_eq!(value_of(a, "-fps_mode"), "cfr");
        assert_eq!(value_of(a, "-c:a"), "aac");
        assert_eq!(value_of(a, "-b:a"), "96k");
        assert_eq!(value_of(a, "-ar"), "48000");
        assert_eq!(value_of(a, "-movflags"), "+frag_keyframe+empty_moov+default_base_moof");
        assert_eq!(value_of(a, "-frag_duration"), "2000000");
        assert_eq!(value_of(a, "-progress"), "pipe:1");
        assert!(a.contains(&"-nostats".to_string()));
        assert_eq!(a.last().unwrap(), "/tmp/seg-000.mp4");
        assert!(a.contains(&"-y".to_string()));
        assert!(!a.contains(&"-t".to_string()));
        let fc = value_of(a, "-filter_complex");
        assert!(fc.contains("[0:v]scale=1920:1200"));
        assert!(fc.contains("[1:v]crop=ih:ih,hflip,scale=264:264"));
        assert!(fc.contains("overlay=1632:912"));
        assert!(fc.contains("[2:a]aresample=48000:async=1,highpass=f=80,speechnorm=e=8:t=0.01:r=0.0005:l=1,alimiter=limit=0.89:level=false[aout]"));
        assert!(has_pair(a, "-map", "[vout]") && has_pair(a, "-map", "[aout]"));
    }

    #[test]
    fn other_encoders_keep_the_tight_rate_cap() {
        let mut plan = sample_plan();
        plan.encoder = crate::encoders::choose(&["hevc_nvenc".to_string()], Platform::Windows, EncoderPreference::Auto).unwrap();
        let a = recording_command("ffmpeg", &plan).args;
        assert_eq!(value_of(&a, "-maxrate"), "1190k");
        assert_eq!(value_of(&a, "-bufsize"), "1586k");
    }

    #[test]
    fn camera_framerate_is_capped_at_30() {
        let mut plan = sample_plan();
        plan.fps = 60;
        let a = recording_command("ffmpeg", &plan).args;
        let rates: Vec<&str> = a.windows(2).filter(|w| w[0] == "-framerate").map(|w| w[1].as_str()).collect();
        assert_eq!(rates, vec!["60", "30"]);
        assert_eq!(value_of(&a, "-g"), "120");
    }

    #[test]
    fn no_camera_means_no_overlay() {
        let mut plan = sample_plan();
        plan.camera = None;
        plan.pip = None;
        let a = recording_command("ffmpeg", &plan).args;
        let fc = value_of(&a, "-filter_complex");
        assert!(!fc.contains("overlay") && !fc.contains("alphamerge"));
        assert!(fc.starts_with("[0:v]scale=1920:1200"));
        assert!(fc.contains("[vout]"));
    }

    #[test]
    fn mixed_and_separate_audio() {
        let mut plan = sample_plan();
        plan.system_audio = Some(InputSource::PcmPipe { path: "/tmp/sys.pcm".into(), sample_rate: 48000, channels: 2 });
        let mixed = recording_command("ffmpeg", &plan).args;
        let fc = value_of(&mixed, "-filter_complex");
        assert!(fc.contains("amix=inputs=2:duration=longest:normalize=0,alimiter=limit=0.89:level=false[aout]"));
        // Only the mic is levelled; computer audio passes through.
        assert!(fc.contains("speechnorm=e=8:t=0.01:r=0.0005:l=1[am]"));
        assert!(fc.contains("aresample=48000:async=1[as]"));
        assert_eq!(mixed.iter().filter(|x| *x == "-map").count(), 2);
        assert!(has_pair(&mixed, "-use_wallclock_as_timestamps", "1"));
        assert!(has_pair(&mixed, "-f", "s16le"));

        plan.audio_tracks = AudioTracks::Separate;
        let sep = recording_command("ffmpeg", &plan).args;
        assert!(has_pair(&sep, "-map", "[a0]") && has_pair(&sep, "-map", "[a1]"));
        assert_eq!(sep.iter().filter(|x| *x == "-map").count(), 3);
        assert!(!value_of(&sep, "-filter_complex").contains("amix"));
        assert!(has_pair(&sep, "-metadata:s:a:0", "title=Microphone"));
    }

    #[test]
    fn silent_recording_has_no_audio_stream() {
        let mut plan = sample_plan();
        plan.mic = None;
        let a = recording_command("ffmpeg", &plan).args;
        assert!(a.contains(&"-an".to_string()));
        assert!(!a.contains(&"-c:a".to_string()));
        assert!(!a.iter().any(|x| x.contains("aresample")));
    }

    #[test]
    fn duration_limit_and_non_fragmented() {
        let mut plan = sample_plan();
        plan.duration_limit_s = Some(3);
        plan.fragmented = false;
        let a = recording_command("ffmpeg", &plan).args;
        assert_eq!(value_of(&a, "-t"), "3");
        assert_eq!(value_of(&a, "-movflags"), "+faststart");
        assert!(!a.contains(&"-frag_duration".to_string()));
    }

    #[test]
    fn windows_ddagrab_goes_through_hwdownload() {
        let mut plan = sample_plan();
        plan.platform = Platform::Windows;
        plan.screen = InputSource::DdaGrab { output_idx: 1 };
        plan.camera = Some(InputSource::DShow { video: Some("Integrated Camera".into()), audio: None });
        plan.mic = Some(InputSource::DShow { video: None, audio: Some("Microphone Array".into()) });
        plan.encoder = crate::encoders::choose(&["hevc_nvenc".to_string()], Platform::Windows, EncoderPreference::Auto).unwrap();
        let a = recording_command("ffmpeg.exe", &plan).args;
        assert!(a.contains(&"ddagrab=output_idx=1:framerate=30:draw_mouse=1".to_string()));
        assert!(has_pair(&a, "-init_hw_device", "d3d11va"));
        assert!(a.contains(&"video=Integrated Camera".to_string()));
        assert!(a.contains(&"audio=Microphone Array".to_string()));
        assert!(value_of(&a, "-filter_complex").starts_with("[0:v]hwdownload,format=bgra,scale=1920:1200"));
        assert_eq!(value_of(&a, "-c:v"), "hevc_nvenc");
    }

    #[test]
    fn region_crop_comes_before_scale() {
        let mut plan = sample_plan();
        plan.crop = Some(Region { x: 100, y: 50, w: 1280, h: 720 });
        let a = recording_command("ffmpeg", &plan).args;
        assert!(value_of(&a, "-filter_complex").starts_with("[0:v]crop=1280:720:100:50,scale="));
    }

    #[test]
    fn realtime_flag_goes_before_each_lavfi_input_only() {
        let mut plan = sample_plan();
        plan.screen = InputSource::Lavfi { graph: "testsrc2=size=1920x1080:rate=30".into() };
        plan.mic = Some(InputSource::Lavfi { graph: "sine=frequency=440".into() });
        let mut cmd = recording_command("ffmpeg", &plan);
        realtime_lavfi(&mut cmd);
        assert_eq!(cmd.args.iter().filter(|x| *x == "-re").count(), 2);
        let i = cmd.args.iter().position(|x| x == "-re").unwrap();
        assert_eq!(&cmd.args[i + 1..i + 4], ["-f", "lavfi", "-i"]);
    }

    #[test]
    fn concat_remux_probe_and_list_commands() {
        let c = concat_command("ffmpeg", Path::new("w/list.txt"), Path::new("out.mp4"));
        assert!(has_pair(&c.args, "-f", "concat") && has_pair(&c.args, "-safe", "0") && has_pair(&c.args, "-c", "copy"));
        assert_eq!(c.args.last().unwrap(), "out.mp4");
        let r = remux_command("ffmpeg", Path::new("seg.mp4"), Path::new("out.mp4"));
        assert!(has_pair(&r.args, "-i", "seg.mp4") && has_pair(&r.args, "-movflags", "+faststart"));
        let p = probe_command("ffprobe", Path::new("a b.mp4"));
        assert_eq!(p.program, "ffprobe");
        assert_eq!(p.args.last().unwrap(), "a b.mp4");
        assert!(p.args.contains(&"-show_streams".to_string()));
        assert!(list_devices_command("ffmpeg", Platform::MacOs).args.contains(&"avfoundation".to_string()));
        assert!(list_devices_command("ffmpeg", Platform::Windows).args.contains(&"dshow".to_string()));
        assert_eq!(list_encoders_command("ffmpeg").args, vec!["-hide_banner", "-encoders"]);
        let q = quality_command("ffmpeg", Path::new("ref.mkv"), Path::new("dist.mp4"));
        assert!(q.args.iter().any(|x| x.contains("ssim") && x.contains("psnr")));
    }

    #[test]
    fn compress_hits_the_target_size() {
        let enc = sample_plan().encoder;
        // 300 MB target for one hour → 666 kbps total, 570 video at 96 kbps audio.
        let c = compress_command("ffmpeg", Path::new("in.mp4"), Path::new("out.mp4"), 3_600_000, 300.0, &enc, 96);
        assert_eq!(value_of(&c.args, "-b:v"), "570k");
        assert_eq!(value_of(&c.args, "-maxrate"), "855k");
        assert_eq!(value_of(&c.args, "-c:v"), "hevc_videotoolbox");
        // An impossible target floors at the minimum video rate instead of going negative.
        let tiny = compress_command("ffmpeg", Path::new("in.mp4"), Path::new("out.mp4"), 3_600_000, 10.0, &enc, 96);
        assert_eq!(value_of(&tiny.args, "-b:v"), "200k");
    }

    #[test]
    fn probe_parsing_of_a_real_file() {
        let json = include_str!("../fixtures/b-ffprobe-sample.json");
        let info = parse_probe(json, "x.mp4").unwrap();
        assert_eq!((info.width, info.height), (1920, 1080));
        assert_eq!(info.video_codec, "hevc");
        assert_eq!(info.audio_codec, "aac");
        assert_eq!(info.audio_tracks, 1);
        assert_eq!(info.bytes, 255_552);
        assert_eq!(info.duration_ms, 1021);
        assert!((info.fps - 29.37).abs() < 0.01, "{}", info.fps);
        assert!((info.bitrate_kbps - 2001.672).abs() < 0.001);
    }

    #[test]
    fn probe_parsing_errors() {
        assert!(parse_probe("not json", "x").is_err());
        assert!(parse_probe(r#"{"streams":[{"codec_type":"audio"}]}"#, "x").is_err());
        assert!(parse_probe("{}", "x").is_err());
    }

    /// Real ffmpeg, synthetic inputs, 3 seconds. Skipped when ffmpeg / ffprobe are not installed.
    #[test]
    fn lavfi_smoke_test_produces_a_playable_file() {
        let tool = |name: &str| std::process::Command::new(name).arg("-version").output().is_ok();
        if !tool("ffmpeg") || !tool("ffprobe") {
            eprintln!("skipped: ffmpeg or ffprobe not installed");
            return;
        }
        let listing = std::process::Command::new("ffmpeg").args(["-hide_banner", "-encoders"]).output().unwrap();
        let names = crate::encoders::parse_encoder_names(&String::from_utf8_lossy(&listing.stdout));
        let encoder = crate::encoders::choose(&names, Platform::current(), EncoderPreference::Auto).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let out: PathBuf = dir.path().join("smoke.mp4");
        let mut plan = sample_plan();
        plan.platform = Platform::current();
        plan.screen = InputSource::Lavfi { graph: "testsrc2=size=1920x1080:rate=30".into() };
        plan.screen_width = 1920;
        plan.screen_height = 1080;
        plan.out_width = 1920;
        plan.out_height = 1080;
        plan.camera = Some(InputSource::Lavfi { graph: "color=c=red:s=640x480:r=30".into() });
        plan.mic = Some(InputSource::Lavfi { graph: "sine=frequency=440:sample_rate=48000".into() });
        plan.system_audio = Some(InputSource::Lavfi { graph: "sine=frequency=880:sample_rate=48000".into() });
        plan.pip = Some(crate::pip::layout(&crate::testutil::camera_settings(), 1920, 1080));
        plan.encoder = encoder;
        plan.output = out.clone();
        plan.duration_limit_s = Some(3);

        let cmd = recording_command("ffmpeg", &plan);
        let run = std::process::Command::new(&cmd.program).args(&cmd.args).output().unwrap();
        assert!(run.status.success(), "ffmpeg failed:\n{}\n{}", cmd.shell_string(), String::from_utf8_lossy(&run.stderr));

        let probe = probe_command("ffprobe", &out);
        let p = std::process::Command::new(&probe.program).args(&probe.args).output().unwrap();
        let info = parse_probe(&String::from_utf8_lossy(&p.stdout), "smoke.mp4").unwrap();
        assert_eq!((info.width, info.height), (1920, 1080));
        assert!((2800..=3300).contains(&info.duration_ms), "duration {} ms", info.duration_ms);
        assert_eq!(info.audio_tracks, 1);
        assert_eq!(info.audio_codec, "aac");
        assert!(info.bytes > 10_000);
    }
}
