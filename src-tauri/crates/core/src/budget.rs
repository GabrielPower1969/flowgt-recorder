//! Bitrate budget. The whole product promise lives here:
//!   400 MB × 8 ÷ 3600 s ≈ 0.89 Mbps total.
//!
//! Units: a megabyte is decimal (1 MB = 1 000 000 bytes, what Finder and Explorer show for a
//! video file), a kbps is 1 000 bits per second, so one kbps carries 125 bytes per second.
//! Rates are whole kbps, halves rounded up, which is what ffmpeg's `-b:v 793k` takes.
use crate::types::*;

/// Bytes in one (decimal) megabyte.
pub const MB: f64 = 1_000_000.0;
/// Bytes carried per second by one kbps.
pub const BYTES_PER_KBPS_SECOND: u64 = 125;
/// Seconds in the hour the budget is quoted for.
pub const SECONDS_PER_HOUR: u64 = 3_600;
/// The video track never drops below this, however small the budget or large the audio share;
/// below it a 1080p screen is unreadable, so the hour is allowed to run over instead.
pub const MIN_VIDEO_KBPS: u32 = 200;

/// Compute the budget for a target size. `audio_tracks` ∈ {1,2}.
/// video_kbps = total − audio; maxrate = 1.5 × video; bufsize = 2 × video (CBR-ish cap, VBR below).
///
/// `total_kbps` is what the file really carries (video + audio). It equals the figure derived
/// from the target unless the video floor kicked in; then `expected_mb_per_hour` shows the
/// overrun honestly instead of hiding it, and disk/recovery estimates stay truthful.
pub fn budget_for(target_mb_per_hour: u32, audio_kbps_per_track: u32, audio_tracks: u32) -> BitrateBudget {
    let target_total_kbps = kbps_for_mb_per_hour(target_mb_per_hour);
    let audio_kbps_total = audio_kbps_per_track.saturating_mul(audio_tracks);
    let video_kbps = target_total_kbps.saturating_sub(audio_kbps_total).max(MIN_VIDEO_KBPS);
    let total_kbps = video_kbps.saturating_add(audio_kbps_total);
    BitrateBudget {
        total_kbps,
        video_kbps,
        audio_kbps_total,
        // 1.5 × video, halves rounded up (793 → 1190).
        maxrate_kbps: video_kbps.saturating_mul(3).div_ceil(2),
        bufsize_kbps: video_kbps.saturating_mul(2),
        expected_mb_per_hour: mb_per_hour_for_kbps(total_kbps),
    }
}

/// target_mb × 1e6 bytes × 8 ÷ 3600 s ÷ 1000, rounded to the nearest kbps (400 → 888.9 → 889).
pub fn kbps_for_mb_per_hour(mb_per_hour: u32) -> u32 {
    let bits_per_hour = u64::from(mb_per_hour) * 8_000_000;
    let bits_per_kbps_hour = 1_000 * SECONDS_PER_HOUR;
    let rounded = (bits_per_hour + bits_per_kbps_hour / 2) / bits_per_kbps_hour;
    u32::try_from(rounded).unwrap_or(u32::MAX)
}

/// The inverse: kbps × 125 bytes/s × 3600 s ÷ 1e6 = kbps × 0.45 MB per hour (889 → 400.05).
pub fn mb_per_hour_for_kbps(kbps: u32) -> f64 {
    bytes_per_hour(kbps) as f64 / MB
}

/// Bytes written per second at `kbps`.
pub fn bytes_per_second(kbps: u32) -> u64 {
    u64::from(kbps) * BYTES_PER_KBPS_SECOND
}

/// Bytes written per hour at `kbps`.
pub fn bytes_per_hour(kbps: u32) -> u64 {
    bytes_per_second(kbps) * SECONDS_PER_HOUR
}

/// Extrapolate a measured sample to one hour: bytes / seconds × 3600 / 1e6 (MB, decimal).
/// A zero-length sample extrapolates to 0 (never NaN or infinity).
pub fn extrapolate_mb_per_hour(bytes: u64, duration_ms: u64) -> f64 {
    if duration_ms == 0 {
        return 0.0;
    }
    let seconds = duration_ms as f64 / 1_000.0;
    bytes as f64 / seconds * SECONDS_PER_HOUR as f64 / MB
}

