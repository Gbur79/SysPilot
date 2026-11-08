#[cfg(test)]
use std::cell::RefCell;
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::time::SystemTime;

#[cfg(test)]
thread_local! {
    static TEST_SENTINELS_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static TEST_SENTINELS_STATE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub fn set_test_sentinels_overrides(root: Option<PathBuf>, state: Option<PathBuf>) {
    TEST_SENTINELS_ROOT.with(|r| *r.borrow_mut() = root);
    TEST_SENTINELS_STATE.with(|s| *s.borrow_mut() = state);
}

fn active_sentinels_root() -> std::path::PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_SENTINELS_ROOT.with(|r| r.borrow().clone()) {
        return root;
    }
    std::path::PathBuf::from("/")
}

fn active_sentinels_state() -> std::path::PathBuf {
    #[cfg(test)]
    if let Some(state) = TEST_SENTINELS_STATE.with(|s| s.borrow().clone()) {
        return state;
    }
    crate::models::state_base()
}

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
    check_reboot_pending_at(&active_sentinels_root())
}

pub fn check_reboot_pending_at(root: &Path) -> bool {
    // 1. Check running kernel modules directory
    let osrelease_path = crate::sys_root::sub_path(root, "/proc/sys/kernel/osrelease");
    if let Ok(release) = fs::read_to_string(osrelease_path) {
        let release = release.trim();
        if !release.is_empty() {
            let modules_path = crate::sys_root::sub_path(root, "/usr/lib/modules").join(release);
            if !modules_path.exists() {
                return true;
            }
        }
    }

    // 2. Fallback: check standard reboot-required marker
    crate::sys_root::sub_path(root, "/var/run/reboot-required").exists()
}

pub fn check_pacman_lock() -> (bool, bool) {
    let root = active_sentinels_root();
    check_pacman_lock_at(&crate::sys_root::sub_path(&root, "/var/lib/pacman/db.lck"))
}

pub fn check_pacman_lock_at(lock_path: &Path) -> (bool, bool) {
    if !lock_path.exists() {
        return (false, false);
    }

    let proc_dir = if lock_path.starts_with("/var/lib/pacman") {
        std::path::PathBuf::from("/proc")
    } else if let Some(root) = lock_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .filter(|_| lock_path.ends_with("var/lib/pacman/db.lck"))
    {
        root.join("proc")
    } else {
        lock_path
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .join("proc")
    };

    // 1. Fast-path: Check PID inside db.lck if written by ALPM
    let mut active = false;
    if let Ok(content) = fs::read_to_string(lock_path) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            let comm_path = proc_dir.join(format!("{pid}/comm"));
            if let Ok(comm) = fs::read_to_string(comm_path) {
                if matches!(comm.trim(), "pacman" | "yay" | "paru" | "pamac-daemon") {
                    active = true;
                }
            }
        }
    }

    // 2. Fallback scan if PID not found or process is wrapping ALPM
    if !active {
        if let Ok(entries) = fs::read_dir(&proc_dir) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let name_str = file_name.to_string_lossy();
                if !name_str.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let comm_path = entry.path().join("comm");
                if let Ok(comm) = fs::read_to_string(comm_path) {
                    let comm = comm.trim();
                    if matches!(comm, "pacman" | "yay" | "paru" | "pamac-daemon") {
                        active = true;
                        break;
                    }
                }
            }
        }
    }

    // 3. TOCTOU Defense: Verify lockfile still exists after /proc scan.
    if !lock_path.exists() {
        return (false, false);
    }

    // (is_locked, is_orphaned_lock)
    (true, !active)
}

pub fn get_disk_stats(path: &str) -> Option<(u8, f32)> {
    crate::triage::get_disk_usage(path).map(|d| (d.used_pct, d.free_gb))
}

pub fn get_pacman_db_mtime() -> Option<SystemTime> {
    get_pacman_db_mtime_at(&active_sentinels_root())
}

pub fn get_pacman_db_mtime_at(root: &Path) -> Option<SystemTime> {
    fs::metadata(crate::sys_root::sub_path(root, "/var/lib/pacman/local"))
        .and_then(|m| m.modified())
        .ok()
}

pub fn get_summary_mtime() -> Option<SystemTime> {
    get_summary_mtime_at(&active_sentinels_state())
}

pub fn get_summary_mtime_at(state_dir: &Path) -> Option<SystemTime> {
    let summary_path = state_dir.join("system-health/summary.json");
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
