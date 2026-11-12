//! Start at login. The system keeps the record (the user can switch it off in the system's own
//! settings), so nothing is stored here: every question is put to the system.
//!
//! macOS: `SMAppService.mainApp` (macOS 13+), the one way that also works inside the App
//! Sandbox. Other platforms report `autostart: false` until they have their own.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutostartStatus {
    On,
    Off,
    /// Registered, but the user has to allow it in the system's settings.
    NeedsApproval,
    Unsupported,
}

pub fn supported() -> bool {
    platform::supported()
}

pub fn status() -> AutostartStatus {
    platform::status()
}

/// Switch it on or off; the answer is what the system says afterwards.
pub fn set(enabled: bool) -> Result<AutostartStatus, String> {
    platform::set(enabled)?;
    Ok(platform::status())
}

/// Open the system's list of login items.
pub fn open_settings() {
    platform::open_settings();
}

/// Whether the system started the app at login (as opposed to the user opening it). Only
/// meaningful while the app is starting.
pub fn launched_at_login() -> bool {
    platform::launched_at_login()
}

/// From the system's status number (SMAppServiceStatus).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn status_from_code(code: isize) -> AutostartStatus {
    match code {
        1 => AutostartStatus::On,
        2 => AutostartStatus::NeedsApproval,
        _ => AutostartStatus::Off, // 0 not registered, 3 not found
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::{msg_send, sel};

    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    fn service() -> Option<&'static AnyObject> {
        let class = AnyClass::get(c"SMAppService")?;
        // SAFETY: `mainAppService` is a documented class property returning a shared object.
        let s: *mut AnyObject = unsafe { msg_send![class, mainAppService] };
        // SAFETY: the shared service object lives as long as the process.
        unsafe { s.as_ref() }
    }

    pub fn supported() -> bool {
        service().is_some()
    }

    pub fn status() -> AutostartStatus {
        let Some(s) = service() else {
            return AutostartStatus::Unsupported;
        };
        // SAFETY: `status` is a documented property returning an NSInteger enum.
        let code: isize = unsafe { msg_send![s, status] };
        status_from_code(code)
    }

    pub fn set(enabled: bool) -> Result<(), String> {
        let s = service().ok_or("start at login is not available on this system")?;
        let mut error: *mut AnyObject = std::ptr::null_mut();
        // SAFETY: both selectors take an NSError** and return BOOL; the error is autoreleased.
        let ok: bool = unsafe {
            if enabled {
                msg_send![s, registerAndReturnError: &mut error]
            } else {
                msg_send![s, unregisterAndReturnError: &mut error]
            }
        };
        if ok {
            return Ok(());
        }
        // Switching off something that is not registered is not a failure worth telling.
        if !enabled && status() == AutostartStatus::Off {
            return Ok(());
        }
        // Registering can "fail" with the item waiting for the user's approval.
        if enabled && status() == AutostartStatus::NeedsApproval {
            return Ok(());
        }
        // SAFETY: a non-nil NSError answers localizedDescription with an NSString.
        let detail = unsafe {
            error.as_ref().map(|e| {
                let text: *mut objc2_foundation::NSString = msg_send![e, localizedDescription];
                text.as_ref().map(|t| t.to_string()).unwrap_or_default()
            })
        };
        Err(detail.unwrap_or_else(|| "the system refused".to_string()))
    }

    pub fn open_settings() {
        if let Some(class) = AnyClass::get(c"SMAppService") {
            // SAFETY: a documented class method without arguments.
            let _: () = unsafe { msg_send![class, openSystemSettingsLoginItems] };
        }
    }

    /// The Apple event that launched the app says so: its `keyAEPropData` is
    /// `keyAELaunchedAsLogInItem`.
    pub fn launched_at_login() -> bool {
        const KEY_AE_PROP_DATA: u32 = u32::from_be_bytes(*b"prdt");
        const LAUNCHED_AS_LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");
        let Some(class) = AnyClass::get(c"NSAppleEventManager") else {
            return false;
        };
        // SAFETY: documented Foundation calls; every result is checked for nil.
        unsafe {
            let manager: *mut AnyObject = msg_send![class, sharedAppleEventManager];
            let Some(manager) = manager.as_ref() else {
                return false;
            };
            let event: *mut AnyObject = msg_send![manager, currentAppleEvent];
            let Some(event) = event.as_ref() else {
                return false;
            };
            if !event.class().responds_to(sel!(paramDescriptorForKeyword:)) {
                return false;
            }
            let param: *mut AnyObject =
                msg_send![event, paramDescriptorForKeyword: KEY_AE_PROP_DATA];
            let Some(param) = param.as_ref() else {
                return false;
            };
            let code: u32 = msg_send![param, enumCodeValue];
            code == LAUNCHED_AS_LOGIN_ITEM
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    //! Not built yet. Windows goes here: the per-user Run entry for the plain installer, the
    //! package's startup task under MSIX.
    use super::*;

    pub fn supported() -> bool {
        false
    }
    pub fn status() -> AutostartStatus {
        AutostartStatus::Unsupported
    }
    pub fn set(_enabled: bool) -> Result<(), String> {
        Err("start at login is not available on this system".to_string())
    }
    pub fn open_settings() {}
    pub fn launched_at_login() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_status_numbers() {
        assert_eq!(status_from_code(0), AutostartStatus::Off);
        assert_eq!(status_from_code(1), AutostartStatus::On);
        assert_eq!(status_from_code(2), AutostartStatus::NeedsApproval);
        assert_eq!(status_from_code(3), AutostartStatus::Off);
    }

    #[test]
    fn asking_never_panics() {
        // Under `cargo test` there is no app bundle; the answer only has to be one of the four.
        let _ = status();
        let _ = supported();
        assert!(!launched_at_login());
    }
}
