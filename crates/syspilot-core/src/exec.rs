use std::ffi::OsStr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

static SUBPROCESS_SPAWN_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Increments the global subprocess spawn counter.
#[inline]
pub fn record_spawn() {
    SUBPROCESS_SPAWN_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Returns the current subprocess spawn count (for zero-fork assertions in tests).
#[inline]
pub fn spawn_count() -> usize {
    SUBPROCESS_SPAWN_COUNT.load(Ordering::Relaxed)
}

/// Resets the subprocess spawn counter to zero.
#[inline]
pub fn reset_spawn_count() {
    SUBPROCESS_SPAWN_COUNT.store(0, Ordering::Relaxed);
}

pub const ALLOWED_READONLY_BINS: &[&str] = &[
    "pacman",
    "checkupdates",
    "yay",
    "paru",
    "systemctl",
    "arch-audit",
    "fwupdmgr",
    "curl",
    "sys-health.sh",
    "bash",
];

pub fn is_allowed_readonly_bin(program: impl AsRef<OsStr>) -> bool {
    let path = Path::new(program.as_ref());
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    ALLOWED_READONLY_BINS.contains(&name)
}

#[allow(clippy::disallowed_methods)]
pub fn new_tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    record_spawn();
    tokio::process::Command::new(program)
}

#[allow(clippy::disallowed_methods)]
pub fn new_std_command(program: impl AsRef<OsStr>) -> std::process::Command {
    record_spawn();
    std::process::Command::new(program)
}
