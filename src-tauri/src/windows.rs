//! The `floating` window: countdown and the recording bar (D11). Created hidden at startup,
//! shown and resized as the recorder changes phase.

use std::sync::Mutex;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

pub const FLOATING: &str = "floating";
pub const BAR_SIZE: (f64, f64) = (420.0, 72.0);
/// Bar plus the live preview (16:9 at 420 wide) and its caption.
pub const PANEL_SIZE: (f64, f64) = (420.0, 356.0);
pub const COUNTDOWN_SIZE: (f64, f64) = (420.0, 200.0);
pub const MARKER: &str = "marker";

/// The screen chosen in the picker, as its position in the screen list.
static CHOSEN_SCREEN: Mutex<Option<usize>> = Mutex::new(None);
/// Whether the recording window shows the live preview under the bar. Remembered for the
/// process: collapsing it once keeps it collapsed for the next recording too.
static EXPANDED: Mutex<bool> = Mutex::new(true);

pub fn floating_expanded() -> bool {
    *EXPANDED.lock().expect("expanded lock")
}

/// Logical rectangle (x, y, w, h) of the nth screen, in the order the capture tool lists them.
#[cfg(target_os = "macos")]
fn screen_rect(_app: &AppHandle, index: usize) -> Option<(f64, f64, f64, f64)> {
    #[repr(C)]
    struct Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayBounds(display: u32) -> Rect;
    }
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    // SAFETY: the buffer holds `max` entries; avfoundation numbers its screens from this same list.
    let ok = unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) } == 0;
    if !ok || index >= count as usize {
        return None;
    }
    // SAFETY: the id just came from the active display list.
    let r = unsafe { CGDisplayBounds(ids[index]) };
    Some((r.x, r.y, r.w, r.h))
}

