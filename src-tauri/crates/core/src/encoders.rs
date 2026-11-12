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
    if names.iter().any(|n| n == "h264_mf") {
        names.push(H264_MF_SOFTWARE.to_string());
    }
    names
}

const MAC_AUTO: &[&str] = &["hevc_videotoolbox", "h264_videotoolbox", "libx264", "libopenh264", "mpeg4"];
const WIN_AUTO: &[&str] = &[
    "hevc_nvenc", "hevc_qsv", "hevc_amf", "hevc_mf", "h264_nvenc", "h264_qsv", "h264_amf", "h264_mf", "libx264", "libopenh264", H264_MF_SOFTWARE, "mpeg4",
];
/// Chain entry for Windows' own software H.264 encoder: ffmpeg's `h264_mf` with hardware switched
/// off. It is not a separate ffmpeg encoder, so it is "available" whenever `h264_mf` is listed. It
/// matters for an ffmpeg built without third-party encoders (the store build): on a PC with no
/// hardware encoder the next step down would otherwise be MPEG-4.
const H264_MF_SOFTWARE: &str = "h264_mf_software";
const LINUX_AUTO: &[&str] = &["libx264", "libopenh264", "mpeg4"];

/// The name ffmpeg knows a chain entry by.
fn ffmpeg_name(entry: &str) -> &str {
    if entry == H264_MF_SOFTWARE {
        "h264_mf"
    } else {
        entry
    }
}

/// The chain entry a choice came from: what [`choose_working`] and the app strike off the list
/// when the choice fails its test frame.
pub fn chain_entry(choice: &EncoderChoice) -> &str {
    if choice.name == "h264_mf" && !choice.hardware {
        H264_MF_SOFTWARE
    } else {
        &choice.name
    }
}

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
    name != H264_MF_SOFTWARE && ["videotoolbox", "nvenc", "qsv", "amf", "_mf"].iter().any(|h| name.contains(h))
}

/// The pixel format handed to an encoder. Media Foundation's hardware encoders take NV12 and turn
/// away planar 4:2:0 ("Invalid argument" on the first frame, measured with Intel's H.264 and HEVC
/// encoders); its software encoder takes either. The picture in the file is 8-bit 4:2:0 both ways.
pub fn input_pix_fmt(encoder_name: &str) -> &'static str {
    if encoder_name.ends_with("_mf") {
        "nv12"
    } else {
        "yuv420p"
    }
}

/// The `-movflags` of a recording segment: always a fragmented MP4 that survives a crash (D4).
/// Media Foundation's hardware encoders hand over their stream headers only with the first frame,
/// which is too late for an initial `moov` written up front (`empty_moov`): the segment then cannot
/// be opened at all, by any player or by the final join (measured with Intel's H.264 and HEVC
/// encoders). `delay_moov` writes that header together with the first fragment instead. The price:
/// a crash in the first two seconds, before the first fragment, leaves nothing readable.
pub fn segment_movflags(encoder_name: &str) -> &'static str {
    if encoder_name.ends_with("_mf") {
        "+frag_keyframe+delay_moov+default_base_moof"
    } else {
        "+frag_keyframe+empty_moov+default_base_moof"
    }
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
        // Peak-constrained VBR: of the modes tried it stayed closest under the size budget.
        H264_MF_SOFTWARE => &["-hw_encoding", "0", "-rate_control", "pc_vbr"],
        "libx264" => &["-preset", "veryfast", "-tune", "zerolatency"],
        _ => &[],
    };
    args.iter().map(|s| (*s).to_string()).collect()
}

/// Fallback chain per platform and preference:
/// macOS  auto: hevc_videotoolbox → h264_videotoolbox → libx264 → libopenh264 → mpeg4
/// windows auto: hevc_nvenc → hevc_qsv → hevc_amf → hevc_mf → h264_nvenc → h264_qsv → h264_amf → h264_mf → libx264 → libopenh264 → h264_mf in software → mpeg4
/// `hevc` preference drops the h264 hardware entries first; `h264` skips hevc entirely.
/// extra_args per encoder (e.g. videotoolbox: -tag:v hvc1 -realtime 1 -allow_sw 1 -profile:v main; nvenc: -preset p4 -tune ll -rc vbr; libx264: -preset veryfast -tune zerolatency).
/// `-pix_fmt` ([`input_pix_fmt`]) is added once by `command::recording_command`, not here.
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
            return Ok(EncoderChoice { name: ffmpeg_name(name).to_string(), codec: codec_of(name), hardware: is_hardware(name), extra_args: extra_args(name) });
        }
    }
    Err(crate::CoreError::NoEncoder(format!("none of the known encoders is available for {platform:?}/{pref:?} (looked for: {})", chain.join(", "))))
}

