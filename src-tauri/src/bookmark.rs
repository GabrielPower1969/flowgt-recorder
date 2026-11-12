//! Keeping access to a save folder the user picked, inside the macOS App Sandbox.
//!
//! A sandboxed app may write to a folder only while the system has granted it: the Movies
//! folder through an entitlement, any other folder because the user picked it in the system's
//! folder dialog. That second grant ends when the app quits. A security-scoped bookmark is the
//! system's way to get it back at the next launch without asking again.
//!
//! Outside the sandbox (and on other platforms) every function here does nothing.

use std::path::{Path, PathBuf};

const FILE: &str = "output-dir.bookmark";

/// Remember the folder the user just picked. Call it right after the folder dialog returns,
/// while the grant from the dialog is still live.
pub fn remember(data_dir: &Path, folder: &Path) {
    if !crate::commands::is_sandboxed() {
        return;
    }
    match imp::create(folder) {
        Some(bytes) => {
            if let Err(e) = std::fs::create_dir_all(data_dir)
                .and_then(|()| std::fs::write(data_dir.join(FILE), bytes))
            {
                log::warn!("cannot store the save-folder bookmark: {e}");
            }
        }
        None => log::warn!(
            "no bookmark for {}; it will need picking again after a restart",
            folder.display()
        ),
    }
}

/// Get back the access remembered by [`remember`]. Returns the folder when it worked.
pub fn restore(data_dir: &Path) -> Option<PathBuf> {
    if !crate::commands::is_sandboxed() {
        return None;
    }
    let bytes = std::fs::read(data_dir.join(FILE)).ok()?;
    let folder = imp::open(&bytes);
    if folder.is_none() {
        log::warn!("the stored save-folder bookmark no longer opens");
    }
    folder
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;
    use std::ffi::c_void;
    use std::path::{Path, PathBuf};
    use std::ptr::null_mut;

    /// NSURLBookmarkCreationWithSecurityScope
    const CREATE_WITH_SECURITY_SCOPE: usize = 1 << 11;
    /// NSURLBookmarkResolutionWithSecurityScope
    const RESOLVE_WITH_SECURITY_SCOPE: usize = 1 << 10;

    pub fn create(folder: &Path) -> Option<Vec<u8>> {
        let text = NSString::from_str(&folder.to_string_lossy());
        // SAFETY: documented NSURL / NSData class and instance methods; every returned object is
        // checked for nil before use and only used inside this call (autoreleased).
        unsafe {
            let url: *mut AnyObject = msg_send![class!(NSURL), fileURLWithPath: &*text];
            if url.is_null() {
                return None;
            }
            let nil: *mut AnyObject = null_mut();
            let data: *mut AnyObject = msg_send![
                url,
                bookmarkDataWithOptions: CREATE_WITH_SECURITY_SCOPE,
                includingResourceValuesForKeys: nil,
                relativeToURL: nil,
                error: null_mut::<*mut AnyObject>(),
            ];
            if data.is_null() {
                return None;
            }
            let len: usize = msg_send![data, length];
            let bytes: *const c_void = msg_send![data, bytes];
            if bytes.is_null() || len == 0 {
                return None;
            }
            Some(std::slice::from_raw_parts(bytes.cast::<u8>(), len).to_vec())
        }
    }

    pub fn open(bytes: &[u8]) -> Option<PathBuf> {
        // SAFETY: as in `create`. The resolved URL is retained and never released on purpose:
        // the access it carries must last until the app quits.
        unsafe {
            let data: *mut AnyObject = msg_send![
                class!(NSData),
                dataWithBytes: bytes.as_ptr().cast::<c_void>(),
                length: bytes.len(),
            ];
            if data.is_null() {
                return None;
            }
            let nil: *mut AnyObject = null_mut();
            let mut stale = Bool::NO;
            let url: *mut AnyObject = msg_send![
                class!(NSURL),
                URLByResolvingBookmarkData: data,
                options: RESOLVE_WITH_SECURITY_SCOPE,
                relativeToURL: nil,
                bookmarkDataIsStale: &mut stale,
                error: null_mut::<*mut AnyObject>(),
            ];
            if url.is_null() {
                return None;
            }
            let started: bool = msg_send![url, startAccessingSecurityScopedResource];
            if !started {
                return None;
            }
            let _: *mut AnyObject = msg_send![url, retain];
            let path: *mut NSString = msg_send![url, path];
            if path.is_null() {
                return None;
            }
            Some(PathBuf::from((*path).to_string()))
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::{Path, PathBuf};
    pub fn create(_folder: &Path) -> Option<Vec<u8>> {
        None
    }
    pub fn open(_bytes: &[u8]) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_the_sandbox_nothing_is_written_or_read() {
        if crate::commands::is_sandboxed() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        remember(dir.path(), dir.path());
        assert!(!dir.path().join(FILE).exists());
        assert_eq!(restore(dir.path()), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_bookmark_of_a_real_folder_opens_to_the_same_folder() {
        // Without the sandbox a security-scoped bookmark cannot be made; nil is the expected
        // answer there, and the calls must not crash.
        let dir = tempfile::tempdir().unwrap();
        if let Some(bytes) = imp::create(dir.path()) {
            let back = imp::open(&bytes).expect("a fresh bookmark opens");
            assert_eq!(
                back.canonicalize().unwrap(),
                dir.path().canonicalize().unwrap()
            );
        }
        assert_eq!(imp::open(b"not a bookmark"), None);
    }
}
