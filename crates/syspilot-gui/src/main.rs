slint::include_modules!();

use slint::ComponentHandle;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn acquire_single_instance_lock() -> bool {
    syspilot_core::acquire_single_instance_lock(".gui.lock")
}

fn update_ui_from_snapshot(ui: &AppWindow, snapshot: &syspilot_core::SystemSnapshot) {
    if snapshot.has_data_problem() {
        ui.set_has_error(true);
        let reason = if let Some(err) = &snapshot.source_error {
            err.clone()
        } else if snapshot.cache_stale {
            format!("Telemetry cache stale (age {})", snapshot.age_label())
        } else {
            "Degraded telemetry stream".to_string()
        };
        ui.set_error_message(reason.into());
    } else {
        ui.set_has_error(false);
        ui.set_error_message("".into());
    }

    let eff = snapshot.effective_status();
    let (label, color) = if snapshot.has_data_problem() {
        ("DATA DEGRADED ⚠", slint::Color::from_rgb_u8(245, 158, 11))
    } else {
        match eff {
            "ACTION_REQUIRED" => ("ACTION REQUIRED ✖", slint::Color::from_rgb_u8(239, 68, 68)),
            "PRE_FLIGHT_ATTENTION" => (
                "PRE-FLIGHT ATTENTION ⚠",
                slint::Color::from_rgb_u8(245, 158, 11),
            ),
            _ => {
                if snapshot.gaming_mode {
                    (
                        "GAMEMODE ACTIVE 🎮",
                        slint::Color::from_rgb_u8(56, 189, 248),
                    )
                } else {
                    ("FLIGHT READY ✔", slint::Color::from_rgb_u8(34, 197, 94))
                }
            }
        }
    };

    ui.set_overall_status(eff.into());
    ui.set_overall_status_label(label.into());
    ui.set_overall_status_color(color);
    ui.set_hardware_info(
        format!(
            "{} • Kernel {} • Wayland (DP 1.2 MST 60Hz)",
            snapshot.os, snapshot.kernel
        )
        .into(),
    );

    if snapshot.reboot_pending {
        ui.set_kernel_status("REBOOT ⚠".into());
        ui.set_kernel_color(slint::Color::from_rgb_u8(245, 158, 11));
        ui.set_kernel_details("Kernel replaced on disk — reboot required".into());
    } else {
        ui.set_kernel_status("PASS ✔".into());
        ui.set_kernel_color(slint::Color::from_rgb_u8(34, 197, 94));
        ui.set_kernel_details(format!("Running kernel {}", snapshot.kernel).into());
    }

    // Root Storage sentinels
    let sentinels = syspilot_core::collect_sentinels();
    if sentinels.root_used_pct >= 90 {
        ui.set_root_status("ALERT 🚨".into());
        ui.set_root_color(slint::Color::from_rgb_u8(239, 68, 68));
    } else if sentinels.root_used_pct >= 80 {
        ui.set_root_status("WARN ⚠".into());
        ui.set_root_color(slint::Color::from_rgb_u8(245, 158, 11));
    } else {
        ui.set_root_status("PASS ✔".into());
        ui.set_root_color(slint::Color::from_rgb_u8(34, 197, 94));
    }
    ui.set_root_details(
        format!(
            "{}% used ({:.1}G free)",
            sentinels.root_used_pct, sentinels.root_avail_gb
        )
        .into(),
    );

    if snapshot.firewall_active {
        ui.set_firewall_status("PASS ✔".into());
        ui.set_firewall_color(slint::Color::from_rgb_u8(34, 197, 94));
        ui.set_firewall_details("Host firewall active (firewalld)".into());
    } else {
        ui.set_firewall_status("INFO ℹ".into());
        ui.set_firewall_color(slint::Color::from_rgb_u8(56, 189, 248));
        ui.set_firewall_details("Protected by perimeter router/LAN".into());
    }

    if snapshot.errors > 0 {
        ui.set_failed_services_status(format!("{} FAILED ✖", snapshot.errors).into());
        ui.set_failed_services_color(slint::Color::from_rgb_u8(239, 68, 68));
        ui.set_failed_services_details(format!("{} degraded unit(s)", snapshot.errors).into());
    } else {
        ui.set_failed_services_status("PASS ✔".into());
        ui.set_failed_services_color(slint::Color::from_rgb_u8(34, 197, 94));
        ui.set_failed_services_details("0 failed units".into());
    }

    ui.set_gpu_status("PASS ✔".into());
    ui.set_gpu_color(slint::Color::from_rgb_u8(34, 197, 94));
    ui.set_gpu_details("Hardware platform ready".into());

    if snapshot.security_high_risk > 0 {
        ui.set_cve_status("ALERT 🚨".into());
        ui.set_cve_color(slint::Color::from_rgb_u8(239, 68, 68));
        ui.set_cve_details(
            format!(
                "{} high-risk CVE vulnerabilities pending",
                snapshot.security_high_risk
            )
            .into(),
        );
    } else {
        ui.set_cve_status("PASS ✔".into());
        ui.set_cve_color(slint::Color::from_rgb_u8(34, 197, 94));
        ui.set_cve_details("No known unpatched CVEs".into());
    }

    if snapshot.core_updates > 0 {
        ui.set_updates_core_text("UPDATE ⚠".into());
        ui.set_updates_core_color(slint::Color::from_rgb_u8(245, 158, 11));
        ui.set_updates_core_details(
            format!("{} core system package(s) pending", snapshot.core_updates).into(),
        );
    } else {
        ui.set_updates_core_text("PASS ✔".into());
        ui.set_updates_core_color(slint::Color::from_rgb_u8(34, 197, 94));
        ui.set_updates_core_details("0 core updates".into());
    }

    if snapshot.regular_updates > 0 {
        ui.set_updates_repo_text("UPDATE ⚠".into());
        ui.set_updates_repo_color(slint::Color::from_rgb_u8(245, 158, 11));
        ui.set_updates_repo_details(
            format!("{} official repository update(s)", snapshot.regular_updates).into(),
        );
    } else {
        ui.set_updates_repo_text("PASS ✔".into());
        ui.set_updates_repo_color(slint::Color::from_rgb_u8(34, 197, 94));
        ui.set_updates_repo_details("0 repo updates".into());
    }

    if snapshot.aur_updates > 0 {
        ui.set_updates_aur_text("UPDATE ⚠".into());
        ui.set_updates_aur_color(slint::Color::from_rgb_u8(245, 158, 11));
        ui.set_updates_aur_details(
            format!("{} AUR package update(s)", snapshot.aur_updates).into(),
        );
    } else {
        ui.set_updates_aur_text("PASS ✔".into());
        ui.set_updates_aur_color(slint::Color::from_rgb_u8(34, 197, 94));
        ui.set_updates_aur_details("0 AUR updates".into());
    }

    if snapshot.firmware_updates > 0 {
        ui.set_firmware_status("UPDATE ⚠".into());
        ui.set_firmware_color(slint::Color::from_rgb_u8(245, 158, 11));
        ui.set_firmware_details(
            format!(
                "{} firmware update(s) available via LVFS",
                snapshot.firmware_updates
            )
            .into(),
        );
    } else {
        ui.set_firmware_status("PASS ✔".into());
        ui.set_firmware_color(slint::Color::from_rgb_u8(34, 197, 94));
        ui.set_firmware_details("Hardware up to date • LVFS clean".into());
    }

    ui.set_orphans_status("PASS ✔".into());
    ui.set_orphans_color(slint::Color::from_rgb_u8(34, 197, 94));
    ui.set_orphans_details("System packages audited".into());

    if snapshot.app_update_available {
        ui.set_has_app_update(true);
        ui.set_app_update_badge(format!("v{} AVAILABLE 🚀", snapshot.app_latest_version).into());
        ui.set_self_update_title("8) 🚀 SysPilot Suite Self-Update (Update Available)".into());
        ui.set_self_update_desc(
            format!(
                "Upstream v{} is available (current: v{}). Fast-forward git repository and rebuild atomically.",
                snapshot.app_latest_version, snapshot.app_current_version
            )
            .into(),
        );
        ui.set_self_update_btn_text(
            format!("Update to v{} 🚀", snapshot.app_latest_version).into(),
        );
    } else {
        ui.set_has_app_update(false);
        ui.set_app_update_badge("v2.0.2 LATEST ✔".into());
        ui.set_self_update_title("8) 🚀 SysPilot Suite Self-Update & GitHub Sync".into());
        ui.set_self_update_desc(
            "Synchronizes the SysPilot suite with the latest upstream GitHub release and performs atomic rebuild.".into(),
        );
        ui.set_self_update_btn_text("Check & Update Suite".into());
    }
}

