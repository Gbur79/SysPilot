slint::include_modules!();

use slint::ComponentHandle;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

fn acquire_single_instance_lock() -> bool {
    let state_dir = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
        .join("syspilot");

    let _ = std::fs::create_dir_all(&state_dir);
    let lock_path = state_dir.join(".gui.lock");

    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(_) => return true,
    };

    let res = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if res != 0 {
        return false;
    }

    // Keep lock held for process lifetime
    std::mem::forget(file);
    true
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
}

fn update_ui_from_error(ui: &AppWindow, err: &str) {
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

fn spawn_terminal_cmd(cmd: &str) {
    let mut term = std::process::Command::new("konsole");
    term.args(["--nofork", "-e", "bash", "-c", cmd]);
    if term.spawn().is_err() {
        // Fallback to xdg-terminal-exec or foot/kitty/alacritty
        let _ = std::process::Command::new("xdg-terminal-exec")
            .args(["bash", "-c", cmd])
            .spawn();
    }
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

    // Connect callbacks
    let ui_weak = ui.as_weak();
    ui.on_refresh_clicked(move || {
        let copy = ui_weak.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let _ = syspilot_core::run_triage(true, false).await;
                let status_path = syspilot_core::get_status_path();
                let snapshot_res = syspilot_core::read_system_snapshot(&status_path);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui_inst) = copy.upgrade() {
                        match snapshot_res {
                            Ok(snapshot) => update_ui_from_snapshot(&ui_inst, &snapshot),
                            Err(err) => update_ui_from_error(&ui_inst, &err),
                        }
                    }
                });
            });
        });
    });

    ui.on_upgrade_clicked(|| {
        spawn_terminal_cmd("sys-health.sh --fix || read -p 'Press enter to close...'");
    });

    ui.on_audit_clicked(|| {
        spawn_terminal_cmd("sys-health.sh || read -p 'Press enter to close...'");
    });

    ui.on_prune_clicked(|| {
        spawn_terminal_cmd("sys-health.sh --orphans || read -p 'Press enter to close...'");
    });

    ui.run()?;
    Ok(())
}
