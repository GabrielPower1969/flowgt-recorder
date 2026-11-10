//! Pick the video encoder from `ffmpeg -hide_banner -encoders` output.
use crate::types::*;
use once_cell::sync::Lazy;
use regex::Regex;

static ENCODER_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*[VAS][A-Za-z.]{5}\s+(\S+)\s").unwrap());

/// Names present in the `-encoders` listing (" V....D hevc_videotoolbox  …").
/// Header legend lines (`V..... = Video`) are not encoders and are skipped.
pub fn parse_encoder_names(encoders_output: &str) -> Vec<String> {
    let mut past_legend = false;
    let mut names = Vec::new();
    for line in encoders_output.lines() {
        if line.trim_start().starts_with("------") {
            past_legend = true;
            continue;
        }
        if !past_legend {
            continue;
        }
        if let Some(c) = ENCODER_LINE.captures(line) {
            names.push(c[1].to_string());
        }
    }
    names
}

const MAC_AUTO: &[&str] = &["hevc_videotoolbox", "h264_videotoolbox", "libx264", "libopenh264", "mpeg4"];
const WIN_AUTO: &[&str] = &[
    "hevc_nvenc", "hevc_qsv", "hevc_amf", "hevc_mf", "h264_nvenc", "h264_qsv", "h264_amf", "h264_mf", "libx264", "libopenh264", "mpeg4",
];
const LINUX_AUTO: &[&str] = &["libx264", "libopenh264", "mpeg4"];

fn codec_of(name: &str) -> VideoCodec {
    if name.starts_with("hevc") {
        VideoCodec::Hevc
    } else if name.starts_with("h264") || name == "libx264" || name == "libopenh264" {
        VideoCodec::H264
    } else {
        VideoCodec::Mpeg4
    }
}

fn is_hardware(name: &str) -> bool {
    ["videotoolbox", "nvenc", "qsv", "amf", "_mf"].iter().any(|h| name.contains(h))
}

fn extra_args(name: &str) -> Vec<String> {
    let args: &[&str] = match name {
        "hevc_videotoolbox" => &["-tag:v", "hvc1", "-realtime", "1", "-allow_sw", "1", "-profile:v", "main"],
        "h264_videotoolbox" => &["-realtime", "1", "-allow_sw", "1", "-profile:v", "high"],
        "hevc_nvenc" => &["-preset", "p4", "-tune", "ll", "-rc", "vbr", "-tag:v", "hvc1"],
        "h264_nvenc" => &["-preset", "p4", "-tune", "ll", "-rc", "vbr"],
        "hevc_qsv" => &["-preset", "veryfast", "-tag:v", "hvc1"],
        "h264_qsv" => &["-preset", "veryfast"],
        "hevc_amf" => &["-usage", "lowlatency", "-tag:v", "hvc1"],
        "h264_amf" => &["-usage", "lowlatency"],
        "hevc_mf" => &["-hw_encoding", "1", "-tag:v", "hvc1"],
        "h264_mf" => &["-hw_encoding", "1"],
        "libx264" => &["-preset", "veryfast", "-tune", "zerolatency"],
        _ => &[],
    };
    args.iter().map(|s| (*s).to_string()).collect()
}

/// Fallback chain per platform and preference:
/// macOS  auto: hevc_videotoolbox → h264_videotoolbox → libx264 → libopenh264 → mpeg4
/// windows auto: hevc_nvenc → hevc_qsv → hevc_amf → hevc_mf → h264_nvenc → h264_qsv → h264_amf → h264_mf → libx264 → libopenh264 → mpeg4
/// `hevc` preference drops the h264 hardware entries first; `h264` skips hevc entirely.
/// extra_args per encoder (e.g. videotoolbox: -tag:v hvc1 -realtime 1 -allow_sw 1 -profile:v main; nvenc: -preset p4 -tune ll -rc vbr; libx264: -preset veryfast -tune zerolatency).
/// `-pix_fmt yuv420p` is added once by `command::recording_command`, not here.
pub fn choose(available: &[String], platform: Platform, pref: EncoderPreference) -> crate::Result<EncoderChoice> {
    let chain = match platform {
        Platform::MacOs => MAC_AUTO,
        Platform::Windows => WIN_AUTO,
        Platform::Linux => LINUX_AUTO,
    };
    let wanted = chain.iter().copied().filter(|n| match pref {
        EncoderPreference::Auto => true,
        EncoderPreference::Hevc => !(n.starts_with("h264_") && is_hardware(n)),
        EncoderPreference::H264 => !n.starts_with("hevc"),
    });
    for name in wanted {
        if available.iter().any(|a| a == name) {
            return Ok(EncoderChoice { name: name.to_string(), codec: codec_of(name), hardware: is_hardware(name), extra_args: extra_args(name) });
        }
    }
    Err(crate::CoreError::NoEncoder(format!("none of the known encoders is available for {platform:?}/{pref:?} (looked for: {})", chain.join(", "))))
}

