use crate::sys_root::sub_path;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PcieLinkStatus {
    #[serde(default)]
    pub bdf: String,
    #[serde(default)]
    pub current_width: Option<u16>,
    #[serde(default)]
    pub effective_max_width: Option<u16>,
    #[serde(default)]
    pub current_speed: String,
    #[serde(default)]
    pub max_speed: String,
    #[serde(default)]
    pub runtime_suspended: bool,
    #[serde(default)]
    pub aer_fatal: u32,
    #[serde(default)]
    pub aer_nonfatal: u32,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DmiBiosInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub date: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct RebootSafetyStatus {
    #[serde(default)]
    pub running_kernel: String,
    #[serde(default)]
    pub installed_kernels: Vec<String>,
    #[serde(default)]
    pub running_modules_present: bool,
    #[serde(default)]
    pub reboot_pending: bool,
    #[serde(default)]
    pub active_gpu_drivers: Vec<String>,
    #[serde(default)]
    pub missing_gpu_modules: Vec<String>,
    #[serde(default)]
    pub modules_dep_missing: bool,
    #[serde(default)]
    pub initramfs_empty: bool,
    #[serde(default)]
    pub black_screen_risk: bool,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default)]
    pub pcie_degraded: bool,
    #[serde(default)]
    pub pcie_links: Vec<PcieLinkStatus>,
    #[serde(default)]
    pub bios_info: Option<DmiBiosInfo>,
}

pub fn check_reboot_safety() -> RebootSafetyStatus {
    check_reboot_safety_at(Path::new("/"))
}

pub fn check_reboot_safety_at(root: &Path) -> RebootSafetyStatus {
    let running_kernel = fs::read_to_string(sub_path(root, "/proc/sys/kernel/osrelease"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let modules_base = sub_path(root, "/usr/lib/modules");
    let mut all_dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(&modules_base) {
        let mut sorted: Vec<_> = entries.flatten().collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                all_dirs.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }

    let has_any_real = all_dirs.iter().any(|kver| {
        let kdir = modules_base.join(kver);
        is_real_kernel_dir(&kdir) || (!running_kernel.is_empty() && kver == &running_kernel)
    });

    let installed_kernels: Vec<String> = if has_any_real {
        all_dirs
            .into_iter()
            .filter(|kver| {
                let kdir = modules_base.join(kver);
                is_real_kernel_dir(&kdir) || (!running_kernel.is_empty() && kver == &running_kernel)
            })
            .collect()
    } else {
        all_dirs
    };

    let running_modules_present =
        !running_kernel.is_empty() && modules_base.join(&running_kernel).is_dir();
    let reboot_pending =
        !running_modules_present || sub_path(root, "/var/run/reboot-required").exists();

    let active_gpu_drivers = detect_active_gpu_drivers_at(root);
    let mut missing_gpu_modules = Vec::new();
    let mut modules_dep_missing = false;
    let mut issues = Vec::new();

    for kver in &installed_kernels {
        let kdir = modules_base.join(kver);
        let dep_path = kdir.join("modules.dep");
        let dep_content = fs::read_to_string(&dep_path).ok();
        let builtin_content = fs::read_to_string(kdir.join("modules.builtin")).ok();
        if dep_content.is_none() {
            modules_dep_missing = true;
            issues.push(format!(
                "Missing modules.dep in /usr/lib/modules/{kver} (incomplete depmod)"
            ));
        }

        for drv in &active_gpu_drivers {
            let in_dep = dep_content
                .as_deref()
                .map(|c| module_list_contains(c, drv))
                .unwrap_or(false);
            let in_builtin = builtin_content
                .as_deref()
                .map(|c| module_list_contains(c, drv))
                .unwrap_or(false);

            let has_valid_ko = has_valid_module_file(&kdir, drv, 1, 6);
            if !in_dep && !in_builtin && !has_valid_ko {
                missing_gpu_modules.push(format!("{drv} ({kver})"));
                issues.push(format!(
                    "Active GPU kernel module '{drv}' missing for kernel {kver}"
                ));
            }
        }
    }

    let mut initramfs_empty = false;
    let boot_dir = sub_path(root, "/boot");
    if let Ok(entries) = fs::read_dir(boot_dir) {
        let mut sorted: Vec<_> = entries.flatten().collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with("initramfs-") && fname.ends_with(".img") {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() && meta.len() == 0 {
                        initramfs_empty = true;
                        issues.push(format!("Empty 0-byte initramfs image in /boot/{fname}"));
                    }
                }
            }
        }
    }

    let black_screen_risk =
        modules_dep_missing || !missing_gpu_modules.is_empty() || initramfs_empty;
    let pcie_links = audit_gpu_pcie_links_at(root);
    let pcie_degraded = pcie_links.iter().any(|l| l.degraded);
    let bios_info = read_dmi_bios_info_at(root);

    RebootSafetyStatus {
        running_kernel,
        installed_kernels,
        running_modules_present,
        reboot_pending,
        active_gpu_drivers,
        missing_gpu_modules,
        modules_dep_missing,
        initramfs_empty,
        black_screen_risk,
        issues,
        pcie_degraded,
        pcie_links,
        bios_info,
    }
}

