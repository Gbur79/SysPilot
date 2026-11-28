slint::include_modules!();

use slint::{ComponentHandle, Model};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn acquire_single_instance_lock() -> bool {
    syspilot_core::acquire_single_instance_lock(".gui.lock")
}

fn format_hardware_info(os: &str, kernel: &str) -> String {
    let clean_os = if os.trim().is_empty() {
        "LINUX"
    } else {
        os.trim()
    };
    let clean_kernel = if kernel.trim().is_empty() {
        "UNKNOWN"
    } else {
        kernel.trim()
    };
    format!(
        "{} // {}",
        clean_os.to_uppercase(),
        clean_kernel.to_uppercase()
    )
}

fn format_kernel_telemetry(
    kernel: &str,
    snapshot_reboot: bool,
    safety: &syspilot_core::RebootSafetyStatus,
) -> (&'static str, (u8, u8, u8), String) {
    let clean_kernel = if kernel.trim().is_empty() {
        "UNKNOWN"
    } else {
        kernel.trim()
    };
    if safety.black_screen_risk {
        let issue = safety
            .issues
            .first()
            .map(|s| s.as_str())
            .unwrap_or("MISSING GPU MODULE / DEPMOD");
        (
            "ALERT",
            (239, 68, 68),
            format!("REBOOT UNSAFE • {}", issue.to_uppercase()),
        )
    } else if snapshot_reboot || safety.reboot_pending {
        (
            "REBOOT",
            (245, 158, 11),
            "KERNEL REPLACED ON DISK — REBOOT SAFE (MODULES VERIFIED)".to_string(),
        )
    } else {
        (
            "PASS",
            (34, 197, 94),
            format!("RUNNING LINUX {} • MODULES OK", clean_kernel.to_uppercase()),
        )
    }
}

fn format_gpu_telemetry(
    base_gpu: &str,
    gaming: &syspilot_core::GamingReadinessStatus,
) -> (&'static str, (u8, u8, u8), String) {
    if !gaming.applicable {
        ("INFO", (56, 189, 248), base_gpu.to_uppercase())
    } else if !gaming.ready {
        let suffix = if !gaming.vulkan_64_ready {
            "VULKAN 64-BIT MISSING"
        } else {
            "VULKAN 32-BIT MISSING"
        };
        (
            "WARN",
            (245, 158, 11),
            format!("{} • {}", base_gpu.to_uppercase(), suffix),
        )
    } else {
        (
            "PASS",
            (34, 197, 94),
            format!("{} • VULKAN 64/32 OK", base_gpu.to_uppercase()),
        )
    }
}

fn format_gpu_telemetry_with_pcie(
    base_gpu: &str,
    gaming: &syspilot_core::GamingReadinessStatus,
    reboot_safety: &syspilot_core::RebootSafetyStatus,
) -> (&'static str, (u8, u8, u8), String) {
    let (status, rgb, mut details) = format_gpu_telemetry(base_gpu, gaming);
    if reboot_safety.pcie_degraded {
        let pcie_msg = reboot_safety
            .pcie_links
            .iter()
            .find_map(|l| l.detail.as_deref())
            .unwrap_or("PCIE LINK DEGRADED");
        return (
            "WARN",
            RowTone::Warn.rgb(),
            format!(
                "{} • {}",
                base_gpu.to_uppercase(),
                pcie_msg.to_ascii_uppercase()
            ),
        );
    }
    if let Some(link) = reboot_safety
        .pcie_links
        .iter()
        .find(|l| !l.runtime_suspended && l.current_width.is_some())
        && let Some(cw) = link.current_width
    {
        details = format!("{details} • PCIE X{cw}");
    }
    (status, rgb, details)
}

