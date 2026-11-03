//! Parse ffmpeg device listings. Fixtures from real machines live in ../fixtures/.
use crate::types::*;

/// macOS: stderr of `ffmpeg -f avfoundation -list_devices true -i ""`.
/// "[AVFoundation indev @ …] [0] Capture screen 0" → screens; "FaceTime HD Camera" → cameras;
/// audio section → microphones; names containing BlackHole/Loopback/Soundflower → loopbacks (also kept in microphones).
pub fn parse_avfoundation(stderr: &str) -> DeviceList {
    todo!()
}

/// Windows: stderr of `ffmpeg -list_devices true -f dshow -i dummy`.
/// Lines like `[dshow @ …] "Logi Webcam" (video)` / `"Microphone (Realtek)" (audio)`; "Stereo Mix"/"virtual-audio-capturer" → loopbacks.
pub fn parse_dshow(stderr: &str) -> DeviceList {
    todo!()
}

/// Windows screens: we cannot list via ffmpeg; the app passes monitor info from the OS.
/// This helper just builds Device entries from (index, name, w, h).
pub fn windows_screens(monitors: &[(u32, String, u32, u32)]) -> Vec<Device> {
    todo!()
}

/// Parse avfoundation "Supported modes" stderr for a screen to find its native size, if present.
pub fn parse_avfoundation_screen_size(stderr: &str) -> Option<(u32, u32)> {
    todo!()
}
