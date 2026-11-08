use crate::sys_root::sub_path;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct GamingReadinessStatus {
    #[serde(default)]
    pub applicable: bool,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub vulkan_64_ready: bool,
    #[serde(default)]
    pub vulkan_32_ready: bool,
    #[serde(default)]
    pub cpu_governor: Option<String>,
    #[serde(default)]
    pub hybrid_gpu_detected: bool,
    #[serde(default)]
    pub dgpu_runtime_status: Option<String>,
    #[serde(default)]
    pub on_battery: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

pub fn check_gaming_readiness() -> GamingReadinessStatus {
    check_gaming_readiness_at(Path::new("/"))
}

pub fn check_gaming_readiness_at(root: &Path) -> GamingReadinessStatus {
    let mut display_devices = Vec::new();
    let pci_dir = sub_path(root, "/sys/bus/pci/devices");
    if let Ok(entries) = fs::read_dir(pci_dir) {
        let mut sorted: Vec<_> = entries.flatten().collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            let dev_path = entry.path();
            if let Ok(class_str) = fs::read_to_string(dev_path.join("class")) {
                if class_str.trim().to_ascii_lowercase().starts_with("0x03") {
                    let vendor = fs::read_to_string(dev_path.join("vendor"))
                        .unwrap_or_default()
                        .trim()
                        .to_ascii_lowercase();
                    let runtime_status = fs::read_to_string(dev_path.join("power/runtime_status"))
                        .ok()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    display_devices.push((vendor, runtime_status));
                }
            }
        }
    }

    let has_nv_proc = sub_path(root, "/proc/driver/nvidia/version").is_file();
    if display_devices.is_empty() && !has_nv_proc {
        return GamingReadinessStatus {
            applicable: false,
            ready: true,
            ..Default::default()
        };
    }

    let active_drivers = crate::reboot_safety::detect_active_gpu_drivers_at(root);
    let mut vulkan_64_ready = false;
    let mut vulkan_32_ready = false;
    let icd_dir = sub_path(root, "/usr/share/vulkan/icd.d");
    if let Ok(entries) = fs::read_dir(icd_dir) {
        let mut sorted: Vec<_> = entries.flatten().collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !fname.ends_with(".json") {
                continue;
            }
            if !icd_matches_active_drivers(&fname, &active_drivers) {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let lib_path = val
                        .get("ICD")
                        .or_else(|| val.get("icd"))
                        .and_then(|icd| icd.get("library_path"))
                        .and_then(|lp| lp.as_str())
                        .unwrap_or("");
                    if lib_path.is_empty() {
                        continue;
                    }
                    if lib_path.starts_with('/') {
                        if lib_path.starts_with("/usr/lib32/") {
                            if sub_path(root, lib_path).is_file() {
                                vulkan_32_ready = true;
                            }
                        } else if (lib_path.starts_with("/usr/lib/")
                            || lib_path.starts_with("/usr/lib64/"))
                            && sub_path(root, lib_path).is_file()
                        {
                            vulkan_64_ready = true;
                        }
                    } else {
                        // Relative library filename (e.g. Arch Linux nvidia_icd.json -> "libGLX_nvidia.so.0")
                        if sub_path(root, &format!("/usr/lib/{lib_path}")).is_file() {
                            vulkan_64_ready = true;
                        }
                        if sub_path(root, &format!("/usr/lib32/{lib_path}")).is_file() {
                            vulkan_32_ready = true;
                        }
                    }
                }
            }
        }
    }

    let cpu_governor = fs::read_to_string(sub_path(
        root,
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
    ))
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());

    let hybrid_gpu_detected = display_devices.len() >= 2;
    let dgpu_runtime_status = if hybrid_gpu_detected {
        display_devices
            .iter()
            .find(|(ven, _)| ven == "0x10de" || ven == "0x1002")
            .and_then(|(_, st)| st.clone())
            .or_else(|| display_devices.get(1).and_then(|(_, st)| st.clone()))
    } else {
        None
    };

    let mut on_battery = false;
    for bat in [
        "/sys/class/power_supply/BAT0/status",
        "/sys/class/power_supply/BAT1/status",
    ] {
        if let Ok(st) = fs::read_to_string(sub_path(root, bat)) {
            if st.trim().eq_ignore_ascii_case("Discharging") {
                on_battery = true;
                break;
            }
        }
    }

    let mut warnings = Vec::new();
    if !vulkan_64_ready {
        warnings.push("Missing 64-bit Vulkan ICD or driver library in /usr/lib".into());
    }
    if vulkan_64_ready && !vulkan_32_ready {
        warnings.push(
            "Missing 32-bit Vulkan driver library in /usr/lib32 (required for Steam/Proton)".into(),
        );
    }
    if on_battery {
        warnings.push("System running on battery power (GPU/CPU throttling active)".into());
    }

    let ready = vulkan_64_ready && vulkan_32_ready;

    GamingReadinessStatus {
        applicable: true,
        ready,
        vulkan_64_ready,
        vulkan_32_ready,
        cpu_governor,
        hybrid_gpu_detected,
        dgpu_runtime_status,
        on_battery,
        warnings,
    }
}

fn icd_matches_active_drivers(fname: &str, active_drivers: &[String]) -> bool {
    if active_drivers.is_empty() {
        return !fname.contains("lvp");
    }
    let mut any_known_driver = false;
    for drv in active_drivers {
        match drv.as_str() {
            "nvidia" => {
                any_known_driver = true;
                if fname.contains("nvidia") {
                    return true;
                }
            }
            "amdgpu" | "radeon" => {
                any_known_driver = true;
                if fname.contains("radeon") || fname.contains("amd") {
                    return true;
                }
            }
            "i915" | "xe" => {
                any_known_driver = true;
                if fname.contains("intel") {
                    return true;
                }
            }
            "nouveau" => {
                any_known_driver = true;
                if fname.contains("nouveau") || fname.contains("nvk") {
                    return true;
                }
            }
            _ => {}
        }
    }
    if any_known_driver {
        false
    } else {
        !fname.contains("lvp")
    }
}
