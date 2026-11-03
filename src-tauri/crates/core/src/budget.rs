//! Bitrate budget. The whole product promise lives here:
//!   400 MB × 8 ÷ 3600 s ≈ 0.89 Mbps total.
use crate::types::*;

/// Compute the budget for a target size. `audio_tracks` ∈ {1,2}.
/// video_kbps = total − audio; maxrate = 1.5 × video; bufsize = 2 × video (CBR-ish cap, VBR below).
pub fn budget_for(target_mb_per_hour: u32, audio_kbps_per_track: u32, audio_tracks: u32) -> BitrateBudget {
    todo!()
}

/// Extrapolate a measured sample to one hour: bytes / seconds × 3600 / 1e6 (MB, decimal).
pub fn extrapolate_mb_per_hour(bytes: u64, duration_ms: u64) -> f64 {
    todo!()
}

/// bytes ÷ (duration_h × target_mb × 1e6) − 1. Positive = over budget.
pub fn over_budget_ratio(bytes: u64, duration_ms: u64, target_mb_per_hour: u32) -> f64 {
    todo!()
}

/// Estimated total bytes at `elapsed_ms` for the floating bar.
pub fn estimate_bytes(budget: &BitrateBudget, elapsed_ms: u64) -> u64 {
    todo!()
}