pub fn read_dmi_bios_info_at(root: &Path) -> Option<DmiBiosInfo> {
    let version = fs::read_to_string(sub_path(root, "/sys/class/dmi/id/bios_version"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let date = fs::read_to_string(sub_path(root, "/sys/class/dmi/id/bios_date"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if version.is_empty() && date.is_empty() {
        return None;
    }
    Some(DmiBiosInfo { version, date })
}

fn parse_pci_link_width(raw: &str) -> Option<u16> {
    raw.trim()
        .parse::<u16>()
        .ok()
        .filter(|&w| (1..=254).contains(&w))
}

fn parse_pci_aer_counter(content: &str, total_key: &str) -> u32 {
    let mut val = 0u32;
    for line in content.lines() {
        let trimmed = line.trim();
        let mut parts = trimmed.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(k), Some(num)) if k == total_key => {
                if let Ok(parsed) = num.parse::<u32>() {
                    val = parsed;
                }
            }
            (Some(num), None) => {
                if let Ok(parsed) = num.parse::<u32>() {
                    val = parsed;
                }
            }
            _ => {}
        }
    }
    val
}

pub fn audit_gpu_pcie_links_at(root: &Path) -> Vec<PcieLinkStatus> {
    let mut results = Vec::new();
    let pci_dir = sub_path(root, "/sys/bus/pci/devices");
    let Ok(entries) = fs::read_dir(&pci_dir) else {
        return results;
    };
    let canon_root = fs::canonicalize(root).ok();
    let canon_devices_dir = fs::canonicalize(&pci_dir).ok();

    let mut sorted: Vec<_> = entries.flatten().collect();
    sorted.sort_by_key(|e| e.file_name());

    for entry in sorted {
        let dev_path = entry.path();
        let Ok(class_str) = fs::read_to_string(dev_path.join("class")) else {
            continue;
        };
        if !class_str.trim().to_ascii_lowercase().starts_with("0x03") {
            continue;
        }
        let bdf = entry.file_name().to_string_lossy().to_string();
        let runtime_suspended = fs::read_to_string(dev_path.join("power/runtime_status"))
            .map(|s| s.trim().eq_ignore_ascii_case("suspended"))
            .unwrap_or(false);

        let current_width = fs::read_to_string(dev_path.join("current_link_width"))
            .ok()
            .and_then(|s| parse_pci_link_width(&s));
        let dev_max_width = fs::read_to_string(dev_path.join("max_link_width"))
            .ok()
            .and_then(|s| parse_pci_link_width(&s));
        let current_speed = fs::read_to_string(dev_path.join("current_link_speed"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let max_speed = fs::read_to_string(dev_path.join("max_link_speed"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let parent_max_width = match (&canon_root, &canon_devices_dir, fs::canonicalize(&dev_path))
        {
            (Some(cr), Some(cd), Ok(canon_dev))
                if canon_dev.starts_with(cr) && canon_dev.parent() != Some(cd.as_path()) =>
            {
                canon_dev
                    .parent()
                    .and_then(|p| fs::read_to_string(p.join("max_link_width")).ok())
                    .and_then(|s| parse_pci_link_width(&s))
            }
            _ => None,
        };
        let effective_max_width = match (dev_max_width, parent_max_width) {
            (Some(d), Some(p)) => Some(d.min(p)),
            (d, _) => d,
        };

        let mut aer_fatal = 0u32;
        let mut aer_nonfatal = 0u32;
        let mut degraded = false;
        let mut detail_parts = Vec::new();

        if !runtime_suspended {
            if let (Some(cw), Some(mw)) = (current_width, effective_max_width) {
                if cw < mw {
                    degraded = true;
                    let spd = if current_speed.is_empty() || max_speed.is_empty() {
                        String::new()
                    } else {
                        format!(" ({current_speed} / {max_speed})")
                    };
                    detail_parts.push(format!("{bdf}: PCIe link degraded x{cw} < x{mw}{spd}"));
                }
            }
            aer_fatal = fs::read_to_string(dev_path.join("aer_dev_fatal"))
                .map(|c| parse_pci_aer_counter(&c, "TOTAL_ERR_FATAL"))
                .unwrap_or(0);
            aer_nonfatal = fs::read_to_string(dev_path.join("aer_dev_nonfatal"))
                .map(|c| parse_pci_aer_counter(&c, "TOTAL_ERR_NONFATAL"))
                .unwrap_or(0);
            if aer_fatal > 0 {
                degraded = true;
                detail_parts.push(format!("{bdf}: aer_dev_fatal={aer_fatal}"));
            }
            if aer_nonfatal > 0 {
                degraded = true;
                detail_parts.push(format!("{bdf}: aer_dev_nonfatal={aer_nonfatal}"));
            }
        }

        let detail = (!detail_parts.is_empty()).then(|| detail_parts.join("; "));
        results.push(PcieLinkStatus {
            bdf,
            current_width,
            effective_max_width,
            current_speed,
            max_speed,
            runtime_suspended,
            aer_fatal,
            aer_nonfatal,
            degraded,
            detail,
        });
    }
    results
}

fn is_real_kernel_dir(dir: &Path) -> bool {
    dir.join("vmlinuz").is_file()
        || dir.join("pkgbase").is_file()
        || dir.join("modules.builtin").is_file()
        || dir.join("kernel").is_dir()
}

fn module_list_contains(content: &str, drv: &str) -> bool {
    content.lines().any(|line| {
        let left = line.split(':').next().unwrap_or("").trim();
        left.ends_with(&format!("/{drv}.ko"))
            || left.ends_with(&format!("/{drv}.ko.zst"))
            || left.ends_with(&format!("/{drv}.ko.xz"))
            || left.ends_with(&format!("/{drv}.ko.gz"))
            || left == format!("{drv}.ko")
            || left == format!("{drv}.ko.zst")
            || left == format!("{drv}.ko.xz")
    })
}

pub(crate) fn detect_active_gpu_drivers_at(root: &Path) -> Vec<String> {
    let mut drivers = Vec::new();
    let pci_dir = sub_path(root, "/sys/bus/pci/devices");
    if let Ok(entries) = fs::read_dir(pci_dir) {
        let mut sorted: Vec<_> = entries.flatten().collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            let dev_path = entry.path();
            if let Ok(class_str) = fs::read_to_string(dev_path.join("class")) {
                if class_str.trim().to_ascii_lowercase().starts_with("0x03") {
                    if let Ok(link) = fs::read_link(dev_path.join("driver")) {
                        if let Some(drv) = link.file_name().and_then(|n| n.to_str()) {
                            let drv_lower = drv.trim().to_ascii_lowercase();
                            if !drv_lower.is_empty() && !drivers.contains(&drv_lower) {
                                drivers.push(drv_lower);
                            }
                        }
                    }
                }
            }
        }
    }

    if sub_path(root, "/proc/driver/nvidia/version").is_file()
        && !drivers.iter().any(|d| d == "nvidia")
    {
        drivers.push("nvidia".to_string());
    }

    drivers
}

fn has_valid_module_file(dir: &Path, driver: &str, depth: usize, max_depth: usize) -> bool {
    if depth > max_depth {
        return false;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let targets = [
        format!("{driver}.ko"),
        format!("{driver}.ko.zst"),
        format!("{driver}.ko.xz"),
        format!("{driver}.ko.gz"),
    ];

    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            if depth < max_depth && has_valid_module_file(&path, driver, depth + 1, max_depth) {
                return true;
            }
        } else if ft.is_file() {
            let fname = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if targets.iter().any(|t| t == &fname) {
                if let Ok(meta) = entry.metadata() {
                    if meta.len() == 0 {
                        continue;
                    }
                    if fname.ends_with(".zst") && meta.len() >= 4 {
                        if let Ok(mut f) = fs::File::open(&path) {
                            let mut magic = [0u8; 4];
                            if f.read_exact(&mut magic).is_ok() && magic == [0x28, 0xb5, 0x2f, 0xfd]
                            {
                                return true;
                            }
                        }
                        continue;
                    }
                    return true;
                }
            }
        }
    }
    false
}
