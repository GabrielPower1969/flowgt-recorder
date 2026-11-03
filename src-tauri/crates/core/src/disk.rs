//! Free-space arithmetic. The app supplies the numbers; this decides.
use crate::budget;
use crate::types::*;

/// Refuse to start a recording with less than this many minutes of space at the current budget.
pub const REFUSE_BELOW_MINUTES: f64 = 10.0;
/// Cap for `hours_recordable`, so a zero bitrate or a huge disk still gives a finite,
/// JSON-serialisable number (serde_json turns infinity into `null`). About eleven years.
pub const MAX_HOURS_RECORDABLE: f64 = 100_000.0;
/// One gibibyte, the unit of `low_disk_warn_gb` (what the OS shows as "GB" free).
pub const GIB: u64 = 1024 * 1024 * 1024;

/// hours_recordable = free_bytes / (total_kbps × 1000 / 8 × 3600); low = free < low_gb GiB.
pub fn status(path: &str, free_bytes: u64, total_bytes: u64, budget: &BitrateBudget, low_gb: u32) -> DiskStatus {
    DiskStatus {
        path: path.to_string(),
        free_bytes,
        total_bytes,
        hours_recordable: hours_recordable(free_bytes, budget),
        low: free_bytes < u64::from(low_gb) * GIB,
    }
}

/// Hours of recording that fit in `free_bytes` at the budget's real total bitrate, capped at
/// [`MAX_HOURS_RECORDABLE`] (a zero bitrate means "unlimited", reported as the cap).
pub fn hours_recordable(free_bytes: u64, budget: &BitrateBudget) -> f64 {
    let bytes_per_hour = budget::bytes_per_hour(budget.total_kbps);
    if bytes_per_hour == 0 {
        return MAX_HOURS_RECORDABLE;
    }
    (free_bytes as f64 / bytes_per_hour as f64).min(MAX_HOURS_RECORDABLE)
}

/// Should we refuse to start? (less than 10 minutes of space at this budget)
pub fn refuse_start(status: &DiskStatus) -> bool {
    status.hours_recordable * 60.0 < REFUSE_BELOW_MINUTES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::budget_for;
    use pretty_assertions::assert_eq;

    // 400 MB/h → 889 kbps → 889 × 125 × 3600 = 400 050 000 bytes per hour.
    const BYTES_PER_HOUR_AT_400: u64 = 400_050_000;

    fn budget() -> BitrateBudget {
        budget_for(400, 96, 1)
    }

    #[test]
    fn ten_hours_free_on_a_healthy_disk() {
        let s = status("/Volumes/Data", 10 * BYTES_PER_HOUR_AT_400, 500_000_000_000, &budget(), 2);
        assert_eq!(s.path, "/Volumes/Data");
        assert_eq!(s.free_bytes, 4_000_500_000);
        assert_eq!(s.total_bytes, 500_000_000_000);
        assert_eq!(s.hours_recordable, 10.0);
        assert!(!s.low, "4 GB free is above the 2 GiB warning line");
        assert!(!refuse_start(&s));
    }

    #[test]
    fn low_flag_uses_gibibytes() {
        let two_gib = 2 * GIB; // 2 147 483 648
        let just_under = status("/", two_gib - 1, two_gib * 100, &budget(), 2);
        assert!(just_under.low);
        let exactly = status("/", two_gib, two_gib * 100, &budget(), 2);
        assert!(!exactly.low, "low means strictly less than the threshold");
        // 2.0 decimal GB is below 2 GiB and must warn.
        assert!(status("/", 2_000_000_000, two_gib * 100, &budget(), 2).low);
        // A zero threshold never warns.
        assert!(!status("/", 0, 1, &budget(), 0).low);
    }

    #[test]
    fn one_gigabyte_is_low_but_still_recordable() {
        let s = status("C:\\", 1_000_000_000, 256_000_000_000, &budget(), 2);
        assert!(s.low);
        assert!((s.hours_recordable - 2.4997).abs() < 0.001, "{}", s.hours_recordable);
        assert!(!refuse_start(&s), "2.5 hours is plenty to start");
    }

    #[test]
    fn refuse_below_ten_minutes() {
        // 9 minutes of space at 889 kbps.
        let nine_min = BYTES_PER_HOUR_AT_400 * 9 / 60;
        let s = status("/", nine_min, 1_000_000_000, &budget(), 2);
        assert!((s.hours_recordable * 60.0 - 9.0).abs() < 1e-6);
        assert!(refuse_start(&s));
        // 11 minutes is allowed.
        let eleven_min = BYTES_PER_HOUR_AT_400 * 11 / 60;
        let s = status("/", eleven_min, 1_000_000_000, &budget(), 2);
        assert!(!refuse_start(&s));
        // A full disk is refused.
        assert!(refuse_start(&status("/", 0, 1_000_000_000, &budget(), 2)));
    }

    #[test]
    fn bigger_budget_means_fewer_hours() {
        let free = 10 * BYTES_PER_HOUR_AT_400;
        let at_400 = status("/", free, free, &budget_for(400, 96, 1), 2).hours_recordable;
        let at_800 = status("/", free, free, &budget_for(800, 96, 1), 2).hours_recordable;
        assert!(at_800 < at_400);
        assert!((at_800 - 5.0).abs() < 0.01, "{at_800}");
    }

    #[test]
    fn zero_bitrate_is_finite_and_never_refused() {
        let zero = BitrateBudget {
            total_kbps: 0,
            video_kbps: 0,
            audio_kbps_total: 0,
            maxrate_kbps: 0,
            bufsize_kbps: 0,
            expected_mb_per_hour: 0.0,
        };
        let s = status("/", 123, 456, &zero, 2);
        assert!(s.hours_recordable.is_finite());
        assert_eq!(s.hours_recordable, MAX_HOURS_RECORDABLE);
        assert!(!refuse_start(&s));
        // serde_json would turn infinity into null; a finite cap survives the wire.
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"hoursRecordable\":100000.0"), "{json}");
    }

    #[test]
    fn huge_disk_is_capped_not_infinite() {
        let s = status("/", u64::MAX, u64::MAX, &budget(), 2);
        assert!(s.hours_recordable.is_finite());
        assert_eq!(s.hours_recordable, MAX_HOURS_RECORDABLE);
    }
}
