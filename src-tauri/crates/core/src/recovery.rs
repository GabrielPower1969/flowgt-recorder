//! Segments + manifest = pause/resume AND crash recovery with one mechanism.
//! Every segment is a fragmented MP4, so even the one being written survives kill -9.
use crate::types::*;
use std::path::{Path, PathBuf};

pub const MANIFEST_VERSION: u32 = 1;
/// Suffix kept for tooling that scans for manifests by extension.
pub const MANIFEST_SUFFIX: &str = ".manifest.json";
/// File name of the manifest inside a session's work dir.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Create a manifest for a new session. work_dir = <output_dir>/.flowgt-recorder/<session_id>/.
pub fn new_manifest(session_id: &str, meeting_name: &str, final_output: PathBuf, work_dir: PathBuf, plan: RecordingPlan, now: chrono::DateTime<chrono::Utc>) -> SessionManifest {
    SessionManifest {
        version: MANIFEST_VERSION,
        session_id: session_id.to_string(),
        meeting_name: meeting_name.to_string(),
        created_at: now,
        final_output,
        work_dir,
        segments: Vec::new(),
        status: SessionStatus::Recording,
        plan,
    }
}

/// Path of segment N inside work_dir: seg-000.mp4.
pub fn segment_path(work_dir: &Path, index: u32) -> PathBuf {
    work_dir.join(format!("seg-{index:03}.mp4"))
}

/// Append a new open segment and return its path.
pub fn open_segment(m: &mut SessionManifest, now: chrono::DateTime<chrono::Utc>) -> PathBuf {
    let index = m.segments.last().map_or(0, |s| s.index + 1);
    let path = segment_path(&m.work_dir, index);
    m.segments.push(Segment { index, path: path.clone(), started_at: now, ended_at: None, duration_ms: None, bytes: None });
    path
}

/// Close the last open segment with probed numbers.
pub fn close_segment(m: &mut SessionManifest, now: chrono::DateTime<chrono::Utc>, duration_ms: Option<u64>, bytes: Option<u64>) {
    if let Some(seg) = m.segments.last_mut().filter(|s| s.ended_at.is_none()) {
        seg.ended_at = Some(now);
        seg.duration_ms = duration_ms;
        seg.bytes = bytes;
    }
}

pub fn manifest_path(work_dir: &Path) -> PathBuf {
    work_dir.join(MANIFEST_FILE)
}

pub fn manifest_to_json(m: &SessionManifest) -> crate::Result<String> {
    Ok(serde_json::to_string_pretty(m)?)
}

pub fn manifest_from_json(json: &str) -> crate::Result<SessionManifest> {
    let m: SessionManifest = serde_json::from_str(json).map_err(|e| crate::CoreError::Recovery(format!("manifest is unreadable: {e}")))?;
    if m.version > MANIFEST_VERSION {
        return Err(crate::CoreError::Recovery(format!("manifest version {} is newer than supported {MANIFEST_VERSION}", m.version)));
    }
    Ok(m)
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
    if m.status == SessionStatus::Complete {
        return RecoveryAction::Discard;
    }
    let segments: Vec<PathBuf> = m.segments.iter().filter(|s| sizes(&s.path).unwrap_or(0) > 0).map(|s| s.path.clone()).collect();
    if segments.is_empty() {
        RecoveryAction::Discard
    } else {
        RecoveryAction::Concat { segments, final_output: m.final_output.clone() }
    }
}

/// The ffmpeg concat demuxer list file content ("file 'seg-000.mp4'\n…"), paths escaped.
pub fn concat_list(segments: &[PathBuf]) -> String {
    let mut out = String::new();
    for p in segments {
        // Forward slashes work on every platform; a single quote is closed, escaped and reopened.
        let s = p.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
        out.push_str("file '");
        out.push_str(&s);
        out.push_str("'\n");
    }
    out
}