/// Whether `available` was probed to actually work (the app may run a 1-frame test encode); pure helper to build that test command.
pub fn self_test_command(ffmpeg: &str, enc: &EncoderChoice) -> FfmpegCommand {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30", "-frames:v", "5", "-c:v"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    args.push(enc.name.clone());
    args.extend(enc.extra_args.iter().cloned());
    args.extend(["-b:v", "500k", "-pix_fmt", "yuv420p", "-f", "null", "-"].iter().map(|s| (*s).to_string()));
    FfmpegCommand { program: ffmpeg.to_string(), args }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const MAC: &str = include_str!("../fixtures/b-encoders-macos.txt");

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parses_the_real_listing() {
        let n = parse_encoder_names(MAC);
        assert!(n.contains(&"hevc_videotoolbox".to_string()));
        assert!(n.contains(&"h264_videotoolbox".to_string()));
        assert!(n.contains(&"libx264".to_string()));
        assert!(n.contains(&"aac".to_string()));
        assert!(!n.iter().any(|x| x == "="), "legend lines must not leak in: {n:?}");
        assert!(!n.iter().any(|x| x == "------"));
        assert!(n.len() > 100);
    }

    #[test]
    fn macos_auto_prefers_hevc_hardware() {
        let c = choose(&parse_encoder_names(MAC), Platform::MacOs, EncoderPreference::Auto).unwrap();
        assert_eq!(c.name, "hevc_videotoolbox");
        assert_eq!(c.codec, VideoCodec::Hevc);
        assert!(c.hardware);
        assert!(c.extra_args.windows(2).any(|w| w == ["-tag:v", "hvc1"]));
    }

    #[test]
    fn macos_fallback_chain() {
        let p = Platform::MacOs;
        let a = EncoderPreference::Auto;
        assert_eq!(choose(&names(&["h264_videotoolbox", "libx264"]), p, a).unwrap().name, "h264_videotoolbox");
        assert_eq!(choose(&names(&["libx264", "libopenh264", "mpeg4"]), p, a).unwrap().name, "libx264");
        let c = choose(&names(&["libopenh264", "mpeg4"]), p, a).unwrap();
        assert_eq!((c.name.as_str(), c.hardware), ("libopenh264", false));
        let c = choose(&names(&["mpeg4"]), p, a).unwrap();
        assert_eq!((c.name.as_str(), c.codec), ("mpeg4", VideoCodec::Mpeg4));
        assert!(choose(&names(&["aac"]), p, a).is_err());
        assert!(choose(&[], p, a).is_err());
    }

    #[test]
    fn preferences_filter_the_chain() {
        let all = names(&["hevc_videotoolbox", "h264_videotoolbox", "libx264"]);
        assert_eq!(choose(&all, Platform::MacOs, EncoderPreference::H264).unwrap().name, "h264_videotoolbox");
        // hevc preference with no hevc encoder skips h264 hardware and lands on software h264.
        assert_eq!(choose(&names(&["h264_videotoolbox", "libx264"]), Platform::MacOs, EncoderPreference::Hevc).unwrap().name, "libx264");
        assert!(choose(&names(&["hevc_videotoolbox"]), Platform::MacOs, EncoderPreference::H264).is_err());
    }

    #[test]
    fn windows_chain_order() {
        let p = Platform::Windows;
        let a = EncoderPreference::Auto;
        let all = names(&["hevc_nvenc", "hevc_qsv", "hevc_amf", "hevc_mf", "h264_nvenc", "libx264"]);
        assert_eq!(choose(&all, p, a).unwrap().name, "hevc_nvenc");
        assert_eq!(choose(&names(&["hevc_amf", "h264_nvenc", "libx264"]), p, a).unwrap().name, "hevc_amf");
        assert_eq!(choose(&names(&["h264_qsv", "h264_amf", "libx264"]), p, a).unwrap().name, "h264_qsv");
        assert_eq!(choose(&names(&["libx264"]), p, a).unwrap().name, "libx264");
        let nv = choose(&all, p, a).unwrap();
        assert!(nv.extra_args.contains(&"p4".to_string()));
        assert!(nv.extra_args.contains(&"vbr".to_string()));
    }

    #[test]
    fn libx264_has_no_tag_and_no_pix_fmt() {
        let c = choose(&names(&["libx264"]), Platform::MacOs, EncoderPreference::Auto).unwrap();
        assert!(!c.extra_args.contains(&"-tag:v".to_string()));
        assert!(!c.extra_args.contains(&"-pix_fmt".to_string()));
        assert!(c.extra_args.contains(&"zerolatency".to_string()));
    }

    #[test]
    fn self_test_command_shape() {
        let c = choose(&names(&["hevc_videotoolbox"]), Platform::MacOs, EncoderPreference::Auto).unwrap();
        let cmd = self_test_command("ffmpeg", &c);
        assert_eq!(cmd.program, "ffmpeg");
        let i = cmd.args.iter().position(|a| a == "-c:v").unwrap();
        assert_eq!(cmd.args[i + 1], "hevc_videotoolbox");
        assert_eq!(cmd.args.last().unwrap(), "-");
        assert!(cmd.args.contains(&"null".to_string()));
    }
}
