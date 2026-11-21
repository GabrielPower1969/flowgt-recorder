//! Menu-bar / system-tray icon: show window, start/stop, pause/resume, quit.

use recorder_core::{Phase, Settings};
use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

#[cfg(not(windows))]
mod glyph {
    //! macOS: a template image the menu bar tints; the red one while recording.
    const IDLE: &[u8] = include_bytes!("../icons/tray@2x.png");
    const REC: &[u8] = include_bytes!("../icons/tray-rec@2x.png");
    pub fn bytes(_app: &tauri::AppHandle, recording: bool) -> &'static [u8] {
        if recording {
            REC
        } else {
            IDLE
        }
    }
    pub const TEMPLATE: bool = true;
}

#[cfg(windows)]
mod glyph {
    //! Windows draws the icon as it is: white on a dark taskbar, black on a light one, red while
    //! recording, at the size the notification area uses for the display scaling (16 px at
    //! 100 %, 20 at 125 %, 24 at 150 %, 32 at 200 %).
    const SIZES: [u32; 4] = [16, 20, 24, 32];
    const DARK: [&[u8]; 4] = [
        include_bytes!("../icons/tray-dark-16.png"),
        include_bytes!("../icons/tray-dark-20.png"),
        include_bytes!("../icons/tray-dark-24.png"),
        include_bytes!("../icons/tray-dark-32.png"),
    ];
    const LIGHT: [&[u8]; 4] = [
        include_bytes!("../icons/tray-light-16.png"),
        include_bytes!("../icons/tray-light-20.png"),
        include_bytes!("../icons/tray-light-24.png"),
        include_bytes!("../icons/tray-light-32.png"),
    ];
    const REC: [&[u8]; 4] = [
        include_bytes!("../icons/tray-rec-16.png"),
        include_bytes!("../icons/tray-rec-20.png"),
        include_bytes!("../icons/tray-rec-24.png"),
        include_bytes!("../icons/tray-rec-32.png"),
    ];
    pub const TEMPLATE: bool = false;

    /// Index into SIZES for a display scale: the largest size not above 16 x scale.
    pub(super) fn size_index(scale: f64) -> usize {
        let want = (16.0 * scale).round() as u32;
        SIZES.iter().rposition(|&s| s <= want).unwrap_or(0)
    }

    /// "SystemUsesLightTheme" (Settings > Personalization > Colors, "Windows mode"); the taskbar
    /// follows it. Documented in Microsoft's Windows settings reference; dark when it is missing.
    fn light_taskbar() -> bool {
        #[link(name = "advapi32")]
        extern "system" {
            fn RegGetValueW(
                key: isize,
                sub: *const u16,
                name: *const u16,
                flags: u32,
                kind: *mut u32,
                data: *mut std::ffi::c_void,
                len: *mut u32,
            ) -> i32;
        }
        const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
        const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
        let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
        let sub = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let name = wide("SystemUsesLightTheme");
        let mut value: u32 = 0;
        let mut len = 4u32;
        // SAFETY: documented registry call reading one DWORD into a 4-byte buffer.
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                sub.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut len,
            )
        };
        rc == 0 && value == 1
    }

    pub fn bytes(app: &tauri::AppHandle, recording: bool) -> &'static [u8] {
        let scale = app
            .primary_monitor()
            .ok()
            .flatten()
            .map_or(1.0, |m| m.scale_factor());
        let i = size_index(scale);
        if recording {
            REC[i]
        } else if light_taskbar() {
            LIGHT[i]
        } else {
            DARK[i]
        }
    }
}

fn menu(app: &AppHandle, settings: Option<&Settings>) -> tauri::Result<Menu<tauri::Wry>> {
    // The shortcut beside each item is the global one from the settings, shown as a reminder.
    let accel = |name: &str| -> Option<String> {
        settings.and_then(|s| {
            crate::shortcuts::named(s)
                .into_iter()
                .find(|(n, a)| *n == name && !a.trim().is_empty())
                .map(|(_, a)| a.trim().to_string())
        })
    };
    let item = |id: &str, text: &str, key: Option<String>| {
        let b = MenuItemBuilder::with_id(id, text);
        match key {
            Some(k) => b.accelerator(k).build(app),
            None => b.build(app),
        }
    };
    let show = item("show", "Show FlowGT Recorder", accel("showWindow"))?;
    let toggle = item("toggle", "Start / stop recording", accel("toggle"))?;
    let pause = item("pause", "Pause / resume", accel("pause"))?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let mut b = MenuBuilder::new(app).items(&[&show, &toggle, &pause]);
    if crate::scroll::supported() {
        let scroll = item("scroll", "Scrolling screenshot", accel("scrollCapture"))?;
        b = b.item(&scroll);
    }
    b.separator().item(&quit).build()
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let settings = crate::commands::load_settings(app).ok();
    let menu = menu(app, settings.as_ref())?;
    let tray = TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(glyph::bytes(app, false))?)
        .icon_as_template(glyph::TEMPLATE)
        .tooltip("FlowGT Recorder")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let handle = app.clone();
            match event.id().as_ref() {
                "show" => crate::windows::show_main(&handle),
                "toggle" => {
                    tauri::async_runtime::spawn(async move {
                        crate::commands::toggle_recording(&handle).await
                    });
                }
                "pause" => {
                    tauri::async_runtime::spawn(async move {
                        crate::commands::toggle_pause(&handle).await
                    });
                }
                "scroll" => {
                    tauri::async_runtime::spawn(async move {
                        crate::commands::toggle_scroll_capture(&handle).await
                    });
                }
                "quit" => handle.exit(0),
                _ => {}
            }
        })
        .build(app)?;
    if let Some(state) = app.try_state::<crate::state::AppState>() {
        *state.tray.lock().expect("tray lock") = Some(tray);
    }
    Ok(())
}

/// Rebuild the menu after the shortcuts changed, so the reminders beside the items are right.
pub fn refresh_menu(app: &AppHandle, settings: &Settings) {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let guard = state.tray.lock().expect("tray lock");
    let Some(tray) = guard.as_ref() else { return };
    match menu(app, Some(settings)) {
        Ok(m) => {
            let _ = tray.set_menu(Some(m));
        }
        Err(e) => log::warn!("cannot rebuild the tray menu: {e}"),
    }
}

/// Swap the icon and tooltip for the phase (red dot while recording).
pub fn update(app: &AppHandle, phase: Phase) {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let guard = state.tray.lock().expect("tray lock");
    let Some(tray) = guard.as_ref() else { return };
    let recording = matches!(
        phase,
        Phase::Recording | Phase::Paused | Phase::Starting | Phase::Countdown | Phase::Finalizing
    );
    if let Ok(img) = Image::from_bytes(glyph::bytes(app, recording)) {
        let _ = tray.set_icon(Some(img));
        let _ = tray.set_icon_as_template(glyph::TEMPLATE && !recording);
    }
    let tip = match phase {
        Phase::Recording => "FlowGT Recorder — recording",
        Phase::Paused => "FlowGT Recorder — paused",
        _ => "FlowGT Recorder",
    };
    let _ = tray.set_tooltip(Some(tip));
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn tray_size_follows_the_scaling() {
        use super::glyph::size_index as i;
        assert_eq!(i(1.0), 0); // 16
        assert_eq!(i(1.25), 1); // 20
        assert_eq!(i(1.5), 2); // 24
        assert_eq!(i(1.75), 2); // 28 -> 24
        assert_eq!(i(2.0), 3); // 32
        assert_eq!(i(3.0), 3);
    }
}