/// [`choose`], skipping every encoder that `works` rejects.
///
/// The `-encoders` listing names what was compiled into ffmpeg, not what this machine can open: a
/// build with NVENC lists `hevc_nvenc` on a PC with no NVIDIA card. The caller's closure does the
/// I/O, normally by running [`self_test_command`]; this function only walks the chain.
pub fn choose_working(
    available: &[String],
    platform: Platform,
    pref: EncoderPreference,
    mut works: impl FnMut(&EncoderChoice) -> bool,
) -> crate::Result<EncoderChoice> {
    let mut names = available.to_vec();
    loop {
        let choice = choose(&names, platform, pref)?;
        if works(&choice) {
            return Ok(choice);
        }
        let failed = chain_entry(&choice).to_string();
        names.retain(|n| n != &failed);
    }
}

/// Whether `available` was probed to actually work (the app may run a 1-frame test encode); pure helper to build that test command.
pub fn self_test_command(ffmpeg: &str, enc: &EncoderChoice) -> FfmpegCommand {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30", "-frames:v", "5", "-c:v"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    args.push(enc.name.clone());
    args.extend(enc.extra_args.iter().cloned());
    args.extend(["-b:v", "500k", "-pix_fmt", input_pix_fmt(&enc.name), "-f", "null", "-"].iter().map(|s| (*s).to_string()));
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
    fn windows_without_third_party_encoders_ends_in_software_h264() {
        // What the store build of ffmpeg lists: Media Foundation and ffmpeg's own encoders only.
        let listing = "Encoders:
 ------
 V....D h264_mf   H264 via MediaFoundation
 V....D hevc_mf   HEVC via MediaFoundation
 V.S..D mpeg4   MPEG-4 part 2
 A....D aac   AAC
";
        let listed = parse_encoder_names(listing);
        let (p, a) = (Platform::Windows, EncoderPreference::Auto);
        let mut tried = Vec::new();
        // A PC with no hardware encoder: both hardware entries fail their test frame.
        let c = choose_working(&listed, p, a, |e| {
            tried.push((e.name.clone(), e.hardware));
            !e.hardware
        })
        .unwrap();
        assert_eq!(tried, vec![("hevc_mf".to_string(), true), ("h264_mf".to_string(), true), ("h264_mf".to_string(), false)]);
        assert_eq!((c.name.as_str(), c.codec, c.hardware), ("h264_mf", VideoCodec::H264, false));
        assert!(c.extra_args.windows(2).any(|w| w == ["-hw_encoding", "0"]));
        assert_eq!(chain_entry(&c), "h264_mf_software");
        // Media Foundation gets NV12, in the test frame as in the recording; everything else 4:2:0 planar.
        let test = self_test_command("ffmpeg", &c);
        assert!(test.args.windows(2).any(|w| w == ["-pix_fmt", "nv12"]));
        assert_eq!((input_pix_fmt("hevc_mf"), input_pix_fmt("hevc_qsv"), input_pix_fmt("libx264")), ("nv12", "yuv420p", "yuv420p"));
        // ...and a segment whose header waits for the encoder; every other encoder keeps the header up front.
        assert_eq!(segment_movflags("hevc_mf"), "+frag_keyframe+delay_moov+default_base_moof");
        assert_eq!(segment_movflags("h264_mf"), "+frag_keyframe+delay_moov+default_base_moof");
        for other in ["hevc_qsv", "hevc_videotoolbox", "libx264", "mpeg4"] {
            assert_eq!(segment_movflags(other), "+frag_keyframe+empty_moov+default_base_moof");
        }
        // And when even that fails, MPEG-4 is still there.
        assert_eq!(choose_working(&listed, p, a, |e| e.name == "mpeg4").unwrap().name, "mpeg4");
        // With hardware present the first entry wins as before.
        assert_eq!(choose_working(&listed, p, a, |_| true).unwrap().name, "hevc_mf");
        // A Mac listing never gains the entry.
        assert!(!parse_encoder_names(MAC).iter().any(|n| n == "h264_mf_software"));
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
    fn choose_working_skips_encoders_that_fail_their_test() {
        let p = Platform::Windows;
        let a = EncoderPreference::Auto;
        // A PC with Intel graphics and an ffmpeg build that also lists the NVIDIA encoders.
        let listed = names(&["hevc_nvenc", "hevc_qsv", "h264_nvenc", "h264_qsv", "libopenh264"]);
        let mut tried = Vec::new();
        let c = choose_working(&listed, p, a, |e| {
            tried.push(e.name.clone());
            !e.name.ends_with("_nvenc")
        })
        .unwrap();
        assert_eq!(c.name, "hevc_qsv");
        assert_eq!(tried, vec!["hevc_nvenc", "hevc_qsv"]);
        // The first entry is taken when it works; an error when nothing does.
        assert_eq!(choose_working(&listed, p, a, |_| true).unwrap().name, "hevc_nvenc");
        assert!(choose_working(&listed, p, a, |_| false).is_err());
        assert!(choose_working(&[], p, a, |_| true).is_err());
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
