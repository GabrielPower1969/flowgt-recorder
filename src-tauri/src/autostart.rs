//! Start at login. The system keeps the record (the user can switch it off in the system's own
//! settings), so nothing is stored here: every question is put to the system.
//!
//! macOS: `SMAppService.mainApp` (macOS 13+), the one way that also works inside the App
//! Sandbox. Windows: the package's startup task under MSIX, the per-user Run value otherwise.
//! Other platforms report `autostart: false`.

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

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    //! Not available on this system.
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

#[cfg(windows)]
mod platform {
    //! Windows has two ways, picked by whether the app runs with package identity (a capability,
    //! not a platform check):
    //! - packaged (MSIX): the package's startup task (`uap5:StartupTask` in the manifest,
    //!   `Windows.ApplicationModel.StartupTask`). A packaged app cannot use the Run key: its
    //!   writes under HKCU go to a private copy the system does not read at login.
    //! - plain installer: a value under HKCU\Software\Microsoft\Windows\CurrentVersion\Run.
    //!   Task Manager's "Disabled" for it has no documented API, so that state cannot be seen.
    use super::*;
    use std::sync::OnceLock;

    /// `TaskId` of the startup task in `src-tauri/msix/AppxManifest.xml`.
    const TASK_ID: &str = "FlowGTRecorderStartup";
    /// Name of the Run value, and the argument it passes.
    const RUN_VALUE: &str = "FlowGT Recorder";
    pub const LOGIN_ARG: &str = "--autostart";

    fn packaged() -> bool {
        crate::commands::package_identity().is_some()
    }

    pub fn supported() -> bool {
        true
    }

    pub fn status() -> AutostartStatus {
        if packaged() {
            task::state().map_or(AutostartStatus::Unsupported, from_task_state)
        } else if run::get().is_some_and(|cmd| cmd == run::command()) {
            AutostartStatus::On
        } else {
            AutostartStatus::Off
        }
    }

    pub fn set(enabled: bool) -> Result<(), String> {
        if packaged() {
            task::set(enabled)
        } else if enabled {
            run::put(&run::command())
        } else {
            run::delete()
        }
    }

    /// Settings > Apps > Startup lists both kinds and is where a user re-enables a task.
    pub fn open_settings() {
        if let Err(e) = tauri_plugin_opener::open_url("ms-settings:startupapps", None::<&str>) {
            log::warn!("cannot open the startup settings: {e}");
        }
    }

    /// Packaged: the activation says "startup task" (the system answers this only once, so the
    /// answer is kept). Plain: the Run value passes `--autostart`.
    pub fn launched_at_login() -> bool {
        static AT_LOGIN: OnceLock<bool> = OnceLock::new();
        *AT_LOGIN.get_or_init(|| {
            if packaged() {
                task::activated_by_startup_task()
            } else {
                std::env::args().any(|a| a == LOGIN_ARG)
            }
        })
    }

    /// StartupTaskState: 0 Disabled, 1 DisabledByUser (only the user can turn it back on),
    /// 2 Enabled, 3 DisabledByPolicy, 4 EnabledByPolicy.
    pub(super) fn from_task_state(state: i32) -> AutostartStatus {
        match state {
            2 | 4 => AutostartStatus::On,
            1 => AutostartStatus::NeedsApproval,
            3 => AutostartStatus::Unsupported,
            _ => AutostartStatus::Off,
        }
    }

    mod task {
        use windows::ApplicationModel::Activation::ActivationKind;
        use windows::ApplicationModel::{AppInstance, StartupTask, StartupTaskState};
        use windows_core::HSTRING;

        fn get() -> Result<StartupTask, String> {
            StartupTask::GetAsync(&HSTRING::from(super::TASK_ID))
                .and_then(|op| op.join())
                .map_err(|e| format!("startup task {}: {e}", super::TASK_ID))
        }

        pub fn state() -> Option<i32> {
            match get().and_then(|t| t.State().map_err(|e| e.to_string())) {
                Ok(s) => Some(s.0),
                Err(e) => {
                    log::warn!("{e}");
                    None
                }
            }
        }

