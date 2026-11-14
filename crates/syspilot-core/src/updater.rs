use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpdateSummary {
    pub core_count: u32,
    pub regular_count: u32,
    pub aur_count: u32,
    pub standalone_count: u32,
    pub core_packages: Vec<String>,
    pub last_checked: Option<Instant>,
}

impl UpdateSummary {
    pub fn total(&self) -> u32 {
        self.core_count + self.regular_count + self.aur_count + self.standalone_count
    }
}

pub fn default_checkupdates_timeout() -> Duration {
    if let Ok(val) = std::env::var("SYSPILOT_UPDATE_TIMEOUT_SECS") {
        if let Ok(secs) = val.trim().parse::<u64>() {
            if secs > 0 {
                return Duration::from_secs(secs);
            }
        }
    }
    Duration::from_secs(15)
}

pub fn default_standalone_timeout() -> Duration {
    if let Ok(val) = std::env::var("SYSPILOT_STANDALONE_TIMEOUT_SECS") {
        if let Ok(secs) = val.trim().parse::<u64>() {
            if secs > 0 {
                return Duration::from_secs(secs);
            }
        }
    }
    Duration::from_secs(10)
}

pub async fn run_command_with_timeout(
    mut cmd: Command,
    timeout_dur: Duration,
) -> Result<std::process::Output, std::io::Error> {
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let child = cmd.spawn()?;
    match timeout(timeout_dur, child.wait_with_output()).await {
        Ok(res) => res,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("subprocess timed out after {}s", timeout_dur.as_secs()),
        )),
    }
}

pub fn is_core_package(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("linux")
        || lower.starts_with("systemd")
        || lower.starts_with("nvidia")
        || matches!(
            lower.as_str(),
            "grub"
                | "mkinitcpio"
                | "dracut"
                | "booster"
                | "glibc"
                | "cryptsetup"
                | "mesa"
                | "pipewire"
                | "wireplumber"
        )
}

pub fn find_executable(name: &str) -> Option<PathBuf> {
    let env_key = format!(
        "SYSPILOT_BIN_{}",
        name.to_ascii_uppercase().replace('-', "_")
    );
    if let Ok(path_str) = std::env::var(env_key) {
        let trimmed = path_str.trim();
        if trimmed.is_empty() || trimmed == "none" || trimmed == "disabled" {
            return None;
        }
        let p = PathBuf::from(trimmed);
        return if p.is_file() { Some(p) } else { None };
    }
    for dir in ["/usr/bin", "/usr/local/bin"] {
        let p = Path::new(dir).join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_bin = PathBuf::from(home).join(".local/bin").join(name);
        if user_bin.is_file() {
            return Some(user_bin);
        }
    }
    None
}

pub fn is_process_running(proc_name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let comm_path = entry.path().join("comm");
        if let Ok(comm) = std::fs::read_to_string(comm_path) {
            if comm.trim() == proc_name {
                return true;
            }
        }
    }
    false
}

pub async fn check_official_updates() -> (u32, u32, Vec<String>) {
    let Some(checkupdates) = find_executable("checkupdates") else {
        return (0, 0, Vec::new());
    };

    let timeout_dur = default_checkupdates_timeout();
    let cmd = crate::exec::new_tokio_command(&checkupdates);
    let output = match run_command_with_timeout(cmd, timeout_dur).await {
        Ok(out) => out,
        Err(_) => return (0, 0, Vec::new()),
    };

    // Auto-heal stale /tmp/checkup-db-<uid>/db.lck if checkupdates fails on lock
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if stderr.contains("database is locked") {
            let uid = unsafe { libc::getuid() };
            let lock_path = PathBuf::from(format!("/tmp/checkup-db-{uid}/db.lck"));
            if lock_path.exists() && !is_process_running("checkupdates") {
                let _ = std::fs::remove_file(lock_path);
                let retry_cmd = crate::exec::new_tokio_command(&checkupdates);
                if let Ok(retry) = run_command_with_timeout(retry_cmd, timeout_dur).await {
                    if retry.status.success() {
                        return parse_checkupdates_stdout(&retry.stdout);
                    }
                }
            }
        }
        return (0, 0, Vec::new());
    }

    parse_checkupdates_stdout(&output.stdout)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CheckupdatesReport {
    pub core_count: u32,
    pub regular_count: u32,
    pub core_packages: Vec<String>,
    pub all_packages: Vec<String>,
}

pub fn parse_checkupdates_str(stdout: &str) -> Result<CheckupdatesReport, String> {
    let mut core_count = 0;
    let mut regular_count = 0;
    let mut core_packages = Vec::new();
    let mut all_packages = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("==> error:") || lower.starts_with("error:") {
            return Err(format!("checkupdates emitted error on stdout: {trimmed}"));
        }
        let Some((left, right)) = trimmed.split_once(" -> ") else {
            return Err(format!("malformed checkupdates line: {trimmed}"));
        };
        let mut left_parts = left.split_whitespace();
        let Some(pkg_name) = left_parts.next() else {
            return Err(format!("malformed checkupdates line: {trimmed}"));
        };
        let Some(_old_ver) = left_parts.next() else {
            return Err(format!("malformed checkupdates line: {trimmed}"));
        };
        if right.trim().is_empty() {
            return Err(format!("malformed checkupdates line: {trimmed}"));
        }

        if is_core_package(pkg_name) {
            core_count += 1;
            core_packages.push(trimmed.to_string());
        } else {
            regular_count += 1;
        }
        all_packages.push(trimmed.to_string());
    }

    Ok(CheckupdatesReport {
        core_count,
        regular_count,
        core_packages,
        all_packages,
    })
}

