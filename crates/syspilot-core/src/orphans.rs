use crate::models::OrphanStatus;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::time::timeout;

pub const ORPHAN_SENSITIVE_PATTERNS: &[(&str, &[&str])] = &[
    (
        "kernel / bootloader",
        &[
            "linux*",
            "*-headers",
            "grub*",
            "systemd*",
            "dracut*",
            "mkinitcpio*",
            "booster*",
            "limine*",
            "refind*",
            "efibootmgr*",
            "syslinux*",
        ],
    ),
    (
        "driver / firmware / graphics",
        &[
            "*firmware*",
            "*-ucode",
            "*-dkms",
            "nvidia*",
            "mesa*",
            "vulkan*",
            "xf86-video*",
            "intel-media-driver",
            "libva*",
            "libvdpau*",
            "xorg-server*",
            "xorg-xwayland*",
        ],
    ),
    (
        "audio subsystem",
        &[
            "pipewire*",
            "wireplumber*",
            "alsa-*",
            "pulseaudio*",
            "jack*",
        ],
    ),
    (
        "development toolchain",
        &[
            "base-devel",
            "rust",
            "cargo",
            "go",
            "gcc*",
            "clang*",
            "llvm*",
            "make",
            "cmake",
            "patch",
            "git",
            "fakeroot",
            "binutils",
        ],
    ),
    (
        "desktop / compositor / display manager",
        &[
            "wayland*",
            "hyprland*",
            "kwin*",
            "plasma-*",
            "sway*",
            "mutter*",
            "weston*",
            "sddm*",
            "gdm*",
            "lightdm*",
        ],
    ),
    (
        "storage / filesystem / crypto",
        &[
            "btrfs-progs*",
            "dosfstools",
            "e2fsprogs",
            "xfsprogs*",
            "zfs*",
            "cryptsetup*",
            "lvm2*",
            "mdadm*",
            "device-mapper*",
        ],
    ),
    (
        "core security / auth / network",
        &[
            "polkit*",
            "shadow",
            "sudo",
            "pam",
            "networkmanager*",
            "iwd*",
            "wpa_supplicant*",
        ],
    ),
    ("multilib / 32-bit runtime", &["lib32-*"]),
];

pub fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern.starts_with('*') && pattern.ends_with('*') && pattern.len() >= 2 {
        text.contains(&pattern[1..pattern.len() - 1])
    } else if let Some(suffix) = pattern.strip_prefix('*') {
        text.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        text.starts_with(prefix)
    } else {
        text == pattern
    }
}

pub fn orphan_sensitive_tag(package: &str) -> Option<&'static str> {
    let lower = package.trim().to_ascii_lowercase();
    for (tag, patterns) in ORPHAN_SENSITIVE_PATTERNS {
        for pattern in *patterns {
            if glob_match(pattern, &lower) {
                return Some(tag);
            }
        }
    }
    None
}

pub async fn query_pacman_package_names(
    pacman_path: &Path,
    query_flag: &str,
    timeout_secs: u64,
) -> Result<Vec<String>, String> {
    let run_cmd = async {
        let mut cmd = crate::exec::new_tokio_command(pacman_path);
        cmd.arg(query_flag)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = cmd.spawn()?;
        child.wait_with_output().await
    };

    match timeout(Duration::from_secs(timeout_secs), run_cmd).await {
        Ok(Ok(output)) => {
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            let stderr_str = String::from_utf8_lossy(&output.stderr).trim().to_string();

            let pkgs: Vec<String> = stdout_str
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();

            if output.status.success() {
                let set: BTreeSet<String> = pkgs.into_iter().collect();
                return Ok(set.into_iter().collect());
            }

            // pacman returns code 1 when no packages match the filter
            if output.status.code() == Some(1) && pkgs.is_empty() && stderr_str.is_empty() {
                return Ok(Vec::new());
            }

            let detail = if stderr_str.is_empty() {
                "no diagnostic text".to_string()
            } else {
                stderr_str
            };
            Err(format!(
                "{} exited {}: {}",
                query_flag,
                output.status.code().unwrap_or(-1),
                detail
            ))
        }
        Ok(Err(e)) => Err(format!("{query_flag} could not execute: {e}")),
        Err(_) => Err(format!("{query_flag} timed out after {timeout_secs}s")),
    }
}

