//! Free-space arithmetic. The app supplies the numbers; this decides.
use crate::types::*;

/// hours_recordable = free_bytes / (total_kbps × 1000 / 8 × 3600); low = free < low_gb GiB.
pub fn status(path: &str, free_bytes: u64, total_bytes: u64, budget: &BitrateBudget, low_gb: u32) -> DiskStatus {
    todo!()
}

/// Should we refuse to start? (less than 10 minutes of space at this budget)
pub fn refuse_start(status: &DiskStatus) -> bool {
    todo!()
}
