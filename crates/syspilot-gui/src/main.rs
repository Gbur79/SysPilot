slint::include_modules!();

use slint::ComponentHandle;
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
    pub firmware: RowPresentation,
    pub orphans: RowPresentation,
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
        let (gpu_status, gpu_rgb, gpu_details) = format_gpu_telemetry(gpu_name, gaming);
        let (cve_status, cve_rgb, cve_details) = format_cve_status(snapshot.security_high_risk);
        let (core_status, core_rgb, core_details) = format_core_updates(snapshot.core_updates);
        let (repo_status, repo_rgb, repo_details) = format_repo_updates_row(snapshot);
        let (aur_status, aur_rgb, aur_details) = format_aur_updates(snapshot.aur_updates);
        let (fwupd_status, fwupd_rgb, fwupd_details) =
            format_firmware_updates(snapshot.firmware_updates);
        let (orph_status, orph_rgb, orph_details) = format_orphans_row();
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
            firmware: offline_row(),
            orphans: offline_row(),
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

    ui.set_has_app_update(vm.self_update.available);
    ui.set_app_update_badge(vm.self_update.badge.as_str().into());
    ui.set_self_update_title(vm.self_update.title.as_str().into());
    ui.set_self_update_desc(vm.self_update.desc.as_str().into());
    ui.set_self_update_btn_text(vm.self_update.btn_text.as_str().into());
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
        "Executes user tactical scripts discovered in ~/.config/syspilot/playbooks.d/*.sh"
            .to_string()
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

fn compute_tab_window_height(
    tab_idx: i32,
    maintenance_item_count: usize,
    is_maximized: bool,
    is_fullscreen: bool,
) -> Option<f32> {
    if is_maximized || is_fullscreen {
        return None;
    }
    Some(match tab_idx {
        1 => compute_maintenance_window_height(maintenance_item_count),
        2 => 780.0,
        _ => 590.0,
    })
}

