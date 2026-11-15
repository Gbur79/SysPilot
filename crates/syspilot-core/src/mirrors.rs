use crate::firmware::parse_timeout_ms;
use crate::models::SysHealthMirrors;
use crate::sys_root::sub_path;
#[cfg(test)]
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};
use tokio::time::timeout;

const DEFAULT_CONTROL_PLANE_URLS: &[&str] = &[
    "https://archlinux.org",
    "https://1.1.1.1",
    "https://cloudflare.com",
];

#[cfg(test)]
thread_local! {
    static TEST_MIRROR_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static TEST_CURL_BIN: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
    static TEST_CONTROL_URLS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub fn set_test_mirror_overrides(
    root: Option<PathBuf>,
    curl_bin: Option<Option<PathBuf>>,
    control_urls: Option<Vec<String>>,
) {
    TEST_MIRROR_ROOT.with(|r| *r.borrow_mut() = root);
    TEST_CURL_BIN.with(|b| *b.borrow_mut() = curl_bin);
    TEST_CONTROL_URLS.with(|u| *u.borrow_mut() = control_urls);
}

pub fn active_mirror_root() -> PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_MIRROR_ROOT.with(|r| r.borrow().clone()) {
        return root;
    }
    PathBuf::from("/")
}

pub fn default_mirror_timeout() -> Duration {
    parse_timeout_ms(
        std::env::var("SYSPILOT_MIRROR_TIMEOUT_MS").ok().as_deref(),
        5500,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredMirrorlist {
    pub repo: String,
    pub kind: &'static str,
    pub path: PathBuf,
    pub servers: Vec<String>,
    pub age_days: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryProbeOutcome {
    Alive { ms: u64 },
    Dead,
    Offline,
    Unavailable,
    Skipped,
}

pub fn classify_mirrorlist_kind(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stem = name
        .trim_end_matches("-mirrorlist")
        .trim_start_matches("mirrorlist-")
        .trim_start_matches("mirrorlist");
    match stem {
        "" | "arch" => "Arch",
        _ => "Distro",
    }
}

pub fn expand_mirror_url(template: &str, repo: &str, arch: &str) -> String {
    template
        .trim()
        .replace("$repo", repo)
        .replace("$arch", arch)
}

pub fn parse_mirrorlist_servers(content: &str) -> Vec<String> {
    let mut servers = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            if key.trim() == "Server" {
                let url = val.trim();
                if !url.is_empty() {
                    servers.push(url.to_string());
                }
            }
        }
    }
    servers
}

pub fn file_age_days(mtime: SystemTime, now: SystemTime) -> f64 {
    match now.duration_since(mtime) {
        Ok(d) => ((d.as_secs_f64() / 86_400.0) * 100.0).round() / 100.0,
        Err(_) => 0.0,
    }
}

pub fn parse_curl_probe_output(raw: &str) -> Option<(u16, u64)> {
    let (code_str, time_str) = raw.trim().split_once('|')?;
    let code = code_str.trim().parse::<u16>().ok()?;
    let secs = time_str.trim().parse::<f64>().ok()?;
    if !(0.0..=3600.0).contains(&secs) {
        return None;
    }
    let ms = (secs * 1000.0).round() as u64;
    Some((code, ms))
}

pub fn discover_mirrorlists_at(root: &Path, now: SystemTime) -> Vec<DiscoveredMirrorlist> {
    let conf_path = sub_path(root, "/etc/pacman.conf");
    let Ok(conf_content) = fs::read_to_string(&conf_path) else {
        return Vec::new();
    };

    let mut discovered: Vec<DiscoveredMirrorlist> = Vec::new();
    let mut current_repo: Option<String> = None;

    for line in conf_content.lines() {
        let trimmed = line.trim();
        // Empty lines and '#' comments naturally fail both `[section]` and `key == "Include"`.
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed[1..trimmed.len() - 1].trim();
            if section.eq_ignore_ascii_case("options") || section.is_empty() {
                current_repo = None;
            } else {
                current_repo = Some(section.to_string());
            }
            continue;
        }

        let Some(ref repo) = current_repo else {
            continue;
        };

        let Some((key, val)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        if key != "Include" || val.is_empty() {
            continue;
        }

        let target_path = sub_path(root, val);
        if let Some(existing) = discovered.iter_mut().find(|d| d.path == target_path) {
            if repo == "core" {
                existing.repo = "core".to_string();
            }
            continue;
        }

        let Ok(ml_content) = fs::read_to_string(&target_path) else {
            continue;
        };
        let servers = parse_mirrorlist_servers(&ml_content);
        let age_days = fs::metadata(&target_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|mtime| file_age_days(mtime, now));
        let kind = classify_mirrorlist_kind(&target_path);

        discovered.push(DiscoveredMirrorlist {
            repo: repo.clone(),
            kind,
            path: target_path,
            servers,
            age_days,
        });
    }

    discovered.sort_by_key(|d| if d.kind == "Arch" { 0 } else { 1 });
    discovered
}

pub fn evaluate_mirror_health(
    lists: &[DiscoveredMirrorlist],
    probe: PrimaryProbeOutcome,
) -> SysHealthMirrors {
    if lists.is_empty() {
        return SysHealthMirrors {
            status: Some("WARN".into()),
            message: Some("no active mirrorlists configured".into()),
            arch_primary_alive: None,
            arch_age_days: None,
            arch_servers: None,
            distro_age_days: None,
            distro_servers: None,
        };
    }

    let mut arch_age_days = None;
    let mut arch_servers = None;
    let mut distro_age_days = None;
    let mut distro_servers = None;
    let mut max_days: f64 = 0.0;
    let mut empty_mirrorlist = false;
    let mut low_redundancy = false;

    let (arch_primary_alive, primary_dead, primary_slow, mut primary_tag) = match probe {
        PrimaryProbeOutcome::Alive { ms } => {
            if ms > 400 {
                (
                    Some(true),
                    false,
                    true,
                    Some(format!("{ms}ms high-latency")),
                )
            } else {
                (Some(true), false, false, Some(format!("{ms}ms")))
            }
        }
        PrimaryProbeOutcome::Dead => (Some(false), true, false, Some("primary DEAD".to_string())),
        PrimaryProbeOutcome::Offline => (None, false, false, Some("offline".to_string())),
        PrimaryProbeOutcome::Unavailable => (None, false, false, Some("probe unavail".to_string())),
        PrimaryProbeOutcome::Skipped => (None, false, false, None),
    };

    let mut labels = Vec::new();
    let total_lists = lists.len();

    for item in lists {
        let srv_cnt = item.servers.len() as u32;
        if srv_cnt == 0 {
            empty_mirrorlist = true;
        } else if srv_cnt < 2 {
            low_redundancy = true;
        }

        let days = item.age_days.unwrap_or(0.0);
        max_days = max_days.max(days);

        if item.kind == "Arch" && arch_servers.is_none() {
            arch_age_days = item.age_days;
            arch_servers = Some(srv_cnt);
        } else if item.kind != "Arch" && distro_servers.is_none() {
            distro_age_days = item.age_days;
            distro_servers = Some(srv_cnt);
        }

        let mut parts = vec![format!("{days:.0}d")];
        if item.kind == "Arch" || total_lists == 1 {
            if let Some(tag) = primary_tag.take() {
                parts.push(tag);
            }
        }
        parts.push(format!("{srv_cnt} srv"));
        labels.push(format!("{}: {}", item.kind, parts.join(", ")));
    }

    let status = if empty_mirrorlist || primary_dead || max_days > 90.0 {
        "WARN"
    } else if primary_slow
        || low_redundancy
        || max_days > 45.0
        || probe == PrimaryProbeOutcome::Offline
    {
        "INFO"
    } else {
        "PASS"
    };

    SysHealthMirrors {
        status: Some(status.into()),
        message: Some(labels.join(" │ ")),
        arch_primary_alive,
        arch_age_days,
        arch_servers,
        distro_age_days,
        distro_servers,
    }
}

fn resolve_curl_bin() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(override_bin) = TEST_CURL_BIN.with(|b| b.borrow().clone()) {
        return override_bin;
    }
    crate::updater::find_executable("curl")
}