pub fn parse_checkupdates_stdout(stdout: &[u8]) -> (u32, u32, Vec<String>) {
    let text = String::from_utf8_lossy(stdout);
    match parse_checkupdates_str(&text) {
        Ok(r) => (r.core_count, r.regular_count, r.core_packages),
        Err(e) => {
            eprintln!("[syspilot-core::updater] parse_checkupdates_str failed: {e}");
            (0, 0, Vec::new())
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AurUpdatesReport {
    pub count: u32,
    pub packages: Vec<String>,
}

pub fn parse_aur_updates_str(stdout: &str) -> Result<AurUpdatesReport, String> {
    let mut count = 0;
    let mut packages = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("==> error:") || lower.starts_with("error:") {
            return Err(format!("AUR update emitted error on stdout: {trimmed}"));
        }
        if trimmed.split_whitespace().count() < 2 {
            return Err(format!("malformed AUR update line: {trimmed}"));
        }
        packages.push(trimmed.to_string());
        count += 1;
    }

    Ok(AurUpdatesReport { count, packages })
}

#[cfg(test)]
thread_local! {
    static TEST_AUR_HELPER_BIN: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static TEST_SYS_HEALTH_BIN: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub fn set_test_updater_bins(helper: Option<PathBuf>, sys_health: Option<PathBuf>) {
    TEST_AUR_HELPER_BIN.with(|b| *b.borrow_mut() = helper);
    TEST_SYS_HEALTH_BIN.with(|b| *b.borrow_mut() = sys_health);
}

pub async fn check_aur_updates() -> u32 {
    let (helper, args) = {
        #[cfg(test)]
        {
            if let Some(p) = TEST_AUR_HELPER_BIN.with(|b| b.borrow().clone()) {
                (p, ["-Qua"])
            } else if let Some(p) = find_executable("yay") {
                (p, ["-Qua"])
            } else if let Some(p) = find_executable("paru") {
                (p, ["-Qua"])
            } else {
                return 0;
            }
        }
        #[cfg(not(test))]
        {
            if let Some(p) = find_executable("yay") {
                (p, ["-Qua"])
            } else if let Some(p) = find_executable("paru") {
                (p, ["-Qua"])
            } else {
                return 0;
            }
        }
    };

    let mut cmd = crate::exec::new_tokio_command(helper);
    cmd.args(args);
    let output = match run_command_with_timeout(cmd, default_checkupdates_timeout()).await {
        Ok(out) => out,
        Err(_) => return 0,
    };

    if !output.status.success() {
        return 0;
    }

    match parse_aur_updates_str(&String::from_utf8_lossy(&output.stdout)) {
        Ok(r) => r.count,
        Err(e) => {
            eprintln!("[syspilot-core::updater] parse_aur_updates_str failed: {e}");
            0
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SoftwareJson {
    #[serde(default)]
    aur: Option<SoftwareAur>,
    #[serde(default)]
    goose: Option<SoftwareTool>,
    #[serde(default)]
    uv: Option<SoftwareTool>,
    #[serde(default)]
    flatpak: Option<SoftwareTool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SoftwareAur {
    #[serde(default)]
    pending_count: u32,
}

#[derive(Debug, Deserialize)]
struct SoftwareTool {
    #[serde(default)]
    update_available: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StandaloneUpdatesReport {
    pub count: u32,
    pub pending_tools: Vec<String>,
}

pub fn parse_standalone_updates_str(stdout: &str) -> Result<StandaloneUpdatesReport, String> {
    if stdout.trim().is_empty() {
        return Err("empty standalone software JSON".to_string());
    }

    let parsed: SoftwareJson =
        serde_json::from_str(stdout).map_err(|e| format!("invalid standalone JSON: {e}"))?;

    let mut count = 0;
    let mut pending_tools = Vec::new();
    if let Some(tool) = &parsed.goose {
        if tool.update_available {
            count += 1;
            pending_tools.push("goose".to_string());
        }
    }
    if let Some(tool) = &parsed.uv {
        if tool.update_available {
            count += 1;
            pending_tools.push("uv".to_string());
        }
    }
    if let Some(tool) = &parsed.flatpak {
        if tool.update_available {
            count += 1;
            pending_tools.push("flatpak".to_string());
        }
    }

    Ok(StandaloneUpdatesReport {
        count,
        pending_tools,
    })
}

pub async fn check_standalone_updates() -> u32 {
    let sys_health_opt = {
        #[cfg(test)]
        {
            TEST_SYS_HEALTH_BIN
                .with(|b| b.borrow().clone())
                .or_else(|| find_executable("sys-health"))
        }
        #[cfg(not(test))]
        {
            find_executable("sys-health")
        }
    };
    let Some(sys_health) = sys_health_opt else {
        return 0;
    };

    let mut cmd = crate::exec::new_tokio_command(sys_health);
    cmd.args(["--software", "--json"]);
    let output = match run_command_with_timeout(cmd, default_standalone_timeout()).await {
        Ok(out) => out,
        Err(_) => return 0,
    };

    if !output.status.success() {
        return 0;
    }

    match parse_standalone_updates_str(&String::from_utf8_lossy(&output.stdout)) {
        Ok(r) => r.count,
        Err(e) => {
            eprintln!("[syspilot-core::updater] parse_standalone_updates_str failed: {e}");
            0
        }
    }
}

pub async fn run_full_update_check(gamemode_active: bool) -> Option<UpdateSummary> {
    if gamemode_active {
        // Strict SRE Invariant: Never perform background disk/network scans during gaming!
        return None;
    }

    let (core_count, regular_count, core_packages) = check_official_updates().await;
    let aur_count = check_aur_updates().await;
    let standalone_count = check_standalone_updates().await;

    Some(UpdateSummary {
        core_count,
        regular_count,
        aur_count,
        standalone_count,
        core_packages,
        last_checked: Some(Instant::now()),
    })
}
