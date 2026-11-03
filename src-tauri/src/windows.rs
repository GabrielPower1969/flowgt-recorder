//! The `floating` window: countdown and the recording bar (D11). Created hidden at startup,
//! shown and resized as the recorder changes phase.

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

pub const FLOATING: &str = "floating";
pub const BAR_SIZE: (f64, f64) = (420.0, 72.0);
pub const COUNTDOWN_SIZE: (f64, f64) = (420.0, 200.0);

pub fn create_floating(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(FLOATING).is_some() {
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(app, FLOATING, WebviewUrl::App("index.html".into()))
        .title("FlowGT Recorder")
        .inner_size(BAR_SIZE.0, BAR_SIZE.1)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        .initialization_script("if (!location.hash || location.hash === '#/') location.hash = '#/floating';")
        .build()?;
    place_top_center(&win, BAR_SIZE.0);
    Ok(())
}

fn place_top_center(win: &tauri::WebviewWindow, width: f64) {
    if let Ok(Some(m)) = win.primary_monitor() {
        let scale = m.scale_factor();
        let screen_w = m.size().width as f64 / scale;
        let origin = m.position().x as f64 / scale;
        let _ = win.set_position(LogicalPosition::new(origin + (screen_w - width) / 2.0, m.position().y as f64 / scale + 48.0));
    }
}

/// Show the floating window in the countdown size or the bar size.
pub fn show_floating(app: &AppHandle, countdown: bool) {
    let Some(win) = app.get_webview_window(FLOATING) else { return };
    let (w, h) = if countdown { COUNTDOWN_SIZE } else { BAR_SIZE };
    let _ = win.set_size(LogicalSize::new(w, h));
    place_top_center(&win, w);
    let _ = win.set_always_on_top(true);
    let _ = win.show();
}

pub fn hide_floating(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(FLOATING) {
        let _ = win.hide();
    }
}

pub fn hide_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_the_design() {
        assert_eq!(BAR_SIZE, (420.0, 72.0));
        assert_eq!(COUNTDOWN_SIZE, (420.0, 200.0));
    }
}