pub fn format_aur_updates(count: u32) -> (&'static str, (u8, u8, u8), String) {
    if count > 0 {
        let details = if count == 1 {
            "1 AUR PACKAGE PENDING".to_string()
        } else {
            format!("{count} AUR PACKAGES PENDING")
        };
        ("UPDATE", (245, 158, 11), details)
    } else {
        ("PASS", (34, 197, 94), "0 AUR PACKAGES PENDING".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowTone {
    Pass,
    Update,
    Warn,
    Alert,
    Info,
    Offline,
}

impl From<syspilot_core::RowSeverity> for RowTone {
    fn from(sev: syspilot_core::RowSeverity) -> Self {
        match sev {
            syspilot_core::RowSeverity::Pass => Self::Pass,
            syspilot_core::RowSeverity::Update => Self::Update,
            syspilot_core::RowSeverity::Warn => Self::Warn,
            syspilot_core::RowSeverity::Alert => Self::Alert,
            syspilot_core::RowSeverity::Info => Self::Info,
            syspilot_core::RowSeverity::Offline => Self::Offline,
        }
    }
}

impl RowTone {
    pub const fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Pass => (34, 197, 94),
            Self::Update | Self::Warn => (245, 158, 11),
            Self::Alert => (239, 68, 68),
            Self::Info => (56, 189, 248),
            Self::Offline => (100, 116, 139),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowPresentation {
    pub status: String,
    pub rgb: (u8, u8, u8),
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfUpdateView {
    pub available: bool,
    pub badge: String,
    pub title: String,
    pub desc: String,
    pub btn_text: String,
}

pub fn format_snapshot_error_banner(snapshot: &syspilot_core::SystemSnapshot) -> (bool, String) {
    if let Some(err) = &snapshot.source_error {
        (true, err.clone())
    } else if snapshot.cache_stale {
        (
            true,
            format!("Telemetry cache stale (age {})", snapshot.age_label()),
        )
    } else if snapshot.status == "DEGRADED" {
        (true, "Degraded telemetry stream".to_string())
    } else {
        (false, String::new())
    }
}

pub fn format_overall_header(
    snapshot: &syspilot_core::SystemSnapshot,
    reboot_safety: &syspilot_core::RebootSafetyStatus,
) -> (&'static str, &'static str, (u8, u8, u8)) {
    let eff: &'static str = if reboot_safety.black_screen_risk {
        "ACTION_REQUIRED"
    } else {
        match snapshot.effective_status() {
            "ACTION_REQUIRED" => "ACTION_REQUIRED",
            "PRE_FLIGHT_ATTENTION" => "PRE_FLIGHT_ATTENTION",
            _ => "ALL_CLEAR",
        }
    };
    let (label, rgb) = if snapshot.has_data_problem() || snapshot.status == "DEGRADED" {
        ("TELEMETRY DEGRADED", RowTone::Warn.rgb())
    } else {
        match eff {
            "ACTION_REQUIRED" => ("RED ALERT", RowTone::Alert.rgb()),
            "PRE_FLIGHT_ATTENTION" => ("SYSTEM ATTENTION", RowTone::Warn.rgb()),
            _ => {
                if snapshot.gaming_mode {
                    ("GAMEMODE ENGAGED", RowTone::Info.rgb())
                } else {
                    ("SYSTEM OPERATIONAL", RowTone::Pass.rgb())
                }
            }
        }
    };
    (eff, label, rgb)
}

pub fn format_root_storage(used_pct: u8, avail_gb: f32) -> (&'static str, (u8, u8, u8), String) {
    let (status, rgb) = if used_pct >= 90 {
        ("ALERT", RowTone::Alert.rgb())
    } else if used_pct >= 80 {
        ("WARN", RowTone::Warn.rgb())
    } else {
        ("PASS", RowTone::Pass.rgb())
    };
    (
        status,
        rgb,
        format!("{used_pct}% USED • {avail_gb:.1} GB AVAILABLE ON ROOT"),
    )
}

pub fn format_firewall_status(active: bool) -> (&'static str, (u8, u8, u8), &'static str) {
    if active {
        (
            "PASS",
            RowTone::Pass.rgb(),
            "FIREWALL ACTIVE • PACKET FILTERING ENFORCED",
        )
    } else {
        (
            "INFO",
            RowTone::Info.rgb(),
            "PROTECTED BY PERIMETER ROUTER/LAN",
        )
    }
}

pub fn format_failed_services(errors: u32) -> (String, (u8, u8, u8), String) {
    if errors == 0 {
        (
            "PASS".to_string(),
            RowTone::Pass.rgb(),
            "0 FAILED SYSTEMD UNITS".to_string(),
        )
    } else if errors == 1 {
        (
            "1 FAILED".to_string(),
            RowTone::Alert.rgb(),
            "1 DEGRADED SYSTEMD UNIT".to_string(),
        )
    } else {
        (
            format!("{errors} FAILED"),
            RowTone::Alert.rgb(),
            format!("{errors} DEGRADED SYSTEMD UNITS"),
        )
    }
}

pub fn format_cve_status(high_risk: u32) -> (&'static str, (u8, u8, u8), String) {
    if high_risk == 0 {
        (
            "PASS",
            RowTone::Pass.rgb(),
            "NO UNPATCHED CVE VULNERABILITIES".to_string(),
        )
    } else if high_risk == 1 {
        (
            "ALERT",
            RowTone::Alert.rgb(),
            "1 HIGH-RISK CVE VULNERABILITY PENDING".to_string(),
        )
    } else {
        (
            "ALERT",
            RowTone::Alert.rgb(),
            format!("{high_risk} HIGH-RISK CVE VULNERABILITIES PENDING"),
        )
    }
}

pub fn format_core_updates(count: u32) -> (&'static str, (u8, u8, u8), String) {
    if count == 0 {
        (
            "PASS",
            RowTone::Pass.rgb(),
            "0 CORE SYSTEM UPDATES PENDING".to_string(),
        )
    } else if count == 1 {
        (
            "UPDATE",
            RowTone::Update.rgb(),
            "1 CORE SYSTEM UPDATE PENDING".to_string(),
        )
    } else {
        (
            "UPDATE",
            RowTone::Update.rgb(),
            format!("{count} CORE SYSTEM UPDATES PENDING"),
        )
    }
}

pub fn format_repo_updates_row(
    snapshot: &syspilot_core::SystemSnapshot,
) -> (String, (u8, u8, u8), String) {
    let state = syspilot_core::format_repo_updates_state(
        snapshot.regular_updates,
        snapshot.updates_check_state,
        snapshot.updates_consecutive_failures,
    );
    let rgb = RowTone::from(state.severity).rgb();
    (state.status, rgb, state.details)
}

pub fn format_mirrors_row(
    snapshot: &syspilot_core::SystemSnapshot,
) -> (String, (u8, u8, u8), String) {
    let state = syspilot_core::format_mirrorlist_state(snapshot.mirrors.as_ref());
    let rgb = RowTone::from(state.severity).rgb();
    (state.status, rgb, state.details)
}

pub fn format_standalone_updates_row(
    snapshot: &syspilot_core::SystemSnapshot,
) -> (String, (u8, u8, u8), String) {
    let state = syspilot_core::format_standalone_updates_state(
        snapshot.standalone_updates,
        &snapshot.standalone_tools,
    );
    let rgb = RowTone::from(state.severity).rgb();
    (state.status, rgb, state.details)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeClickAction {
    ShowMirrorWarning,
    ShowStandalonePrompt,
    LaunchDirectSystemUpgrade,
}

pub fn resolve_upgrade_click_action(
    mirror_primary_dead: bool,
    standalone_count: u32,
) -> UpgradeClickAction {
    if mirror_primary_dead {
        UpgradeClickAction::ShowMirrorWarning
    } else if standalone_count > 0 {
        UpgradeClickAction::ShowStandalonePrompt
    } else {
        UpgradeClickAction::LaunchDirectSystemUpgrade
    }
}

pub fn sh_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn build_chained_upgrade_software_cmd(syspilot_bin: &std::path::Path) -> String {
    let quoted = sh_single_quote(&syspilot_bin.to_string_lossy());
    format!("{quoted} --upgrade && {quoted} --software")
}

pub fn handle_upgrade_click<FW: FnOnce(), FU: FnOnce()>(
    mirror_primary_dead: bool,
    show_warning: FW,
    launch_upgrade: FU,
) {
    if mirror_primary_dead {
        show_warning();
    } else {
        launch_upgrade();
    }
}

pub fn format_repo_updates(count: u32) -> (&'static str, (u8, u8, u8), String) {
    if count == 0 {
        (
            "PASS",
            RowTone::Pass.rgb(),
            "0 OFFICIAL PACKAGES PENDING".to_string(),
        )
    } else if count == 1 {
        (
            "UPDATE",
            RowTone::Update.rgb(),
            "1 OFFICIAL PACKAGE PENDING".to_string(),
        )
    } else {
        (
            "UPDATE",
            RowTone::Update.rgb(),
            format!("{count} OFFICIAL PACKAGES PENDING"),
        )
    }
}

pub fn format_firmware_updates(count: u32) -> (&'static str, (u8, u8, u8), String) {
    if count == 0 {
        (
            "PASS",
            RowTone::Pass.rgb(),
            "FIRMWARE SIGNATURES VERIFIED • LVFS CLEAN".to_string(),
        )
    } else if count == 1 {
        (
            "UPDATE",
            RowTone::Update.rgb(),
            "1 FIRMWARE UPDATE AVAILABLE VIA LVFS".to_string(),
        )
    } else {
        (
            "UPDATE",
            RowTone::Update.rgb(),
            format!("{count} FIRMWARE UPDATES AVAILABLE VIA LVFS"),
        )
    }
}

pub fn format_orphans_row() -> (&'static str, (u8, u8, u8), &'static str) {
    (
        "PASS",
        RowTone::Pass.rgb(),
        "0 ORPHANS DETECTED • PACMAN CACHE TRIMMED",
    )
}

pub fn format_self_update(
    snapshot: &syspilot_core::SystemSnapshot,
    pkg_version: &str,
) -> SelfUpdateView {
    if snapshot.app_update_available {
        SelfUpdateView {
            available: true,
            badge: format!("v{} AVAILABLE", snapshot.app_latest_version),
            title: "SysPilot Suite Self-Update (Update Available)".to_string(),
            desc: format!(
                "Upstream v{} is available (current: v{}). Fast-forward git repository and rebuild atomically.",
                snapshot.app_latest_version, snapshot.app_current_version
            ),
            btn_text: format!("Update to v{}", snapshot.app_latest_version),
        }
    } else {
        SelfUpdateView {
            available: false,
            badge: format!("v{} LATEST", pkg_version),
            title: "SysPilot Suite Self-Update & GitHub Sync".to_string(),
            desc: "Synchronizes the SysPilot suite with the latest upstream GitHub release and performs atomic rebuild.".to_string(),
            btn_text: "Check & Update Suite".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnomalyPresentation {
    pub label: String,
    pub details: String,
    pub status: String,
    pub accent_rgb: (u8, u8, u8),
    pub status_rgb: (u8, u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardViewModel {
    pub has_error: bool,
    pub error_message: String,
    pub has_attention: bool,
    pub attention_badge: String,
    pub attention_message: String,
    pub overall_status: String,
    pub overall_status_label: String,
    pub overall_status_rgb: (u8, u8, u8),
    pub hardware_info: String,
    pub kernel: RowPresentation,
    pub root: RowPresentation,
    pub firewall: RowPresentation,
    pub failed_services: RowPresentation,
    pub gpu: RowPresentation,
    pub cve: RowPresentation,
    pub updates_core: RowPresentation,
    pub updates_repo: RowPresentation,
    pub updates_aur: RowPresentation,
    pub updates_standalone: RowPresentation,
    pub standalone_updates_count: u32,
    pub mirrors: RowPresentation,
    pub mirror_primary_dead: bool,
    pub mirror_upgrade_warning: String,
    pub firmware: RowPresentation,
    pub orphans: RowPresentation,
    pub anomalies: Vec<AnomalyPresentation>,
    pub pod00_accent_rgb: (u8, u8, u8),
    pub tab0_max_detail_chars: usize,
    pub self_update: SelfUpdateView,
}

impl DashboardViewModel {
    pub fn from_snapshot_inputs(
        snapshot: &syspilot_core::SystemSnapshot,
        reboot_safety: &syspilot_core::RebootSafetyStatus,
        sentinels: &syspilot_core::SentinelsStatus,
        gaming: &syspilot_core::GamingReadinessStatus,
        gpu_name: &str,
        pkg_version: &str,
    ) -> Self {
        let (has_error, error_message) = format_snapshot_error_banner(snapshot);
        let attention = syspilot_core::compute_attention_view(snapshot, reboot_safety, gaming);
        let (overall_status, overall_status_label, overall_status_rgb) =
            format_overall_header(snapshot, reboot_safety);
        let hardware_info = format_hardware_info(&snapshot.os, &snapshot.kernel);

        let (k_status, k_rgb, k_details) =
            format_kernel_telemetry(&snapshot.kernel, snapshot.reboot_pending, reboot_safety);
        let (root_status, root_rgb, root_details) =
            format_root_storage(sentinels.root_used_pct, sentinels.root_avail_gb);
        let (fw_status, fw_rgb, fw_details) = format_firewall_status(snapshot.firewall_active);
        let (svc_status, svc_rgb, svc_details) = format_failed_services(snapshot.errors);
        let (gpu_status, gpu_rgb, gpu_details) =
            format_gpu_telemetry_with_pcie(gpu_name, gaming, reboot_safety);
        let (cve_status, cve_rgb, cve_details) = format_cve_status(snapshot.security_high_risk);
        let (core_status, core_rgb, core_details) = format_core_updates(snapshot.core_updates);
        let (repo_status, repo_rgb, repo_details) = format_repo_updates_row(snapshot);
        let (aur_status, aur_rgb, aur_details) = format_aur_updates(snapshot.aur_updates);
        let (standalone_status, standalone_rgb, standalone_details) =
            format_standalone_updates_row(snapshot);
        let (mirrors_status, mirrors_rgb, mirrors_details) = format_mirrors_row(snapshot);
        let mirror_primary_dead = syspilot_core::is_primary_mirror_dead(snapshot.mirrors.as_ref());
        let mirror_upgrade_warning =
            syspilot_core::mirror_upgrade_warning_message(snapshot.mirrors.as_ref())
                .unwrap_or_default();
        let (fwupd_status, fwupd_rgb, fwupd_details) =
            format_firmware_updates(snapshot.firmware_updates);
        let (orph_status, orph_rgb, orph_details) = format_orphans_row();
        let raw_anomalies = syspilot_core::build_dashboard_anomalies(snapshot);
        let has_alert = raw_anomalies
            .iter()
            .any(|a| a.severity == syspilot_core::RowSeverity::Alert);
        let pod00_accent_rgb = if has_alert {
            RowTone::Alert.rgb()
        } else {
            (255, 153, 0)
        };
        let anomalies: Vec<AnomalyPresentation> = raw_anomalies
            .into_iter()
            .map(|a| AnomalyPresentation {
                label: a.label,
                details: a.details,
                status: a.status,
                accent_rgb: pod00_accent_rgb,
                status_rgb: RowTone::from(a.severity).rgb(),
            })
            .collect();
        let tab0_max_detail_chars = [
            k_details.chars().count(),
            root_details.chars().count(),
            fw_details.chars().count(),
            svc_details.chars().count(),
            gpu_details.chars().count(),
            cve_details.chars().count(),
            core_details.chars().count(),
            repo_details.chars().count(),
            aur_details.chars().count(),
            standalone_details.chars().count(),
            mirrors_details.chars().count(),
            fwupd_details.chars().count(),
            orph_details.chars().count(),
        ]
        .into_iter()
        .chain(anomalies.iter().map(|a| a.details.chars().count()))
        .max()
        .unwrap_or(0);
        let self_update = format_self_update(snapshot, pkg_version);

        Self {
            has_error,
            error_message,
            has_attention: attention.visible,
            attention_badge: attention.badge_text,
            attention_message: attention.message,
            overall_status: overall_status.to_string(),
            overall_status_label: overall_status_label.to_string(),
            overall_status_rgb,
            hardware_info,
            kernel: RowPresentation {
                status: k_status.to_string(),
                rgb: k_rgb,
                details: k_details,
            },
            root: RowPresentation {
                status: root_status.to_string(),
                rgb: root_rgb,
                details: root_details,
            },
            firewall: RowPresentation {
                status: fw_status.to_string(),
                rgb: fw_rgb,
                details: fw_details.to_string(),
            },
            failed_services: RowPresentation {
                status: svc_status,
                rgb: svc_rgb,
                details: svc_details,
            },
            gpu: RowPresentation {
                status: gpu_status.to_string(),
                rgb: gpu_rgb,
                details: gpu_details,
            },
            cve: RowPresentation {
                status: cve_status.to_string(),
                rgb: cve_rgb,
                details: cve_details,
            },
            updates_core: RowPresentation {
                status: core_status.to_string(),
                rgb: core_rgb,
                details: core_details,
            },
            updates_repo: RowPresentation {
                status: repo_status,
                rgb: repo_rgb,
                details: repo_details,
            },
            updates_aur: RowPresentation {
                status: aur_status.to_string(),
                rgb: aur_rgb,
                details: aur_details,
            },
            updates_standalone: RowPresentation {
                status: standalone_status,
                rgb: standalone_rgb,
                details: standalone_details,
            },
            standalone_updates_count: snapshot.standalone_updates,
            mirrors: RowPresentation {
                status: mirrors_status,
                rgb: mirrors_rgb,
                details: mirrors_details,
            },
            mirror_primary_dead,
            mirror_upgrade_warning,
            firmware: RowPresentation {
                status: fwupd_status.to_string(),
                rgb: fwupd_rgb,
                details: fwupd_details,
            },
            orphans: RowPresentation {
                status: orph_status.to_string(),
                rgb: orph_rgb,
                details: orph_details.to_string(),
            },
            anomalies,
            pod00_accent_rgb,
            tab0_max_detail_chars,
            self_update,
        }
    }

    pub fn from_error(err: &str, pkg_version: &str) -> Self {
        let offline_row = || RowPresentation {
            status: "OFFLINE".to_string(),
            rgb: RowTone::Offline.rgb(),
            details: "TELEMETRY UNAVAILABLE".to_string(),
        };

        Self {
            has_error: true,
            error_message: err.to_string(),
            has_attention: false,
            attention_badge: String::new(),
            attention_message: String::new(),
            overall_status: "DATA_UNAVAILABLE".to_string(),
            overall_status_label: "DATA UNAVAILABLE".to_string(),
            overall_status_rgb: RowTone::Warn.rgb(),
            hardware_info: "TELEMETRY STREAM DISCONNECTED".to_string(),
            kernel: offline_row(),
            root: offline_row(),
            firewall: offline_row(),
            failed_services: offline_row(),
            gpu: offline_row(),
            cve: offline_row(),
            updates_core: offline_row(),
            updates_repo: offline_row(),
            updates_aur: offline_row(),
            updates_standalone: offline_row(),
            standalone_updates_count: 0,
            mirrors: offline_row(),
            mirror_primary_dead: false,
            mirror_upgrade_warning: String::new(),
            firmware: offline_row(),
            orphans: offline_row(),
            anomalies: Vec::new(),
            pod00_accent_rgb: (255, 153, 0),
            tab0_max_detail_chars: 0,
            self_update: SelfUpdateView {
                available: false,
                badge: format!("v{} LATEST", pkg_version),
                title: "SysPilot Suite Self-Update & GitHub Sync".to_string(),
                desc: "Synchronizes the SysPilot suite with the latest upstream GitHub release and performs atomic rebuild.".to_string(),
                btn_text: "Check & Update Suite".to_string(),
            },
        }
    }
}

fn apply_dashboard_view_model(ui: &AppWindow, vm: &DashboardViewModel) {
    ui.set_has_error(vm.has_error);
    ui.set_error_message(vm.error_message.as_str().into());
    ui.set_has_attention(vm.has_attention);
    ui.set_attention_badge(vm.attention_badge.as_str().into());
    ui.set_attention_message(vm.attention_message.as_str().into());
    ui.set_overall_status(vm.overall_status.as_str().into());
    ui.set_overall_status_label(vm.overall_status_label.as_str().into());
    ui.set_overall_status_color(slint::Color::from_rgb_u8(
        vm.overall_status_rgb.0,
        vm.overall_status_rgb.1,
        vm.overall_status_rgb.2,
    ));
    ui.set_hardware_info(vm.hardware_info.as_str().into());

    ui.set_kernel_status(vm.kernel.status.as_str().into());
    ui.set_kernel_color(slint::Color::from_rgb_u8(
        vm.kernel.rgb.0,
        vm.kernel.rgb.1,
        vm.kernel.rgb.2,
    ));
    ui.set_kernel_details(vm.kernel.details.as_str().into());

    ui.set_root_status(vm.root.status.as_str().into());
    ui.set_root_color(slint::Color::from_rgb_u8(
        vm.root.rgb.0,
        vm.root.rgb.1,
        vm.root.rgb.2,
    ));
    ui.set_root_details(vm.root.details.as_str().into());

    ui.set_firewall_status(vm.firewall.status.as_str().into());
    ui.set_firewall_color(slint::Color::from_rgb_u8(
        vm.firewall.rgb.0,
        vm.firewall.rgb.1,
        vm.firewall.rgb.2,
    ));
    ui.set_firewall_details(vm.firewall.details.as_str().into());

    ui.set_failed_services_status(vm.failed_services.status.as_str().into());
    ui.set_failed_services_color(slint::Color::from_rgb_u8(
        vm.failed_services.rgb.0,
        vm.failed_services.rgb.1,
        vm.failed_services.rgb.2,
    ));
    ui.set_failed_services_details(vm.failed_services.details.as_str().into());

    ui.set_gpu_status(vm.gpu.status.as_str().into());
    ui.set_gpu_color(slint::Color::from_rgb_u8(
        vm.gpu.rgb.0,
        vm.gpu.rgb.1,
        vm.gpu.rgb.2,
    ));
    ui.set_gpu_details(vm.gpu.details.as_str().into());

    ui.set_cve_status(vm.cve.status.as_str().into());
    ui.set_cve_color(slint::Color::from_rgb_u8(
        vm.cve.rgb.0,
        vm.cve.rgb.1,
        vm.cve.rgb.2,
    ));
    ui.set_cve_details(vm.cve.details.as_str().into());

    ui.set_updates_core_text(vm.updates_core.status.as_str().into());
    ui.set_updates_core_color(slint::Color::from_rgb_u8(
        vm.updates_core.rgb.0,
        vm.updates_core.rgb.1,
        vm.updates_core.rgb.2,
    ));
    ui.set_updates_core_details(vm.updates_core.details.as_str().into());

    ui.set_updates_repo_text(vm.updates_repo.status.as_str().into());
    ui.set_updates_repo_color(slint::Color::from_rgb_u8(
        vm.updates_repo.rgb.0,
        vm.updates_repo.rgb.1,
        vm.updates_repo.rgb.2,
    ));
    ui.set_updates_repo_details(vm.updates_repo.details.as_str().into());

    ui.set_updates_aur_text(vm.updates_aur.status.as_str().into());
    ui.set_updates_aur_color(slint::Color::from_rgb_u8(
        vm.updates_aur.rgb.0,
        vm.updates_aur.rgb.1,
        vm.updates_aur.rgb.2,
    ));
    ui.set_updates_aur_details(vm.updates_aur.details.as_str().into());

    ui.set_updates_standalone_status(vm.updates_standalone.status.as_str().into());
    ui.set_updates_standalone_color(slint::Color::from_rgb_u8(
        vm.updates_standalone.rgb.0,
        vm.updates_standalone.rgb.1,
        vm.updates_standalone.rgb.2,
    ));
    ui.set_updates_standalone_details(vm.updates_standalone.details.as_str().into());
    ui.set_standalone_updates_count(vm.standalone_updates_count as i32);

    ui.set_mirrors_status(vm.mirrors.status.as_str().into());
    ui.set_mirrors_color(slint::Color::from_rgb_u8(
        vm.mirrors.rgb.0,
        vm.mirrors.rgb.1,
        vm.mirrors.rgb.2,
    ));
    ui.set_mirrors_details(vm.mirrors.details.as_str().into());
    ui.set_mirror_primary_dead(vm.mirror_primary_dead);
    ui.set_mirror_upgrade_warning(vm.mirror_upgrade_warning.as_str().into());

    ui.set_firmware_status(vm.firmware.status.as_str().into());
    ui.set_firmware_color(slint::Color::from_rgb_u8(
        vm.firmware.rgb.0,
        vm.firmware.rgb.1,
        vm.firmware.rgb.2,
    ));
    ui.set_firmware_details(vm.firmware.details.as_str().into());

    ui.set_orphans_status(vm.orphans.status.as_str().into());
    ui.set_orphans_color(slint::Color::from_rgb_u8(
        vm.orphans.rgb.0,
        vm.orphans.rgb.1,
        vm.orphans.rgb.2,
    ));
    ui.set_orphans_details(vm.orphans.details.as_str().into());

    let anomaly_rows: Vec<AnomalyRow> = vm
        .anomalies
        .iter()
        .map(|a| AnomalyRow {
            label: a.label.as_str().into(),
            value: a.details.as_str().into(),
            status: a.status.as_str().into(),
            accent_color: slint::Color::from_rgb_u8(a.accent_rgb.0, a.accent_rgb.1, a.accent_rgb.2),
            status_color: slint::Color::from_rgb_u8(a.status_rgb.0, a.status_rgb.1, a.status_rgb.2),
        })
        .collect();
    ui.set_anomalies(slint::ModelRc::new(slint::VecModel::from(anomaly_rows)));
    ui.set_pod00_accent_color(slint::Color::from_rgb_u8(
        vm.pod00_accent_rgb.0,
        vm.pod00_accent_rgb.1,
        vm.pod00_accent_rgb.2,
    ));

    let w_i32 = compute_tab0_window_width(vm.tab0_max_detail_chars) as i32;
    let h_i32 =
        compute_tab0_window_height(vm.anomalies.len(), vm.has_error, ui.get_is_lcars_skin()) as i32;
    let prev_target = (ui.get_tab0_target_width(), ui.get_tab0_target_height());
    ui.set_tab0_target_width(w_i32);
    ui.set_tab0_target_height(h_i32);

    ui.set_has_app_update(vm.self_update.available);
    ui.set_app_update_badge(vm.self_update.badge.as_str().into());
    ui.set_self_update_title(vm.self_update.title.as_str().into());
    ui.set_self_update_desc(vm.self_update.desc.as_str().into());
    ui.set_self_update_btn_text(vm.self_update.btn_text.as_str().into());

    if ui.get_active_tab() == 0 && (w_i32, h_i32) != prev_target {
        apply_tab_selection_to_window(ui, 0);
    }
}

fn update_ui_from_snapshot(ui: &AppWindow, snapshot: &syspilot_core::SystemSnapshot) {
    let reboot_safety = syspilot_core::check_reboot_safety();
    let sentinels = syspilot_core::collect_sentinels();
    let gaming = syspilot_core::check_gaming_readiness();
    let gpu_name = syspilot_core::detect_gpu();
    let vm = DashboardViewModel::from_snapshot_inputs(
        snapshot,
        &reboot_safety,
        &sentinels,
        &gaming,
        &gpu_name,
        env!("CARGO_PKG_VERSION"),
    );
    apply_dashboard_view_model(ui, &vm);
}

fn update_ui_from_error(ui: &AppWindow, err: &str) {
    let vm = DashboardViewModel::from_error(err, env!("CARGO_PKG_VERSION"));
    apply_dashboard_view_model(ui, &vm);
}

static REFRESH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static LAST_SPAWN_MONO_MS: AtomicU64 = AtomicU64::new(0);
const SPAWN_DEBOUNCE_WINDOW_MS: u64 = 750;

fn format_custom_playbook_desc(playbooks: &[syspilot_core::UserPlaybook]) -> String {
    if playbooks.is_empty() {
        "Executes user tactical scripts discovered in ~/.config/syspilot/playbooks/*.sh".to_string()
    } else {
        let first = &playbooks[0];
        if first.description.is_empty() {
            format!(
                "Run playbook '{}' ({} script(s) available)",
                first.name,
                playbooks.len()
            )
        } else {
            format!("{}: {}", first.name, first.description)
        }
    }
}

fn update_playbook_telemetry(ui: &AppWindow) {
    ui.set_sre_telemetry_export(syspilot_core::build_ai_telemetry_markdown().into());
    let playbooks = syspilot_core::discover_user_playbooks();
    ui.set_custom_playbook_desc(format_custom_playbook_desc(&playbooks).into());
}

const TAB0_DEFAULT_WIDTH: f32 = 920.0;
const TAB0_MAX_WIDTH: f32 = 1040.0;
const TAB1_WINDOW_WIDTH: f32 = 920.0;
const TAB2_WINDOW_WIDTH: f32 = 980.0;

const TAB0_LCARS_BASE_HEIGHT: f32 = 620.0;
const TAB0_CLASSIC_BASE_HEIGHT: f32 = 760.0;
const TAB0_MAX_HEIGHT: f32 = 900.0;

fn compute_tab0_window_width(max_detail_chars: usize) -> f32 {
    let extra_chars = max_detail_chars.saturating_sub(58);
    let raw = TAB0_DEFAULT_WIDTH + (extra_chars as f32) * 6.5;
    ((raw / 10.0).round() * 10.0).min(TAB0_MAX_WIDTH)
}

fn compute_tab0_window_height(anomaly_count: usize, has_error: bool, is_lcars_skin: bool) -> f32 {
    let err_px = if has_error { 42.0 } else { 0.0 };
    let (base, anomaly_px) = if is_lcars_skin {
        let pod00 = if anomaly_count > 0 {
            36.0 + (anomaly_count as f32) * 30.0
        } else {
            0.0
        };
        (TAB0_LCARS_BASE_HEIGHT, pod00)
    } else {
        let card0 = if anomaly_count > 0 {
            56.0 + (anomaly_count as f32) * 28.0
        } else {
            0.0
        };
        (TAB0_CLASSIC_BASE_HEIGHT, card0)
    };
    (base + err_px + anomaly_px).min(TAB0_MAX_HEIGHT)
}

#[allow(clippy::too_many_arguments)]
fn compute_tab_window_size(
    tab_idx: i32,
    maintenance_item_count: usize,
    tab0_width: f32,
    anomaly_count: usize,
    has_error: bool,
    is_lcars_skin: bool,
    is_maximized: bool,
    is_fullscreen: bool,
) -> Option<(f32, f32)> {
    if is_maximized || is_fullscreen {
        return None;
    }
    Some(match tab_idx {
        1 => (
            TAB1_WINDOW_WIDTH,
            compute_maintenance_window_height(maintenance_item_count),
        ),
        2 => (TAB2_WINDOW_WIDTH, 780.0),
        _ => (
            tab0_width,
            compute_tab0_window_height(anomaly_count, has_error, is_lcars_skin),
        ),
    })
}

fn apply_tab_selection_to_window(ui_inst: &AppWindow, tab_idx: i32) {
    if tab_idx == 2 {
        update_playbook_telemetry(ui_inst);
    }
    let anomaly_count = ui_inst.get_anomalies().row_count();
    let has_error = ui_inst.get_has_error();
    let is_lcars = ui_inst.get_is_lcars_skin();
    let tab0_h = compute_tab0_window_height(anomaly_count, has_error, is_lcars);
    ui_inst.set_tab0_target_height(tab0_h as i32);
    if let Some((target_width, target_height)) = compute_tab_window_size(
        tab_idx,
        ui_inst.get_maintenance_item_count() as usize,
        ui_inst.get_tab0_target_width() as f32,
        anomaly_count,
        has_error,
        is_lcars,
        ui_inst.window().is_maximized(),
        ui_inst.window().is_fullscreen(),
    ) {
        ui_inst
            .window()
            .set_size(slint::LogicalSize::new(target_width, target_height));
    }
}

fn handle_playbook_click<FT, FS, FC>(
    playbook_id: &str,
    mut run_terminal: FT,
    mut run_shell: FS,
    mut run_custom: FC,
) where
    FT: FnMut(&'static [&'static str], &'static str, bool),
    FS: FnMut(&'static str, &'static str),
    FC: FnMut(),
{
    match playbook_id {
        "faf" => {
            run_terminal(&["--faf-repair"], "SysPilot FAF Runner Repair", false);
        }
        "proton" => {
            run_terminal(&["--gaming"], "SysPilot Gaming & Proton Readiness", true);
        }
        "pacnew" => {
            run_shell(
                "echo '=== SysPilot Configuration Drift (.pacnew) Reviewer ==='; if type -P pacdiff &>/dev/null; then sudo -E pacdiff; else echo 'pacdiff (pacman-contrib) not installed.'; fi",
                "SysPilot .pacnew Reviewer",
            );
        }
        "audio" => {
            run_shell(
                "echo '=== Resetting PipeWire & WirePlumber User Audio Stack ==='; systemctl --user restart pipewire pipewire-pulse wireplumber && sleep 1 && wpctl status",
                "SysPilot Audio Stack Reset",
            );
        }
        "custom" => {
            run_custom();
        }
        "audit" | "repair" => {
            run_terminal(&["--audit"], "SysPilot Diagnostic Audit", true);
        }
        _ => {}
    }
}

fn monotonic_now_ms() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let ret = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    if ret == 0 {
        (ts.tv_sec as u64) * 1000 + (ts.tv_nsec as u64) / 1_000_000
    } else {
        0
    }
}

fn can_spawn_task_with_window(window_ms: u64) -> bool {
    let now = monotonic_now_ms();
    let mut last = LAST_SPAWN_MONO_MS.load(Ordering::Acquire);
    loop {
        if now.saturating_sub(last) < window_ms {
            return false;
        }
        match LAST_SPAWN_MONO_MS.compare_exchange_weak(
            last,
            now,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(actual) => last = actual,
        }
    }
}

fn can_spawn_task() -> bool {
    can_spawn_task_with_window(SPAWN_DEBOUNCE_WINDOW_MS)
}

struct RefreshFlightGuard;

impl Drop for RefreshFlightGuard {
    fn drop(&mut self) {
        REFRESH_IN_FLIGHT.store(false, Ordering::Release);
    }
}

fn trigger_task_with_post_triage(ui_weak: slint::Weak<AppWindow>, args: &[&str], title: &str) {
    if !can_spawn_task() {
        return;
    }

    let _ = syspilot_core::spawn_syspilot_task_with_callback(args, title, move || {
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(err) => {
                    eprintln!("SysPilot: failed to build runtime for post-triage: {err}");
                    return;
                }
            };
            rt.block_on(async {
                let _ = syspilot_core::run_triage(true, false).await;
                let status_path = syspilot_core::get_status_path();
                let snapshot_res = syspilot_core::read_system_snapshot(&status_path);
                let _ = ui_weak.upgrade_in_event_loop(move |ui_inst| match snapshot_res {
                    Ok(snapshot) => update_ui_from_snapshot(&ui_inst, &snapshot),
                    Err(err) => update_ui_from_error(&ui_inst, &err),
                });
            });
        });
    });
}

#[cfg(test)]
thread_local! {
    static TEST_CAN_SPAWN_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static TEST_SHELL_TASK_RECORDER: std::cell::RefCell<Option<Vec<(String, String)>>> = const { std::cell::RefCell::new(None) };
}

fn trigger_shell_task_with_post_triage(
    ui_weak: slint::Weak<AppWindow>,
    command: &str,
    title: &str,
) -> bool {
    #[cfg(test)]
    let can_spawn = TEST_CAN_SPAWN_OVERRIDE
        .with(|cell| cell.get())
        .unwrap_or_else(can_spawn_task);
    #[cfg(not(test))]
    let can_spawn = can_spawn_task();

    if !can_spawn {
        return false;
    }

    #[cfg(test)]
    {
        let recorded = TEST_SHELL_TASK_RECORDER.with(|rec| {
            if let Some(list) = rec.borrow_mut().as_mut() {
                list.push((command.to_string(), title.to_string()));
                true
            } else {
                false
            }
        });
        if recorded {
            return true;
        }
    }

    syspilot_core::spawn_shell_task_with_callback(command, title, move || {
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(err) => {
                    eprintln!("SysPilot: failed to build runtime for post-triage: {err}");
                    return;
                }
            };
            rt.block_on(async {
                let _ = syspilot_core::run_triage(true, false).await;
                let status_path = syspilot_core::get_status_path();
                let snapshot_res = syspilot_core::read_system_snapshot(&status_path);
                let _ = ui_weak.upgrade_in_event_loop(move |ui_inst| match snapshot_res {
                    Ok(snapshot) => update_ui_from_snapshot(&ui_inst, &snapshot),
                    Err(err) => update_ui_from_error(&ui_inst, &err),
                });
            });
        });
    })
    .is_ok()
}

fn trigger_shell_task(command: &str, title: &str) {
    if !can_spawn_task() {
        return;
    }
    let _ = syspilot_core::spawn_shell_task(command, title);
}

fn trigger_terminal_task(args: &[&str], title: &str) {
    if !can_spawn_task() {
        return;
    }
    let _ = syspilot_core::spawn_syspilot_task(args, title);
}

fn compute_stardate() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now();
    let epoch_secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

    unsafe {
        let mut tm = std::mem::MaybeUninit::<libc::tm>::zeroed();
        libc::localtime_r(&epoch_secs, tm.as_mut_ptr());
        let tm = tm.assume_init();
        let year = tm.tm_year + 1900;
        let day_of_year = tm.tm_yday + 1; // 1-366
        let sec_of_day = (tm.tm_hour as u32) * 3600 + (tm.tm_min as u32) * 60 + (tm.tm_sec as u32);
        let fraction = (sec_of_day * 100) / 86400;
        format!("STARDATE {year}.{day_of_year:03}.{fraction:02}")
    }
}

fn compute_maintenance_window_height(count: usize) -> f32 {
    let h = 180.0 + (count as f32) * 56.0;
    let rounded = (h / 10.0).round() * 10.0;
    rounded.clamp(590.0, 1200.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkinOverride {
    Lcars,
    Classic,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GuiLaunchArgs {
    smoke: bool,
    skin: Option<SkinOverride>,
}

fn parse_skin_value(value: &str) -> Result<SkinOverride, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "lcars" => Ok(SkinOverride::Lcars),
        "classic" => Ok(SkinOverride::Classic),
        other => Err(format!(
            "invalid skin '{other}': expected 'lcars' or 'classic'"
        )),
    }
}

fn parse_gui_args<I, S>(args: I) -> Result<GuiLaunchArgs, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut parsed = GuiLaunchArgs::default();
    let mut iter = args.into_iter().skip(1);
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if arg == "--smoke" {
            parsed.smoke = true;
        } else if arg == "--skin" {
            let val = iter
                .next()
                .ok_or_else(|| "--skin requires a value ('lcars' or 'classic')".to_string())?;
            parsed.skin = Some(parse_skin_value(val.as_ref())?);
        } else if let Some(val) = arg.strip_prefix("--skin=") {
            parsed.skin = Some(parse_skin_value(val)?);
        } else {
            return Err(format!("unknown option: {arg}"));
        }
    }
    Ok(parsed)
}

fn should_exit_due_to_lock<F>(smoke: bool, acquire_lock: F) -> bool
where
    F: FnOnce() -> bool,
{
    if smoke { false } else { !acquire_lock() }
}

fn resolve_initial_skin(override_skin: Option<SkinOverride>, persisted_is_lcars: bool) -> bool {
    match override_skin {
        Some(SkinOverride::Lcars) => true,
        Some(SkinOverride::Classic) => false,
        None => persisted_is_lcars,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::getuid() == 0 } {
        eprintln!("FATAL: SysPilot GUI must not be executed as root (UID 0).");
        std::process::exit(1);
    }

    let args = parse_gui_args(std::env::args()).unwrap_or_else(|err| {
        eprintln!("SysPilot GUI: {err}");
        std::process::exit(2);
    });

    if should_exit_due_to_lock(args.smoke, acquire_single_instance_lock) {
        eprintln!("SysPilot GUI is already running.");
        return Ok(());
    }

    let ui = AppWindow::new()?;
    let is_lcars = resolve_initial_skin(args.skin, syspilot_core::skin::load_skin_preference());
    ui.set_is_lcars_skin(is_lcars);

    let initial_tab = syspilot_core::skin::load_last_tab_preference();
    ui.set_active_tab(initial_tab);

    ui.set_stardate_label(compute_stardate().into());

    // Skin Toggle & Persistence
    let ui_skin = ui.as_weak();
    ui.on_toggle_skin(move || {
        if let Some(ui_inst) = ui_skin.upgrade() {
            let current = ui_inst.get_is_lcars_skin();
            let next = !current;
            ui_inst.set_is_lcars_skin(next);
            syspilot_core::skin::save_skin_preference(next);
            apply_tab_selection_to_window(&ui_inst, ui_inst.get_active_tab());
        }
    });

    // Dynamic Window Auto-Expansion on Tab Navigation & Last Tab Persistence
    let ui_window = ui.as_weak();
    ui.on_tab_changed(move |tab_idx| {
        syspilot_core::skin::save_last_tab_preference(tab_idx);
        if let Some(ui_inst) = ui_window.upgrade() {
            apply_tab_selection_to_window(&ui_inst, tab_idx);
        }
    });

    // Continuous dynamic Stardate timer (updates every 10s on GUI event loop)
    let ui_stardate = ui.as_weak();
    let stardate_timer = slint::Timer::default();
    stardate_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(10),
        move || {
            if let Some(ui_inst) = ui_stardate.upgrade() {
                ui_inst.set_stardate_label(compute_stardate().into());
            }
        },
    );

    // Initial population from cached status.json
    let status_path = syspilot_core::get_status_path();
    match syspilot_core::read_system_snapshot(&status_path) {
        Ok(snapshot) => update_ui_from_snapshot(&ui, &snapshot),
        Err(error) => update_ui_from_error(&ui, &error),
    }
    apply_tab_selection_to_window(&ui, initial_tab);

    // Populate Copilot Metadata & SRE Telemetry
    let copilot_info = syspilot_core::get_copilot_info();
    if copilot_info.ready {
        ui.set_copilot_badge_label("COPILOT ONLINE".into());
    } else {
        ui.set_copilot_badge_label("COPILOT OFFLINE".into());
    }
    ui.set_copilot_status_text(copilot_info.status_line().into());
    update_playbook_telemetry(&ui);

    // Connect Callbacks - Dashboard & General
    let ui_weak = ui.as_weak();
    ui.on_refresh_clicked(move || {
        if REFRESH_IN_FLIGHT
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        if let Some(ui_inst) = ui_weak.upgrade() {
            ui_inst.set_is_refreshing(true);
            ui_inst.set_refresh_label("04  SCANNING...".into());
        }

        let copy = ui_weak.clone();
        std::thread::spawn(move || {
            let _guard = RefreshFlightGuard;
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(err) => {
                    eprintln!("SysPilot: failed to build runtime for refresh: {err}");
                    let _ = copy.upgrade_in_event_loop(|ui_inst| {
                        ui_inst.set_is_refreshing(false);
                        ui_inst.set_refresh_label("04  REFRESH DATA".into());
                    });
                    return;
                }
            };
            rt.block_on(async {
                let _ = syspilot_core::run_triage(true, false).await;
                let status_path = syspilot_core::get_status_path();
                let snapshot_res = syspilot_core::read_system_snapshot(&status_path);

                // Small tactile pacing delay so the user feels the telemetry scan
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;

                let copy_timer = copy.clone();
                let _ = copy.upgrade_in_event_loop(move |ui_inst| {
                    match snapshot_res {
                        Ok(snapshot) => update_ui_from_snapshot(&ui_inst, &snapshot),
                        Err(err) => update_ui_from_error(&ui_inst, &err),
                    }
                    update_playbook_telemetry(&ui_inst);
                    ui_inst.set_is_refreshing(false);
                    ui_inst.set_refresh_label("04  SCAN COMPLETE".into());

                    // Revert label after 1.2s
                    slint::Timer::single_shot(std::time::Duration::from_millis(1200), move || {
                        if let Some(ui) = copy_timer.upgrade() {
                            ui.set_refresh_label("04  REFRESH DATA".into());
                        }
                    });
                });
            });
        });
    });

    let ui_weak_upgrade = ui.as_weak();
    ui.on_upgrade_clicked(move || {
        let Some(ui_inst) = ui_weak_upgrade.upgrade() else {
            return;
        };
        let is_primary_dead = ui_inst.get_mirror_primary_dead();
        let standalone_count = ui_inst.get_standalone_updates_count().max(0) as u32;
        match resolve_upgrade_click_action(is_primary_dead, standalone_count) {
            UpgradeClickAction::ShowMirrorWarning => {
                ui_inst.set_upgrade_mirror_warning_visible(true);
            }
            UpgradeClickAction::ShowStandalonePrompt => {
                ui_inst.set_upgrade_standalone_prompt_visible(true);
            }
            UpgradeClickAction::LaunchDirectSystemUpgrade => {
                trigger_task_with_post_triage(
                    ui_weak_upgrade.clone(),
                    &["--upgrade"],
                    "SysPilot Guarded Upgrade",
                );
            }
        }
    });

    let ui_weak_confirm_upgrade = ui.as_weak();
    ui.on_confirm_upgrade_anyway_clicked(move || {
        let Some(ui_inst) = ui_weak_confirm_upgrade.upgrade() else {
            return;
        };
        let standalone_count = ui_inst.get_standalone_updates_count().max(0) as u32;
        match resolve_upgrade_click_action(false, standalone_count) {
            UpgradeClickAction::ShowStandalonePrompt => {
                ui_inst.set_upgrade_standalone_prompt_visible(true);
            }
            UpgradeClickAction::LaunchDirectSystemUpgrade
            | UpgradeClickAction::ShowMirrorWarning => {
                trigger_task_with_post_triage(
                    ui_weak_confirm_upgrade.clone(),
                    &["--upgrade"],
                    "SysPilot Guarded Upgrade",
                );
            }
        }
    });

    let ui_weak_upgrade_with_software = ui.as_weak();
    ui.on_confirm_upgrade_with_software_clicked(move || {
        let syspilot_bin = syspilot_core::resolve_syspilot()
            .unwrap_or_else(|| std::path::PathBuf::from("syspilot"));
        let chained_cmd = build_chained_upgrade_software_cmd(&syspilot_bin);
        let _ = trigger_shell_task_with_post_triage(
            ui_weak_upgrade_with_software.clone(),
            &chained_cmd,
            "SysPilot Full Upgrade (System + Tools)",
        );
    });

    let ui_weak_upgrade_system_only = ui.as_weak();
    ui.on_confirm_upgrade_system_only_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_upgrade_system_only.clone(),
            &["--upgrade"],
            "SysPilot Guarded Upgrade (System Only)",
        );
    });

    let ui_weak_run_software = ui.as_weak();
    ui.on_run_software_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_run_software.clone(),
            &["--software"],
            "SysPilot Standalone Software Updates",
        );
    });

    let ui_weak_refresh_mirrors = ui.as_weak();
    ui.on_refresh_mirrors_from_warning_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_refresh_mirrors.clone(),
            &["--mirrors"],
            "SysPilot Mirror Ranking",
        );
    });

    let ui_weak_audit = ui.as_weak();
    ui.on_audit_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_audit.clone(),
            &["--audit"],
            "SysPilot Diagnostic Audit",
        );
    });

    let ui_weak_prune = ui.as_weak();
    ui.on_prune_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_prune.clone(),
            &["--orphans"],
            "SysPilot Orphan Package Triage",
        );
    });

    // Connect Callbacks - Maintenance Suite
    let ui_weak_orphans = ui.as_weak();
    ui.on_run_orphans_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_orphans.clone(),
            &["--orphans"],
            "SysPilot Orphan Package Triage",
        );
    });

    let ui_weak_mirrors = ui.as_weak();
    ui.on_run_mirrors_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_mirrors.clone(),
            &["--mirrors"],
            "SysPilot Mirror Ranking",
        );
    });

    let ui_weak_maint = ui.as_weak();
    ui.on_run_maintenance_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_maint.clone(),
            &["--maintenance"],
            "SysPilot Safe Maintenance",
        );
    });

    let ui_weak_clean = ui.as_weak();
    ui.on_run_deep_clean_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_clean.clone(),
            &["--deep-clean"],
            "SysPilot Deep Clean",
        );
    });

    ui.on_run_cve_clicked(|| {
        trigger_shell_task(
            "echo '🔍 Arch Linux Security Advisories (CVE Triage)...'; if type -P arch-audit &>/dev/null; then arch-audit -u; else echo 'arch-audit is not installed. Install via: sudo pacman -S arch-audit'; fi",
            "SysPilot Security CVE Inspection",
        );
    });

    ui.on_run_firmware_clicked(|| {
        trigger_shell_task(
            "echo '⚡ Hardware Firmware Upgrade (fwupd / LVFS)...'; if type -P fwupdmgr &>/dev/null; then fwupdmgr update; else echo 'fwupd is not installed. Install via: sudo pacman -S fwupd'; fi",
            "SysPilot Hardware Firmware Upgrade",
        );
    });

    ui.on_run_precache_clicked(|| {
        trigger_shell_task(
            "echo '⚡ Pre-caching pending package updates into pacman cache...'; if type -P checkupdates &>/dev/null; then checkupdates -d || sudo pacman -Sw --noconfirm; else sudo pacman -Sw --noconfirm; fi",
            "SysPilot Package Pre-Caching",
        );
    });

    let ui_weak_update = ui.as_weak();
    ui.on_run_self_update_clicked(move || {
        trigger_task_with_post_triage(
            ui_weak_update.clone(),
            &["--self-update"],
            "SysPilot Suite Self-Update",
        );
    });

    // Connect Callbacks - AI Copilot & SRE Tactical Playbooks
    ui.on_copilot_terminal_clicked(|| {
        trigger_terminal_task(&["--copilot-shell"], "SysPilot AI Copilot Interactive");
    });

    let ui_weak_copy = ui.as_weak();
    ui.on_copy_telemetry_clicked(move || {
        if let Some(ui_inst) = ui_weak_copy.upgrade() {
            ui_inst
                .set_copilot_status_text("✔ SRE Telemetry copied to clipboard (Markdown)".into());
            let ui_weak_restore = ui_weak_copy.clone();
            slint::Timer::single_shot(std::time::Duration::from_millis(2500), move || {
                if let Some(ui_inst) = ui_weak_restore.upgrade() {
                    let copilot_info = syspilot_core::get_copilot_info();
                    ui_inst.set_copilot_status_text(copilot_info.status_line().into());
                }
            });
        }
    });

    let ui_weak_pb = ui.as_weak();
    ui.on_copilot_playbook_clicked(move |playbook_id| {
        let ui_for_pb = ui_weak_pb.clone();
        handle_playbook_click(
            playbook_id.as_str(),
            |args, title, post_triage| {
                if post_triage {
                    trigger_task_with_post_triage(ui_for_pb.clone(), args, title);
                } else {
                    trigger_terminal_task(args, title);
                }
            },
            |cmd, title| {
                trigger_shell_task(cmd, title);
            },
            || {
                let playbooks = syspilot_core::discover_user_playbooks();
                if let Some(first) = playbooks.first() {
                    let _ = syspilot_core::spawn_terminal_command(&first.path, &[], &first.name);
                } else {
                    trigger_shell_task(
                        "echo 'No executable *.sh playbooks in ~/.config/syspilot/playbooks/'",
                        "SysPilot Custom Playbooks",
                    );
                }
            },
        );
    });

    if args.smoke {
        ui.window().request_redraw();
        slint::Timer::single_shot(std::time::Duration::from_millis(150), || {
            let _ = slint::quit_event_loop();
        });
        ui.run()?;
        return Ok(());
    }

    // Live telemetry file watcher: poll status.json and summary.json mtime every 1s
    let ui_weak_watcher = ui.as_weak();
    let gui_running = Arc::new(AtomicBool::new(true));
    let gui_running_watcher = Arc::clone(&gui_running);

    std::thread::spawn(move || {
        let status_path = syspilot_core::get_status_path();
        let summary_path = syspilot_core::state_base().join("system-health/summary.json");
        let mut last_status_mtime = std::fs::metadata(&status_path)
            .and_then(|m| m.modified())
            .ok();
        let mut last_summary_mtime = std::fs::metadata(&summary_path)
            .and_then(|m| m.modified())
            .ok();
        while gui_running_watcher.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if !gui_running_watcher.load(Ordering::Relaxed) {
                break;
            }

            let current_status_mtime = std::fs::metadata(&status_path)
                .and_then(|m| m.modified())
                .ok();
            let current_summary_mtime = std::fs::metadata(&summary_path)
                .and_then(|m| m.modified())
                .ok();
            if current_status_mtime != last_status_mtime
                || current_summary_mtime != last_summary_mtime
            {
                last_status_mtime = current_status_mtime;
                last_summary_mtime = current_summary_mtime;
                let snapshot_res = syspilot_core::read_system_snapshot(&status_path);
                let dispatch_res = ui_weak_watcher.upgrade_in_event_loop(move |ui_inst| {
                    match snapshot_res {
                        Ok(snapshot) => update_ui_from_snapshot(&ui_inst, &snapshot),
                        Err(err) => update_ui_from_error(&ui_inst, &err),
                    }
                    update_playbook_telemetry(&ui_inst);
                });
                if dispatch_res.is_err() {
                    break;
                }
            }
        }
    });

    let run_res = ui.run();
    gui_running.store(false, Ordering::Relaxed);
    run_res?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_debounce_window_rejection() {
        assert!(can_spawn_task_with_window(0));
        assert!(!can_spawn_task_with_window(5000));
    }

    #[test]
    fn test_refresh_flight_guard_raii() {
        REFRESH_IN_FLIGHT.store(false, Ordering::SeqCst);
        assert!(!REFRESH_IN_FLIGHT.load(Ordering::SeqCst));
        {
            let _guard = RefreshFlightGuard;
            REFRESH_IN_FLIGHT.store(true, Ordering::SeqCst);
            assert!(REFRESH_IN_FLIGHT.load(Ordering::SeqCst));
        }
        assert!(!REFRESH_IN_FLIGHT.load(Ordering::SeqCst));
    }

    #[test]
    fn test_compute_maintenance_window_height() {
        assert_eq!(compute_maintenance_window_height(9), 680.0);
        assert_eq!(compute_maintenance_window_height(8), 630.0);
        assert_eq!(compute_maintenance_window_height(0), 590.0);
        assert_eq!(compute_maintenance_window_height(10), 740.0);
        assert_eq!(compute_maintenance_window_height(20), 1200.0);
    }

    #[test]
    fn test_compute_tab_window_size_and_tab0_geometry() {
        assert_eq!(compute_tab0_window_width(0), 920.0);
        assert_eq!(compute_tab0_window_width(58), 920.0);
        assert_eq!(compute_tab0_window_width(72), 1010.0);
        assert_eq!(compute_tab0_window_width(140), 1040.0);

        assert_eq!(compute_tab0_window_height(0, false, true), 620.0);
        assert_eq!(compute_tab0_window_height(1, false, true), 686.0);
        assert_eq!(compute_tab0_window_height(2, true, true), 758.0);
        assert_eq!(compute_tab0_window_height(12, false, true), 900.0);

        assert_eq!(compute_tab0_window_height(0, false, false), 760.0);
        assert_eq!(compute_tab0_window_height(1, true, false), 886.0);
        assert_eq!(compute_tab0_window_height(6, false, false), 900.0);

        assert_eq!(
            compute_tab_window_size(0, 10, 920.0, 1, false, true, false, false),
            Some((920.0, 686.0))
        );
        assert_eq!(
            compute_tab_window_size(1, 10, 900.0, 1, false, true, false, false),
            Some((920.0, 740.0))
        );
        assert_eq!(
            compute_tab_window_size(2, 10, 900.0, 1, false, true, false, false),
            Some((980.0, 780.0))
        );
        assert_eq!(
            compute_tab_window_size(0, 10, 900.0, 0, false, true, true, false),
            None
        );
        assert_eq!(
            compute_tab_window_size(0, 10, 900.0, 0, false, true, false, true),
            None
        );
    }

    #[test]
    fn test_format_hardware_info() {
        assert_eq!(
            format_hardware_info("EndeavourOS", "6.12.8-arch1-1"),
            "ENDEAVOUROS // 6.12.8-ARCH1-1"
        );
        assert_eq!(
            format_hardware_info("Arch Linux", "6.6.10-lts"),
            "ARCH LINUX // 6.6.10-LTS"
        );
        assert_eq!(format_hardware_info("", ""), "LINUX // UNKNOWN");
    }

    #[test]
    fn test_format_kernel_and_gpu_telemetry_for_classic_and_lcars() {
        // Healthy kernel & modules -> ("PASS", (34, 197, 94), "RUNNING LINUX 7.2.14-ZEN1-1-ZEN • MODULES OK")
        let healthy_safety = syspilot_core::RebootSafetyStatus {
            black_screen_risk: false,
            reboot_pending: false,
            ..Default::default()
        };
        let (status, rgb, text) =
            format_kernel_telemetry("7.2.14-zen1-1-zen", false, &healthy_safety);
        assert_eq!(status, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(text, "RUNNING LINUX 7.2.14-ZEN1-1-ZEN • MODULES OK");

        // Safe reboot pending -> ("REBOOT", (245, 158, 11), "KERNEL REPLACED ON DISK — REBOOT SAFE (MODULES VERIFIED)")
        let reboot_safe = syspilot_core::RebootSafetyStatus {
            black_screen_risk: false,
            reboot_pending: true,
            ..Default::default()
        };
        let (status, rgb, text) = format_kernel_telemetry("7.2.14-zen1-1-zen", true, &reboot_safe);
        assert_eq!(status, "REBOOT");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(
            text,
            "KERNEL REPLACED ON DISK — REBOOT SAFE (MODULES VERIFIED)"
        );

        // Black screen risk -> ("ALERT", (239, 68, 68), "REBOOT UNSAFE • ACTIVE GPU KERNEL MODULE 'NVIDIA' MISSING FOR KERNEL 7.2.15-ZEN1-1-ZEN")
        let unsafe_safety = syspilot_core::RebootSafetyStatus {
            black_screen_risk: true,
            issues: vec![
                "ACTIVE GPU KERNEL MODULE 'NVIDIA' MISSING FOR KERNEL 7.2.15-ZEN1-1-ZEN"
                    .to_string(),
            ],
            ..Default::default()
        };
        let (status, rgb, text) =
            format_kernel_telemetry("7.2.14-zen1-1-zen", true, &unsafe_safety);
        assert_eq!(status, "ALERT");
        assert_eq!(rgb, (239, 68, 68));
        assert_eq!(
            text,
            "REBOOT UNSAFE • ACTIVE GPU KERNEL MODULE 'NVIDIA' MISSING FOR KERNEL 7.2.15-ZEN1-1-ZEN"
        );

        // GPU with 64/32 Vulkan ready -> ("PASS", (34, 197, 94), "NVIDIA GEFORCE GTX 970 • DRIVER 580.196.07 • VULKAN 64/32 OK")
        let ready_gpu = syspilot_core::GamingReadinessStatus {
            applicable: true,
            ready: true,
            vulkan_64_ready: true,
            vulkan_32_ready: true,
            ..Default::default()
        };
        let (status, rgb, text) =
            format_gpu_telemetry("NVIDIA GeForce GTX 970 • Driver 580.196.07", &ready_gpu);
        assert_eq!(status, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(
            text,
            "NVIDIA GEFORCE GTX 970 • DRIVER 580.196.07 • VULKAN 64/32 OK"
        );

        // GPU with missing 32-bit Vulkan -> ("WARN", (245, 158, 11), "NVIDIA GEFORCE GTX 970 • DRIVER 580.196.07 • VULKAN 32-BIT MISSING")
        let missing_32 = syspilot_core::GamingReadinessStatus {
            applicable: true,
            ready: false,
            vulkan_64_ready: true,
            vulkan_32_ready: false,
            ..Default::default()
        };
        let (status, rgb, text) =
            format_gpu_telemetry("NVIDIA GeForce GTX 970 • Driver 580.196.07", &missing_32);
        assert_eq!(status, "WARN");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(
            text,
            "NVIDIA GEFORCE GTX 970 • DRIVER 580.196.07 • VULKAN 32-BIT MISSING"
        );

        // Headless VM (applicable: false) -> ("INFO", (56, 189, 248), "DEFAULT DISPLAY ADAPTER")
        let non_applicable = syspilot_core::GamingReadinessStatus {
            applicable: false,
            ..Default::default()
        };
        let (status, rgb, text) = format_gpu_telemetry("Default Display Adapter", &non_applicable);
        assert_eq!(status, "INFO");
        assert_eq!(rgb, (56, 189, 248));
        assert_eq!(text, "DEFAULT DISPLAY ADAPTER");
    }

    #[test]
    fn test_format_aur_updates_zero() {
        let (status, rgb, text) = format_aur_updates(0);
        assert_eq!(status, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(text, "0 AUR PACKAGES PENDING");
    }

    #[test]
    fn test_format_aur_updates_singular() {
        let (status, rgb, text) = format_aur_updates(1);
        assert_eq!(status, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(text, "1 AUR PACKAGE PENDING");
    }

    #[test]
    fn test_format_aur_updates_plural() {
        let (status, rgb, text) = format_aur_updates(4);
        assert_eq!(status, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(text, "4 AUR PACKAGES PENDING");
    }

    #[test]
    fn test_gui_formatter_permutation_matrix_0_1_n_error() {
        // format_failed_services (0, 1, N)
        let (st, rgb, det) = format_failed_services(0);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "0 FAILED SYSTEMD UNITS");

        let (st, rgb, det) = format_failed_services(1);
        assert_eq!(st, "1 FAILED");
        assert_eq!(rgb, (239, 68, 68));
        assert_eq!(det, "1 DEGRADED SYSTEMD UNIT");

        let (st, rgb, det) = format_failed_services(5);
        assert_eq!(st, "5 FAILED");
        assert_eq!(rgb, (239, 68, 68));
        assert_eq!(det, "5 DEGRADED SYSTEMD UNITS");

        // format_cve_status (0, 1, N)
        let (st, rgb, det) = format_cve_status(0);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "NO UNPATCHED CVE VULNERABILITIES");

        let (st, rgb, det) = format_cve_status(1);
        assert_eq!(st, "ALERT");
        assert_eq!(rgb, (239, 68, 68));
        assert_eq!(det, "1 HIGH-RISK CVE VULNERABILITY PENDING");

        let (st, rgb, det) = format_cve_status(3);
        assert_eq!(st, "ALERT");
        assert_eq!(rgb, (239, 68, 68));
        assert_eq!(det, "3 HIGH-RISK CVE VULNERABILITIES PENDING");

        // format_core_updates (0, 1, N)
        let (st, rgb, det) = format_core_updates(0);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "0 CORE SYSTEM UPDATES PENDING");

        let (st, rgb, det) = format_core_updates(1);
        assert_eq!(st, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "1 CORE SYSTEM UPDATE PENDING");

        let (st, rgb, det) = format_core_updates(4);
        assert_eq!(st, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "4 CORE SYSTEM UPDATES PENDING");

        // format_repo_updates (0, 1, N)
        let (st, rgb, det) = format_repo_updates(0);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "0 OFFICIAL PACKAGES PENDING");

        let (st, rgb, det) = format_repo_updates(1);
        assert_eq!(st, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "1 OFFICIAL PACKAGE PENDING");

        let (st, rgb, det) = format_repo_updates(7);
        assert_eq!(st, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "7 OFFICIAL PACKAGES PENDING");

        // format_firmware_updates (0, 1, N)
        let (st, rgb, det) = format_firmware_updates(0);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "FIRMWARE SIGNATURES VERIFIED • LVFS CLEAN");

        let (st, rgb, det) = format_firmware_updates(1);
        assert_eq!(st, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "1 FIRMWARE UPDATE AVAILABLE VIA LVFS");

        let (st, rgb, det) = format_firmware_updates(2);
        assert_eq!(st, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "2 FIRMWARE UPDATES AVAILABLE VIA LVFS");

        // format_aur_updates (0, 1, N)
        let (st, rgb, det) = format_aur_updates(0);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "0 AUR PACKAGES PENDING");

        let (st, rgb, det) = format_aur_updates(1);
        assert_eq!(st, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "1 AUR PACKAGE PENDING");

        let (st, rgb, det) = format_aur_updates(8);
        assert_eq!(st, "UPDATE");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "8 AUR PACKAGES PENDING");

        // format_root_storage (0%, 79%, 80%, 89%, 90%, 100%)
        let (st, rgb, det) = format_root_storage(0, 50.0);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "0% USED • 50.0 GB AVAILABLE ON ROOT");

        let (st, rgb, det) = format_root_storage(79, 25.5);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "79% USED • 25.5 GB AVAILABLE ON ROOT");

        let (st, rgb, det) = format_root_storage(80, 20.0);
        assert_eq!(st, "WARN");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "80% USED • 20.0 GB AVAILABLE ON ROOT");

        let (st, rgb, det) = format_root_storage(89, 12.0);
        assert_eq!(st, "WARN");
        assert_eq!(rgb, (245, 158, 11));
        assert_eq!(det, "89% USED • 12.0 GB AVAILABLE ON ROOT");

        let (st, rgb, det) = format_root_storage(90, 10.0);
        assert_eq!(st, "ALERT");
        assert_eq!(rgb, (239, 68, 68));
        assert_eq!(det, "90% USED • 10.0 GB AVAILABLE ON ROOT");

        let (st, rgb, det) = format_root_storage(100, 0.1);
        assert_eq!(st, "ALERT");
        assert_eq!(rgb, (239, 68, 68));
        assert_eq!(det, "100% USED • 0.1 GB AVAILABLE ON ROOT");

        // format_firewall_status (true/false)
        let (st, rgb, det) = format_firewall_status(true);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, (34, 197, 94));
        assert_eq!(det, "FIREWALL ACTIVE • PACKET FILTERING ENFORCED");

        let (st, rgb, det) = format_firewall_status(false);
        assert_eq!(st, "INFO");
        assert_eq!(rgb, (56, 189, 248));
        assert_eq!(det, "PROTECTED BY PERIMETER ROUTER/LAN");

        // format_overall_header
        let nominal_snap = syspilot_core::SystemSnapshot {
            status: "ALL_CLEAR".to_string(),
            cache_stale: false,
            ..Default::default()
        };
        let nominal_safety = syspilot_core::RebootSafetyStatus::default();
        let (eff, label, rgb) = format_overall_header(&nominal_snap, &nominal_safety);
        assert_eq!(eff, "ALL_CLEAR");
        assert_eq!(label, "SYSTEM OPERATIONAL");
        assert_eq!(rgb, (34, 197, 94));

        let gamemode_snap = syspilot_core::SystemSnapshot {
            status: "ALL_CLEAR".to_string(),
            gaming_mode: true,
            cache_stale: false,
            ..Default::default()
        };
        let (eff, label, rgb) = format_overall_header(&gamemode_snap, &nominal_safety);
        assert_eq!(eff, "ALL_CLEAR");
        assert_eq!(label, "GAMEMODE ENGAGED");
        assert_eq!(rgb, (56, 189, 248));

        let preflight_snap = syspilot_core::SystemSnapshot {
            status: "PRE_FLIGHT_ATTENTION".to_string(),
            warnings: 2,
            cache_stale: false,
            ..Default::default()
        };
        let (eff, label, rgb) = format_overall_header(&preflight_snap, &nominal_safety);
        assert_eq!(eff, "PRE_FLIGHT_ATTENTION");
        assert_eq!(label, "SYSTEM ATTENTION");
        assert_eq!(rgb, (245, 158, 11));

        let action_snap = syspilot_core::SystemSnapshot {
            status: "ACTION_REQUIRED".to_string(),
            errors: 1,
            cache_stale: false,
            ..Default::default()
        };
        let (eff, label, rgb) = format_overall_header(&action_snap, &nominal_safety);
        assert_eq!(eff, "ACTION_REQUIRED");
        assert_eq!(label, "RED ALERT");
        assert_eq!(rgb, (239, 68, 68));

        let black_screen_safety = syspilot_core::RebootSafetyStatus {
            black_screen_risk: true,
            ..Default::default()
        };
        let (eff, label, rgb) = format_overall_header(&nominal_snap, &black_screen_safety);
        assert_eq!(eff, "ACTION_REQUIRED");
        assert_eq!(label, "RED ALERT");
        assert_eq!(rgb, (239, 68, 68));

        let stale_snap = syspilot_core::SystemSnapshot {
            status: "ALL_CLEAR".to_string(),
            cache_stale: true,
            ..Default::default()
        };
        let (eff, label, rgb) = format_overall_header(&stale_snap, &nominal_safety);
        assert_eq!(eff, "ALL_CLEAR");
        assert_eq!(label, "TELEMETRY DEGRADED");
        assert_eq!(rgb, (245, 158, 11));

        let error_snap = syspilot_core::SystemSnapshot {
            status: "ALL_CLEAR".to_string(),
            source_error: Some("socket timeout".to_string()),
            ..Default::default()
        };
        let (eff, label, rgb) = format_overall_header(&error_snap, &nominal_safety);
        assert_eq!(eff, "ALL_CLEAR");
        assert_eq!(label, "TELEMETRY DEGRADED");
        assert_eq!(rgb, (245, 158, 11));

        // format_snapshot_error_banner
        let (has_err, msg) = format_snapshot_error_banner(&error_snap);
        assert!(has_err);
        assert_eq!(msg, "socket timeout");

        let (has_err, msg) = format_snapshot_error_banner(&stale_snap);
        assert!(has_err);
        assert!(msg.starts_with("Telemetry cache stale"));

        let degraded_snap = syspilot_core::SystemSnapshot {
            cache_stale: false,
            source_error: None,
            status: "DEGRADED".to_string(),
            ..Default::default()
        };
        let (has_err, msg) = format_snapshot_error_banner(&degraded_snap);
        assert!(has_err);
        assert_eq!(msg, "Degraded telemetry stream");

        let (has_err, msg) = format_snapshot_error_banner(&nominal_snap);
        assert!(!has_err);
        assert_eq!(msg, "");

        // format_self_update
        let update_snap = syspilot_core::SystemSnapshot {
            app_update_available: true,
            app_latest_version: "2.2.0".to_string(),
            app_current_version: "2.1.3".to_string(),
            ..Default::default()
        };
        let su = format_self_update(&update_snap, "2.1.3");
        assert!(su.available);
        assert_eq!(su.badge, "v2.2.0 AVAILABLE");
        assert_eq!(su.title, "SysPilot Suite Self-Update (Update Available)");
        assert_eq!(su.btn_text, "Update to v2.2.0");

        let no_update_snap = syspilot_core::SystemSnapshot {
            app_update_available: false,
            ..Default::default()
        };
        let su = format_self_update(&no_update_snap, "2.1.3");
        assert!(!su.available);
        assert_eq!(su.badge, "v2.1.3 LATEST");
        assert_eq!(su.btn_text, "Check & Update Suite");

        // format_orphans_row
        let (st, rgb, det) = format_orphans_row();
        assert_eq!(st, "PASS");
        assert_eq!(rgb, RowTone::Pass.rgb());
        assert_eq!(det, "0 ORPHANS DETECTED • PACMAN CACHE TRIMMED");

        // RowTone::rgb
        assert_eq!(RowTone::Pass.rgb(), (34, 197, 94));
        assert_eq!(RowTone::Update.rgb(), (245, 158, 11));
        assert_eq!(RowTone::Warn.rgb(), (245, 158, 11));
        assert_eq!(RowTone::Alert.rgb(), (239, 68, 68));
        assert_eq!(RowTone::Info.rgb(), (56, 189, 248));
        assert_eq!(RowTone::Offline.rgb(), (100, 116, 139));
    }

    #[test]
    fn test_dashboard_view_model_snapshot_and_error_permutations() {
        // 1. Nominal
        let nominal_snap = syspilot_core::SystemSnapshot {
            os: "EndeavourOS".to_string(),
            kernel: "6.12.8-arch1-1".to_string(),
            status: "ALL_CLEAR".to_string(),
            firewall_active: true,
            cache_stale: false,
            updates_check_state: syspilot_core::UpdateCheckState::Ok,
            ..Default::default()
        };
        let nominal_safety = syspilot_core::RebootSafetyStatus::default();
        let nominal_sentinels = syspilot_core::SentinelStatus {
            root_used_pct: 45,
            root_avail_gb: 120.0,
            ..Default::default()
        };
        let nominal_gaming = syspilot_core::GamingReadinessStatus {
            applicable: true,
            ready: true,
            vulkan_64_ready: true,
            vulkan_32_ready: true,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &nominal_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert!(!vm.has_error);
        assert!(!vm.has_attention);
        assert_eq!(vm.attention_badge, "");
        assert_eq!(vm.attention_message, "");
        assert_eq!(vm.overall_status, "ALL_CLEAR");
        assert_eq!(vm.overall_status_label, "SYSTEM OPERATIONAL");
        assert_eq!(vm.overall_status_rgb, RowTone::Pass.rgb());
        assert_eq!(vm.hardware_info, "ENDEAVOUROS // 6.12.8-ARCH1-1");
        assert_eq!(vm.kernel.status, "PASS");
        assert_eq!(vm.root.status, "PASS");
        assert_eq!(vm.firewall.status, "PASS");
        assert_eq!(vm.failed_services.status, "PASS");
        assert_eq!(vm.gpu.status, "PASS");
        assert_eq!(vm.cve.status, "PASS");
        assert_eq!(vm.updates_core.status, "PASS");
        assert_eq!(vm.updates_repo.status, "PASS");
        assert_eq!(vm.updates_aur.status, "PASS");
        assert_eq!(vm.mirrors.status, "INFO");
        assert_eq!(vm.firmware.status, "PASS");
        assert_eq!(vm.orphans.status, "PASS");
        assert!(!vm.self_update.available);

        // 2. Singular
        let singular_snap = syspilot_core::SystemSnapshot {
            os: "Arch Linux".to_string(),
            kernel: "6.12.8-arch1-1".to_string(),
            status: "PRE_FLIGHT_ATTENTION".to_string(),
            errors: 1,
            security_high_risk: 1,
            core_updates: 1,
            regular_updates: 1,
            aur_updates: 1,
            firmware_updates: 1,
            app_update_available: true,
            app_latest_version: "2.2.0".to_string(),
            app_current_version: "2.1.4".to_string(),
            cache_stale: false,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &singular_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert!(vm.has_attention);
        assert_eq!(vm.attention_badge, "RED ALERT");
        assert_eq!(vm.failed_services.status, "1 FAILED");
        assert_eq!(vm.failed_services.details, "1 DEGRADED SYSTEMD UNIT");
        assert_eq!(vm.cve.status, "ALERT");
        assert_eq!(vm.cve.details, "1 HIGH-RISK CVE VULNERABILITY PENDING");
        assert_eq!(vm.updates_core.status, "UPDATE");
        assert_eq!(vm.updates_core.details, "1 CORE SYSTEM UPDATE PENDING");
        assert_eq!(vm.updates_repo.status, "UPDATE");
        assert_eq!(vm.updates_repo.details, "1 OFFICIAL PACKAGE PENDING");
        assert_eq!(vm.updates_aur.status, "UPDATE");
        assert_eq!(vm.updates_aur.details, "1 AUR PACKAGE PENDING");
        assert_eq!(vm.firmware.status, "UPDATE");
        assert_eq!(vm.firmware.details, "1 FIRMWARE UPDATE AVAILABLE VIA LVFS");
        assert!(vm.self_update.available);

        // 3. Plural
        let plural_snap = syspilot_core::SystemSnapshot {
            os: "Arch Linux".to_string(),
            kernel: "6.12.8-arch1-1".to_string(),
            status: "ACTION_REQUIRED".to_string(),
            errors: 4,
            security_high_risk: 3,
            core_updates: 5,
            regular_updates: 12,
            aur_updates: 7,
            firmware_updates: 2,
            cache_stale: false,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &plural_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert!(vm.has_attention);
        assert_eq!(vm.attention_badge, "RED ALERT");
        assert_eq!(vm.failed_services.status, "4 FAILED");
        assert_eq!(vm.failed_services.details, "4 DEGRADED SYSTEMD UNITS");
        assert_eq!(vm.cve.status, "ALERT");
        assert_eq!(vm.cve.details, "3 HIGH-RISK CVE VULNERABILITIES PENDING");
        assert_eq!(vm.updates_core.status, "UPDATE");
        assert_eq!(vm.updates_core.details, "5 CORE SYSTEM UPDATES PENDING");
        assert_eq!(vm.updates_repo.status, "UPDATE");
        assert_eq!(vm.updates_repo.details, "12 OFFICIAL PACKAGES PENDING");
        assert_eq!(vm.updates_aur.status, "UPDATE");
        assert_eq!(vm.updates_aur.details, "7 AUR PACKAGES PENDING");
        assert_eq!(vm.firmware.status, "UPDATE");
        assert_eq!(vm.firmware.details, "2 FIRMWARE UPDATES AVAILABLE VIA LVFS");

        // 4. Error permutations (source_error, cache_stale, black_screen_risk)
        let source_err_snap = syspilot_core::SystemSnapshot {
            source_error: Some("DBus connection lost".to_string()),
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &source_err_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert!(vm.has_error);
        assert!(!vm.has_attention);
        assert_eq!(vm.error_message, "DBus connection lost");
        assert_eq!(vm.overall_status_label, "TELEMETRY DEGRADED");

        let stale_err_snap = syspilot_core::SystemSnapshot {
            cache_stale: true,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &stale_err_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert!(vm.has_error);
        assert!(!vm.has_attention);
        assert!(vm.error_message.starts_with("Telemetry cache stale"));

        let black_screen_safety = syspilot_core::RebootSafetyStatus {
            black_screen_risk: true,
            issues: vec!["NVIDIA MODULE NOT FOUND".to_string()],
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &nominal_snap,
            &black_screen_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert_eq!(vm.overall_status, "ACTION_REQUIRED");
        assert_eq!(vm.overall_status_label, "RED ALERT");
        assert!(vm.has_attention);
        assert_eq!(vm.attention_badge, "RED ALERT");
        assert_eq!(vm.kernel.status, "ALERT");

        // 5. from_error
        let vm =
            DashboardViewModel::from_error("telemetry socket timeout", env!("CARGO_PKG_VERSION"));
        assert!(vm.has_error);
        assert!(!vm.has_attention);
        assert_eq!(vm.attention_badge, "");
        assert_eq!(vm.attention_message, "");
        assert_eq!(vm.error_message, "telemetry socket timeout");
        assert_eq!(vm.overall_status, "DATA_UNAVAILABLE");
        assert_eq!(vm.overall_status_label, "DATA UNAVAILABLE");
        assert_eq!(vm.overall_status_rgb, RowTone::Warn.rgb());
        assert_eq!(vm.hardware_info, "TELEMETRY STREAM DISCONNECTED");
        assert!(!vm.self_update.available);

        let rows = [
            &vm.kernel,
            &vm.root,
            &vm.firewall,
            &vm.failed_services,
            &vm.gpu,
            &vm.cve,
            &vm.updates_core,
            &vm.updates_repo,
            &vm.updates_aur,
            &vm.mirrors,
            &vm.firmware,
            &vm.orphans,
        ];
        assert_eq!(rows.len(), 12);
        for row in rows {
            assert_eq!(row.status, "OFFLINE");
            assert_eq!(row.rgb, RowTone::Offline.rgb());
            assert_eq!(row.details, "TELEMETRY UNAVAILABLE");
        }
    }

    #[test]
    fn test_mirrors_presentation_and_upgrade_click_permutations() {
        let nominal_safety = syspilot_core::RebootSafetyStatus::default();
        let nominal_sentinels = syspilot_core::SentinelStatus::default();
        let nominal_gaming = syspilot_core::GamingReadinessStatus::default();

        // 1. None
        let snap_none = syspilot_core::SystemSnapshot::default();
        let (st, rgb, det) = format_mirrors_row(&snap_none);
        assert_eq!(st, "INFO");
        assert_eq!(rgb, RowTone::Info.rgb());
        assert_eq!(det, "MIRROR TELEMETRY PENDING — RUN DIAGNOSTIC AUDIT");

        // 2. unknown
        let snap_unknown = syspilot_core::SystemSnapshot {
            mirrors: Some(syspilot_core::SysHealthMirrors {
                status: Some("unknown".into()),
                arch_primary_alive: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (st, _, det) = format_mirrors_row(&snap_unknown);
        assert_eq!(st, "INFO");
        assert_eq!(det, "MIRROR TELEMETRY PENDING — RUN DIAGNOSTIC AUDIT");

        // 3. PASS healthy
        let snap_pass = syspilot_core::SystemSnapshot {
            mirrors: Some(syspilot_core::SysHealthMirrors {
                status: Some("PASS".into()),
                arch_primary_alive: Some(true),
                arch_age_days: Some(3.0),
                arch_servers: Some(8),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (st, rgb, det) = format_mirrors_row(&snap_pass);
        assert_eq!(st, "PASS");
        assert_eq!(rgb, RowTone::Pass.rgb());
        assert_eq!(det, "ARCH MIRRORS HEALTHY (3D, 8 SRV)");

        // 4. INFO aging 46d
        let snap_aging = syspilot_core::SystemSnapshot {
            mirrors: Some(syspilot_core::SysHealthMirrors {
                arch_age_days: Some(46.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (st, rgb, det) = format_mirrors_row(&snap_aging);
        assert_eq!(st, "INFO");
        assert_eq!(rgb, RowTone::Info.rgb());
        assert_eq!(det, "ARCH MIRRORLIST AGING (46D)");

        // 5. WARN stale 91d
        let snap_stale = syspilot_core::SystemSnapshot {
            mirrors: Some(syspilot_core::SysHealthMirrors {
                arch_age_days: Some(91.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (st, rgb, det) = format_mirrors_row(&snap_stale);
        assert_eq!(st, "WARN");
        assert_eq!(rgb, RowTone::Warn.rgb());
        assert_eq!(det, "ARCH MIRRORLIST STALE (91D) — REFRESH RECOMMENDED");

        // 6. WARN primary DEAD
        let snap_dead = syspilot_core::SystemSnapshot {
            mirrors: Some(syspilot_core::SysHealthMirrors {
                status: Some("WARN".into()),
                message: Some("Arch mirror status: primary DEAD".into()),
                arch_primary_alive: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &snap_dead,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "Generic GPU",
            "2.1.4",
        );
        assert_eq!(vm.mirrors.status, "WARN");
        assert!(vm.mirror_primary_dead);
        assert!(vm.mirror_upgrade_warning.contains("primary DEAD"));

        // 7. handle_upgrade_click
        let mut warning_shown = false;
        let mut upgrade_launched = false;
        handle_upgrade_click(true, || warning_shown = true, || upgrade_launched = true);
        assert!(warning_shown);
        assert!(!upgrade_launched);

        warning_shown = false;
        upgrade_launched = false;
        handle_upgrade_click(false, || warning_shown = true, || upgrade_launched = true);
        assert!(!warning_shown);
        assert!(upgrade_launched);
    }

    #[test]
    fn test_dashboard_view_model_update_check_state_and_attention_permutations() {
        let nominal_safety = syspilot_core::RebootSafetyStatus::default();
        let nominal_sentinels = syspilot_core::SentinelStatus::default();
        let nominal_gaming = syspilot_core::GamingReadinessStatus {
            applicable: true,
            ready: true,
            ..Default::default()
        };

        // 1. Unknown update check state
        let unknown_snap = syspilot_core::SystemSnapshot {
            updates_check_state: syspilot_core::UpdateCheckState::Unknown,
            regular_updates: 0,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &unknown_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "Generic GPU",
            "2.1.4",
        );
        assert_eq!(vm.updates_repo.status, "INFO");
        assert_eq!(vm.updates_repo.details, "UPDATE CHECK PENDING");

        // 2. Ok update check state (0 updates)
        let ok_snap = syspilot_core::SystemSnapshot {
            updates_check_state: syspilot_core::UpdateCheckState::Ok,
            regular_updates: 0,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &ok_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "Generic GPU",
            "2.1.4",
        );
        assert_eq!(vm.updates_repo.status, "PASS");
        assert_eq!(vm.updates_repo.details, "0 OFFICIAL PACKAGES PENDING");

        // 3. Failed update check state with 1 failure (transient)
        let failed_1_snap = syspilot_core::SystemSnapshot {
            updates_check_state: syspilot_core::UpdateCheckState::Failed,
            updates_consecutive_failures: 1,
            regular_updates: 0,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &failed_1_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "Generic GPU",
            "2.1.4",
        );
        assert_eq!(vm.updates_repo.status, "INFO");
        assert_eq!(
            vm.updates_repo.details,
            "0 OFFICIAL PACKAGES PENDING (TRANSIENT RETRY)"
        );

        // 4. Failed update check state with >= 2 failures (warn)
        let failed_2_snap = syspilot_core::SystemSnapshot {
            updates_check_state: syspilot_core::UpdateCheckState::Failed,
            updates_consecutive_failures: 2,
            regular_updates: 0,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &failed_2_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "Generic GPU",
            "2.1.4",
        );
        assert_eq!(vm.updates_repo.status, "WARN");
        assert_eq!(
            vm.updates_repo.details,
            "OFFICIAL REPO CHECK FAILED (NETWORK/DB LOCK)"
        );

        // 5. Attention when warnings > 0
        let warn_snap = syspilot_core::SystemSnapshot {
            status: "PRE_FLIGHT_ATTENTION".to_string(),
            warnings: 3,
            cache_stale: false,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &warn_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "Generic GPU",
            "2.1.4",
        );
        assert!(vm.has_attention);
        assert_eq!(vm.attention_badge, "ATTENTION");
        assert!(vm.attention_message.contains("3 warning(s)"));

        // 6. Attention when status_reasons is non-empty
        let reasons_snap = syspilot_core::SystemSnapshot {
            status: "PRE_FLIGHT_ATTENTION".to_string(),
            status_reasons: vec!["Root filesystem approaching quota threshold".to_string()],
            cache_stale: false,
            ..Default::default()
        };
        let vm = DashboardViewModel::from_snapshot_inputs(
            &reasons_snap,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "Generic GPU",
            "2.1.4",
        );
        assert!(vm.has_attention);
        assert_eq!(vm.attention_badge, "ATTENTION");
        assert_eq!(
            vm.attention_message,
            "Root filesystem approaching quota threshold"
        );
    }

    thread_local! {
        static HEADLESS_WINDOW: std::cell::RefCell<
            Option<std::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>>,
        > = const { std::cell::RefCell::new(None) };
    }

    struct HeadlessTestPlatform {
        start: std::time::Instant,
    }

    impl slint::platform::Platform for HeadlessTestPlatform {
        fn create_window_adapter(
            &self,
        ) -> Result<std::rc::Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
            let window =
                slint::platform::software_renderer::MinimalSoftwareWindow::new(Default::default());
            HEADLESS_WINDOW.with(|cell| {
                *cell.borrow_mut() = Some(window.clone());
            });
            Ok(window)
        }
        fn duration_since_start(&self) -> std::time::Duration {
            self.start.elapsed()
        }
    }

    fn init_headless_platform() {
        let _ = slint::platform::set_platform(Box::new(HeadlessTestPlatform {
            start: std::time::Instant::now(),
        }));
    }

    #[test]
    fn test_antonio_font_autohint_ready() {
        let data = include_bytes!("../ui/fonts/Antonio.ttf");
        assert!(data.len() >= 12);
        let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
        let search_range = u16::from_be_bytes([data[6], data[7]]);
        let entry_selector = u16::from_be_bytes([data[8], data[9]]);
        let range_shift = u16::from_be_bytes([data[10], data[11]]);
        let max_pow2 = (1usize << (usize::BITS - 1 - num_tables.leading_zeros())) as u16;
        assert_eq!(search_range, max_pow2 * 16);
        assert_eq!(entry_selector, max_pow2.trailing_zeros() as u16);
        assert_eq!(range_shift, (num_tables as u16) * 16 - search_range);

        let calc_csum = |slice: &[u8]| -> u32 {
            let mut sum = 0u32;
            for chunk in slice.chunks(4) {
                let mut buf = [0u8; 4];
                buf[..chunk.len()].copy_from_slice(chunk);
                sum = sum.wrapping_add(u32::from_be_bytes(buf));
            }
            sum
        };
        assert_eq!(calc_csum(data), 0xB1B0AFBA);

        let mut tags = Vec::with_capacity(num_tables);
        for i in 0..num_tables {
            let rec = &data[12 + i * 16..28 + i * 16];
            let tag = [rec[0], rec[1], rec[2], rec[3]];
            let csum = u32::from_be_bytes([rec[4], rec[5], rec[6], rec[7]]);
            let off = u32::from_be_bytes([rec[8], rec[9], rec[10], rec[11]]) as usize;
            let len = u32::from_be_bytes([rec[12], rec[13], rec[14], rec[15]]) as usize;
            if &tag == b"head" {
                let mut head = data[off..off + len].to_vec();
                head[8..12].fill(0);
                assert_eq!(calc_csum(&head), csum);
            } else {
                assert_eq!(calc_csum(&data[off..off + len]), csum);
            }
            if &tag == b"maxp" {
                assert_eq!(&data[off..off + 4], &[0x00, 0x01, 0x00, 0x00]);
                let max_instr = u16::from_be_bytes([data[off + 26], data[off + 27]]);
                assert_eq!(max_instr, 0);
            }
            tags.push(tag);
        }
        assert!(tags.windows(2).all(|w| w[0] < w[1]));
        assert!(!tags.contains(b"prep"));
        assert!(!tags.contains(b"fpgm"));
        assert!(!tags.contains(b"DSIG"));
        assert!(tags.contains(b"fvar"));
        assert!(tags.contains(b"glyf"));
        assert!(tags.contains(b"gvar"));
    }

    #[test]
    fn test_headless_appwindow_snapshot_and_error_binding() {
        init_headless_platform();
        let ui = AppWindow::new().unwrap();
        let headless_window = HEADLESS_WINDOW
            .with(|cell| cell.borrow().clone())
            .expect("headless window initialized");

        let snap = syspilot_core::SystemSnapshot {
            os: "EndeavourOS".to_string(),
            kernel: "6.12.8-arch1-1".to_string(),
            status: "ALL_CLEAR".to_string(),
            firewall_active: true,
            cache_stale: false,
            updates_check_state: syspilot_core::UpdateCheckState::Ok,
            ..Default::default()
        };

        update_ui_from_snapshot(&ui, &snap);
        headless_window.set_size(slint::PhysicalSize::new(920, 620));
        ui.set_is_lcars_skin(true);
        ui.set_hardware_info("ENDEAVOUROS // 7.3.5-ZEN1-1-ZEN".into());
        ui.show().unwrap();
        let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(920, 620);
        headless_window.request_redraw();
        let drawn = headless_window.draw_if_needed(|renderer| {
            let stride = buf.width() as usize;
            renderer.render(buf.make_mut_slice(), stride);
        });
        assert!(drawn);
        let px = |x: usize, y: usize| buf.as_slice()[y * 920 + x];
        let is_black = |p: slint::Rgb8Pixel| p.r == 0 && p.g == 0 && p.b == 0;
        for y in 20..48 {
            for x in 233..244 {
                assert!(is_black(px(x, y)), "left cutout gap not black at ({x},{y})");
            }
            for x in 564..575 {
                assert!(
                    is_black(px(x, y)),
                    "right cutout gap not black at ({x},{y})"
                );
            }
        }
        assert!(
            (20..48).any(|y| (248..560).any(|x| !is_black(px(x, y)))),
            "top cutout title failed to render"
        );
        assert!(!is_black(px(580, 34)), "trailing top rail missing");
        assert!(
            !is_black(px(141, 560)),
            "bottom elbow inner concave fillet has seam/notch"
        );
        assert!(
            is_black(px(158, 546)),
            "bottom elbow concave corner not black"
        );
        assert!(
            is_black(px(14, 606)),
            "bottom elbow outer 28px corner not rounded"
        );

        assert_eq!(ui.get_overall_status().as_str(), "ALL_CLEAR");
        assert_eq!(ui.get_overall_status_label().as_str(), "SYSTEM OPERATIONAL");
        assert!(!ui.get_has_error());
        assert!(!ui.get_has_attention());
        assert_eq!(ui.get_attention_badge().as_str(), "");
        assert_eq!(ui.get_attention_message().as_str(), "");
        assert_eq!(ui.get_kernel_status().as_str(), "PASS");
        assert_eq!(ui.get_firewall_status().as_str(), "PASS");
        assert_eq!(ui.get_failed_services_status().as_str(), "PASS");
        assert_eq!(ui.get_cve_status().as_str(), "PASS");
        assert_eq!(ui.get_updates_core_text().as_str(), "PASS");
        assert_eq!(ui.get_updates_repo_text().as_str(), "PASS");
        assert_eq!(ui.get_updates_aur_text().as_str(), "PASS");
        assert_eq!(ui.get_mirrors_status().as_str(), "INFO");
        assert_eq!(ui.get_firmware_status().as_str(), "PASS");
        assert_eq!(ui.get_orphans_status().as_str(), "PASS");
        assert!(!ui.get_mirror_primary_dead());
        assert!(!ui.get_upgrade_mirror_warning_visible());

        // Attention & multi-status matrix snapshot test (REBOOT, UPDATE, WARN, ALERT, 1 FAILED) across LCARS & Classic
        let attention_snap = syspilot_core::SystemSnapshot {
            os: "EndeavourOS".to_string(),
            kernel: "6.12.8-arch1-1".to_string(),
            status: "PRE_FLIGHT_ATTENTION".to_string(),
            warnings: 2,
            reboot_pending: true,
            core_updates: 1,
            regular_updates: 3,
            aur_updates: 2,
            firmware_updates: 1,
            standalone_updates: 2,
            standalone_tools: vec!["goose".into(), "uv".into()],
            security_high_risk: 1,
            errors: 1,
            cache_stale: false,
            updates_check_state: syspilot_core::UpdateCheckState::Ok,
            ..Default::default()
        };
        update_ui_from_snapshot(&ui, &attention_snap);
        assert!(!ui.get_has_error());
        assert!(ui.get_has_attention());
        assert_eq!(ui.get_attention_badge().as_str(), "RED ALERT");
        assert!(ui.get_attention_message().as_str().contains("1 error(s)"));
        assert_eq!(ui.get_kernel_status().as_str(), "REBOOT");
        assert_eq!(ui.get_updates_core_text().as_str(), "UPDATE");
        assert_eq!(ui.get_updates_repo_text().as_str(), "UPDATE");
        assert_eq!(ui.get_updates_aur_text().as_str(), "UPDATE");
        assert_eq!(ui.get_firmware_status().as_str(), "UPDATE");
        assert_eq!(ui.get_updates_standalone_status().as_str(), "UPDATE");
        assert_eq!(ui.get_cve_status().as_str(), "ALERT");
        assert_eq!(ui.get_failed_services_status().as_str(), "1 FAILED");
        ui.set_is_lcars_skin(false);
        assert!(ui.get_has_attention());
        ui.set_is_lcars_skin(true);
        assert!(ui.get_has_attention());

        // Dead primary mirror snapshot binding & modal check
        let dead_snap = syspilot_core::SystemSnapshot {
            os: "EndeavourOS".to_string(),
            kernel: "6.12.8-arch1-1".to_string(),
            status: "PRE_FLIGHT_ATTENTION".to_string(),
            cache_stale: false,
            mirrors: Some(syspilot_core::SysHealthMirrors {
                status: Some("WARN".into()),
                message: Some("Arch mirror status: primary DEAD".into()),
                arch_primary_alive: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        update_ui_from_snapshot(&ui, &dead_snap);
        assert_eq!(ui.get_mirrors_status().as_str(), "WARN");
        assert!(ui.get_mirror_primary_dead());
        assert!(!ui.get_mirror_upgrade_warning().as_str().is_empty());
        assert!(!ui.get_upgrade_mirror_warning_visible());

        // Test handle_upgrade_click toggling modal
        handle_upgrade_click(
            ui.get_mirror_primary_dead(),
            || ui.set_upgrade_mirror_warning_visible(true),
            || panic!("Should not trigger upgrade directly when primary is dead"),
        );
        assert!(ui.get_upgrade_mirror_warning_visible());

        // Cancel dialog
        ui.set_upgrade_mirror_warning_visible(false);
        assert!(!ui.get_upgrade_mirror_warning_visible());

        // SRE Telemetry export test
        apply_tab_selection_to_window(&ui, 0);
        assert_eq!(ui.get_sre_telemetry_export().as_str(), "");
        apply_tab_selection_to_window(&ui, 2);
        assert!(
            ui.get_sre_telemetry_export()
                .as_str()
                .contains("SysPilot AI Telemetry")
        );
        assert!(!ui.get_custom_playbook_desc().as_str().is_empty());

        // POD 00 / Classic Exception Telemetry ModelRc binding & non-clobbering resize
        let snap_anom = syspilot_core::SystemSnapshot {
            status: "PRE_FLIGHT_ATTENTION".into(),
            cache_stale: false,
            actionable_remediations: vec![syspilot_core::ActionableRemediation {
                check: Some("previous_boot".into()),
                severity: Some("WARN".into()),
                code: Some("SYS_UNCLEAN_SHUTDOWN".into()),
                summary: Some("Previous session crashed".into()),
                ..Default::default()
            }],
            pacnew_files: vec!["/etc/pacman.conf.pacnew".into()],
            ..Default::default()
        };
        let vm_anom = DashboardViewModel::from_snapshot_inputs(
            &snap_anom,
            &syspilot_core::RebootSafetyStatus::default(),
            &syspilot_core::SentinelStatus::default(),
            &syspilot_core::GamingReadinessStatus::default(),
            "NVIDIA GeForce GTX 970",
            "2.1.6",
        );
        assert_eq!(vm_anom.anomalies.len(), 2);
        assert_eq!(vm_anom.pod00_accent_rgb, (255, 153, 0));

        let snap_alert = syspilot_core::SystemSnapshot {
            status: "ACTION_REQUIRED".into(),
            cache_stale: false,
            actionable_remediations: vec![syspilot_core::ActionableRemediation {
                check: Some("previous_boot".into()),
                severity: Some("ERROR".into()),
                code: Some("SYS_KERNEL_PANIC".into()),
                summary: Some("Kernel panic in previous boot".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let vm_alert = DashboardViewModel::from_snapshot_inputs(
            &snap_alert,
            &syspilot_core::RebootSafetyStatus::default(),
            &syspilot_core::SentinelStatus::default(),
            &syspilot_core::GamingReadinessStatus::default(),
            "NVIDIA GeForce GTX 970",
            "2.1.6",
        );
        assert_eq!(vm_alert.pod00_accent_rgb, RowTone::Alert.rgb());

        ui.set_active_tab(0);
        ui.set_is_lcars_skin(true);
        apply_dashboard_view_model(&ui, &vm_anom);
        assert_eq!(ui.get_anomalies().row_count(), 2);
        assert_eq!(
            ui.get_anomalies().row_data(0).unwrap().label.as_str(),
            "PREVIOUS BOOT"
        );
        assert_eq!(
            ui.get_anomalies().row_data(1).unwrap().label.as_str(),
            "PACNEW DRIFT"
        );
        assert_eq!(ui.get_tab0_target_height(), 716);

        // Manual resize must NOT be clobbered on refresh with unchanged anomaly bounds
        ui.window().set_size(slint::LogicalSize::new(940.0, 740.0));
        apply_dashboard_view_model(&ui, &vm_anom);
        assert_eq!(ui.window().size().to_logical(1.0).width, 940.0);

        // Clearing anomalies (2 -> 0) transitions bounds and updates target height to 620
        let snap_clean = syspilot_core::SystemSnapshot {
            status: "ALL_CLEAR".into(),
            cache_stale: false,
            ..Default::default()
        };
        let vm_clean = DashboardViewModel::from_snapshot_inputs(
            &snap_clean,
            &syspilot_core::RebootSafetyStatus::default(),
            &syspilot_core::SentinelStatus::default(),
            &syspilot_core::GamingReadinessStatus::default(),
            "NVIDIA GeForce GTX 970",
            "2.1.6",
        );
        apply_dashboard_view_model(&ui, &vm_clean);
        assert_eq!(ui.get_anomalies().row_count(), 0);
        assert_eq!(ui.get_tab0_target_height(), 620);

        // Refresh while active_tab != 0 must NOT resize the window to Tab 0 dimensions
        ui.set_active_tab(1);
        ui.window().set_size(slint::LogicalSize::new(920.0, 740.0));
        apply_dashboard_view_model(&ui, &vm_anom);
        assert_eq!(ui.window().size().to_logical(1.0).height, 740.0);
        assert_eq!(ui.window().size().to_logical(1.0).width, 920.0);
        ui.set_active_tab(0);

        // Resize AppWindow down to min-width 920px and toggle is_lcars_skin (true and false)
        ui.window().set_size(slint::LogicalSize::new(920.0, 520.0));
        ui.set_is_lcars_skin(true);
        assert!(ui.get_is_lcars_skin());
        ui.set_is_lcars_skin(false);
        assert!(!ui.get_is_lcars_skin());

        update_ui_from_error(&ui, "stream disconnected");
        assert!(ui.get_has_error());
        assert!(!ui.get_has_attention());
        assert_eq!(ui.get_error_message().as_str(), "stream disconnected");
        assert_eq!(ui.get_overall_status().as_str(), "DATA_UNAVAILABLE");
        assert_eq!(ui.get_overall_status_label().as_str(), "DATA UNAVAILABLE");
        assert_eq!(ui.get_kernel_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_root_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_firewall_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_failed_services_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_gpu_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_cve_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_updates_core_text().as_str(), "OFFLINE");
        assert_eq!(ui.get_updates_repo_text().as_str(), "OFFLINE");
        assert_eq!(ui.get_updates_aur_text().as_str(), "OFFLINE");
        assert_eq!(ui.get_mirrors_status().as_str(), "OFFLINE");
        assert!(!ui.get_mirror_primary_dead());
        assert_eq!(ui.get_firmware_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_orphans_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_updates_standalone_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_standalone_updates_count(), 0);
        assert_eq!(ui.get_maintenance_item_count(), 9);

        // Standalone upgrade prompt modal bindings in both skins
        ui.set_is_lcars_skin(true);
        assert!(!ui.get_upgrade_standalone_prompt_visible());
        ui.set_upgrade_standalone_prompt_visible(true);
        assert!(ui.get_upgrade_standalone_prompt_visible());
        ui.set_upgrade_standalone_prompt_visible(false);
        assert!(!ui.get_upgrade_standalone_prompt_visible());

        ui.set_is_lcars_skin(false);
        assert!(!ui.get_upgrade_standalone_prompt_visible());
        ui.set_upgrade_standalone_prompt_visible(true);
        assert!(ui.get_upgrade_standalone_prompt_visible());
        ui.set_upgrade_standalone_prompt_visible(false);
        assert!(!ui.get_upgrade_standalone_prompt_visible());
    }

    #[test]
    fn test_format_custom_playbook_desc() {
        use std::path::PathBuf;
        use syspilot_core::UserPlaybook;

        // 1. empty &[]
        let empty_desc = format_custom_playbook_desc(&[]);
        assert_eq!(
            empty_desc,
            "Executes user tactical scripts discovered in ~/.config/syspilot/playbooks/*.sh"
        );

        // 2. [playbook without description]
        let pb_no_desc = vec![UserPlaybook {
            name: "quick_fix".into(),
            filename: "quick_fix.sh".into(),
            path: PathBuf::from("/test/quick_fix.sh"),
            description: "".into(),
        }];
        assert_eq!(
            format_custom_playbook_desc(&pb_no_desc),
            "Run playbook 'quick_fix' (1 script(s) available)"
        );

        // 3. [playbook with description]
        let pb_with_desc = vec![UserPlaybook {
            name: "proton_clean".into(),
            filename: "proton_clean.sh".into(),
            path: PathBuf::from("/test/proton_clean.sh"),
            description: "Purge stale proton prefix caches".into(),
        }];
        assert_eq!(
            format_custom_playbook_desc(&pb_with_desc),
            "proton_clean: Purge stale proton prefix caches"
        );
    }

    #[test]
    fn test_handle_playbook_click_all_routes() {
        let terminal_calls = std::cell::RefCell::new(Vec::new());
        let shell_calls = std::cell::RefCell::new(Vec::new());
        let custom_calls = std::cell::Cell::new(0);

        let mut record_terminal =
            |args: &'static [&'static str], title: &'static str, post_triage: bool| {
                terminal_calls.borrow_mut().push((args, title, post_triage));
            };
        let mut record_shell = |cmd: &'static str, title: &'static str| {
            shell_calls.borrow_mut().push((cmd, title));
        };
        let mut record_custom = || {
            custom_calls.set(custom_calls.get() + 1);
        };

        // 1. faf
        handle_playbook_click(
            "faf",
            &mut record_terminal,
            &mut record_shell,
            &mut record_custom,
        );
        assert_eq!(
            terminal_calls.borrow_mut().pop(),
            Some((&["--faf-repair"][..], "SysPilot FAF Runner Repair", false))
        );

        // 2. proton
        handle_playbook_click(
            "proton",
            &mut record_terminal,
            &mut record_shell,
            &mut record_custom,
        );
        assert_eq!(
            terminal_calls.borrow_mut().pop(),
            Some((
                &["--gaming"][..],
                "SysPilot Gaming & Proton Readiness",
                true
            ))
        );

        // 3. pacnew
        handle_playbook_click(
            "pacnew",
            &mut record_terminal,
            &mut record_shell,
            &mut record_custom,
        );
        let (cmd, title) = shell_calls.borrow_mut().pop().unwrap();
        assert_eq!(title, "SysPilot .pacnew Reviewer");
        assert!(cmd.contains("sudo -E pacdiff"));

        // 4. audio
        handle_playbook_click(
            "audio",
            &mut record_terminal,
            &mut record_shell,
            &mut record_custom,
        );
        let (cmd, title) = shell_calls.borrow_mut().pop().unwrap();
        assert_eq!(title, "SysPilot Audio Stack Reset");
        assert!(cmd.contains("systemctl --user restart pipewire"));

        // 5. custom
        handle_playbook_click(
            "custom",
            &mut record_terminal,
            &mut record_shell,
            &mut record_custom,
        );
        assert_eq!(custom_calls.get(), 1);

        // 6. audit
        handle_playbook_click(
            "audit",
            &mut record_terminal,
            &mut record_shell,
            &mut record_custom,
        );
        assert_eq!(
            terminal_calls.borrow_mut().pop(),
            Some((&["--audit"][..], "SysPilot Diagnostic Audit", true))
        );

        // 7. repair
        handle_playbook_click(
            "repair",
            &mut record_terminal,
            &mut record_shell,
            &mut record_custom,
        );
        assert_eq!(
            terminal_calls.borrow_mut().pop(),
            Some((&["--audit"][..], "SysPilot Diagnostic Audit", true))
        );

        // 8. unknown
        handle_playbook_click(
            "unknown",
            &mut record_terminal,
            &mut record_shell,
            &mut record_custom,
        );
        assert!(terminal_calls.borrow().is_empty());
        assert!(shell_calls.borrow().is_empty());
        assert_eq!(custom_calls.get(), 1);
    }

    #[test]
    fn test_dashboard_view_model_standalone_matrix() {
        let nominal_safety = syspilot_core::RebootSafetyStatus::default();
        let nominal_sentinels = syspilot_core::SentinelStatus::default();
        let nominal_gaming = syspilot_core::GamingReadinessStatus::default();

        // 1. Standalone count 0
        let snap_0 = syspilot_core::SystemSnapshot {
            standalone_updates: 0,
            standalone_tools: vec![],
            ..Default::default()
        };
        let vm_0 = DashboardViewModel::from_snapshot_inputs(
            &snap_0,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert_eq!(vm_0.updates_standalone.status, "PASS");
        assert_eq!(
            vm_0.updates_standalone.details,
            "ALL STANDALONE TOOLS UP TO DATE"
        );
        assert_eq!(vm_0.standalone_updates_count, 0);

        // 2. Standalone count 1
        let snap_1 = syspilot_core::SystemSnapshot {
            standalone_updates: 1,
            standalone_tools: vec!["goose".into()],
            ..Default::default()
        };
        let vm_1 = DashboardViewModel::from_snapshot_inputs(
            &snap_1,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert_eq!(vm_1.updates_standalone.status, "UPDATE");
        assert_eq!(vm_1.updates_standalone.details, "1 UPDATE PENDING (GOOSE)");
        assert_eq!(vm_1.standalone_updates_count, 1);

        // 3. Standalone count N (2)
        let snap_n = syspilot_core::SystemSnapshot {
            standalone_updates: 2,
            standalone_tools: vec!["goose".into(), "uv".into()],
            ..Default::default()
        };
        let vm_n = DashboardViewModel::from_snapshot_inputs(
            &snap_n,
            &nominal_safety,
            &nominal_sentinels,
            &nominal_gaming,
            "NVIDIA GeForce GTX 970",
            "2.1.4",
        );
        assert_eq!(vm_n.updates_standalone.status, "UPDATE");
        assert_eq!(
            vm_n.updates_standalone.details,
            "2 UPDATES PENDING (GOOSE, UV)"
        );
        assert_eq!(vm_n.standalone_updates_count, 2);

        // 4. from_error
        let vm_err = DashboardViewModel::from_error("connection reset", "2.1.4");
        assert_eq!(vm_err.updates_standalone.status, "OFFLINE");
        assert_eq!(vm_err.updates_standalone.details, "TELEMETRY UNAVAILABLE");
        assert_eq!(vm_err.standalone_updates_count, 0);
    }

    #[test]
    fn test_resolve_upgrade_click_action_permutations() {
        // Dead mirror always forces warning modal, regardless of standalone tools
        assert_eq!(
            resolve_upgrade_click_action(true, 0),
            UpgradeClickAction::ShowMirrorWarning
        );
        assert_eq!(
            resolve_upgrade_click_action(true, 3),
            UpgradeClickAction::ShowMirrorWarning
        );

        // Alive mirror with standalone updates prompts for upgrade scope
        assert_eq!(
            resolve_upgrade_click_action(false, 1),
            UpgradeClickAction::ShowStandalonePrompt
        );
        assert_eq!(
            resolve_upgrade_click_action(false, 5),
            UpgradeClickAction::ShowStandalonePrompt
        );

        // Alive mirror without standalone tools launches direct upgrade
        assert_eq!(
            resolve_upgrade_click_action(false, 0),
            UpgradeClickAction::LaunchDirectSystemUpgrade
        );
    }

    #[test]
    fn test_sh_single_quote_and_build_chained_upgrade_software_cmd() {
        use std::path::Path;

        assert_eq!(sh_single_quote("simple"), "'simple'");
        assert_eq!(sh_single_quote("with spaces"), "'with spaces'");
        assert_eq!(sh_single_quote("don't fail"), "'don'\\''t fail'");

        let bin_clean = Path::new("/usr/bin/syspilot"); // syspilot-lint:allow (unit test fixture path)
        assert_eq!(
            build_chained_upgrade_software_cmd(bin_clean),
            "'/usr/bin/syspilot' --upgrade && '/usr/bin/syspilot' --software"
        );

        let bin_tricky = Path::new("/opt/my tools/user's_syspilot");
        assert_eq!(
            build_chained_upgrade_software_cmd(bin_tricky),
            "'/opt/my tools/user'\\''s_syspilot' --upgrade && '/opt/my tools/user'\\''s_syspilot' --software"
        );
    }

    #[test]
    fn test_trigger_shell_task_with_post_triage_thread_local() {
        let ui_weak = slint::Weak::<AppWindow>::default();
        let cmd = "'/usr/bin/syspilot' --upgrade && '/usr/bin/syspilot' --software";
        let title = "SysPilot Full Upgrade";

        // Initialize recorder
        TEST_SHELL_TASK_RECORDER.with(|rec| *rec.borrow_mut() = Some(Vec::new()));

        // Case 1: TEST_CAN_SPAWN_OVERRIDE = Some(false) -> returns false and records nothing
        TEST_CAN_SPAWN_OVERRIDE.with(|cell| cell.set(Some(false)));
        let spawned = trigger_shell_task_with_post_triage(ui_weak.clone(), cmd, title);
        assert!(!spawned);
        let recorded = TEST_SHELL_TASK_RECORDER.with(|rec| rec.borrow().clone().unwrap());
        assert!(recorded.is_empty());

        // Case 2: TEST_CAN_SPAWN_OVERRIDE = Some(true) -> returns true and records command/title
        TEST_CAN_SPAWN_OVERRIDE.with(|cell| cell.set(Some(true)));
        let spawned2 = trigger_shell_task_with_post_triage(ui_weak, cmd, title);
        assert!(spawned2);
        let recorded2 = TEST_SHELL_TASK_RECORDER.with(|rec| rec.borrow().clone().unwrap());
        assert_eq!(recorded2, vec![(cmd.to_string(), title.to_string())]);

        // Reset both thread-locals to None before finishing
        TEST_CAN_SPAWN_OVERRIDE.with(|cell| cell.set(None));
        TEST_SHELL_TASK_RECORDER.with(|rec| *rec.borrow_mut() = None);
    }

    #[test]
    fn test_parse_gui_args_and_skin_helpers_matrix() {
        assert_eq!(
            parse_gui_args(["syspilot-gui"]).unwrap(),
            GuiLaunchArgs {
                smoke: false,
                skin: None,
            }
        );
        assert_eq!(
            parse_gui_args(["syspilot-gui", "--smoke", "--skin", "lcars"]).unwrap(),
            GuiLaunchArgs {
                smoke: true,
                skin: Some(SkinOverride::Lcars),
            }
        );
        assert_eq!(
            parse_gui_args(["syspilot-gui", "--skin=CLASSIC", "--smoke"]).unwrap(),
            GuiLaunchArgs {
                smoke: true,
                skin: Some(SkinOverride::Classic),
            }
        );
        assert!(parse_gui_args(["syspilot-gui", "--skin"]).is_err());
        assert!(parse_gui_args(["syspilot-gui", "--skin", "neon"]).is_err());
        assert!(parse_gui_args(["syspilot-gui", "--unknown-flag"]).is_err());

        // Smoke mode bypasses single-instance lock without calling closure
        assert!(!should_exit_due_to_lock(true, || unreachable!(
            "smoke mode must not acquire lock"
        )));
        assert!(!should_exit_due_to_lock(false, || true));
        assert!(should_exit_due_to_lock(false, || false));

        // Skin override precedence over persisted setting
        assert!(resolve_initial_skin(Some(SkinOverride::Lcars), false));
        assert!(!resolve_initial_skin(Some(SkinOverride::Classic), true));
        assert!(resolve_initial_skin(None, true));
        assert!(!resolve_initial_skin(None, false));
    }

    fn strip_slint_comments_and_strings(src: &str) -> String {
        let bytes = src.as_bytes();
        let mut out = String::with_capacity(src.len());
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if bytes[i] == b'\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            } else if bytes[i] == b'"' {
                out.push('"');
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        out.push('"');
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            } else {
                let ch = src[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out
    }

    fn extract_slint_text_blocks(cleaned: &str) -> Vec<(usize, String)> {
        let mut blocks = Vec::new();
        let bytes = cleaned.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if cleaned[i..].starts_with("Text") {
                let prev_ok =
                    i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_');
                let mut j = i + 4;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if prev_ok && j < bytes.len() && bytes[j] == b'{' {
                    let line = cleaned[..i].bytes().filter(|&b| b == b'\n').count() + 1;
                    let start = j + 1;
                    let mut depth = 1usize;
                    let mut k = start;
                    while k < bytes.len() && depth > 0 {
                        if bytes[k] == b'{' {
                            depth += 1;
                        } else if bytes[k] == b'}' {
                            depth -= 1;
                        }
                        k += 1;
                    }
                    assert_eq!(depth, 0, "unterminated Text block at line {line}");
                    blocks.push((line, cleaned[start..k - 1].to_string()));
                    i = k;
                    continue;
                }
            }
            i += 1;
        }
        blocks
    }

    fn has_centered_vertical_alignment(body: &str) -> bool {
        body.split(';').any(|stmt| {
            let mut parts = stmt.splitn(2, ':');
            let key = parts.next().unwrap_or("").trim();
            let val = parts.next().unwrap_or("").trim();
            key == "vertical-alignment" && val == "center"
        })
    }

    fn extract_height_prop(body: &str) -> Option<String> {
        let mut depth = 0usize;
        let mut start = 0usize;
        let bytes = body.as_bytes();
        for (idx, &b) in bytes.iter().enumerate() {
            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth = depth.saturating_sub(1);
            } else if b == b';' && depth == 0 {
                let stmt = body[start..idx].trim();
                if let Some((k, v)) = stmt.split_once(':')
                    && k.trim() == "height"
                {
                    return Some(v.trim().to_string());
                }
                start = idx + 1;
            }
        }
        None
    }

    #[test]
    fn test_slint_centered_single_line_text_has_even_height() {
        let ui_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let mut files = vec![ui_dir.join("appwindow.slint")];
        for sub in ["components", "patterns"] {
            let dir = ui_dir.join(sub);
            let mut entries: Vec<_> = std::fs::read_dir(&dir)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()))
                .filter_map(|res| res.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("slint"))
                .collect();
            entries.sort();
            files.extend(entries);
        }

        let mut checked_centered = 0usize;
        for path in &files {
            let raw = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let cleaned = strip_slint_comments_and_strings(&raw);
            for (line, body) in extract_slint_text_blocks(&cleaned) {
                if !has_centered_vertical_alignment(&body) {
                    continue;
                }
                checked_centered += 1;
                let h = extract_height_prop(&body).unwrap_or_else(|| {
                    panic!(
                        "{}:{line}: Text with `vertical-alignment: center` must declare explicit even height or `height: 100%` (FemtoVG 0.5px GL_NEAREST guard)",
                        path.display()
                    )
                });
                if h == "100%" {
                    continue;
                }
                let px = h
                    .strip_suffix("px")
                    .and_then(|n| n.trim().parse::<u32>().ok())
                    .unwrap_or_else(|| {
                        panic!(
                            "{}:{line}: centered Text height `{h}` must be `100%` or an even integer `px` literal",
                            path.display()
                        )
                    });
                assert!(
                    px > 0 && px.is_multiple_of(2),
                    "{}:{line}: centered Text height `{h}` must be an even integer > 0",
                    path.display()
                );
            }
        }
        assert!(
            checked_centered >= 20,
            "expected at least 20 centered Text elements across UI, saw {checked_centered}"
        );

        let pod_hdr = std::fs::read_to_string(ui_dir.join("components/lcars_pod_header.slint"))
            .expect("read lcars_pod_header.slint");
        assert!(
            pod_hdr.contains("height: 22px;"),
            "LcarsPodHeader Text must pin height: 22px"
        );
        let row_src = std::fs::read_to_string(ui_dir.join("components/lcars_telemetry_row.slint"))
            .expect("read lcars_telemetry_row.slint");
        let row_blocks = extract_slint_text_blocks(&strip_slint_comments_and_strings(&row_src));
        assert_eq!(row_blocks.len(), 3);
        for (line, body) in row_blocks {
            assert_eq!(
                extract_height_prop(&body).as_deref(),
                Some("100%"),
                "lcars_telemetry_row.slint:{line}: expected height: 100%"
            );
        }
    }

    fn collect_glyph_column_tops(
        buf: &slint::SharedPixelBuffer<slint::Rgb8Pixel>,
        x_range: std::ops::Range<usize>,
        y_range: std::ops::Range<usize>,
    ) -> Vec<usize> {
        let width = buf.width() as usize;
        let pixels = buf.as_slice();
        let mut tops = Vec::new();
        let mut cur_top: Option<usize> = None;
        let mut cur_bottom = 0usize;
        let mut cur_cols = 0usize;
        let mut blank_gap = 0usize;

        for x in x_range {
            let mut col_top: Option<usize> = None;
            let mut col_bottom = 0usize;
            for y in y_range.clone() {
                let p = pixels[y * width + x];
                if p.r > 24 || p.g > 24 || p.b > 24 {
                    col_top = Some(col_top.map_or(y, |t| t.min(y)));
                    col_bottom = col_bottom.max(y);
                }
            }
            match col_top {
                Some(t) => {
                    blank_gap = 0;
                    cur_cols += 1;
                    cur_top = Some(cur_top.map_or(t, |prev| prev.min(t)));
                    cur_bottom = cur_bottom.max(col_bottom);
                }
                None => {
                    if let Some(t) = cur_top {
                        let h = cur_bottom.saturating_sub(t) + 1;
                        if h >= 7 && cur_cols >= 2 {
                            tops.push(t);
                        }
                        cur_top = None;
                        cur_bottom = 0;
                        cur_cols = 0;
                    }
                    blank_gap += 1;
                    if blank_gap >= 6 && !tops.is_empty() {
                        break;
                    }
                }
            }
        }
        if let Some(t) = cur_top {
            let h = cur_bottom.saturating_sub(t) + 1;
            if h >= 7 && cur_cols >= 2 {
                tops.push(t);
            }
        }
        tops
    }

    #[test]
    fn test_headless_lcars_text_baseline_uniformity() {
        init_headless_platform();
        let ui = AppWindow::new().unwrap();
        let headless_window = HEADLESS_WINDOW
            .with(|cell| cell.borrow().clone())
            .expect("headless window initialized");

        let snap = syspilot_core::SystemSnapshot {
            os: "EndeavourOS".to_string(),
            kernel: "6.12.8-arch1-1".to_string(),
            status: "ALL_CLEAR".to_string(),
            firewall_active: true,
            cache_stale: false,
            updates_check_state: syspilot_core::UpdateCheckState::Ok,
            ..Default::default()
        };
        update_ui_from_snapshot(&ui, &snap);
        ui.set_anomalies(slint::ModelRc::new(slint::VecModel::from(vec![
            AnomalyRow {
                label: "TEST KERNEL".into(),
                value: "ALL SYSTEMS NOMINAL".into(),
                status: "PASS".into(),
                accent_color: slint::Color::from_rgb_u8(255, 170, 102),
                status_color: slint::Color::from_rgb_u8(34, 197, 94),
            },
        ])));
        headless_window.set_size(slint::PhysicalSize::new(920, 620));
        ui.set_is_lcars_skin(true);
        ui.set_active_tab(0);
        ui.show().unwrap();

        let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(920, 620);
        headless_window.request_redraw();
        let drawn = headless_window.draw_if_needed(|renderer| {
            let stride = buf.width() as usize;
            renderer.render(buf.make_mut_slice(), stride);
        });
        assert!(drawn);

        let header_tops = collect_glyph_column_tops(&buf, 198..440, 66..88);
        assert!(
            header_tops.len() >= 12,
            "expected >= 12 glyph clusters in POD 00 header, got {}: {:?}",
            header_tops.len(),
            header_tops
        );
        let hdr_min = *header_tops.iter().min().unwrap();
        let hdr_max = *header_tops.iter().max().unwrap();
        assert!(
            hdr_max - hdr_min <= 1,
            "POD 00 header cap-top spread too large ({hdr_min}..={hdr_max}): {header_tops:?}"
        );
        assert!(
            (68..=77).contains(&hdr_min) && (68..=77).contains(&hdr_max),
            "POD 00 header cap-top ({hdr_min}..={hdr_max}) not centered inside 16x16 cap (70..86)"
        );

        let row_tops = collect_glyph_column_tops(&buf, 390..620, 96..120);
        assert!(
            row_tops.len() >= 10,
            "expected >= 10 glyph clusters in LcarsTelemetryRow value, got {}: {:?}",
            row_tops.len(),
            row_tops
        );
        let row_min = *row_tops.iter().min().unwrap();
        let row_max = *row_tops.iter().max().unwrap();
        assert!(
            row_max - row_min <= 1,
            "LcarsTelemetryRow cap-top spread too large ({row_min}..={row_max}): {row_tops:?}"
        );
    }

    #[test]
    fn test_pre_boot_stale_summary_cleared_in_gui_pod00() {
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        init_headless_platform();
        let ui = AppWindow::new().unwrap();

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("syspilot-gui-preboot-{nanos}"));
        let state_dir = root.join("state");
        let health_dir = state_dir.join("system-health");
        std::fs::create_dir_all(&health_dir).unwrap();
        std::fs::create_dir_all(root.join("proc/sys/kernel")).unwrap();
        std::fs::create_dir_all(root.join("etc")).unwrap();

        let btime_secs: u64 = 1_700_000_000;
        std::fs::write(root.join("proc/stat"), format!("btime {btime_secs}\n")).unwrap();
        std::fs::write(root.join("proc/sys/kernel/osrelease"), "7.3.5-zen1-1-zen\n").unwrap();
        std::fs::write(root.join("etc/os-release"), "PRETTY_NAME=\"EndeavourOS\"\n").unwrap();

        let summary_path = health_dir.join("summary.json");
        std::fs::write(
            &summary_path,
            r#"{
                "counts": { "errors": 0, "warnings": 1 },
                "environment": { "os": "OldOS", "kernel": "6.1.0-old" },
                "actionable_remediations": [
                    { "check": "previous_boot", "severity": "WARN", "code": "SYS_UNCLEAN_SHUTDOWN", "summary": "Previous session crashed" }
                ]
            }"#,
        )
        .unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&summary_path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(btime_secs - 30))
            .unwrap();

        let health = syspilot_core::load_sys_health_at(&state_dir, &root);
        let mut status = "FLIGHT_READY".to_string();
        let mut reasons = Vec::new();
        syspilot_core::apply_sys_health_gates(&mut status, &mut reasons, &health);

        let status_path = root.join("status.json");
        let payload = syspilot_core::StatusJson {
            schema_version: "1.0".into(),
            status,
            status_reasons: reasons,
            informational_reasons: Vec::new(),
            timestamp: Some("1700000010".into()),
            gaming_mode: false,
            reboot_pending: false,
            updates: syspilot_core::StatusUpdates {
                check_state: syspilot_core::UpdateCheckState::Ok,
                ..Default::default()
            },
            failed_services: None,
            disk: None,
            pacnew: None,
            orphans: None,
            sys_health: Some(health),
            security: None,
            firmware: None,
            firewall: None,
            app_update: None,
            reboot_safety: None,
            gaming_readiness: None,
            mirrors: None,
        };
        syspilot_core::write_status_json_atomic(&status_path, &payload).unwrap();
        let snap = syspilot_core::read_system_snapshot(&status_path).unwrap();
        let _ = std::fs::remove_dir_all(&root);

        let vm = DashboardViewModel::from_snapshot_inputs(
            &snap,
            &syspilot_core::RebootSafetyStatus::default(),
            &syspilot_core::SentinelStatus::default(),
            &syspilot_core::GamingReadinessStatus::default(),
            "NVIDIA GeForce GTX 970",
            env!("CARGO_PKG_VERSION"),
        );
        assert_eq!(vm.overall_status, "ALL_CLEAR");
        assert!(!vm.has_attention);
        assert!(vm.anomalies.is_empty());

        for is_lcars in [true, false] {
            ui.set_is_lcars_skin(is_lcars);
            apply_dashboard_view_model(&ui, &vm);
            assert!(!ui.get_has_attention());
            assert_eq!(ui.get_anomalies().row_count(), 0);
        }
    }

    #[test]
    fn test_dashboard_view_model_gpu_pcie_degraded_and_nominal() {
        let snap = syspilot_core::SystemSnapshot {
            status: "FLIGHT_READY".into(),
            cache_stale: false,
            ..Default::default()
        };
        let gaming = syspilot_core::GamingReadinessStatus {
            applicable: true,
            ready: true,
            vulkan_64_ready: true,
            vulkan_32_ready: true,
            ..Default::default()
        };
        let degraded_safety = syspilot_core::RebootSafetyStatus {
            pcie_degraded: true,
            pcie_links: vec![syspilot_core::PcieLinkStatus {
                bdf: "0000:01:00.0".into(),
                current_width: Some(4),
                effective_max_width: Some(16),
                degraded: true,
                detail: Some("0000:01:00.0: PCIe link degraded x4 < x16".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let vm_deg = DashboardViewModel::from_snapshot_inputs(
            &snap,
            &degraded_safety,
            &syspilot_core::SentinelStatus::default(),
            &gaming,
            "NVIDIA GeForce GTX 970",
            env!("CARGO_PKG_VERSION"),
        );
        assert_eq!(vm_deg.gpu.status, "WARN");
        assert!(vm_deg.gpu.details.contains("PCIE LINK DEGRADED X4 < X16"));
        assert!(vm_deg.has_attention);
        assert!(vm_deg.attention_message.contains("x4 < x16"));

        let nominal_safety = syspilot_core::RebootSafetyStatus {
            pcie_degraded: false,
            pcie_links: vec![
                syspilot_core::PcieLinkStatus {
                    bdf: "0000:02:00.0".into(),
                    current_width: Some(1),
                    effective_max_width: Some(16),
                    runtime_suspended: true,
                    degraded: false,
                    ..Default::default()
                },
                syspilot_core::PcieLinkStatus {
                    bdf: "0000:01:00.0".into(),
                    current_width: Some(16),
                    effective_max_width: Some(16),
                    degraded: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let vm_nom = DashboardViewModel::from_snapshot_inputs(
            &snap,
            &nominal_safety,
            &syspilot_core::SentinelStatus::default(),
            &gaming,
            "NVIDIA GeForce GTX 970",
            env!("CARGO_PKG_VERSION"),
        );
        assert_eq!(vm_nom.gpu.status, "PASS");
        assert!(vm_nom.gpu.details.contains("PCIE X16"));
        assert!(!vm_nom.has_attention);
    }
}
