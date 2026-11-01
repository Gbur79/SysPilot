use std::fs;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SentinelStatus {
    pub reboot_pending: bool,
    pub pacman_locked: bool,
    pub pacman_orphaned_lock: bool,
    pub root_used_pct: u8,
    pub root_avail_gb: f32,
    pub home_avail_gb: f32,
    pub pacman_db_mtime: Option<SystemTime>,
    pub summary_mtime: Option<SystemTime>,
}

pub fn check_reboot_pending() -> bool {
    // 1. Check running kernel modules directory
    if let Ok(release) = fs::read_to_string("/proc/sys/kernel/osrelease") {
        let release = release.trim();
        if !release.is_empty() {
            let modules_path = Path::new("/usr/lib/modules").join(release);
            if !modules_path.exists() {
                return true;
            }
        }
    }

    // 2. Fallback: check standard reboot-required marker
    Path::new("/var/run/reboot-required").exists()
}

pub fn check_pacman_lock() -> (bool, bool) {
    check_pacman_lock_at(Path::new("/var/lib/pacman/db.lck"))
}

pub fn check_pacman_lock_at(lock_path: &Path) -> (bool, bool) {
    if !lock_path.exists() {
        return (false, false);
    }

    // 1. Fast-path: Check PID inside db.lck if written by ALPM
    let mut active = false;
    if let Ok(content) = fs::read_to_string(lock_path) {
        if let Ok(pid) = content.trim().parse::<i32>() {
            if pid > 0 {
                let comm_path = format!("/proc/{pid}/comm");
                if let Ok(comm) = fs::read_to_string(comm_path) {
                    if matches!(
                        comm.trim(),
                        "pacman" | "yay" | "paru" | "checkupdates" | "pamac-daemon"
                    ) {
                        active = true;
                    }
                }
            }
        }
    }

    // 2. Fallback scan if PID not found or process is wrapping ALPM
    if !active {
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let name_str = file_name.to_string_lossy();
                if !name_str.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let comm_path = entry.path().join("comm");
                if let Ok(comm) = fs::read_to_string(comm_path) {
                    let comm = comm.trim();
                    if matches!(
                        comm,
                        "pacman" | "yay" | "paru" | "checkupdates" | "pamac-daemon"
                    ) {
                        active = true;
                        break;
                    }
                }
            }
        }
    }

    // 3. TOCTOU Defense: Verify lockfile still exists after /proc scan.
    // If pacman unlinked db.lck while we were scanning, it is not an orphaned lock!
    if !lock_path.exists() {
        return (false, false);
    }

    // (is_locked, is_orphaned_lock)
    (true, !active)
}

pub fn get_disk_stats(path: &str) -> Option<(u8, f32)> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let c_path = CString::new(path).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();

    let res = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if res != 0 {
        return None;
    }

    let stat = unsafe { stat.assume_init() };
    let block_size = stat.f_frsize as f64;
    let total_bytes = stat.f_blocks as f64 * block_size;
    let avail_bytes = stat.f_bavail as f64 * block_size;

    if total_bytes <= 0.0 {
        return None;
    }

    let used_bytes = total_bytes - avail_bytes;
    let used_pct = ((used_bytes / total_bytes) * 100.0).clamp(0.0, 100.0) as u8;
    let avail_gb = (avail_bytes / (1024.0 * 1024.0 * 1024.0)) as f32;

    Some((used_pct, avail_gb))
}

pub fn get_pacman_db_mtime() -> Option<SystemTime> {
    fs::metadata("/var/lib/pacman/local")
        .and_then(|m| m.modified())
        .ok()
}

pub fn get_summary_mtime() -> Option<SystemTime> {
    let summary_path = crate::models::state_base().join("system-health/summary.json");
    fs::metadata(summary_path).and_then(|m| m.modified()).ok()
}

pub fn collect_sentinels() -> SentinelStatus {
    let reboot_pending = check_reboot_pending();
    let (pacman_locked, pacman_orphaned_lock) = check_pacman_lock();

    let (root_used_pct, root_avail_gb) = get_disk_stats("/").unwrap_or((0, 0.0));
    let (_, home_avail_gb) = get_disk_stats("/home").unwrap_or((root_used_pct, root_avail_gb));

    let pacman_db_mtime = get_pacman_db_mtime();
    let summary_mtime = get_summary_mtime();

    SentinelStatus {
        reboot_pending,
        pacman_locked,
        pacman_orphaned_lock,
        root_used_pct,
        root_avail_gb,
        home_avail_gb,
        pacman_db_mtime,
        summary_mtime,
    }
}
