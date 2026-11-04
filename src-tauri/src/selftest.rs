//! `flowgt-recorder --selftest N [--pause-at A --resume-at B] [--system-audio]`
//!
//! Runs the real supervisor against synthetic ffmpeg inputs (testsrc2 screen, a red "camera",
//! a sine "microphone"): no window, no permission prompt, no device. Prints the
//! `RecordingResult` as JSON on stdout and exits 0, or prints the error on stderr and exits 1.

use crate::recorder::{NullSink, Recorder, StartParams};
use crate::sidecar::Tools;
use recorder_core::*;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Synthetic-input parameters, shared with the supervisor's tests.
pub fn lavfi_params(dir: &Path, camera: bool) -> Option<StartParams> {
    let tools = Tools::discover().ok()?;
    let listing = std::process::Command::new(&tools.ffmpeg).args(["-hide_banner", "-encoders"]).output().ok()?;
    let names = encoders::parse_encoder_names(&String::from_utf8_lossy(&listing.stdout));
    let encoder = encoders::choose(&names, Platform::current(), EncoderPreference::Auto).ok()?;
    let mut settings = settings::default_settings(Platform::current());
    settings.camera.enabled = camera;
    let mut plan = command::make_plan(&settings, Platform::current(), 1920, 1080, encoder, &dir.join("x.mp4"), None).ok()?;
    plan.screen = InputSource::Lavfi { graph: "testsrc2=size=1920x1080:rate=30".into() };
    plan.camera = camera.then(|| InputSource::Lavfi { graph: "color=c=red:s=640x480:r=30".into() });
    plan.mic = Some(InputSource::Lavfi { graph: "sine=frequency=440:sample_rate=48000".into() });
    Some(StartParams {
        plan,
        session_id: "selftest".into(),
        meeting_name: "selftest".into(),
        final_output: dir.join("selftest.mp4"),
        work_dir: dir.join("work"),
        target_mb_per_hour: 400,
        low_disk_warn_gb: 0,
        tools,
        helper: None,
    })
}

fn arg_value(args: &[String], name: &str) -> Option<f64> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
}

/// `Some(exit_code)` when the process was started with `--selftest`.
pub fn maybe_run() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == "--selftest")?;
    let seconds: f64 = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(5.0);
    let pause_at = arg_value(&args, "--pause-at");
    let resume_at = arg_value(&args, "--resume-at");
    let system_audio = args.iter().any(|a| a == "--system-audio");
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("selftest: cannot start the async runtime: {e}");
            return Some(1);
        }
    };
    Some(rt.block_on(run(seconds, pause_at, resume_at, system_audio)))
}

async fn run(seconds: f64, pause_at: Option<f64>, resume_at: Option<f64>, system_audio: bool) -> i32 {
    let dir = std::env::temp_dir().join(format!("flowgt-selftest-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("selftest: cannot create {}: {e}", dir.display());
        return 1;
    }
    let Some(mut params) = lavfi_params(&dir, true) else {
        eprintln!("selftest: ffmpeg / ffprobe not found (run scripts/fetch-ffmpeg.sh or install ffmpeg)");
        return 1;
    };
    if system_audio {
        // The real ScreenCaptureKit helper (needs the screen-recording permission).
        let Some(helper) = crate::commands::find_helper() else {
            eprintln!("selftest: sck-audio helper not found (run scripts/build-helper.sh)");
            return 1;
        };
        params.plan.system_audio = Some(InputSource::PcmPipe { path: String::new(), sample_rate: 48_000, channels: 2 });
        params.helper = Some(helper);
    }
    eprintln!("selftest: encoder {}, output {}", params.plan.encoder.name, params.final_output.display());
    let rec = Recorder::new(Arc::new(NullSink));
    let sleep = |s: f64| tokio::time::sleep(Duration::from_millis((s.max(0.0) * 1000.0) as u64));
    let outcome: crate::error::CmdResult<RecordingResult> = async {
        rec.start(params).await?;
        match (pause_at, resume_at) {
            (Some(p), Some(r)) if p < r && p < seconds => {
                sleep(p).await;
                rec.pause().await?;
                eprintln!("selftest: paused at {p}s");
                sleep(r - p).await;
                rec.resume().await?;
                eprintln!("selftest: resumed at {r}s");
                sleep(seconds - p).await;
            }
            _ => sleep(seconds).await,
        }
        rec.stop().await
    }
    .await;
    match outcome {
        Ok(res) => {
            println!("{}", serde_json::to_string_pretty(&res).expect("result serialises"));
            0
        }
        Err(e) => {
            eprintln!("selftest failed: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_read_by_name() {
        let a: Vec<String> = ["x", "--selftest", "5", "--pause-at", "2", "--resume-at", "4.5"].iter().map(|s| s.to_string()).collect();
        assert_eq!(arg_value(&a, "--pause-at"), Some(2.0));
        assert_eq!(arg_value(&a, "--resume-at"), Some(4.5));
        assert_eq!(arg_value(&a, "--missing"), None);
    }
}
