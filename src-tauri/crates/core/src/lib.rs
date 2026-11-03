//! recorder-core — the pure core of FlowGT Recorder.
//!
//! Everything in this crate is deterministic and testable without a GUI,
//! without ffmpeg installed and on any OS: functions take strings (ffmpeg
//! output), paths and plain data, and return commands, plans and parsed data.
//! The Tauri app crate (`src-tauri/src`) does the I/O: spawning processes,
//! querying the OS, talking to the webview.
//!
//! Module map (one owner per module during the build; see docs/ARCHITECTURE.md):
//! - [`types`]     shared data model (Settings, RecordingPlan, Device, Progress …) — THE contract
//! - [`budget`]    MB/hour → bitrates, maxrate/bufsize, size extrapolation
//! - [`settings`]  defaults, validation, migration of the persisted Settings
//! - [`filename`]  file-name template rendering and sanitisation
//! - [`disk`]      free-space arithmetic and warnings
//! - [`progress`]  parsing of `ffmpeg -progress pipe:1` key=value stream
//! - [`recovery`]  session manifest, segment bookkeeping, crash recovery decisions
//! - [`devices`]   parsing of ffmpeg device listings (avfoundation / dshow / ddagrab)
//! - [`encoders`]  parsing of `ffmpeg -encoders`, hardware encoder selection and fallback
//! - [`pip`]       picture-in-picture geometry → filter_complex fragment
//! - [`command`]   RecordingPlan → full ffmpeg argv (recording, concat, remux, compress, probe)

pub mod budget;
pub mod command;
pub mod devices;
pub mod disk;
pub mod encoders;
pub mod filename;
pub mod pip;
pub mod progress;
pub mod recovery;
pub mod settings;
pub mod types;

pub use types::*;

/// Crate-wide error type. Keep variants coarse; the UI maps them to i18n keys.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid settings: {0}")]
    InvalidSettings(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("no usable encoder: {0}")]
    NoEncoder(String),
    #[error("unsupported on this platform: {0}")]
    Unsupported(String),
    #[error("recovery: {0}")]
    Recovery(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
