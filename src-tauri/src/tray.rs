//! Menu-bar / system-tray icon: show window, start/stop, pause/resume, quit.

use recorder_core::{Phase, Settings};
use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

const IDLE_ICON: &[u8] = include_bytes!("../icons/tray@2x.png");
const REC_ICON: &[u8] = include_bytes!("../icons/tray-rec@2x.png");

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
        .icon(Image::from_bytes(IDLE_ICON)?)
        .icon_as_template(true)
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
    let bytes = if recording { REC_ICON } else { IDLE_ICON };
    if let Ok(img) = Image::from_bytes(bytes) {
        let _ = tray.set_icon(Some(img));
        let _ = tray.set_icon_as_template(!recording);
    }
    let tip = match phase {
        Phase::Recording => "FlowGT Recorder — recording",
        Phase::Paused => "FlowGT Recorder — paused",
        _ => "FlowGT Recorder",
    };
    let _ = tray.set_tooltip(Some(tip));
}
