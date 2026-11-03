//! The error type that crosses the IPC boundary.
//!
//! `code` is an i18n key the webview resolves (`error.noFfmpeg`, `error.noEncoder`,
//! `error.diskFull`, `error.permission`, `error.ffmpegExit`, `error.busy`, …);
//! `detail` is a human-readable, English, developer-facing explanation.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub detail: String,
}

pub type CmdResult<T> = Result<T, CommandError>;

impl CommandError {
    pub fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
        }
    }

    /// The recorder is in a phase that does not allow the requested transition.
    pub fn busy(detail: impl Into<String>) -> Self {
        Self::new("error.busy", detail)
    }

    pub fn io(detail: impl Into<String>) -> Self {
        Self::new("error.io", detail)
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for CommandError {}

impl From<recorder_core::CoreError> for CommandError {
    fn from(e: recorder_core::CoreError) -> Self {
        use recorder_core::CoreError as E;
        let code = match &e {
            E::InvalidSettings(_) => "error.invalidSettings",
            E::Parse(_) => "error.parse",
            E::NoEncoder(_) => "error.noEncoder",
            E::Unsupported(_) => "error.unsupported",
            E::Recovery(_) => "error.recovery",
            E::Io(_) => "error.io",
            E::Json(_) => "error.json",
        };
        Self::new(code, e.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        Self::io(e.to_string())
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("error.json", e.to_string())
    }
}

impl From<tauri::Error> for CommandError {
    fn from(e: tauri::Error) -> Self {
        Self::new("error.tauri", e.to_string())
    }
}

impl From<tauri_plugin_store::Error> for CommandError {
    fn from(e: tauri_plugin_store::Error) -> Self {
        Self::io(format!("settings store: {e}"))
    }
}
