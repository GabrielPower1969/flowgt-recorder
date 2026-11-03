//! FlowGT Recorder — Tauri app crate. Owns all I/O: process supervision, OS queries, IPC.
//! Module map is in docs/ARCHITECTURE.md. Placeholder until the app agent fills it in.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