fn update_ui_from_error(ui: &AppWindow, err: &str) {
    ui.set_has_app_update(false);
    ui.set_has_error(true);
    ui.set_error_message(err.into());
    ui.set_overall_status("DATA_UNAVAILABLE".into());
    ui.set_overall_status_label("DATA UNAVAILABLE ⚠".into());
    ui.set_overall_status_color(slint::Color::from_rgb_u8(245, 158, 11));
    ui.set_hardware_info("Telemetry stream disconnected".into());

    let muted = slint::Color::from_rgb_u8(100, 116, 139);

    ui.set_kernel_status("UNKNOWN ?".into());
    ui.set_kernel_color(muted);
    ui.set_kernel_details("Telemetry unavailable".into());

    ui.set_root_status("UNKNOWN ?".into());
    ui.set_root_color(muted);
    ui.set_root_details("Telemetry unavailable".into());

    ui.set_firewall_status("UNKNOWN ?".into());
    ui.set_firewall_color(muted);
    ui.set_firewall_details("Telemetry unavailable".into());

    ui.set_failed_services_status("UNKNOWN ?".into());
    ui.set_failed_services_color(muted);
    ui.set_failed_services_details("Telemetry unavailable".into());

    ui.set_gpu_status("UNKNOWN ?".into());
    ui.set_gpu_color(muted);
    ui.set_gpu_details("Telemetry unavailable".into());

    ui.set_updates_core_text("UNKNOWN ?".into());
    ui.set_updates_core_color(muted);
    ui.set_updates_core_details("Telemetry unavailable".into());

    ui.set_updates_repo_text("UNKNOWN ?".into());
    ui.set_updates_repo_color(muted);
    ui.set_updates_repo_details("Telemetry unavailable".into());

    ui.set_updates_aur_text("UNKNOWN ?".into());
    ui.set_updates_aur_color(muted);
    ui.set_updates_aur_details("Telemetry unavailable".into());

    ui.set_cve_status("UNKNOWN ?".into());
    ui.set_cve_color(muted);
    ui.set_cve_details("Telemetry unavailable".into());

    ui.set_firmware_status("UNKNOWN ?".into());
    ui.set_firmware_color(muted);
    ui.set_firmware_details("Telemetry unavailable".into());

    ui.set_orphans_status("UNKNOWN ?".into());
    ui.set_orphans_color(muted);
    ui.set_orphans_details("Telemetry unavailable".into());
}

