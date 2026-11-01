use serde::Deserialize;
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
    let cmd = Command::new(&checkupdates);
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
                let retry_cmd = Command::new(&checkupdates);
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

pub fn parse_checkupdates_stdout(stdout: &[u8]) -> (u32, u32, Vec<String>) {
    let mut core_count = 0;
    let mut regular_count = 0;
    let mut core_packages = Vec::new();

    let text = String::from_utf8_lossy(stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let pkg_name = trimmed.split_whitespace().next().unwrap_or("");
        if is_core_package(pkg_name) {
            core_count += 1;
            core_packages.push(trimmed.to_string());
        } else {
            regular_count += 1;
        }
    }

    (core_count, regular_count, core_packages)
}

pub async fn check_aur_updates() -> u32 {
    let (helper, args) = if let Some(p) = find_executable("yay") {
        (p, ["-Qua"])
    } else if let Some(p) = find_executable("paru") {
        (p, ["-Qua"])
    } else {
        return 0;
    };

    let mut cmd = Command::new(helper);
    cmd.args(args);
    let output = match run_command_with_timeout(cmd, default_checkupdates_timeout()).await {
        Ok(out) => out,
        Err(_) => return 0,
    };

    if !output.status.success() {
        return 0;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().filter(|l| !l.trim().is_empty()).count() as u32
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

pub async fn check_standalone_updates() -> u32 {
    let Some(sys_health) = find_executable("sys-health") else {
        return 0;
    };

    let mut cmd = Command::new(sys_health);
    cmd.args(["--software", "--json"]);
    let output = match run_command_with_timeout(cmd, default_standalone_timeout()).await {
        Ok(out) => out,
        Err(_) => return 0,
    };

    if !output.status.success() {
        return 0;
    }

    let parsed: SoftwareJson = match serde_json::from_slice(&output.stdout) {
        Ok(p) => p,
        Err(_) => return 0,
    };

    let mut count = 0;
    if let Some(goose) = parsed.goose {
        if goose.update_available {
            count += 1;
        }
    }
    if let Some(uv) = parsed.uv {
        if uv.update_available {
            count += 1;
        }
    }
    if let Some(flatpak) = parsed.flatpak {
        if flatpak.update_available {
            count += 1;
        }
    }

    count
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
