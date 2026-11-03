//! Parse ffmpeg device listings. Fixtures from real machines live in ../fixtures/.
use crate::types::*;
use once_cell::sync::Lazy;
use regex::Regex;

static AVF_ENTRY: Lazy<Regex> = Lazy::new(|| Regex::new(r"\]\s+\[(\d+)\]\s+(.+?)\s*$").unwrap());
static DSHOW_ENTRY: Lazy<Regex> = Lazy::new(|| Regex::new(r#""([^"]+)"\s+\((video|audio|none)\)\s*$"#).unwrap());
static SCREEN_SIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"Video:[^\n]*?,\s*(\d{3,5})x(\d{3,5})[\s,\[]").unwrap());

const LOOPBACK_HINTS: &[&str] = &["blackhole", "loopback", "soundflower", "stereo mix", "virtual-audio-capturer", "cable output", "what u hear"];

fn is_loopback(name: &str) -> bool {
    let n = name.to_lowercase();
    LOOPBACK_HINTS.iter().any(|h| n.contains(h))
}

fn device(id: String, name: &str, kind: DeviceKind, is_default: bool) -> Device {
    Device { id, name: name.to_string(), kind, is_default, width: None, height: None }
}

/// macOS: stderr of `ffmpeg -f avfoundation -list_devices true -i ""`.
/// "[AVFoundation indev @ …] [0] Capture screen 0" → screens; "FaceTime HD Camera" → cameras;
/// audio section → microphones; names containing BlackHole/Loopback/Soundflower → loopbacks (also kept in microphones).
///
/// Ids are the avfoundation indices of the section the device is listed in, because that is
/// what `-i "<video>:<audio>"` takes. The first camera is marked default; ffmpeg does not say
/// which microphone is the system default, so none is.
pub fn parse_avfoundation(stderr: &str) -> DeviceList {
    #[derive(PartialEq)]
    enum Section {
        None,
        Video,
        Audio,
    }
    let mut list = DeviceList::default();
    let mut section = Section::None;
    for line in stderr.lines() {
        let lower = line.to_lowercase();
        if lower.contains("video devices:") {
            section = Section::Video;
            continue;
        }
        if lower.contains("audio devices:") {
            section = Section::Audio;
            continue;
        }
        let Some(c) = AVF_ENTRY.captures(line) else { continue };
        let (idx, name) = (c[1].to_string(), c[2].to_string());
        match section {
            Section::Video if name.to_lowercase().starts_with("capture screen") => {
                list.screens.push(device(idx, &name, DeviceKind::Screen, list.screens.is_empty()));
            }
            Section::Video => {
                let first = list.cameras.is_empty();
                list.cameras.push(device(idx, &name, DeviceKind::Camera, first));
            }
            Section::Audio => {
                let d = device(idx, &name, DeviceKind::Microphone, false);
                if is_loopback(&name) {
                    list.loopbacks.push(d.clone());
                }
                list.microphones.push(d);
            }
            Section::None => {}
        }
    }
    list
}

/// Windows: stderr of `ffmpeg -list_devices true -f dshow -i dummy`.
/// Lines like `[dshow @ …] "Logi Webcam" (video)` / `"Microphone (Realtek)" (audio)`; "Stereo Mix"/"virtual-audio-capturer" → loopbacks.
///
/// Ids are the device names (dshow addresses devices by name). `Alternative name` lines are skipped.
pub fn parse_dshow(stderr: &str) -> DeviceList {
    let mut list = DeviceList::default();
    for line in stderr.lines() {
        if line.contains("Alternative name") {
            continue;
        }
        let Some(c) = DSHOW_ENTRY.captures(line) else { continue };
        let name = c[1].to_string();
        match &c[2] {
            "video" => {
                let first = list.cameras.is_empty();
                list.cameras.push(device(name.clone(), &name, DeviceKind::Camera, first));
            }
            "audio" => {
                let first = list.microphones.is_empty();
                let d = device(name.clone(), &name, DeviceKind::Microphone, first);
                if is_loopback(&name) {
                    list.loopbacks.push(d.clone());
                }
                list.microphones.push(d);
            }
            _ => {}
        }
    }
    list
}

/// Windows screens: we cannot list via ffmpeg; the app passes monitor info from the OS.
/// This helper just builds Device entries from (index, name, w, h).
pub fn windows_screens(monitors: &[(u32, String, u32, u32)]) -> Vec<Device> {
    monitors
        .iter()
        .enumerate()
        .map(|(i, (idx, name, w, h))| Device {
            id: idx.to_string(),
            name: name.clone(),
            kind: DeviceKind::Screen,
            is_default: i == 0,
            width: Some(*w),
            height: Some(*h),
        })
        .collect()
}

