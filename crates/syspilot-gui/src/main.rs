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
    let eff = snapshot.effective_status();
    let (label, color) = match eff {
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
        ui.set_kernel_details("Kernel replaced on disk — reboot required".into());
    } else {
        ui.set_kernel_status("PASS ✔".into());
        ui.set_kernel_details(format!("Running kernel {}", snapshot.kernel).into());
    }

    if snapshot.firewall_active {
        ui.set_firewall_status("PASS ✔".into());
        ui.set_firewall_details("Host firewall active (firewalld)".into());
    } else {
        ui.set_firewall_status("INFO ℹ".into());
        ui.set_firewall_details("Protected by perimeter router/LAN".into());
    }

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
    if let Ok(snapshot) = syspilot_core::read_system_snapshot(&status_path) {
        update_ui_from_snapshot(&ui, &snapshot);
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
                if let Ok(snapshot) = syspilot_core::read_system_snapshot(&status_path) {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui_inst) = copy.upgrade() {
                            update_ui_from_snapshot(&ui_inst, &snapshot);
                        }
                    });
                }
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