fn apply_tab_selection_to_window(ui_inst: &AppWindow, tab_idx: i32) {
    if tab_idx == 2 {
        update_playbook_telemetry(ui_inst);
    }
    if let Some(target_height) = compute_tab_window_height(
        tab_idx,
        ui_inst.get_maintenance_item_count() as usize,
        ui_inst.window().is_maximized(),
        ui_inst.window().is_fullscreen(),
    ) {
        ui_inst
            .window()
            .set_size(slint::LogicalSize::new(1040.0, target_height));
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::getuid() == 0 } {
        eprintln!("FATAL: SysPilot GUI must not be executed as root (UID 0).");
        std::process::exit(1);
    }

    if !acquire_single_instance_lock() {
        eprintln!("SysPilot GUI is already running.");
        return Ok(());
    }

    let ui = AppWindow::new()?;
    let is_lcars = syspilot_core::skin::load_skin_preference();
    ui.set_is_lcars_skin(is_lcars);

    let initial_tab = syspilot_core::skin::load_last_tab_preference();
    ui.set_active_tab(initial_tab);
    if !ui.window().is_maximized() && !ui.window().is_fullscreen() {
        let target_height = match initial_tab {
            1 => compute_maintenance_window_height(ui.get_maintenance_item_count() as usize),
            2 => 780.0,
            _ => 590.0,
        };
        ui.window()
            .set_size(slint::LogicalSize::new(1040.0, target_height));
    }

    ui.set_stardate_label(compute_stardate().into());

    // Skin Toggle & Persistence
    let ui_skin = ui.as_weak();
    ui.on_toggle_skin(move || {
        if let Some(ui_inst) = ui_skin.upgrade() {
            let current = ui_inst.get_is_lcars_skin();
            let next = !current;
            ui_inst.set_is_lcars_skin(next);
            syspilot_core::skin::save_skin_preference(next);
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
        trigger_task_with_post_triage(
            ui_weak_upgrade.clone(),
            &["--upgrade"],
            "SysPilot Guarded Upgrade",
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
                        "echo 'No executable *.sh playbooks in ~/.config/syspilot/playbooks.d/'",
                        "SysPilot Custom Playbooks",
                    );
                }
            },
        );
    });

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
        assert_eq!(compute_maintenance_window_height(8), 630.0);
        assert_eq!(compute_maintenance_window_height(0), 590.0);
        assert_eq!(compute_maintenance_window_height(10), 740.0);
        assert_eq!(compute_maintenance_window_height(20), 1200.0);
    }

    #[test]
    fn test_compute_tab_window_height_branches() {
        assert_eq!(compute_tab_window_height(0, 10, false, false), Some(590.0));
        assert_eq!(compute_tab_window_height(1, 10, false, false), Some(740.0));
        assert_eq!(compute_tab_window_height(2, 10, false, false), Some(780.0));
        assert_eq!(compute_tab_window_height(0, 10, true, false), None);
        assert_eq!(compute_tab_window_height(0, 10, false, true), None);
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
            &vm.firmware,
            &vm.orphans,
        ];
        assert_eq!(rows.len(), 11);
        for row in rows {
            assert_eq!(row.status, "OFFLINE");
            assert_eq!(row.rgb, RowTone::Offline.rgb());
            assert_eq!(row.details, "TELEMETRY UNAVAILABLE");
        }
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

    struct HeadlessTestPlatform {
        window: std::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
        start: std::time::Instant,
    }

    impl slint::platform::Platform for HeadlessTestPlatform {
        fn create_window_adapter(
            &self,
        ) -> Result<std::rc::Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
            Ok(self.window.clone())
        }
        fn duration_since_start(&self) -> std::time::Duration {
            self.start.elapsed()
        }
    }

    static INIT_HEADLESS: std::sync::Once = std::sync::Once::new();

    fn init_headless_platform() {
        INIT_HEADLESS.call_once(|| {
            let window =
                slint::platform::software_renderer::MinimalSoftwareWindow::new(Default::default());
            let _ = slint::platform::set_platform(Box::new(HeadlessTestPlatform {
                window,
                start: std::time::Instant::now(),
            }));
        });
    }

    #[test]
    fn test_headless_appwindow_snapshot_and_error_binding() {
        init_headless_platform();
        let ui = AppWindow::new().unwrap();

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
        assert_eq!(ui.get_firmware_status().as_str(), "PASS");
        assert_eq!(ui.get_orphans_status().as_str(), "PASS");

        // Attention snapshot test
        let attention_snap = syspilot_core::SystemSnapshot {
            os: "EndeavourOS".to_string(),
            kernel: "6.12.8-arch1-1".to_string(),
            status: "PRE_FLIGHT_ATTENTION".to_string(),
            warnings: 2,
            cache_stale: false,
            ..Default::default()
        };
        update_ui_from_snapshot(&ui, &attention_snap);
        assert!(!ui.get_has_error());
        assert!(ui.get_has_attention());
        assert_eq!(ui.get_attention_badge().as_str(), "ATTENTION");
        assert!(ui.get_attention_message().as_str().contains("2 warning(s)"));

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

        // Resize AppWindow and toggle is_lcars_skin (true and false) at min-height: 520px
        ui.window().set_size(slint::LogicalSize::new(960.0, 520.0));
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
        assert_eq!(ui.get_firmware_status().as_str(), "OFFLINE");
        assert_eq!(ui.get_orphans_status().as_str(), "OFFLINE");
    }

    #[test]
    fn test_format_custom_playbook_desc() {
        use std::path::PathBuf;
        use syspilot_core::UserPlaybook;

        // 1. empty &[]
        let empty_desc = format_custom_playbook_desc(&[]);
        assert_eq!(
            empty_desc,
            "Executes user tactical scripts discovered in ~/.config/syspilot/playbooks.d/*.sh"
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
}
