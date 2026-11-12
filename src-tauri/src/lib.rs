//! FlowGT Recorder — Tauri app crate. Owns all I/O: process supervision, OS queries, IPC.
//! Module map is in docs/ARCHITECTURE.md.

pub mod bookmark;
pub mod commands;
pub mod disk;
pub mod drive;
pub mod error;
pub mod permissions;
pub mod recorder;
pub mod selftest;
pub mod shortcuts;
pub mod sidecar;
pub mod state;
pub mod tray;
pub mod webview_permissions;
pub mod windows;

use std::sync::Arc;
use tauri::Manager;

/// WebView2 opens cameras through Media Foundation by default. On Windows a camera belongs to one
/// client at a time, and after the self-view stops, that path kept the device for about 100 s on
/// the test machine: ffmpeg (dshow) then failed with "Could not run graph" and the recording
/// started without the camera. With DirectShow capture in the webview the device is free at once
/// (measured: ffmpeg opens it within 1.2 s). Must run before the first webview is created; the
/// setting is merged with the arguments Tauri passes itself.
#[cfg(windows)]
fn use_directshow_camera_in_webview() {
    const KEY: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
    const FEATURE: &str = "MediaFoundationVideoCapture";
    let current = std::env::var(KEY).unwrap_or_default();
    if !current.contains(FEATURE) {
        std::env::set_var(KEY, format!("{current} --disable-features={FEATURE}").trim());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    use_directshow_camera_in_webview();
    // `--selftest N`: headless end-to-end check with synthetic inputs; never opens a window.
    if let Some(code) = selftest::maybe_run() {
        std::process::exit(code);
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(shortcuts::plugin())
        .setup(|app| {
            let handle = app.handle().clone();
            let sink = Arc::new(commands::TauriSink::new(handle.clone()));
            app.manage(state::AppState::new(recorder::Recorder::new(sink)));
            if let Some(name) = commands::package_identity() {
                log::info!("package identity: {name}");
            }
            if let Ok(dir) = app.path().app_data_dir() {
                if let Some(folder) = bookmark::restore(&dir) {
                    log::info!("save folder access restored: {}", folder.display());
                }
            }
            windows::create_floating(&handle)?;
            #[cfg(windows)]
            if let Some(main) = app.get_webview_window("main") {
                webview_permissions::allow_own_media(&main);
            }
            tray::build(&handle)?;
            match commands::load_settings(&handle) {
                Ok(s) => shortcuts::register(&handle, &s),
                Err(e) => log::warn!("settings unavailable at startup: {e}"),
            }
            Ok(())
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