/// bytes ÷ (duration_h × target_mb × 1e6) − 1. Positive = over budget.
/// Zero duration or zero target has no meaningful ratio and returns 0.
pub fn over_budget_ratio(bytes: u64, duration_ms: u64, target_mb_per_hour: u32) -> f64 {
    if duration_ms == 0 || target_mb_per_hour == 0 {
        return 0.0;
    }
    let hours = duration_ms as f64 / (SECONDS_PER_HOUR as f64 * 1_000.0);
    let allowed_bytes = hours * f64::from(target_mb_per_hour) * MB;
    bytes as f64 / allowed_bytes - 1.0
}

/// Estimated total bytes at `elapsed_ms` for the floating bar.
pub fn estimate_bytes(budget: &BitrateBudget, elapsed_ms: u64) -> u64 {
    // kbps × 125 bytes/s × ms ÷ 1000, in u128 so no input can overflow.
    let bytes = u128::from(budget.total_kbps) * u128::from(BYTES_PER_KBPS_SECOND) * u128::from(elapsed_ms) / 1_000;
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const HOUR_MS: u64 = 3_600_000;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn four_hundred_mb_one_track_is_the_product_promise() {
        let b = budget_for(400, 96, 1);
        assert_eq!(b.total_kbps, 889);
        assert_eq!(b.video_kbps, 793);
        assert_eq!(b.audio_kbps_total, 96);
        assert_eq!(b.maxrate_kbps, 1190, "1.5 × 793 = 1189.5 rounds up");
        assert_eq!(b.bufsize_kbps, 1586);
        assert!(close(b.expected_mb_per_hour, 400.05, 0.001), "{}", b.expected_mb_per_hour);
    }

    #[test]
    fn four_hundred_mb_two_tracks() {
        let b = budget_for(400, 96, 2);
        assert_eq!(b.total_kbps, 889);
        assert_eq!(b.audio_kbps_total, 192);
        assert_eq!(b.video_kbps, 697);
        assert_eq!(b.maxrate_kbps, 1046);
        assert_eq!(b.bufsize_kbps, 1394);
        assert!(close(b.expected_mb_per_hour, 400.05, 0.001));
    }

    #[test]
    fn two_hundred_mb() {
        let b = budget_for(200, 96, 1);
        assert_eq!(b.total_kbps, 444, "200 × 8000 / 3600 = 444.4");
        assert_eq!(b.video_kbps, 348);
        assert_eq!(b.maxrate_kbps, 522);
        assert_eq!(b.bufsize_kbps, 696);
        assert!(close(b.expected_mb_per_hour, 199.8, 0.001));
    }

    #[test]
    fn eight_hundred_mb() {
        let b = budget_for(800, 96, 1);
        assert_eq!(b.total_kbps, 1778, "800 × 8000 / 3600 = 1777.8");
        assert_eq!(b.video_kbps, 1682);
        assert_eq!(b.maxrate_kbps, 2523);
        assert_eq!(b.bufsize_kbps, 3364);
        assert!(close(b.expected_mb_per_hour, 800.1, 0.001));
    }

    #[test]
    fn video_floor_keeps_the_picture_watchable_and_reports_the_overrun() {
        // 100 MB/h with two 128 kbps tracks: 222 − 256 < 0 → video floors at 200.
        let b = budget_for(100, 128, 2);
        assert_eq!(b.video_kbps, MIN_VIDEO_KBPS);
        assert_eq!(b.audio_kbps_total, 256);
        assert_eq!(b.total_kbps, 456, "total is the real sum, not the target");
        assert!(b.expected_mb_per_hour > 100.0);
        assert!(close(b.expected_mb_per_hour, 205.2, 0.001));
    }

    #[test]
    fn no_audio_tracks_gives_everything_to_video() {
        let b = budget_for(400, 96, 0);
        assert_eq!(b.audio_kbps_total, 0);
        assert_eq!(b.video_kbps, 889);
        assert_eq!(b.total_kbps, 889);
    }

    #[test]
    fn total_is_always_video_plus_audio() {
        for target in [100, 150, 400, 999, 2000] {
            for tracks in [0, 1, 2] {
                for audio in [64, 96, 128] {
                    let b = budget_for(target, audio, tracks);
                    assert_eq!(b.total_kbps, b.video_kbps + b.audio_kbps_total);
                    assert!(b.video_kbps >= MIN_VIDEO_KBPS);
                    assert_eq!(b.bufsize_kbps, 2 * b.video_kbps);
                    assert!(b.maxrate_kbps >= b.video_kbps && b.maxrate_kbps <= b.bufsize_kbps);
                }
            }
        }
    }

    #[test]
    fn kbps_rounding_is_nearest_half_up() {
        assert_eq!(kbps_for_mb_per_hour(0), 0);
        assert_eq!(kbps_for_mb_per_hour(450), 1000, "450 MB/h is exactly 1000 kbps");
        assert_eq!(kbps_for_mb_per_hour(400), 889);
        assert_eq!(kbps_for_mb_per_hour(u32::MAX), u32::MAX, "saturates instead of overflowing");
        assert!(close(mb_per_hour_for_kbps(1000), 450.0, 1e-9));
    }

    #[test]
    fn expected_mb_matches_estimate_bytes_over_one_hour() {
        let b = budget_for(400, 96, 1);
        let hour_bytes = estimate_bytes(&b, HOUR_MS);
        assert_eq!(hour_bytes, 400_050_000);
        assert!(close(hour_bytes as f64 / MB, b.expected_mb_per_hour, 1e-9));
    }

    #[test]
    fn extrapolate_two_minute_sample_to_the_hour() {
        // 13.3 MB in 2 minutes ≈ 399 MB/h, i.e. on budget.
        let mb_h = extrapolate_mb_per_hour(13_300_000, 120_000);
        assert!(close(mb_h, 399.0, 0.001), "{mb_h}");
        assert!((mb_h - 400.0).abs() < 2.0);
        // A full hour maps to itself.
        assert!(close(extrapolate_mb_per_hour(400_000_000, HOUR_MS), 400.0, 1e-9));
    }

    #[test]
    fn extrapolate_zero_duration_is_zero_not_nan() {
        let v = extrapolate_mb_per_hour(123_456, 0);
        assert!(v.is_finite());
        assert_eq!(v, 0.0);
        assert_eq!(extrapolate_mb_per_hour(0, 0), 0.0);
    }

    #[test]
    fn over_budget_ratio_sign_and_magnitude() {
        assert!(close(over_budget_ratio(448_000_000, HOUR_MS, 400), 0.12, 1e-9), "12 % over");
        assert!(close(over_budget_ratio(360_000_000, HOUR_MS, 400), -0.10, 1e-9), "10 % under");
        assert!(close(over_budget_ratio(400_000_000, HOUR_MS, 400), 0.0, 1e-9), "exactly on");
        // Half an hour at the full-hour budget: 200 MB is on budget.
        assert!(close(over_budget_ratio(200_000_000, HOUR_MS / 2, 400), 0.0, 1e-9));
        assert!(close(over_budget_ratio(0, HOUR_MS, 400), -1.0, 1e-9), "an empty file is 100 % under");
    }

    #[test]
    fn over_budget_ratio_degenerate_inputs_are_zero_and_finite() {
        for v in [
            over_budget_ratio(1_000_000, 0, 400),
            over_budget_ratio(1_000_000, HOUR_MS, 0),
            over_budget_ratio(0, 0, 0),
        ] {
            assert!(v.is_finite());
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn estimate_bytes_is_linear_in_time() {
        let b = budget_for(400, 96, 1);
        assert_eq!(estimate_bytes(&b, 0), 0);
        let one_min = estimate_bytes(&b, 60_000);
        assert_eq!(one_min, 889 * 125 * 60);
        assert_eq!(estimate_bytes(&b, 120_000), 2 * one_min);
        assert_eq!(estimate_bytes(&b, 600_000), 10 * one_min);
        // Huge inputs saturate instead of panicking.
        let huge = BitrateBudget { total_kbps: u32::MAX, ..b };
        assert_eq!(estimate_bytes(&huge, u64::MAX), u64::MAX);
    }

    #[test]
    fn byte_rate_helpers() {
        assert_eq!(bytes_per_second(889), 111_125);
        assert_eq!(bytes_per_hour(889), 400_050_000);
        assert_eq!(bytes_per_hour(0), 0);
    }
}
