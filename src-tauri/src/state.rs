//! Application state shared by commands, the tray and the shortcuts.

use crate::recorder::Recorder;
use crate::sidecar::Tools;
use recorder_core::{EncoderChoice, EncoderPreference};
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;

pub struct AppState {
    pub recorder: Recorder,
    pub tools: Mutex<Option<Tools>>,
    /// The encoder that passed its test frame, with the preference it was resolved for.
    pub encoder: Mutex<Option<(EncoderPreference, EncoderChoice)>>,
    pub last_meeting: StdMutex<String>,
    pub tray: StdMutex<Option<tauri::tray::TrayIcon>>,
}

impl AppState {
    pub fn new(recorder: Recorder) -> Self {
        AppState { recorder, tools: Mutex::new(None), encoder: Mutex::new(None), last_meeting: StdMutex::new(String::new()), tray: StdMutex::new(None) }
    }
}
