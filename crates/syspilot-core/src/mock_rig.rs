use crate::sys_root::sub_path;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct MockRig {
    dir: TempDir,
}

impl MockRig {
    pub fn empty() -> Self {
        let dir = TempDir::new().expect("failed to create MockRig tempdir");
        let rig = Self { dir };
        for base in [
            "/proc/sys/kernel",
            "/sys/bus/pci/devices",
            "/sys/bus/pci/drivers",
            "/sys/class/drm",
            "/sys/devices/system/cpu/cpu0/cpufreq",
            "/usr/lib/modules",
            "/usr/lib32",
            "/usr/share/vulkan/icd.d",
            "/var/lib/pacman/local",
            "/var/lib/dkms",
            "/var/run",
            "/etc",
            "/boot",
        ] {
            fs::create_dir_all(rig.sub(base)).expect("failed to create base mock dir");
        }
        rig
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn sub(&self, abs_path: &str) -> PathBuf {
        sub_path(self.root(), abs_path)
    }

    pub fn write(&self, abs_path: &str, content: impl AsRef<[u8]>) {
        let target = self.sub(abs_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir in MockRig");
        }
        fs::write(target, content).expect("failed to write file in MockRig");
    }

    pub fn symlink(&self, target_abs: &str, link_abs: &str) {
        let target = self.sub(target_abs);
        let link = self.sub(link_abs);
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent).expect("failed to create symlink parent dir");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).expect("failed to create symlink in MockRig");
    }

    /// Archetype 1: Arch Linux + AMD Radeon RDNA3 (in-tree amdgpu.ko.zst, Mesa RADV)
    pub fn arch_amd_radeon() -> Self {
        let rig = Self::empty();
        let kver = "6.12.1-arch1-1";
        rig.write(
            "/etc/os-release",
            "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n",
        );
        rig.write("/proc/sys/kernel/osrelease", format!("{kver}\n"));
        rig.write(
            &format!("/usr/lib/modules/{kver}/kernel/drivers/gpu/drm/amd/amdgpu/amdgpu.ko.zst"),
            [0x28, 0xb5, 0x2f, 0xfd],
        );
        rig.write(
            &format!("/usr/lib/modules/{kver}/modules.dep"),
            "amdgpu.ko.zst:\n",
        );

        rig.write("/sys/bus/pci/devices/0000:03:00.0/class", "0x030000\n");
        rig.write("/sys/bus/pci/devices/0000:03:00.0/vendor", "0x1002\n");
        rig.write("/sys/bus/pci/devices/0000:03:00.0/device", "0x744c\n");
        fs::create_dir_all(rig.sub("/sys/bus/pci/drivers/amdgpu")).unwrap();
        rig.symlink(
            "/sys/bus/pci/drivers/amdgpu",
            "/sys/bus/pci/devices/0000:03:00.0/driver",
        );

        rig.write(
            "/usr/share/vulkan/icd.d/radeon_icd.x86_64.json",
            r#"{"ICD":{"api_version":"1.3.295","library_path":"/usr/lib/libvulkan_radeon.so"}}"#,
        );
        rig.write(
            "/usr/share/vulkan/icd.d/radeon_icd.i686.json",
            r#"{"ICD":{"api_version":"1.3.295","library_path":"/usr/lib32/libvulkan_radeon.so"}}"#,
        );
        rig.write("/usr/lib/libvulkan_radeon.so", b"ELF64");
        rig.write("/usr/lib32/libvulkan_radeon.so", b"ELF32");
        rig
    }

    /// Archetype 2: EndeavourOS + NVIDIA Maxwell DKMS (shared nvidia_icd.json + /usr/lib32)
    pub fn endeavouros_nvidia_dkms() -> Self {
        let rig = Self::empty();
        let kver = "7.2.14-zen1-1-zen";
        rig.write(
            "/etc/os-release",
            "NAME=\"EndeavourOS\"\nPRETTY_NAME=\"EndeavourOS\"\nID=endeavouros\n",
        );
        rig.write("/proc/sys/kernel/osrelease", format!("{kver}\n"));
        rig.write(
            "/proc/driver/nvidia/gpus/0000:01:00.0/information",
            "Model: \t\t NVIDIA GeForce GTX 970\nIRQ:   \t\t 131\n",
        );
        rig.write(
            "/proc/driver/nvidia/version",
            "NVRM version: NVIDIA UNIX x86_64 Kernel Module  580.95.05  Sat Oct 18 00:00:00 UTC 2026\n",
        );
        rig.write(
            &format!("/usr/lib/modules/{kver}/updates/dkms/nvidia.ko.zst"),
            [0x28, 0xb5, 0x2f, 0xfd],
        );
        rig.write(
            &format!("/usr/lib/modules/{kver}/modules.dep"),
            "updates/dkms/nvidia.ko.zst:\n",
        );
        rig.write(
            "/usr/share/vulkan/icd.d/nvidia_icd.json",
            r#"{"file_format_version":"1.0.1","ICD":{"library_path":"libGLX_nvidia.so.0","api_version":"1.4.312"}}"#,
        );
        rig.write("/usr/lib/libGLX_nvidia.so.0", b"ELF64");
        rig.write("/usr/lib32/libGLX_nvidia.so.0", b"ELF32");
        rig
    }

    /// Archetype 3: CachyOS + Intel Iris Xe + NVIDIA Optimus Laptop on Battery
    pub fn cachyos_intel_hybrid_laptop() -> Self {
        let rig = Self::empty();
        let kver = "6.12.2-cachyos";
        rig.write(
            "/etc/os-release",
            "NAME=\"CachyOS Linux\"\nPRETTY_NAME=\"CachyOS\"\nID=cachyos\n",
        );
        rig.write("/proc/sys/kernel/osrelease", format!("{kver}\n"));
        rig.write(
            &format!("/usr/lib/modules/{kver}/kernel/drivers/gpu/drm/i915/i915.ko.zst"),
            [0x28, 0xb5, 0x2f, 0xfd],
        );
        rig.write(
            &format!("/usr/lib/modules/{kver}/modules.dep"),
            "kernel/drivers/gpu/drm/i915/i915.ko.zst:\n",
        );

        rig.write("/sys/bus/pci/devices/0000:00:02.0/class", "0x030000\n");
        rig.write("/sys/bus/pci/devices/0000:00:02.0/vendor", "0x8086\n");
        rig.write("/sys/bus/pci/devices/0000:00:02.0/device", "0x46a6\n");
        fs::create_dir_all(rig.sub("/sys/bus/pci/drivers/i915")).unwrap();
        rig.symlink(
            "/sys/bus/pci/drivers/i915",
            "/sys/bus/pci/devices/0000:00:02.0/driver",
        );

        rig.write(
            "/usr/share/vulkan/icd.d/intel_icd.x86_64.json",
            r#"{"ICD":{"api_version":"1.3.295","library_path":"/usr/lib/libvulkan_intel.so"}}"#,
        );
        rig.write(
            "/usr/share/vulkan/icd.d/intel_icd.i686.json",
            r#"{"ICD":{"api_version":"1.3.295","library_path":"/usr/lib32/libvulkan_intel.so"}}"#,
        );
        rig.write("/usr/lib/libvulkan_intel.so", b"ELF64");
        rig.write("/usr/lib32/libvulkan_intel.so", b"ELF32");

        rig.write("/sys/bus/pci/devices/0000:01:00.0/class", "0x030200\n");
        rig.write("/sys/bus/pci/devices/0000:01:00.0/vendor", "0x10de\n");
        rig.write("/sys/bus/pci/devices/0000:01:00.0/device", "0x25a0\n");
        rig.write(
            "/sys/bus/pci/devices/0000:01:00.0/power/runtime_status",
            "suspended\n",
        );
        rig.write("/sys/class/power_supply/BAT0/status", "Discharging\n");
        rig.write(
            "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
            "powersave\n",
        );
        rig
    }

    /// Archetype 4: Kernel upgraded on disk (running modules removed) + missing 32-bit lib
    pub fn broken_reboot_state() -> Self {
        let rig = Self::empty();
        rig.write("/etc/os-release", "PRETTY_NAME=\"EndeavourOS\"\n");
        rig.write("/proc/sys/kernel/osrelease", "7.2.14-zen1-1-zen\n");
        // Running kernel 7.2.14 directory does NOT exist; only 7.2.15 exists without nvidia.ko
        fs::create_dir_all(rig.sub("/usr/lib/modules/7.2.15-zen1-1-zen")).unwrap();
        rig.write("/boot/initramfs-linux-zen.img", b"");
        rig.write(
            "/proc/driver/nvidia/version",
            "NVRM version: NVIDIA UNIX x86_64 Kernel Module  580.95.05\n",
        );
        rig.write(
            "/usr/share/vulkan/icd.d/nvidia_icd.json",
            r#"{"ICD":{"library_path":"libGLX_nvidia.so.0"}}"#,
        );
        rig.write("/usr/lib/libGLX_nvidia.so.0", b"ELF64");
        rig
    }

    /// Archetype 5: Headless Minimal VM (no /etc/os-release, only /usr/lib/os-release, no PCI 0x03)
    pub fn headless_minimal_vm() -> Self {
        let rig = Self::empty();
        let kver = "6.6.60-1-lts";
        rig.write("/usr/lib/os-release", "NAME=\"Arch Linux ARM / VM\"\n");
        rig.write("/proc/sys/kernel/osrelease", format!("{kver}\n"));
        fs::create_dir_all(rig.sub(&format!("/usr/lib/modules/{kver}"))).unwrap();
        rig.write(&format!("/usr/lib/modules/{kver}/modules.dep"), "");
        rig
    }

    /// Hydrates a MockRig from a JSON snapshot generated by dev-tools/dump-fixture.sh
    pub fn from_host_fixture(json_str: &str) -> Self {
        let rig = Self::empty();
        let v: serde_json::Value =
            serde_json::from_str(json_str).expect("invalid host fixture JSON");

        if let Some(os_map) = v.get("os_release").and_then(|x| x.as_object()) {
            let mut os_lines = String::new();
            for (k, val) in os_map {
                if let Some(s) = val.as_str() {
                    os_lines.push_str(&format!("{k}=\"{s}\"\n"));
                }
            }
            rig.write("/etc/os-release", os_lines);
        }

        let kver = v
            .get("kernel")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown");
        rig.write("/proc/sys/kernel/osrelease", format!("{kver}\n"));

        if let Some(mod_dir) = v.get("modules_dir") {
            if mod_dir
                .get("running_kernel_dir_exists")
                .and_then(|b| b.as_bool())
                .unwrap_or(false)
            {
                fs::create_dir_all(rig.sub(&format!("/usr/lib/modules/{kver}"))).unwrap();
                if mod_dir
                    .get("has_modules_dep")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false)
                {
                    if v.get("nvidia_proc").is_some() {
                        rig.write(
                            &format!("/usr/lib/modules/{kver}/modules.dep"),
                            "updates/dkms/nvidia.ko.zst:\n",
                        );
                        rig.write(
                            &format!("/usr/lib/modules/{kver}/updates/dkms/nvidia.ko.zst"),
                            [0x28, 0xb5, 0x2f, 0xfd],
                        );
                    } else {
                        rig.write(&format!("/usr/lib/modules/{kver}/modules.dep"), "");
                    }
                }
            }
        }

        if let Some(nv) = v.get("nvidia_proc") {
            if let Some(ver) = nv.get("version").and_then(|x| x.as_str()) {
                rig.write("/proc/driver/nvidia/version", format!("{ver}\n"));
            }
            if let Some(gpus) = nv.get("gpus").and_then(|x| x.as_object()) {
                for (bdf, info) in gpus {
                    if let Some(info_str) = info.as_str() {
                        rig.write(
                            &format!("/proc/driver/nvidia/gpus/{bdf}/information"),
                            format!("{info_str}\n"),
                        );
                    }
                }
            }
        }

        if let Some(pci_list) = v.get("pci_display_devices").and_then(|x| x.as_array()) {
            for dev in pci_list {
                let Some(bdf) = dev.get("bdf").and_then(|x| x.as_str()) else {
                    continue;
                };
                if let Some(cls) = dev.get("class").and_then(|x| x.as_str()) {
                    rig.write(
                        &format!("/sys/bus/pci/devices/{bdf}/class"),
                        format!("{cls}\n"),
                    );
                }
                if let Some(ven) = dev.get("vendor").and_then(|x| x.as_str()) {
                    rig.write(
                        &format!("/sys/bus/pci/devices/{bdf}/vendor"),
                        format!("{ven}\n"),
                    );
                }
                if let Some(did) = dev.get("device").and_then(|x| x.as_str()) {
                    rig.write(
                        &format!("/sys/bus/pci/devices/{bdf}/device"),
                        format!("{did}\n"),
                    );
                }
                if let Some(drv) = dev.get("driver").and_then(|x| x.as_str()) {
                    if !drv.is_empty() {
                        let drv_dir = format!("/sys/bus/pci/drivers/{drv}");
                        fs::create_dir_all(rig.sub(&drv_dir)).unwrap();
                        rig.symlink(&drv_dir, &format!("/sys/bus/pci/devices/{bdf}/driver"));
                    }
                }
            }
        }

        if let Some(icds) = v.get("vulkan_icd").and_then(|x| x.as_array()) {
            for icd in icds {
                if let (Some(fname), Some(content)) =
                    (icd.get("file").and_then(|x| x.as_str()), icd.get("content"))
                {
                    rig.write(
                        &format!("/usr/share/vulkan/icd.d/{fname}"),
                        serde_json::to_string(content).unwrap_or_default(),
                    );
                }
            }
        }

        if let Some(libs) = v.get("libraries").and_then(|x| x.as_object()) {
            for (lib_path, exists) in libs {
                if exists.as_bool().unwrap_or(false) {
                    rig.write(lib_path, b"ELF");
                }
            }
        }

        rig
    }
}