pub fn classify_orphans(
    strict_pkgs: Result<Vec<String>, String>,
    extended_pkgs: Result<Vec<String>, String>,
) -> OrphanStatus {
    let strict_ok = strict_pkgs.is_ok();
    let extended_ok = extended_pkgs.is_ok();

    let strict_set: BTreeSet<String> = strict_pkgs.unwrap_or_default().into_iter().collect();
    let extended_set: BTreeSet<String> = extended_pkgs.unwrap_or_default().into_iter().collect();

    let candidate_set: BTreeSet<String> = strict_set.union(&extended_set).cloned().collect();

    let optional_set: BTreeSet<String> = if strict_ok && extended_ok {
        extended_set.difference(&strict_set).cloned().collect()
    } else {
        BTreeSet::new()
    };

    let mut tier1 = Vec::new();
    let mut tier2 = Vec::new();
    let mut tier3 = Vec::new();
    let mut tier3_tags = BTreeMap::new();
    let mut unclassified = Vec::new();

    for pkg in &candidate_set {
        if let Some(tag) = orphan_sensitive_tag(pkg) {
            tier3.push(pkg.clone());
            tier3_tags.insert(pkg.clone(), tag.to_string());
        } else if !strict_ok {
            unclassified.push(pkg.clone());
        } else if strict_set.contains(pkg) {
            tier1.push(pkg.clone());
        } else {
            tier2.push(pkg.clone());
        }
    }

    let checked = strict_ok;
    let query_status = if strict_ok && extended_ok {
        "CLEAN".to_string()
    } else if strict_ok {
        "PARTIAL".to_string()
    } else {
        "ERROR".to_string()
    };

    let severity = if checked {
        "INFO".to_string()
    } else {
        "WARN".to_string()
    };
    let affects_flight_status = !checked;

    let message = if !checked {
        "Orphan query unavailable: pacman query error".to_string()
    } else if tier1.is_empty() && tier2.is_empty() && tier3.is_empty() {
        "No orphan packages detected".to_string()
    } else {
        format!(
            "{} strict orphan(s), {} optional candidate(s), {} sensitive",
            tier1.len(),
            tier2.len(),
            tier3.len()
        )
    };

    OrphanStatus {
        checked,
        query_status,
        severity,
        affects_flight_status,
        tiering_complete: strict_ok && extended_ok,
        count: strict_set.len(),
        packages: strict_set.into_iter().collect(),
        optional_count: optional_set.len(),
        optional_packages: optional_set.into_iter().collect(),
        candidate_count: candidate_set.len(),
        tier1_count: tier1.len(),
        tier1_packages: tier1,
        tier2_count: tier2.len(),
        tier2_packages: tier2,
        tier3_count: tier3.len(),
        tier3_packages: tier3,
        tier3_tags,
        unclassified_packages: unclassified,
        message,
    }
}

pub async fn check_orphan_packages() -> OrphanStatus {
    let pacman_path = match crate::updater::find_executable("pacman") {
        Some(p) => p,
        None => {
            return OrphanStatus {
                checked: false,
                query_status: "ERROR".into(),
                severity: "WARN".into(),
                affects_flight_status: true,
                tiering_complete: false,
                count: 0,
                packages: Vec::new(),
                optional_count: 0,
                optional_packages: Vec::new(),
                candidate_count: 0,
                tier1_count: 0,
                tier1_packages: Vec::new(),
                tier2_count: 0,
                tier2_packages: Vec::new(),
                tier3_count: 0,
                tier3_packages: Vec::new(),
                tier3_tags: BTreeMap::new(),
                unclassified_packages: Vec::new(),
                message: "Orphan query unavailable: pacman executable not found".into(),
            };
        }
    };

    let strict = query_pacman_package_names(&pacman_path, "-Qdtq", 3).await;
    let extended = query_pacman_package_names(&pacman_path, "-Qdttq", 3).await;

    classify_orphans(strict, extended)
}
