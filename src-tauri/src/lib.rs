//! FlowGT Recorder — Tauri app crate. Owns all I/O: process supervision, OS queries, IPC.
//! Module map is in docs/ARCHITECTURE.md.

pub mod commands;
pub mod disk;
pub mod error;
pub mod permissions;
pub mod recorder;
pub mod selftest;
pub mod shortcuts;
pub mod sidecar;
pub mod state;
pub mod tray;
pub mod windows;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
            windows::create_floating(&handle)?;
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
