//! Segments + manifest = pause/resume AND crash recovery with one mechanism.
//! Every segment is a fragmented MP4, so even the one being written survives kill -9.
use crate::types::*;
use std::path::{Path, PathBuf};

pub const MANIFEST_VERSION: u32 = 1;
pub const MANIFEST_SUFFIX: &str = ".manifest.json";

/// Create a manifest for a new session. work_dir = <output_dir>/.flowgt-recorder/<session_id>/.
pub fn new_manifest(session_id: &str, meeting_name: &str, final_output: PathBuf, work_dir: PathBuf, plan: RecordingPlan, now: chrono::DateTime<chrono::Utc>) -> SessionManifest {
    todo!()
}

/// Path of segment N inside work_dir: seg-000.mp4.
pub fn segment_path(work_dir: &Path, index: u32) -> PathBuf {
    todo!()
}

/// Append a new open segment and return its path.
pub fn open_segment(m: &mut SessionManifest, now: chrono::DateTime<chrono::Utc>) -> PathBuf {
    todo!()
}

/// Close the last open segment with probed numbers.
pub fn close_segment(m: &mut SessionManifest, now: chrono::DateTime<chrono::Utc>, duration_ms: Option<u64>, bytes: Option<u64>) {
    todo!()
}

pub fn manifest_path(work_dir: &Path) -> PathBuf {
    todo!()
}

pub fn manifest_to_json(m: &SessionManifest) -> crate::Result<String> {
    todo!()
}

pub fn manifest_from_json(json: &str) -> crate::Result<SessionManifest> {
    todo!()
}

/// What to do with a manifest found at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Nothing to recover (complete, or no segment has bytes) → delete work_dir.
    Discard,
    /// Segments exist → remux/concat them into final_output.
    Concat { segments: Vec<PathBuf>, final_output: PathBuf },
}

/// Decide, given the manifest and the real on-disk sizes of its segments (path → bytes, missing = absent).
pub fn decide(m: &SessionManifest, sizes: &dyn Fn(&Path) -> Option<u64>) -> RecoveryAction {
    todo!()
}

/// The ffmpeg concat demuxer list file content ("file 'seg-000.mp4'\n…"), paths escaped.
pub fn concat_list(segments: &[PathBuf]) -> String {
    todo!()
}

/// Summary for the Settings → 恢复 list.
pub fn summarize(m: &SessionManifest, sizes: &dyn Fn(&Path) -> Option<u64>) -> RecoverableSession {
    todo!()
}
