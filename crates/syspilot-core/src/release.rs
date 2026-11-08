use crate::models::state_base;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "Gbur79/SysPilot";
pub const RELEASE_CACHE_TTL_SECS: u64 = 24 * 60 * 60;
pub const RELEASE_FETCH_TIMEOUT_SECS: u64 = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GitHubRelease {
    #[serde(default)]
    pub tag_name: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CachedReleaseInfo {
    #[serde(default)]
    pub timestamp: u64,
    pub release: GitHubRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallOrigin {
    Pacman,
    GitClone,
    Standalone,
}

pub fn parse_version_components(v: &str) -> Vec<u64> {
    let clean = v.trim().trim_start_matches(['v', 'V']);
    clean
        .split('.')
        .filter_map(|part| {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u64>().ok()
        })
        .collect()
}

pub fn is_newer_version(current: &str, upstream: &str) -> bool {
    let curr = parse_version_components(current);
    let ups = parse_version_components(upstream);
    let max_len = curr.len().max(ups.len());
    for i in 0..max_len {
        let c = curr.get(i).copied().unwrap_or(0);
        let u = ups.get(i).copied().unwrap_or(0);
        if u > c {
            return true;
        } else if u < c {
            return false;
        }
    }
    false
}

pub fn get_release_cache_path() -> PathBuf {
    state_base().join("syspilot/github_release.json")
}

pub fn read_cached_release(cache_path: &Path) -> Option<CachedReleaseInfo> {
    if !cache_path.is_file() {
        return None;
    }
    let data = std::fs::read_to_string(cache_path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn write_cached_release(cache_path: &Path, info: &CachedReleaseInfo) -> std::io::Result<()> {
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json_bytes = serde_json::to_vec_pretty(info)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let rand_suffix: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345);
    let tmp_path = cache_path.with_extension(format!("tmp.{}", rand_suffix));
    std::fs::write(&tmp_path, json_bytes)?;
    std::fs::rename(&tmp_path, cache_path)?;
    Ok(())
}

pub async fn fetch_latest_github_release() -> Result<GitHubRelease, String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let mut cmd = crate::exec::new_tokio_command("curl");
    cmd.args([
        "-sS",
        "--fail",
        "--max-time",
        &RELEASE_FETCH_TIMEOUT_SECS.to_string(),
        "-H",
        "User-Agent: SysPilot-Updater",
        &url,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

    let child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn curl: {e}"))?;
    let output = match timeout(
        Duration::from_secs(RELEASE_FETCH_TIMEOUT_SECS + 2),
        child.wait_with_output(),
    )
    .await
    {
        Ok(res) => res.map_err(|e| format!("curl wait failed: {e}"))?,
        Err(_) => {
            return Err(format!(
                "fetch timed out after {RELEASE_FETCH_TIMEOUT_SECS}s"
            ));
        }
    };

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "curl error (exit {}): {}",
            output.status.code().unwrap_or(-1),
            err_msg.trim()
        ));
    }

    serde_json::from_slice::<GitHubRelease>(&output.stdout)
        .map_err(|e| format!("failed to parse GitHub release JSON: {e}"))
}

pub async fn check_upstream_release(force_refresh: bool) -> Option<GitHubRelease> {
    let cache_path = get_release_cache_path();
    let cached = read_cached_release(&cache_path);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if !force_refresh {
        if let Some(ref c) = cached {
            if now.saturating_sub(c.timestamp) < RELEASE_CACHE_TTL_SECS {
                return Some(c.release.clone());
            }
        }
    }

    match fetch_latest_github_release().await {
        Ok(rel) => {
            let info = CachedReleaseInfo {
                timestamp: now,
                release: rel.clone(),
            };
            let _ = write_cached_release(&cache_path, &info);
            Some(rel)
        }
        Err(_) => cached.map(|c| c.release),
    }
}

pub fn detect_install_origin() -> InstallOrigin {
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(output) = crate::exec::new_std_command("pacman")
            .args(["-Qo", &exe.to_string_lossy()])
            .output()
        {
            if output.status.success() {
                return InstallOrigin::Pacman;
            }
        }
    }
    if std::path::Path::new("/usr/bin/syspilot").exists() {
        if let Ok(output) = crate::exec::new_std_command("pacman")
            .args(["-Qo", "/usr/bin/syspilot"])
            .output()
        {
            if output.status.success() {
                return InstallOrigin::Pacman;
            }
        }
    }

    if let Ok(mut current) = std::env::current_exe() {
        while current.pop() {
            if current.join(".git").exists() {
                return InstallOrigin::GitClone;
            }
        }
    }
    if let Ok(mut current) = std::env::current_dir() {
        loop {
            if current.join(".git").exists() {
                return InstallOrigin::GitClone;
            }
            if !current.pop() {
                break;
            }
        }
    }

    InstallOrigin::Standalone
}
