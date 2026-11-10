//! Free-space queries (the arithmetic lives in recorder-core::disk).
use std::path::{Path, PathBuf};

/// Walk up to the nearest existing ancestor, because the output folder may not exist yet.
fn existing_ancestor(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    while !p.exists() {
        if !p.pop() {
            return PathBuf::from(".");
        }
    }
    p
}

/// (free_bytes, total_bytes) of the volume holding `path`.
pub fn space(path: &Path) -> std::io::Result<(u64, u64)> {
    let anchor = existing_ancestor(path);
    Ok((fs2::available_space(&anchor)?, fs2::total_space(&anchor)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_of_a_missing_child_uses_its_parent() {
        let dir = tempfile::tempdir().unwrap();
        let (free, total) = space(&dir.path().join("not/yet/created")).unwrap();
        assert!(total > 0 && free <= total);
    }
}
