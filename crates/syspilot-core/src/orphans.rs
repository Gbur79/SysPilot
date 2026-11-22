use crate::models::OrphanStatus;
use crate::sys_root::sub_path;
#[cfg(test)]
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::time::timeout;

#[cfg(test)]
thread_local! {
    static TEST_ORPHANS_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub fn set_test_orphans_root(root: Option<PathBuf>) {
    TEST_ORPHANS_ROOT.with(|r| *r.borrow_mut() = root);
}

fn active_orphans_root() -> PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_ORPHANS_ROOT.with(|r| r.borrow().clone()) {
        return root;
    }
    PathBuf::from("/")
}

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
    if let Some(inner) = pattern.strip_prefix('*').and_then(|s| s.strip_suffix('*')) {
        text.contains(inner)
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

pub fn list_local_pacman_db_entries_at(root: &Path) -> Vec<String> {
    let db_dir = sub_path(root, "/var/lib/pacman/local");
    let Ok(entries) = std::fs::read_dir(db_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

pub fn parse_pacman_package_list(stdout: &str) -> Result<Vec<String>, String> {
    let mut set = BTreeSet::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("error:") || lower.starts_with("warning:") {
            return Err(format!("pacman diagnostic on stdout: {trimmed}"));
        }
        if trimmed.split_whitespace().count() != 1 {
            return Err(format!("malformed pacman package token: {trimmed}"));
        }
        set.insert(trimmed.to_string());
    }
    Ok(set.into_iter().collect())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PacmanPackageInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub install_reason: String,
}

pub fn parse_pacman_qi_str(stdout: &str) -> Result<Vec<PacmanPackageInfo>, String> {
    if stdout.trim().is_empty() {
        return Err("empty pacman -Qi output".to_string());
    }

    let mut packages = Vec::new();
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut description: Option<String> = None;
    let mut install_reason: Option<String> = None;
    let mut has_fields = false;

    for line in stdout.lines().chain(std::iter::once("")) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if has_fields {
                let Some(n) = name.take() else {
                    return Err("pacman stanza missing Name field".to_string());
                };
                let Some(v) = version.take() else {
                    return Err("pacman stanza missing Version field".to_string());
                };
                if n.is_empty() || v.is_empty() {
                    return Err("pacman stanza has empty Name or Version".to_string());
                }
                packages.push(PacmanPackageInfo {
                    name: n,
                    version: v,
                    description: description.take().unwrap_or_default(),
                    install_reason: install_reason.take().unwrap_or_default(),
                });
                has_fields = false;
            }
            continue;
        }

        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }

        let Some((key, val)) = trimmed.split_once(':') else {
            return Err(format!("malformed line in pacman -Qi output: {trimmed}"));
        };
        has_fields = true;
        match key.trim() {
            "Name" => name = Some(val.trim().to_string()),
            "Version" => version = Some(val.trim().to_string()),
            "Description" => description = Some(val.trim().to_string()),
            "Install Reason" => install_reason = Some(val.trim().to_string()),
            _ => {}
        }
    }

    if packages.is_empty() {
        return Err("no package stanzas found in pacman -Qi output".to_string());
    }

    Ok(packages)
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

            if output.status.success() {
                return parse_pacman_package_list(&stdout_str);
            }

            // pacman returns code 1 when no packages match the filter
            if output.status.code() == Some(1)
                && stdout_str.trim().is_empty()
                && stderr_str.is_empty()
            {
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
    let tiering_complete = strict_ok && extended_ok;

    let strict_set: BTreeSet<String> = strict_pkgs.unwrap_or_default().into_iter().collect();
    let extended_set: BTreeSet<String> = extended_pkgs.unwrap_or_default().into_iter().collect();

    let candidate_set: BTreeSet<String> = strict_set.union(&extended_set).cloned().collect();

    let optional_set: BTreeSet<String> = if tiering_complete {
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
    let query_status = if tiering_complete {
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
    } else if candidate_set.is_empty() {
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
        tiering_complete,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AlpmPackageDesc {
    pub name: String,
    pub version: String,
    pub reason: u8,
    pub depends: Vec<String>,
    pub optdepends: Vec<String>,
    pub provides: Vec<String>,
}

pub fn extract_alpm_dep_name(raw: &str) -> String {
    raw.split(['>', '<', '=', ':'])
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn parse_alpm_desc_str(content: &str) -> Result<AlpmPackageDesc, String> {
    let mut section = "";
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut reason: u8 = 0;
    let mut depends = Vec::new();
    let mut optdepends = Vec::new();
    let mut provides = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('%') && trimmed.ends_with('%') {
            section = trimmed;
            continue;
        }
        match section {
            "%NAME%" => name = Some(trimmed.to_string()),
            "%VERSION%" => version = Some(trimmed.to_string()),
            "%REASON%" => {
                reason = match trimmed {
                    "0" => 0,
                    "1" => 1,
                    other => return Err(format!("invalid ALPM %REASON%: {other}")),
                };
            }
            "%DEPENDS%" => {
                let dep = extract_alpm_dep_name(trimmed);
                if !dep.is_empty() {
                    depends.push(dep);
                }
            }
            "%OPTDEPENDS%" => {
                let dep = extract_alpm_dep_name(trimmed);
                if !dep.is_empty() {
                    optdepends.push(dep);
                }
            }
            "%PROVIDES%" => {
                let prov = extract_alpm_dep_name(trimmed);
                if !prov.is_empty() {
                    provides.push(prov);
                }
            }
            _ => {}
        }
    }

    let Some(name) = name.filter(|s| !s.is_empty()) else {
        return Err("ALPM desc missing %NAME%".to_string());
    };
    let Some(version) = version.filter(|s| !s.is_empty()) else {
        return Err("ALPM desc missing %VERSION%".to_string());
    };

    Ok(AlpmPackageDesc {
        name,
        version,
        reason,
        depends,
        optdepends,
        provides,
    })
}

pub fn read_alpm_local_db_at(root: &Path) -> Result<Vec<AlpmPackageDesc>, String> {
    let db_dir = sub_path(root, "/var/lib/pacman/local");
    let entries = std::fs::read_dir(&db_dir)
        .map_err(|e| format!("failed to read ALPM local db {}: {e}", db_dir.display()))?;
    let mut pkgs = Vec::new();
    for entry_res in entries {
        let entry = entry_res.map_err(|e| format!("failed to read ALPM db entry: {e}"))?;
        let ft = entry
            .file_type()
            .map_err(|e| format!("failed to stat ALPM db entry: {e}"))?;
        if !ft.is_dir() {
            continue;
        }
        let desc_path = entry.path().join("desc");
        let content = std::fs::read_to_string(&desc_path)
            .map_err(|e| format!("failed to read {}: {e}", desc_path.display()))?;
        pkgs.push(parse_alpm_desc_str(&content)?);
    }
    pkgs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(pkgs)
}

fn record_alpm_req<'a>(map: &mut HashMap<&'a str, Option<usize>>, dep: &'a str, owner_idx: usize) {
    map.entry(dep)
        .and_modify(|existing| {
            if *existing != Some(owner_idx) {
                *existing = None;
            }
        })
        .or_insert(Some(owner_idx));
}

fn is_required_by_other(map: &HashMap<&str, Option<usize>>, key: &str, self_idx: usize) -> bool {
    map.get(key).is_some_and(|owner| *owner != Some(self_idx))
}

pub fn compute_alpm_orphans(pkgs: &[AlpmPackageDesc]) -> (Vec<String>, Vec<String>) {
    let mut hard_reqs: HashMap<&str, Option<usize>> = HashMap::new();
    let mut opt_reqs: HashMap<&str, Option<usize>> = HashMap::new();

    for (idx, pkg) in pkgs.iter().enumerate() {
        for dep in &pkg.depends {
            record_alpm_req(&mut hard_reqs, dep.as_str(), idx);
        }
        for opt in &pkg.optdepends {
            record_alpm_req(&mut opt_reqs, opt.as_str(), idx);
        }
    }

    let mut strict = Vec::new();
    let mut extended = Vec::new();
    for (idx, pkg) in pkgs.iter().enumerate() {
        if pkg.reason != 1 {
            continue;
        }
        let has_hard = is_required_by_other(&hard_reqs, &pkg.name, idx)
            || pkg
                .provides
                .iter()
                .any(|p| is_required_by_other(&hard_reqs, p, idx));
        if !has_hard {
            extended.push(pkg.name.clone());
            let has_opt = is_required_by_other(&opt_reqs, &pkg.name, idx)
                || pkg
                    .provides
                    .iter()
                    .any(|p| is_required_by_other(&opt_reqs, p, idx));
            if !has_opt {
                strict.push(pkg.name.clone());
            }
        }
    }
    strict.sort();
    extended.sort();
    (strict, extended)
}

pub fn check_orphan_packages_at(root: &Path) -> OrphanStatus {
    match read_alpm_local_db_at(root) {
        Ok(pkgs) => {
            let (strict, extended) = compute_alpm_orphans(&pkgs);
            classify_orphans(Ok(strict), Ok(extended))
        }
        Err(err) => {
            eprintln!("[syspilot-core::orphans] ALPM local DB read failed: {err}");
            classify_orphans(Err(err), Ok(Vec::new()))
        }
    }
}

pub async fn check_orphan_packages() -> OrphanStatus {
    let root = active_orphans_root();
    tokio::task::spawn_blocking(move || check_orphan_packages_at(&root))
        .await
        .unwrap_or_else(|err| {
            eprintln!("[syspilot-core::orphans] ALPM scan worker failed: {err}");
            classify_orphans(
                Err(format!("orphan worker task failed: {err}")),
                Ok(Vec::new()),
            )
        })
}