pub fn resolve_control_urls() -> Vec<String> {
    #[cfg(test)]
    if let Some(urls) = TEST_CONTROL_URLS.with(|u| u.borrow().clone()) {
        return urls;
    }
    DEFAULT_CONTROL_PLANE_URLS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

async fn probe_control_plane(curl_bin: &Path) -> bool {
    for endpoint in resolve_control_urls() {
        let mut cmd = crate::exec::new_tokio_command(curl_bin);
        cmd.args([
            "-fsSIL",
            "--connect-timeout",
            "2",
            "--max-time",
            "3",
            &endpoint,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
        let Ok(child) = cmd.spawn() else {
            continue;
        };
        if let Ok(Ok(out)) = timeout(Duration::from_millis(3500), child.wait_with_output()).await {
            if out.status.success() {
                return true;
            }
        }
    }
    false
}

pub async fn probe_primary_mirror_at(
    root: &Path,
    lists: &[DiscoveredMirrorlist],
    skip_network: bool,
) -> PrimaryProbeOutcome {
    let Some((repo, raw_url)) = lists
        .iter()
        .find_map(|l| l.servers.first().map(|u| (l.repo.as_str(), u.as_str())))
    else {
        return PrimaryProbeOutcome::Skipped;
    };

    let expanded = expand_mirror_url(raw_url, repo, std::env::consts::ARCH);
    let base_url = expanded.trim_end_matches('/');

    if let Some(local_rel) = base_url.strip_prefix("file://") {
        let local_dir = sub_path(root, local_rel);
        return if local_dir.is_dir() {
            PrimaryProbeOutcome::Alive { ms: 1 }
        } else {
            PrimaryProbeOutcome::Dead
        };
    }

    if skip_network {
        return PrimaryProbeOutcome::Skipped;
    }

    let Some(curl_bin) = resolve_curl_bin() else {
        return PrimaryProbeOutcome::Unavailable;
    };

    let target_url = format!("{base_url}/{repo}.db");
    let run_probe = async {
        let mut cmd = crate::exec::new_tokio_command(&curl_bin);
        cmd.args([
            "-s",
            "-L",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}|%{time_starttransfer}",
            "--connect-timeout",
            "3",
            "--max-time",
            "5",
            &target_url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
        let child = cmd.spawn()?;
        child.wait_with_output().await
    };

    match timeout(default_mirror_timeout(), run_probe).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some((code, ms)) = parse_curl_probe_output(&stdout) {
                if matches!(code, 200 | 301 | 302) {
                    return PrimaryProbeOutcome::Alive { ms };
                }
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "[syspilot-core::mirrors] curl probe failed for {target_url} (rc={:?}): {}",
                output.status.code(),
                stderr.trim()
            );
            if probe_control_plane(&curl_bin).await {
                PrimaryProbeOutcome::Dead
            } else {
                PrimaryProbeOutcome::Offline
            }
        }
        Ok(Err(e)) => {
            eprintln!("[syspilot-core::mirrors] failed to spawn curl: {e}");
            PrimaryProbeOutcome::Unavailable
        }
        Err(_) => {
            eprintln!("[syspilot-core::mirrors] curl probe timed out for {target_url}");
            if probe_control_plane(&curl_bin).await {
                PrimaryProbeOutcome::Dead
            } else {
                PrimaryProbeOutcome::Offline
            }
        }
    }
}

pub async fn check_mirrors_at(root: &Path, skip_network: bool) -> SysHealthMirrors {
    let now = SystemTime::now();
    let lists = discover_mirrorlists_at(root, now);
    if lists.is_empty() {
        return evaluate_mirror_health(&lists, PrimaryProbeOutcome::Skipped);
    }
    let outcome = probe_primary_mirror_at(root, &lists, skip_network).await;
    evaluate_mirror_health(&lists, outcome)
}

pub async fn check_mirrors(skip_network: bool) -> SysHealthMirrors {
    let root = active_mirror_root();
    check_mirrors_at(&root, skip_network).await
}
