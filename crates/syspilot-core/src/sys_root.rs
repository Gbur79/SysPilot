use std::path::{Path, PathBuf};

/// Safely resolves an absolute-style Linux path (e.g. `/proc/sys/kernel/osrelease`)
/// inside `root` without triggering `Path::join`'s absolute-path replacement trap.
#[inline]
pub fn sub_path(root: &Path, abs_path: &str) -> PathBuf {
    let trimmed = abs_path.trim_start_matches('/');
    if root == Path::new("/") {
        if abs_path.starts_with('/') {
            PathBuf::from(abs_path)
        } else {
            PathBuf::from("/").join(trimmed)
        }
    } else if trimmed.is_empty() {
        root.to_path_buf()
    } else {
        root.join(trimmed)
    }
}
