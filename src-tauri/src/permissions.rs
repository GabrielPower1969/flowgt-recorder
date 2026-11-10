//! System permissions (macOS TCC; Windows has per-app toggles for camera / microphone only).
//!
//! macOS cannot tell "never asked" from "denied" for screen recording: `CGPreflight…` is just a
//! bool. We report `notDetermined` for false until `request` has been called once and returned
//! false, then `denied`.

use recorder_core::{PermissionState, PermissionStatus};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
static SCREEN_ASKED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Screen,
    Camera,
    Microphone,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "screen" => Some(Kind::Screen),
            "camera" => Some(Kind::Camera),
            "microphone" => Some(Kind::Microphone),
            _ => None,
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo};
    use std::sync::mpsc;
    use std::time::Duration;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn screen() -> PermissionState {
        // SAFETY: plain CoreGraphics query, no arguments.
        if unsafe { CGPreflightScreenCaptureAccess() } {
            PermissionState::Granted
        } else if SCREEN_ASKED.load(Ordering::Relaxed) {
            PermissionState::Denied
        } else {
            PermissionState::NotDetermined
        }
    }

    pub fn request_screen() -> PermissionState {
        SCREEN_ASKED.store(true, Ordering::Relaxed);
        // SAFETY: shows the system prompt the first time; returns immediately.
        if unsafe { CGRequestScreenCaptureAccess() } {
            PermissionState::Granted
        } else {
            PermissionState::Denied
        }
    }

    fn media(kind: Kind) -> &'static objc2_av_foundation::AVMediaType {
        // SAFETY: the two constants are always non-null on supported macOS versions.
        unsafe {
            match kind {
                Kind::Camera => AVMediaTypeVideo.expect("AVMediaTypeVideo"),
                _ => AVMediaTypeAudio.expect("AVMediaTypeAudio"),
            }
        }
    }

    pub fn av_status(kind: Kind) -> PermissionState {
        // SAFETY: class method with a valid media type.
        let st = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media(kind)) };
        if st == AVAuthorizationStatus::Authorized {
            PermissionState::Granted
        } else if st == AVAuthorizationStatus::NotDetermined {
            PermissionState::NotDetermined
        } else {
            PermissionState::Denied
        }
    }

    /// Name of the device macOS currently uses by default. Does not prompt.
    pub fn default_device_name(kind: Kind) -> Option<String> {
        // SAFETY: class method with a valid media type; the name is copied out.
        unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media(kind)).map(|d| d.localizedName().to_string()) }
    }

    /// Blocks until the user answers the system dialog (up to two minutes).
    pub fn av_request(kind: Kind) -> PermissionState {
        if av_status(kind) != PermissionState::NotDetermined {
            return av_status(kind);
        }
        let (tx, rx) = mpsc::channel::<bool>();
        let block = RcBlock::new(move |granted: Bool| {
            let _ = tx.send(granted.as_bool());
        });
        // SAFETY: the block outlives the call (RcBlock is retained by the framework).
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media(kind), &block) };
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(true) => PermissionState::Granted,
            Ok(false) => PermissionState::Denied,
            Err(_) => av_status(kind),
        }
    }
}

pub fn check() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        PermissionStatus { screen: mac::screen(), camera: mac::av_status(Kind::Camera), microphone: mac::av_status(Kind::Microphone) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows has no screen-capture gate; the camera/microphone toggles are only visible to
        // the user in Settings → Privacy, so they are reported as not applicable.
        PermissionStatus { screen: PermissionState::NotApplicable, camera: PermissionState::NotApplicable, microphone: PermissionState::NotApplicable }
    }
}

/// The system's current default camera or microphone, by name, where the OS can tell us.
pub fn default_device_name(kind: Kind) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        mac::default_device_name(kind)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        None
    }
}

/// May block for a long time (a human answers a dialog): call from `spawn_blocking`.
pub fn request(kind: Kind) -> PermissionState {
    #[cfg(target_os = "macos")]
    {
        match kind {
            Kind::Screen => mac::request_screen(),
            _ => mac::av_request(kind),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        PermissionState::NotApplicable
    }
}

/// The URL that opens the right pane of the system settings.
pub fn settings_url(kind: Kind) -> &'static str {
    if cfg!(target_os = "macos") {
        match kind {
            Kind::Screen => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            Kind::Camera => "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera",
            Kind::Microphone => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        }
    } else {
        match kind {
            Kind::Screen => "ms-settings:privacy",
            Kind::Camera => "ms-settings:privacy-webcam",
            Kind::Microphone => "ms-settings:privacy-microphone",
        }
    }
}

pub fn open_settings(kind: Kind) -> std::io::Result<()> {
    let url = settings_url(kind);
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut c = std::process::Command::new("explorer");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut c = std::process::Command::new("xdg-open");
    c.arg(url).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_parse() {
        assert_eq!(Kind::parse("screen"), Some(Kind::Screen));
        assert_eq!(Kind::parse("camera"), Some(Kind::Camera));
        assert_eq!(Kind::parse("microphone"), Some(Kind::Microphone));
        assert_eq!(Kind::parse("nope"), None);
    }

    #[test]
    fn settings_urls_are_the_documented_ones() {
        if cfg!(target_os = "macos") {
            assert!(settings_url(Kind::Screen).ends_with("Privacy_ScreenCapture"));
            assert!(settings_url(Kind::Camera).ends_with("Privacy_Camera"));
            assert!(settings_url(Kind::Microphone).ends_with("Privacy_Microphone"));
        } else {
            assert_eq!(settings_url(Kind::Camera), "ms-settings:privacy-webcam");
            assert_eq!(settings_url(Kind::Microphone), "ms-settings:privacy-microphone");
        }
    }

    #[test]
    fn check_does_not_prompt_and_returns_three_states() {
        // Read-only queries: safe to run in CI and on a developer machine.
        let s = check();
        let _ = (s.screen, s.camera, s.microphone);
    }
}
