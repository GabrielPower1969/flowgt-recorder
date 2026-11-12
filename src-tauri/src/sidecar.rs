//! Locating and running the ffmpeg / ffprobe sidecars.
//!
//! Search order: Contents/Helpers or next to the executable (a bundled release build; Tauri strips
//! the target triple; on macOS a tool left in Contents/MacOS gets its own Dock icon), `src-tauri/binaries/<tool>-<triple>` (development, copied by scripts/fetch-ffmpeg.sh),
//! then `PATH`.

use crate::error::{CmdResult, CommandError};
use recorder_core::{command, FfmpegCommand, MediaInfo};
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    /// true = found next to the executable (the LGPL build shipped with a release bundle).
    pub bundled: bool,
}

/// Host target triple as used in `binaries/<tool>-<triple>`.
pub fn host_triple() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "aarch64-pc-windows-msvc"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

/// The `src-tauri` folder this binary was built from, for finding development copies of the
/// tools. A store build has none: it runs only from its bundle, and the path of the build
/// machine must not ship inside it.
pub fn source_dir() -> Option<&'static Path> {
    #[cfg(feature = "mas")]
    {
        None
    }
    #[cfg(not(feature = "mas"))]
    {
        Some(Path::new(env!("CARGO_MANIFEST_DIR")))
    }
}

fn exe_name(tool: &str) -> String {
    if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_string()
    }
}

impl Tools {
    pub fn discover() -> CmdResult<Tools> {
        let find = |tool: &str| -> Option<(PathBuf, bool)> {
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    for p in [dir.join("../Helpers").join(exe_name(tool)), dir.join(exe_name(tool))] {
                        if p.is_file() {
                            return Some((p, true));
                        }
                    }
                }
            }
            if let Some(root) = source_dir() {
                let dev = root.join("binaries").join(format!("{tool}-{}{}", host_triple(), if cfg!(windows) { ".exe" } else { "" }));
                if dev.is_file() {
                    return Some((dev, false));
                }
            }
            which::which(tool).ok().map(|p| (p, false))
        };
        match (find("ffmpeg"), find("ffprobe")) {
            (Some((ffmpeg, bundled)), Some((ffprobe, _))) => Ok(Tools { ffmpeg, ffprobe, bundled }),
            (None, _) => Err(CommandError::new("error.noFfmpeg", "ffmpeg was not found next to the app, in src-tauri/binaries or on PATH")),
            (_, None) => Err(CommandError::new("error.noFfmpeg", "ffprobe was not found next to the app, in src-tauri/binaries or on PATH")),
        }
    }

    pub fn ffmpeg_str(&self) -> String {
        self.ffmpeg.to_string_lossy().into_owned()
    }

    pub fn ffprobe_str(&self) -> String {
        self.ffprobe.to_string_lossy().into_owned()
    }

    /// First line of `ffmpeg -version`, e.g. "ffmpeg version 7.1.1 Copyright …" → "7.1.1".
    pub async fn version(&self) -> String {
        match run(&FfmpegCommand { program: self.ffmpeg_str(), args: vec!["-version".into()] }).await {
            Ok(out) => out
                .stdout
                .lines()
                .next()
                .and_then(|l| l.strip_prefix("ffmpeg version "))
                .and_then(|l| l.split_whitespace().next())
                .unwrap_or("unknown")
                .to_string(),
            Err(_) => "unknown".to_string(),
        }
    }
}

pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Run a command to completion, capturing both streams. A non-zero exit is NOT an error here
/// (device listings exit non-zero by design); only a failure to spawn is.
pub async fn run(cmd: &FfmpegCommand) -> CmdResult<Output> {
    let mut c = tokio::process::Command::new(&cmd.program);
    c.args(&cmd.args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x0800_0000);
    let out = c.output().await.map_err(|e| CommandError::new("error.noFfmpeg", format!("cannot run {}: {e}", cmd.program)))?;
    Ok(Output {
        success: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// ffprobe a file into [`MediaInfo`].
pub async fn probe(tools: &Tools, path: &Path) -> CmdResult<MediaInfo> {
    let out = run(&command::probe_command(&tools.ffprobe_str(), path)).await?;
    if !out.success {
        return Err(CommandError::new("error.parse", format!("ffprobe failed for {}: {}", path.display(), out.stderr.trim())));
    }
    Ok(command::parse_probe(&out.stdout, &path.to_string_lossy())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triple_is_known() {
        assert!(host_triple().contains('-'));
    }

    #[tokio::test]
    async fn discover_and_version_when_ffmpeg_exists() {
        let Ok(t) = Tools::discover() else {
            eprintln!("skipped: no ffmpeg");
            return;
        };
        assert!(t.ffmpeg.is_file());
        let v = t.version().await;
        assert!(v.chars().next().is_some_and(|c| c.is_ascii_digit() || c == 'n' || c == 'N'), "version: {v}");
    }

    #[tokio::test]
    async fn spawn_failure_is_an_error_but_exit_code_is_not() {
        let missing = run(&FfmpegCommand { program: "definitely-not-a-program-xyz".into(), args: vec![] }).await;
        assert!(missing.is_err());
        if cfg!(unix) {
            let r = run(&FfmpegCommand { program: "false".into(), args: vec![] }).await.unwrap();
            assert!(!r.success);
        }
    }
}