        /// A packaged desktop app gets no consent dialog from RequestEnableAsync; a task the user
        /// disabled stays disabled whatever the app asks.
        pub fn set(enabled: bool) -> Result<(), String> {
            let t = get()?;
            if enabled {
                let s = t
                    .RequestEnableAsync()
                    .and_then(|op| op.join())
                    .map_err(|e| e.to_string())?;
                if s == StartupTaskState::DisabledByUser || s == StartupTaskState::DisabledByPolicy
                {
                    log::info!("startup task stays off: state {}", s.0);
                }
                Ok(())
            } else {
                t.Disable().map_err(|e| e.to_string())
            }
        }

        pub fn activated_by_startup_task() -> bool {
            AppInstance::GetActivatedEventArgs()
                .and_then(|a| a.Kind())
                .is_ok_and(|k| k == ActivationKind::StartupTask)
        }
    }

    mod run {
        use std::ffi::c_void;

        #[link(name = "advapi32")]
        extern "system" {
            fn RegSetKeyValueW(
                key: isize,
                sub: *const u16,
                name: *const u16,
                kind: u32,
                data: *const c_void,
                len: u32,
            ) -> i32;
            fn RegGetValueW(
                key: isize,
                sub: *const u16,
                name: *const u16,
                flags: u32,
                kind: *mut u32,
                data: *mut c_void,
                len: *mut u32,
            ) -> i32;
            fn RegDeleteKeyValueW(key: isize, sub: *const u16, name: *const u16) -> i32;
        }
        const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
        const REG_SZ: u32 = 1;
        const RRF_RT_REG_SZ: u32 = 0x0000_0002;
        const ERROR_FILE_NOT_FOUND: i32 = 2;
        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }

        /// The command line the Run value should hold for this program.
        pub fn command() -> String {
            let exe = std::env::current_exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("\"{exe}\" {}", super::LOGIN_ARG)
        }

        pub fn get() -> Option<String> {
            let (sub, name) = (wide(SUBKEY), wide(super::RUN_VALUE));
            let mut buf = vec![0u16; 1024];
            let mut len = (buf.len() * 2) as u32;
            // SAFETY: documented registry call; the buffer length is passed in bytes.
            let rc = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    sub.as_ptr(),
                    name.as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut len,
                )
            };
            if rc != 0 {
                return None;
            }
            let chars = (len as usize / 2).saturating_sub(1);
            Some(String::from_utf16_lossy(&buf[..chars.min(buf.len())]))
        }

        pub fn put(cmd: &str) -> Result<(), String> {
            let (sub, name, data) = (wide(SUBKEY), wide(super::RUN_VALUE), wide(cmd));
            // SAFETY: documented registry call; `data` is a NUL-terminated UTF-16 string.
            let rc = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    sub.as_ptr(),
                    name.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
            if rc == 0 {
                Ok(())
            } else {
                Err(format!("cannot write the Run value: error {rc}"))
            }
        }

        pub fn delete() -> Result<(), String> {
            let (sub, name) = (wide(SUBKEY), wide(super::RUN_VALUE));
            // SAFETY: documented registry call.
            let rc = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, sub.as_ptr(), name.as_ptr()) };
            if rc == 0 || rc == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(format!("cannot remove the Run value: error {rc}"))
            }
        }
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
    #[cfg(windows)]
    fn startup_task_states() {
        use platform::from_task_state as s;
        assert_eq!(s(0), AutostartStatus::Off);
        assert_eq!(s(1), AutostartStatus::NeedsApproval);
        assert_eq!(s(2), AutostartStatus::On);
        assert_eq!(s(3), AutostartStatus::Unsupported);
        assert_eq!(s(4), AutostartStatus::On);
    }

    #[test]
    fn asking_never_panics() {
        // Under `cargo test` there is no app bundle; the answer only has to be one of the four.
        let _ = status();
        let _ = supported();
        assert!(!launched_at_login());
    }
}