/// Parse avfoundation "Supported modes" stderr for a screen to find its native size, if present.
/// Works on the `Stream #0:0: Video: rawvideo (…), uyvy422, 1920x1200, …` line that ffmpeg
/// prints after it has opened the screen input (e.g. `ffmpeg -f avfoundation -i "3:" -t 0.1 …`).
pub fn parse_avfoundation_screen_size(stderr: &str) -> Option<(u32, u32)> {
    let c = SCREEN_SIZE.captures(stderr)?;
    Some((c[1].parse().ok()?, c[2].parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const AVF: &str = include_str!("../fixtures/b-avfoundation-list.txt");
    const PROBE: &str = include_str!("../fixtures/b-avfoundation-screen-probe.txt");
    const DSHOW: &str = include_str!("../fixtures/b-dshow-list.txt");

    #[test]
    fn avfoundation_real_listing() {
        let l = parse_avfoundation(AVF);
        assert_eq!(l.screens.len(), 1);
        assert_eq!(l.screens[0].id, "3");
        assert_eq!(l.screens[0].name, "Capture screen 0");
        assert!(l.screens[0].is_default);
        let cams: Vec<_> = l.cameras.iter().map(|d| (d.id.as_str(), d.name.as_str())).collect();
        assert_eq!(cams, vec![("0", "FaceTime高清相机"), ("1", "Insta360 Virtual Camera"), ("2", "Casablanca Virtual Camera")]);
        assert!(l.cameras[0].is_default && !l.cameras[1].is_default);
        assert_eq!(l.microphones.len(), 7);
        assert_eq!(l.microphones[1].name, "MacBook Pro麦克风");
        assert_eq!(l.microphones[1].id, "1");
        assert!(l.microphones.iter().all(|m| !m.is_default));
    }

    #[test]
    fn avfoundation_loopbacks_are_also_microphones() {
        let text = "[AVFoundation indev @ 0x1] AVFoundation audio devices:\n[AVFoundation indev @ 0x1] [0] MacBook Pro Microphone\n[AVFoundation indev @ 0x1] [1] BlackHole 2ch\n";
        let l = parse_avfoundation(text);
        assert_eq!(l.microphones.len(), 2);
        assert_eq!(l.loopbacks.len(), 1);
        assert_eq!(l.loopbacks[0].name, "BlackHole 2ch");
        assert_eq!(l.loopbacks[0].id, "1");
    }

    #[test]
    fn avfoundation_garbage_and_empty() {
        assert_eq!(parse_avfoundation(""), DeviceList::default());
        assert_eq!(parse_avfoundation("Error opening input: Input/output error\n"), DeviceList::default());
        // Entries before any section header are ignored.
        assert_eq!(parse_avfoundation("[AVFoundation indev @ 0x1] [0] Orphan\n"), DeviceList::default());
    }

    #[test]
    fn dshow_fixture() {
        let l = parse_dshow(DSHOW);
        assert_eq!(l.cameras.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), vec!["Integrated Camera", "OBS Virtual Camera"]);
        assert!(l.cameras[0].is_default);
        assert_eq!(l.microphones.len(), 3);
        assert_eq!(l.microphones[0].name, "Microphone Array (Realtek(R) Audio)");
        assert!(l.microphones[0].is_default);
        let loop_names: Vec<_> = l.loopbacks.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(loop_names, vec!["Stereo Mix (Realtek(R) Audio)", "virtual-audio-capturer"]);
        assert!(l.screens.is_empty());
    }

    #[test]
    fn windows_screen_entries() {
        let s = windows_screens(&[(0, "Display 1".into(), 2560, 1440), (1, "Display 2".into(), 1920, 1080)]);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].id, "0");
        assert!(s[0].is_default && !s[1].is_default);
        assert_eq!((s[1].width, s[1].height), (Some(1920), Some(1080)));
        assert!(windows_screens(&[]).is_empty());
    }

    #[test]
    fn screen_size_from_probe_output() {
        assert_eq!(parse_avfoundation_screen_size(PROBE), Some((1920, 1200)));
        assert_eq!(parse_avfoundation_screen_size("nothing here"), None);
        // A camera's mode list is not a screen probe and has no Video: stream line.
        assert_eq!(parse_avfoundation_screen_size(include_str!("../fixtures/b-avfoundation-modes.txt")), None);
    }
}
