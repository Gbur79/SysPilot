use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

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

fn find_executable(name: &str) -> Option<PathBuf> {
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

pub async fn check_official_updates() -> (u32, u32, Vec<String>) {
    let Some(checkupdates) = find_executable("checkupdates") else {
        return (0, 0, Vec::new());
    };

    let output = match Command::new(&checkupdates)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(out) => out,
        Err(_) => return (0, 0, Vec::new()),
    };

    // Auto-heal stale /tmp/checkup-db-<uid>/db.lck if checkupdates fails on lock
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if stderr.contains("database is locked") {
            let uid = unsafe { libc::getuid() };
            let lock_path = PathBuf::from(format!("/tmp/checkup-db-{uid}/db.lck"));
            if lock_path.exists() {
                let _ = tokio::fs::remove_file(lock_path).await;
                if let Ok(retry) = Command::new(&checkupdates).output().await {
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

    let output = match Command::new(helper)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
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

    let output = match Command::new(sys_health)
        .args(["--software", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
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
