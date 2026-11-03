//! Menu-bar / system-tray icon: show window, start/stop, pause/resume, quit.

use recorder_core::Phase;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

const IDLE_ICON: &[u8] = include_bytes!("../icons/tray@2x.png");
const REC_ICON: &[u8] = include_bytes!("../icons/tray-rec@2x.png");

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "Show FlowGT Recorder").build(app)?;
    let toggle = MenuItemBuilder::with_id("toggle", "Start / stop recording").build(app)?;
    let pause = MenuItemBuilder::with_id("pause", "Pause / resume").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &toggle, &pause]).separator().item(&quit).build()?;
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
                    tauri::async_runtime::spawn(async move { crate::commands::toggle_recording(&handle).await });
                }
                "pause" => {
                    tauri::async_runtime::spawn(async move { crate::commands::toggle_pause(&handle).await });
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

/// Swap the icon and tooltip for the phase (red dot while recording).
pub fn update(app: &AppHandle, phase: Phase) {
    let Some(state) = app.try_state::<crate::state::AppState>() else { return };
    let guard = state.tray.lock().expect("tray lock");
    let Some(tray) = guard.as_ref() else { return };
    let recording = matches!(phase, Phase::Recording | Phase::Paused | Phase::Starting | Phase::Countdown | Phase::Finalizing);
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