/// CGDirectDisplayID of the nth screen in the capture tool's order, and its size in pixels.
#[cfg(target_os = "macos")]
pub fn display_at(index: usize) -> Option<(u32, u32, u32)> {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayCopyDisplayMode(display: u32) -> *mut std::ffi::c_void;
        fn CGDisplayModeGetPixelWidth(mode: *mut std::ffi::c_void) -> usize;
        fn CGDisplayModeGetPixelHeight(mode: *mut std::ffi::c_void) -> usize;
        fn CGDisplayModeRelease(mode: *mut std::ffi::c_void);
    }
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    // SAFETY: the buffer holds `max` entries.
    if unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) } != 0
        || index >= count as usize
    {
        return None;
    }
    let id = ids[index];
    // SAFETY: the id is from the active list; the mode is released after reading two numbers.
    unsafe {
        let mode = CGDisplayCopyDisplayMode(id);
        if mode.is_null() {
            return None;
        }
        let (w, h) = (
            CGDisplayModeGetPixelWidth(mode),
            CGDisplayModeGetPixelHeight(mode),
        );
        CGDisplayModeRelease(mode);
        Some((id, w as u32, h as u32))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn display_at(_index: usize) -> Option<(u32, u32, u32)> {
    None
}

#[cfg(not(target_os = "macos"))]
fn screen_rect(app: &AppHandle, index: usize) -> Option<(f64, f64, f64, f64)> {
    let monitors = app.available_monitors().ok()?;
    let m = monitors.get(index)?;
    let s = m.scale_factor();
    Some((
        m.position().x as f64 / s,
        m.position().y as f64 / s,
        m.size().width as f64 / s,
        m.size().height as f64 / s,
    ))
}

/// Outline the screen that is about to be recorded. It must be gone before capture starts:
/// a window hidden from sharing still shows up in avfoundation's capture (tested, D23).
pub fn show_marker(app: &AppHandle, index: usize) -> tauri::Result<()> {
    *CHOSEN_SCREEN.lock().expect("screen lock") = Some(index);
    let Some((x, y, w, h)) = screen_rect(app, index) else {
        return Ok(());
    };
    let n = index + 1;
    let win = match app.get_webview_window(MARKER) {
        Some(win) => {
            let _ = win.eval(format!("window.__setMarker && window.__setMarker({n})"));
            win
        }
        None => {
            let win = WebviewWindowBuilder::new(app, MARKER, WebviewUrl::App("index.html".into()))
                .title("FlowGT Recorder")
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .resizable(false)
                .skip_taskbar(true)
                .shadow(false)
                .focused(false)
                .visible(false)
                .initialization_script(format!(
                    "window.__markerN = {n}; location.hash = '#/marker';"
                ))
                .build()?;
            let _ = win.set_ignore_cursor_events(true);
            win
        }
    };
    // Size first: macOS keeps the bottom-left corner when resizing, which would undo the position.
    let _ = win.set_size(LogicalSize::new(w, h));
    let _ = win.set_position(LogicalPosition::new(x, y));
    raise_above_menu_bar(&win);
    let _ = win.show();
    Ok(())
}

#[cfg(target_os = "macos")]
fn raise_above_menu_bar(win: &tauri::WebviewWindow) {
    use objc2::{msg_send, runtime::AnyObject};
    const SCREEN_SAVER_LEVEL: isize = 1000;
    if let Ok(ns) = win.ns_window() {
        // SAFETY: a live NSWindow pointer from Tauri; setLevel: takes an NSInteger.
        unsafe {
            let w = ns.cast::<AnyObject>();
            let _: () = msg_send![&*w, setLevel: SCREEN_SAVER_LEVEL];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn raise_above_menu_bar(_win: &tauri::WebviewWindow) {}

pub fn hide_marker(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(MARKER) {
        let _ = win.hide();
    }
}

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
        .initialization_script(
            "if (!location.hash || location.hash === '#/') location.hash = '#/floating';",
        )
        .build()?;
    place_top_center(&win, BAR_SIZE.0);
    Ok(())
}

fn place_top_center(win: &tauri::WebviewWindow, width: f64) {
    // On the screen being recorded, when the picker told us which one that is.
    let chosen = *CHOSEN_SCREEN.lock().expect("screen lock");
    if let Some((x, y, w, _)) = chosen.and_then(|i| screen_rect(win.app_handle(), i)) {
        let _ = win.set_position(LogicalPosition::new(x + (w - width) / 2.0, y + 48.0));
        return;
    }
    if let Ok(Some(m)) = win.primary_monitor() {
        let scale = m.scale_factor();
        let screen_w = m.size().width as f64 / scale;
        let origin = m.position().x as f64 / scale;
        let _ = win.set_position(LogicalPosition::new(
            origin + (screen_w - width) / 2.0,
            m.position().y as f64 / scale + 48.0,
        ));
    }
}

/// Show the floating window in the countdown size, or as the recording panel or bar.
pub fn show_floating(app: &AppHandle, countdown: bool) {
    let Some(win) = app.get_webview_window(FLOATING) else {
        return;
    };
    let (w, h) = if countdown {
        COUNTDOWN_SIZE
    } else if floating_expanded() {
        PANEL_SIZE
    } else {
        BAR_SIZE
    };
    let _ = win.set_size(LogicalSize::new(w, h));
    place_top_center(&win, w);
    let _ = win.set_always_on_top(true);
    let _ = win.show();
}

/// Grow the recording window into the panel or shrink it to the bar, keeping its top edge
/// where it is (macOS resizes from the bottom-left corner).
pub fn set_floating_expanded(app: &AppHandle, expanded: bool) {
    *EXPANDED.lock().expect("expanded lock") = expanded;
    let Some(win) = app.get_webview_window(FLOATING) else {
        return;
    };
    let (w, h) = if expanded { PANEL_SIZE } else { BAR_SIZE };
    let scale = win.scale_factor().unwrap_or(1.0);
    let top_left = win
        .outer_position()
        .ok()
        .map(|p| LogicalPosition::new(p.x as f64 / scale, p.y as f64 / scale));
    let _ = win.set_size(LogicalSize::new(w, h));
    if let Some(p) = top_left {
        let _ = win.set_position(p);
    }
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
        assert_eq!(PANEL_SIZE.0, BAR_SIZE.0);
        assert!(PANEL_SIZE.1 > BAR_SIZE.1 + 236.0);
    }
}
