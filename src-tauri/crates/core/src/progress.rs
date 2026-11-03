//! Parse `ffmpeg -progress pipe:1 -nostats` output. One block per update, keys like
//! frame=123\nfps=30.0\nout_time_us=…\ntotal_size=…\ndrop_frames=…\nprogress=continue
use crate::types::Progress;

/// Incremental parser: feed chunks, get completed Progress blocks.
#[derive(Debug, Default)]
pub struct ProgressParser {
    buf: String,
    current: Progress,
}

impl ProgressParser {
    pub fn new() -> Self {
        Self::default()
    }
    /// Feed raw bytes/text; returns every block completed by this chunk (ending in `progress=…`).
    pub fn feed(&mut self, chunk: &str) -> Vec<Progress> {
        todo!()
    }
}

/// Parse a single complete block (used by tests and by feed).
pub fn parse_block(block: &str) -> Progress {
    todo!()
}

/// Parse ffmpeg stderr lines for events we care about: camera disappearing, device errors.
/// Returns an i18n warning code when a line matters, e.g. "warn.cameraLost".
pub fn classify_stderr_line(line: &str) -> Option<&'static str> {
    todo!()
}