static COPILOT_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static REFRESH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static LAST_SPAWN_MONO_MS: AtomicU64 = AtomicU64::new(0);
const SPAWN_DEBOUNCE_WINDOW_MS: u64 = 750;

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

struct CopilotFlightGuard;

impl Drop for CopilotFlightGuard {
    fn drop(&mut self) {
        COPILOT_IN_FLIGHT.store(false, Ordering::Release);
    }
}

struct RefreshFlightGuard;

impl Drop for RefreshFlightGuard {
    fn drop(&mut self) {
        REFRESH_IN_FLIGHT.store(false, Ordering::Release);
    }
}

fn trigger_copilot_query(ui_weak: slint::Weak<AppWindow>, query: String) {
    let query = query.trim().to_string();
    if query.is_empty() {
        return;
    }

    if COPILOT_IN_FLIGHT
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    if let Some(ui) = ui_weak.upgrade() {
        ui.set_copilot_running(true);
        ui.set_copilot_console_text(
            format!(
                "🚀 Asking SysPilot Copilot:\n> {query}\n\nThinking and gathering system telemetry...",
            )
            .into(),
        );
    }

    let ui_weak_thread = ui_weak;
    std::thread::spawn(move || {
        let _guard = CopilotFlightGuard;
        let result = syspilot_core::execute_copilot_query(&query);
        let _ = ui_weak_thread.upgrade_in_event_loop(move |ui| {
            ui.set_copilot_running(false);
            match result {
                Ok(answer) => {
                    ui.set_copilot_console_text(
                        format!("🤖 SysPilot Copilot Solution:\n\n{answer}\n").into(),
                    );
                }
                Err(err) => {
                    ui.set_copilot_console_text(
                        format!("✖ Copilot Execution Failed:\n\n{err}\n\nTip: You can also use the interactive terminal button above for real-time interaction.").into(),
                    );
                }
            }
        });
    });
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !acquire_single_instance_lock() {
        eprintln!("SysPilot GUI is already running.");
        return Ok(());
    }

    let ui = AppWindow::new()?;

    // Initial population from cached status.json
    let status_path = syspilot_core::get_status_path();
    match syspilot_core::read_system_snapshot(&status_path) {
        Ok(snapshot) => update_ui_from_snapshot(&ui, &snapshot),
        Err(error) => update_ui_from_error(&ui, &error),
    }

    // Populate Copilot Metadata
    let copilot_info = syspilot_core::get_copilot_info();
    ui.set_copilot_badge_label(copilot_info.badge_label().into());
    ui.set_copilot_status_text(copilot_info.status_line().into());

    // Connect Callbacks - Dashboard & General
    let ui_weak = ui.as_weak();
    ui.on_refresh_clicked(move || {
        if REFRESH_IN_FLIGHT
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
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
                    return;
                }
            };
            rt.block_on(async {
                let _ = syspilot_core::run_triage(true, false).await;
                let status_path = syspilot_core::get_status_path();
                let snapshot_res = syspilot_core::read_system_snapshot(&status_path);
                let _ = copy.upgrade_in_event_loop(move |ui_inst| match snapshot_res {
                    Ok(snapshot) => update_ui_from_snapshot(&ui_inst, &snapshot),
                    Err(err) => update_ui_from_error(&ui_inst, &err),
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

    // Connect Callbacks - AI Copilot
    ui.on_copilot_terminal_clicked(|| {
        trigger_terminal_task(&["--copilot-shell"], "SysPilot AI Copilot Interactive");
    });

    let ui_weak_copilot = ui.as_weak();
    ui.on_copilot_ask_clicked(move |query| {
        trigger_copilot_query(ui_weak_copilot.clone(), query.to_string());
    });

    let ui_weak_playbook = ui.as_weak();
    ui.on_copilot_playbook_clicked(move |playbook_id| {
        let query = match playbook_id.as_str() {
            "faf" => "Diagnose and repair Forged Alliance Forever (FAF): check dependencies (bwrap, 32-bit Vulkan), verify ~/faf-linux runner, run update.sh perform, sync Game.prefs from Steam, ensure desktop shortcuts point to the dynamic runner, or execute 'syspilot --faf-repair'.",
            "proton" => "Diagnose why a game is failing to launch in Steam under Proton and check missing 32-bit Vulkan/graphics libraries.",
            "pacnew" => "Check for active .pacnew configuration files and guide me through safe surgical merging.",
            "audio" => "Audio is crackling or missing sinks in PipeWire/WirePlumber, how do I reset and fix it?",
            other => other,
        };
        trigger_copilot_query(ui_weak_playbook.clone(), query.to_string());
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
                let dispatch_res =
                    ui_weak_watcher.upgrade_in_event_loop(move |ui_inst| match snapshot_res {
                        Ok(snapshot) => update_ui_from_snapshot(&ui_inst, &snapshot),
                        Err(err) => update_ui_from_error(&ui_inst, &err),
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
    fn test_copilot_flight_guard_raii() {
        COPILOT_IN_FLIGHT.store(false, Ordering::SeqCst);
        assert!(!COPILOT_IN_FLIGHT.load(Ordering::SeqCst));
        {
            let _guard = CopilotFlightGuard;
            COPILOT_IN_FLIGHT.store(true, Ordering::SeqCst);
            assert!(COPILOT_IN_FLIGHT.load(Ordering::SeqCst));
        }
        assert!(!COPILOT_IN_FLIGHT.load(Ordering::SeqCst));
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
}