/// Summary for the Settings → 恢复 list.
pub fn summarize(m: &SessionManifest, sizes: &dyn Fn(&Path) -> Option<u64>) -> RecoverableSession {
    let mut bytes = 0u64;
    let mut count = 0u32;
    let mut duration_ms = 0u64;
    for s in m.segments.iter().filter(|s| sizes(&s.path).unwrap_or(0) > 0) {
        let b = sizes(&s.path).unwrap_or(0);
        bytes += b;
        count += 1;
        duration_ms += s.duration_ms.unwrap_or_else(|| {
            // Unclosed segment: estimate from bytes at the planned bitrate.
            let bps = crate::budget::bytes_per_second(m.plan.budget.total_kbps).max(1);
            b * 1000 / bps
        });
    }
    RecoverableSession {
        session_id: m.session_id.clone(),
        meeting_name: m.meeting_name.clone(),
        created_at: m.created_at,
        segments: count,
        estimated_duration_ms: duration_ms,
        bytes,
        final_output: m.final_output.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::sample_plan;
    use chrono::{TimeZone, Utc};
    use pretty_assertions::assert_eq;

    fn manifest() -> SessionManifest {
        let t = Utc.with_ymd_and_hms(2026, 11, 4, 1, 30, 0).unwrap();
        new_manifest("abc", "周会", PathBuf::from("out/final.mp4"), PathBuf::from("out/.flowgt-recorder/abc"), sample_plan(), t)
    }

    #[test]
    fn segment_names_are_three_digits() {
        assert_eq!(segment_path(Path::new("w"), 0), PathBuf::from("w/seg-000.mp4"));
        assert_eq!(segment_path(Path::new("w"), 12), PathBuf::from("w/seg-012.mp4"));
    }

    #[test]
    fn open_and_close_segments() {
        let mut m = manifest();
        let t = Utc.with_ymd_and_hms(2026, 11, 4, 1, 31, 0).unwrap();
        let p0 = open_segment(&mut m, t);
        assert!(p0.ends_with("seg-000.mp4"));
        close_segment(&mut m, t, Some(1000), Some(5));
        let p1 = open_segment(&mut m, t);
        assert!(p1.ends_with("seg-001.mp4"));
        assert_eq!(m.segments[0].duration_ms, Some(1000));
        assert!(m.segments[0].ended_at.is_some());
        assert!(m.segments[1].ended_at.is_none());
        // Closing twice never touches an already-closed segment.
        close_segment(&mut m, t, Some(7), Some(7));
        close_segment(&mut m, t, Some(9), Some(9));
        assert_eq!(m.segments[0].duration_ms, Some(1000));
        assert_eq!(m.segments[1].duration_ms, Some(7));
    }

    #[test]
    fn manifest_round_trip_and_garbage() {
        let mut m = manifest();
        let t0 = m.created_at;
        open_segment(&mut m, t0);
        let json = manifest_to_json(&m).unwrap();
        assert_eq!(manifest_from_json(&json).unwrap(), m);
        assert!(manifest_from_json("{}").is_err());
        assert!(manifest_from_json("nope").is_err());
        assert_eq!(manifest_path(Path::new("w")), PathBuf::from("w/manifest.json"));
    }

    #[test]
    fn newer_manifest_version_is_refused() {
        let mut m = manifest();
        m.version = MANIFEST_VERSION + 1;
        let json = manifest_to_json(&m).unwrap();
        assert!(manifest_from_json(&json).is_err());
    }

    #[test]
    fn complete_sessions_are_discarded() {
        let mut m = manifest();
        let t0 = m.created_at;
        open_segment(&mut m, t0);
        m.status = SessionStatus::Complete;
        assert_eq!(decide(&m, &|_| Some(10)), RecoveryAction::Discard);
    }

    #[test]
    fn segments_with_bytes_are_concatenated_in_order() {
        let mut m = manifest();
        for _ in 0..3 {
            let t0 = m.created_at;
        open_segment(&mut m, t0);
        }
        let empty = m.segments[1].path.clone();
        let sizes = move |p: &Path| if p == empty { Some(0) } else { Some(100) };
        match decide(&m, &sizes) {
            RecoveryAction::Concat { segments, final_output } => {
                assert_eq!(segments, vec![m.segments[0].path.clone(), m.segments[2].path.clone()]);
                assert_eq!(final_output, PathBuf::from("out/final.mp4"));
            }
            other => panic!("expected concat, got {other:?}"),
        }
    }

    #[test]
    fn nothing_on_disk_means_discard() {
        let mut m = manifest();
        let t0 = m.created_at;
        open_segment(&mut m, t0);
        assert_eq!(decide(&m, &|_| None), RecoveryAction::Discard);
        assert_eq!(decide(&manifest(), &|_| Some(1)), RecoveryAction::Discard);
    }

    #[test]
    fn concat_list_escapes_quotes_and_backslashes() {
        let list = concat_list(&[PathBuf::from("a/seg-000.mp4"), PathBuf::from("it's/seg-001.mp4"), PathBuf::from("C:\\x\\seg-002.mp4")]);
        assert_eq!(list, "file 'a/seg-000.mp4'\nfile 'it'\\''s/seg-001.mp4'\nfile 'C:/x/seg-002.mp4'\n");
    }

    #[test]
    fn summary_uses_probed_duration_or_estimates_from_bytes() {
        let mut m = manifest();
        let t0 = m.created_at;
        open_segment(&mut m, t0);
        let t1 = m.created_at;
        close_segment(&mut m, t1, Some(60_000), Some(1000));
        let t0 = m.created_at;
        open_segment(&mut m, t0);
        // Open segment: 111_125 bytes at 889 kbps (111 125 B/s) ≈ 1000 ms.
        let first = m.segments[0].path.clone();
        let sizes = move |p: &Path| if p == first { Some(1000) } else { Some(111_125) };
        let s = summarize(&m, &sizes);
        assert_eq!(s.segments, 2);
        assert_eq!(s.bytes, 112_125);
        assert_eq!(s.estimated_duration_ms, 61_000);
        assert_eq!(s.meeting_name, "周会");
    }
}
