#![allow(clippy::disallowed_methods)]

use crate::firewall::parse_firewalld_conf_zone;
use crate::firmware::parse_fwupd_devices;
use crate::models::*;
use crate::orphans::{classify_orphans, glob_match, orphan_sensitive_tag};
use crate::security::{check_security_vulnerabilities, parse_arch_audit_json};
use crate::services::{is_candidate_app_unit, parse_systemctl_show_transient};
use crate::triage::{FlightStatusInputs, compute_flight_status, write_status_json_atomic};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_parse_arch_audit_json_valid() {
    let sample = r#"[
        {"name": "AVG-100", "packages": ["systemd", "systemd-libs"], "severity": "High"},
        {"name": "AVG-101", "packages": ["chromium"], "severity": "Critical"},
        {"name": "AVG-102", "packages": ["systemd"], "severity": "Medium"},
        {"name": "AVG-103", "packages": ["curl"], "severity": "Low"}
    ]"#;

    let (total, high_risk, pkgs) = parse_arch_audit_json(sample).unwrap();
    assert_eq!(total, 4);
    assert_eq!(high_risk, 2); // High + Critical
    assert_eq!(pkgs, vec!["chromium", "curl", "systemd", "systemd-libs"]);
}

#[test]
fn test_parse_arch_audit_json_empty_and_invalid() {
    let empty = "[]";
    let (total, high_risk, pkgs) = parse_arch_audit_json(empty).unwrap();
    assert_eq!(total, 0);
    assert_eq!(high_risk, 0);
    assert!(pkgs.is_empty());

    assert!(parse_arch_audit_json("not valid json").is_err());
}

#[tokio::test]
async fn test_security_check_skip_network() {
    let res = check_security_vulnerabilities(true).await;
    assert!(!res.available);
    assert_eq!(res.reason, "skipped");
    assert_eq!(res.vulnerabilities_total, 0);
}

#[test]
fn test_parse_fwupd_devices_nvram_quirk_bypassed_when_secure_boot_disabled() {
    use serde_json::json;

    // Both KEK CA with fresh pending state (UpdateState: 1, no error) and hardware device
    let raw = json!([
        {
            "Name": "KEK CA",
            "Plugin": "uefi_kek",
            "Protocol": "org.uefi.dbx",
            "UpdateState": 1
        },
        {
            "Name": "ADATA SX8200PNP SSD",
            "Plugin": "nvme",
            "Protocol": "org.nvmexpress",
            "UpdateState": 1
        }
    ]);

    let devices = serde_json::from_value(raw).unwrap();
    // Secure Boot is DISABLED (false) -> KEK CA must be proactively bypassed
    let (updates_count, total_monitored, pending, quirks) = parse_fwupd_devices(devices, false);

    assert_eq!(total_monitored, 2);
    assert_eq!(updates_count, 1);
    assert_eq!(pending, vec!["ADATA SX8200PNP SSD"]);
    assert_eq!(quirks.len(), 1);
    assert_eq!(quirks[0].name, "KEK CA");
    assert_eq!(quirks[0].plugin, "uefi_kek");
    assert_eq!(quirks[0].reason, "NVRAM write limit (Secure Boot disabled)");
}

#[test]
fn test_parse_fwupd_devices_nvram_quirk_not_bypassed_when_secure_boot_enabled() {
    use serde_json::json;

    let raw = json!([
        {
            "Name": "KEK CA",
            "Plugin": "uefi_kek",
            "Protocol": "org.uefi.dbx",
            "UpdateState": 1
        }
    ]);

    let devices = serde_json::from_value(raw).unwrap();
    // Secure Boot is ENABLED (true) -> must flag as real alert, not bypass
    let (updates_count, total_monitored, pending, quirks) = parse_fwupd_devices(devices, true);

    assert_eq!(total_monitored, 1);
    assert_eq!(updates_count, 1);
    assert_eq!(pending, vec!["KEK CA"]);
    assert_eq!(quirks.len(), 0);
}

#[test]
fn test_parse_firewalld_conf_zone() {
    let conf = r#"
# firewalld configuration
DefaultZone=trusted
CleanupOnExit=yes
"#;
    assert_eq!(parse_firewalld_conf_zone(conf), Some("trusted".into()));
    assert_eq!(parse_firewalld_conf_zone("DefaultZone=   \n"), None);
    assert_eq!(
        parse_firewalld_conf_zone("DefaultZone=   \nDefaultZone=work\n"),
        Some("work".into())
    );

    let conf_empty = "# only comments\n";
    assert_eq!(parse_firewalld_conf_zone(conf_empty), None);
}

#[test]
fn test_glob_match_and_sensitive_tag() {
    assert!(glob_match("linux*", "linux-zen"));
    assert!(glob_match("*-dkms", "nvidia-580xx-dkms"));
    assert!(glob_match("*firmware*", "linux-firmware"));
    assert!(glob_match("base-devel", "base-devel"));
    assert!(!glob_match("base-devel", "base-devel-fake"));

    assert_eq!(
        orphan_sensitive_tag("linux-zen"),
        Some("kernel / bootloader")
    );
    assert_eq!(
        orphan_sensitive_tag("nvidia-580xx-dkms"),
        Some("driver / firmware / graphics")
    );
    assert_eq!(
        orphan_sensitive_tag("pipewire-pulse"),
        Some("audio subsystem")
    );
    assert_eq!(
        orphan_sensitive_tag("lib32-vulkan-icd-loader"),
        Some("multilib / 32-bit runtime")
    );
    assert_eq!(orphan_sensitive_tag("unrelated-app"), None);
}

#[test]
fn test_classify_orphans_tiers() {
    let strict = Ok(vec!["unrelated-tool".into(), "kwin-x11".into()]);
    let extended = Ok(vec![
        "unrelated-tool".into(),
        "kwin-x11".into(),
        "luit".into(),
    ]);

    let orphans = classify_orphans(strict, extended);
    assert!(orphans.checked);
    assert_eq!(orphans.count, 2); // 2 strict
    assert_eq!(orphans.optional_count, 1); // luit is optional
    assert_eq!(orphans.tier1_packages, vec!["unrelated-tool"]);
    assert_eq!(orphans.tier2_packages, vec!["luit"]);
    assert_eq!(orphans.tier3_packages, vec!["kwin-x11"]);
    assert_eq!(
        orphans.tier3_tags.get("kwin-x11"),
        Some(&"desktop / compositor / display manager".to_string())
    );
}

#[test]
fn test_services_transient_filtering() {
    assert!(is_candidate_app_unit("app-java@12345.service"));
    assert!(is_candidate_app_unit("app-org.kde.dolphin@run.scope"));
    assert!(!is_candidate_app_unit("systemd-journald.service"));

    let show_output = r#"
Id=app-java@12345.service
Transient=yes
Slice=app.slice

Id=app-backup.service
Transient=no
Slice=app.slice
"#;
    let mut candidates = HashSet::new();
    candidates.insert("app-java@12345.service".into());
    candidates.insert("app-backup.service".into());

    let ignored = parse_systemctl_show_transient(show_output, &candidates);
    assert!(ignored.contains("app-java@12345.service"));
    assert!(!ignored.contains("app-backup.service")); // Persistent service kept!
}

#[test]
fn test_compute_flight_status_priorities() {
    let empty_failed = FailedServicesStatus::default();
    let disk_ok = DiskStatus {
        root: Some(DiskUsage {
            total_gb: 100.0,
            used_gb: 50.0,
            free_gb: 50.0,
            used_pct: 50,
        }),
        home: None,
    };
    let orphans_clean = OrphanStatus {
        checked: true,
        ..Default::default()
    };
    let updates_clean = StatusUpdates::default();
    let sec_clean = StatusSecurity::default();
    let fw_clean = StatusFirmware::default();
    let net_clean = StatusFirewall::default();

    // 1. All clear -> FLIGHT_READY
    let (status, reasons, _) = compute_flight_status(FlightStatusInputs {
        failed: &empty_failed,
        disk: &disk_ok,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &orphans_clean,
        updates: &updates_clean,
        security: &sec_clean,
        firmware: &fw_clean,
        firewall: &net_clean,
    });
    assert_eq!(status, "FLIGHT_READY");
    assert!(reasons.is_empty());

    // 2. Core updates -> PRE_FLIGHT_ATTENTION
    let mut core_updates = updates_clean.clone();
    core_updates.core_count = 1;
    let (status2, reasons2, _) = compute_flight_status(FlightStatusInputs {
        failed: &empty_failed,
        disk: &disk_ok,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &orphans_clean,
        updates: &core_updates,
        security: &sec_clean,
        firmware: &fw_clean,
        firewall: &net_clean,
    });
    assert_eq!(status2, "PRE_FLIGHT_ATTENTION");
    assert_eq!(reasons2, vec!["1 core system update(s) available"]);

    // 3. Failed system service -> ACTION_REQUIRED
    let mut sys_failed = empty_failed.clone();
    sys_failed.system.push("systemd-coredump.service".into());
    let (status3, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &sys_failed,
        disk: &disk_ok,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &orphans_clean,
        updates: &core_updates,
        security: &sec_clean,
        firmware: &fw_clean,
        firewall: &net_clean,
    });
    assert_eq!(status3, "ACTION_REQUIRED");
}

#[test]
fn test_write_and_read_status_json_atomic() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_file = std::env::temp_dir().join(format!("syspilot-test-{nanos}/status.json"));

    let payload = StatusJson {
        schema_version: "1.0".into(),
        status: "FLIGHT_READY".into(),
        status_reasons: Vec::new(),
        informational_reasons: Vec::new(),
        timestamp: Some("2026-10-30T17:00:00Z".into()),
        gaming_mode: false,
        reboot_pending: false,
        updates: StatusUpdates {
            total: 2,
            core_count: 1,
            regular_count: 1,
            aur_count: 0,
        },
        failed_services: None,
        disk: None,
        pacnew: None,
        orphans: None,
        sys_health: None,
        security: None,
        firmware: None,
        firewall: None,
        app_update: None,
        reboot_safety: None,
        gaming_readiness: None,
    };

    write_status_json_atomic(&tmp_file, &payload).unwrap();
    assert!(tmp_file.is_file());

    let snapshot = read_system_snapshot(&tmp_file).unwrap();
    assert_eq!(snapshot.status, "FLIGHT_READY");
    assert_eq!(snapshot.updates_total, 2);
    assert_eq!(snapshot.core_updates, 1);

    let _ = std::fs::remove_dir_all(tmp_file.parent().unwrap());
}

#[test]
fn test_deserialize_schema_1_0_disk_compatibility() {
    let raw_schema_1_0 = r#"{
        "schema_version": "1.0",
        "status": "FLIGHT_READY",
        "disk": {
            "root": {
                "path": "/",
                "total_gb": 457.7,
                "avail_gb": 311.1,
                "used_pct": 29
            },
            "home": {
                "path": "/home",
                "total_gb": 457.7,
                "avail_gb": 311.1,
                "used_pct": 29
            }
        },
        "updates": {
            "total": 0,
            "core_count": 0,
            "regular_count": 0,
            "aur_count": 0
        }
    }"#;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_file = std::env::temp_dir().join(format!("syspilot-compat-{nanos}/status.json"));
    std::fs::create_dir_all(tmp_file.parent().unwrap()).unwrap();
    std::fs::write(&tmp_file, raw_schema_1_0).unwrap();

    let snapshot =
        read_system_snapshot(&tmp_file).expect("Must parse Schema 1.0 status JSON with avail_gb");
    assert_eq!(snapshot.status, "FLIGHT_READY");
    assert_eq!(snapshot.updates_total, 0);

    let parsed: StatusJson = serde_json::from_str(raw_schema_1_0).unwrap();
    let root = parsed.disk.unwrap().root.unwrap();
    assert_eq!(root.total_gb, 457.7);
    assert_eq!(root.free_gb, 311.1); // mapped from avail_gb
    assert_eq!(root.used_gb, 0.0); // defaulted
    assert_eq!(root.used_pct, 29);

    let _ = std::fs::remove_dir_all(tmp_file.parent().unwrap());
}

#[test]
fn test_terminal_utilities() {
    use crate::terminal::{find_program, is_executable};
    use std::path::Path;

    assert!(is_executable(Path::new("/usr/bin/bash")));
    assert!(!is_executable(Path::new("/nonexistent/binary/xyz")));

    let bash = find_program("bash");
    assert!(bash.is_some());
    assert!(bash.unwrap().ends_with("bash"));

    let nonexistent = find_program("syspilot_nonexistent_command_xyz");
    assert!(nonexistent.is_none());
}

#[test]
fn test_copilot_info_parser() {
    use crate::copilot::{execute_copilot_query, get_copilot_info};

    let info = get_copilot_info();
    // In Karol's rig or test rig, info returns structured data
    let badge = info.badge_label();
    assert!(badge.starts_with("🤖"));

    let err = execute_copilot_query("   ");
    assert!(err.is_err());
    assert_eq!(err.unwrap_err(), "Query cannot be empty.");
}

#[test]
fn test_terminal_custom_fallback() {
    use crate::terminal::build_terminal;
    use std::path::Path;

    // Must not panic on unknown / custom terminals
    let cmd = build_terminal(
        "wezterm",
        Path::new("/usr/bin/wezterm"),
        Path::new("/usr/bin/true"),
        &["--test"],
        "Test Title",
    );
    assert_eq!(cmd.get_program(), "/usr/bin/wezterm");

    let cmd_path = build_terminal(
        "/usr/local/bin/urxvt",
        Path::new("/usr/local/bin/urxvt"),
        Path::new("/usr/bin/true"),
        &[],
        "Urxvt Title",
    );
    assert_eq!(cmd_path.get_program(), "/usr/local/bin/urxvt");
}

#[test]
fn test_goose_recipe_resolution() {
    use crate::copilot::{get_compact_health, resolve_goose_recipe};
    use std::path::PathBuf;

    let health = get_compact_health();
    assert!(health.contains("status=") || health == "cache=unavailable");

    // Test with explicit SYSPILOT_GOOSE_RECIPE
    let dummy_recipe = std::env::temp_dir().join("syspilot-test-recipe.yaml");
    std::fs::write(&dummy_recipe, "# test recipe").unwrap();

    unsafe {
        std::env::set_var("SYSPILOT_GOOSE_RECIPE", &dummy_recipe);
    }
    let resolved = resolve_goose_recipe();
    assert_eq!(resolved, Some(PathBuf::from(&dummy_recipe)));

    unsafe {
        std::env::remove_var("SYSPILOT_GOOSE_RECIPE");
    }
    let _ = std::fs::remove_file(dummy_recipe);
}

#[test]
fn test_agy_copilot_detection() {
    let _guard = TERMINAL_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::copilot::get_copilot_info;

    unsafe {
        std::env::set_var("SYSPILOT_COPILOT_CMD", "agy --agent eos-admin");
        std::env::set_var("COPILOT_LABEL", "Antigravity (eos-admin)");
    }

    let info = get_copilot_info();
    assert_eq!(info.provider, "Antigravity CLI");
    assert_eq!(info.model, "Antigravity (eos-admin)");
    assert_eq!(
        info.badge_label(),
        "🤖 Antigravity (eos-admin) (Antigravity CLI)"
    );

    unsafe {
        std::env::remove_var("SYSPILOT_COPILOT_CMD");
        std::env::remove_var("COPILOT_LABEL");
    }
}

#[test]
fn test_agy_command_identification() {
    use crate::copilot::is_agy_command;

    assert!(is_agy_command("agy --agent eos-admin"));
    assert!(is_agy_command("/usr/bin/agy -i"));
    assert!(is_agy_command("/home/user/.local/bin/agy"));
    assert!(is_agy_command("env FOO=1 agy -i"));
    assert!(!is_agy_command("bagy --agent foo"));
    assert!(!is_agy_command("pedagogy --mode learn"));
    assert!(!is_agy_command("strategy-tool --run"));
    assert!(!is_agy_command("goose run --text"));
}

#[test]
fn test_agy_flag_adaptation_hygiene() {
    use crate::copilot::adapt_agy_cmd_noninteractive;

    // 1. Standalone -i replaced with -p
    assert_eq!(
        adapt_agy_cmd_noninteractive("agy --agent eos-admin -i"),
        "agy --agent eos-admin -p"
    );

    // 2. Standalone --prompt-interactive replaced with -p
    assert_eq!(
        adapt_agy_cmd_noninteractive("agy --prompt-interactive"),
        "agy -p"
    );

    // 3. Substring with '-i' in flag/agent name NOT corrupted
    assert_eq!(
        adapt_agy_cmd_noninteractive("agy --agent sys-info"),
        "agy --agent sys-info -p"
    );
    assert_eq!(
        adapt_agy_cmd_noninteractive("agy --interactive"),
        "agy --interactive -p"
    );
    assert_eq!(
        adapt_agy_cmd_noninteractive("agy --stdin"),
        "agy --stdin -p"
    );

    // 4. Substring with '-p' in long flags NOT treated as -p print mode
    assert_eq!(
        adapt_agy_cmd_noninteractive("agy --project eos"),
        "agy --project eos -p"
    );
    assert_eq!(
        adapt_agy_cmd_noninteractive("agy --profile dev"),
        "agy --profile dev -p"
    );
    assert_eq!(
        adapt_agy_cmd_noninteractive("agy --plugins /opt/plugins"),
        "agy --plugins /opt/plugins -p"
    );

    // 5. Existing print flags preserved untouched
    assert_eq!(adapt_agy_cmd_noninteractive("agy -p"), "agy -p");
    assert_eq!(adapt_agy_cmd_noninteractive("agy --print"), "agy --print");
    assert_eq!(adapt_agy_cmd_noninteractive("agy --prompt"), "agy --prompt");
}

#[test]
fn test_dynamic_kernel_and_os_fallback() {
    let kernel = detect_kernel();
    assert!(!kernel.is_empty());

    let os = detect_os();
    assert!(!os.is_empty());

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_file = std::env::temp_dir().join(format!("syspilot-fallback-{nanos}/status.json"));
    std::fs::create_dir_all(tmp_file.parent().unwrap()).unwrap();

    let payload = StatusJson {
        schema_version: "1.0".into(),
        status: "ALL_CLEAR".into(),
        status_reasons: vec![],
        informational_reasons: vec![],
        timestamp: Some("1700000000".into()),
        gaming_mode: false,
        reboot_pending: false,
        updates: StatusUpdates::default(),
        failed_services: None,
        disk: None,
        pacnew: None,
        orphans: None,
        sys_health: None,
        security: None,
        firmware: None,
        firewall: None,
        app_update: None,
        reboot_safety: None,
        gaming_readiness: None,
    };
    write_status_json_atomic(&tmp_file, &payload).unwrap();

    let snapshot = read_system_snapshot(&tmp_file).unwrap();
    assert_eq!(snapshot.kernel, kernel);
    assert_eq!(snapshot.os, os);

    let _ = std::fs::remove_dir_all(tmp_file.parent().unwrap());
}

static TERMINAL_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_terminal_slot_bounds_and_underflow_immunity() {
    let _guard = TERMINAL_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::terminal::{active_terminals_count, release_slot, reserve_slot};

    // Drain any leftover active terminal slots from earlier runs
    while active_terminals_count() > 0 {
        release_slot();
    }

    // Verify underflow immunity: saturating subtraction must stay at 0
    assert_eq!(active_terminals_count(), 0);
    release_slot();
    release_slot();
    assert_eq!(active_terminals_count(), 0);

    // Reserve slots up to MAX_TERMINALS (8)
    for _ in 0..8 {
        assert!(reserve_slot(), "Reservation within limit must succeed");
    }
    assert_eq!(active_terminals_count(), 8);

    // 9th reservation must be rejected
    assert!(
        !reserve_slot(),
        "Reservation beyond MAX_TERMINALS must fail"
    );
    assert_eq!(active_terminals_count(), 8);

    // Release all 8 slots
    for _ in 0..8 {
        release_slot();
    }
    assert_eq!(active_terminals_count(), 0);

    // Underflow immunity check again
    release_slot();
    assert_eq!(active_terminals_count(), 0);
}

#[test]
fn test_detached_gui_reaper_isolation() {
    let _guard = TERMINAL_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::terminal::{
        TrackedProcess, active_terminals_count, reaper, release_slot, reserve_slot,
    };
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    // Reset slot counter
    while active_terminals_count() > 0 {
        release_slot();
    }

    // Reserve 1 slot to verify it is NOT decremented by detached processes
    assert!(reserve_slot());
    assert_eq!(active_terminals_count(), 1);

    // Spawn a detached process (using /usr/bin/true)
    let detached_child = Command::new("/usr/bin/true")
        .spawn()
        .expect("Failed to spawn detached process");

    reaper()
        .try_send(TrackedProcess::Detached(detached_child))
        .expect("Failed to send detached child to reaper");

    // Allow reaper to reap the exited child
    thread::sleep(Duration::from_millis(300));

    // Active terminal slots must remain 1
    assert_eq!(
        active_terminals_count(),
        1,
        "Detached GUI process must not release terminal slots"
    );

    // Now spawn a terminal process (using /usr/bin/true)
    let terminal_child = Command::new("/usr/bin/true")
        .spawn()
        .expect("Failed to spawn terminal process");

    reaper()
        .try_send(TrackedProcess::Terminal(terminal_child))
        .expect("Failed to send terminal child to reaper");

    // Poll until reaper reaps the child and releases the slot (up to 2 seconds)
    let start = std::time::Instant::now();
    while active_terminals_count() != 0 && start.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(50));
    }

    // Active terminal slots must now be decremented to 0
    assert_eq!(
        active_terminals_count(),
        0,
        "Terminal process exit must release terminal slot"
    );
}

#[test]
fn test_terminal_callback_executed_on_exit() {
    let _guard = TERMINAL_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::terminal::{
        TrackedProcess, active_terminals_count, reaper, release_slot, reserve_slot,
    };
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    // Reset slots to clean state
    while active_terminals_count() > 0 {
        release_slot();
    }

    assert!(reserve_slot());
    assert_eq!(active_terminals_count(), 1);

    let callback_called = Arc::new(AtomicBool::new(false));
    let callback_called_clone = Arc::clone(&callback_called);

    let child = Command::new("/usr/bin/true")
        .spawn()
        .expect("Failed to spawn process");

    let callback = Box::new(move || {
        callback_called_clone.store(true, Ordering::SeqCst);
    });

    reaper()
        .try_send(TrackedProcess::TerminalWithCallback(child, Some(callback)))
        .expect("Failed to send terminal process with callback to reaper");

    let start = std::time::Instant::now();
    while !callback_called.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(50));
    }

    assert!(
        callback_called.load(Ordering::SeqCst),
        "Callback must be executed when process exits"
    );
    assert_eq!(
        active_terminals_count(),
        0,
        "Terminal slot must be released when process with callback exits"
    );
}

#[test]
fn test_killed_child_reaped_and_slot_freed() {
    let _guard = TERMINAL_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::terminal::{
        TrackedProcess, active_terminals_count, reaper, release_slot, reserve_slot,
    };
    use std::path::Path;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    // Drain any leftover active terminal slots from earlier tests
    while active_terminals_count() > 0 {
        release_slot();
    }

    // Reserve 1 terminal slot
    assert!(reserve_slot(), "Reservation must succeed");
    assert_eq!(active_terminals_count(), 1);

    // Spawn a long-lived process to simulate sudden external kill
    let child = Command::new("/usr/bin/sleep")
        .arg("60")
        .spawn()
        .expect("Failed to spawn child process");
    let pid = child.id() as libc::pid_t;

    // Register child with the background reaper thread
    reaper()
        .try_send(TrackedProcess::Terminal(child))
        .expect("Failed to dispatch process to reaper");

    // Process should initially exist in the OS process table
    assert!(
        Path::new(&format!("/proc/{pid}")).exists(),
        "Process /proc/{pid} must exist after spawn"
    );

    // Simulate abrupt external termination (SIGKILL)
    unsafe {
        let rc = libc::kill(pid, libc::SIGKILL);
        assert_eq!(rc, 0, "SIGKILL to child process must succeed");
    }

    // Poll until reaper detects exit, reaps the child, and releases the slot (up to 3s)
    let start = std::time::Instant::now();
    while (active_terminals_count() != 0 || Path::new(&format!("/proc/{pid}")).exists())
        && start.elapsed() < Duration::from_secs(3)
    {
        thread::sleep(Duration::from_millis(50));
    }

    // Assert that the active terminal slot was freed back to 0
    assert_eq!(
        active_terminals_count(),
        0,
        "Active terminal slot must be restored to 0 by reaper"
    );

    // Assert that the process has been reaped and does not remain in zombie state
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "Killed child process must be reaped from /proc without leaving zombie state"
    );
}

#[test]
fn test_copilot_timeout_enforcement() {
    let _guard = TERMINAL_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::copilot::execute_copilot_query_with_timeout;
    use std::time::Duration;

    unsafe {
        std::env::set_var("SYSPILOT_COPILOT_CMD", "bash -c 'sleep 5' --");
        std::env::set_var("COPILOT_LABEL", "Test Sleep");
    }

    let start = std::time::Instant::now();
    let res = execute_copilot_query_with_timeout("test query", Duration::from_millis(150));
    let elapsed = start.elapsed();

    unsafe {
        std::env::remove_var("SYSPILOT_COPILOT_CMD");
        std::env::remove_var("COPILOT_LABEL");
    }

    assert!(res.is_err(), "Query must fail due to timeout");
    let err = res.unwrap_err();
    assert!(
        err.contains("timed out"),
        "Error message must indicate timeout: {err}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "Timeout must abort fast (elapsed: {elapsed:?})"
    );
}

#[tokio::test]
async fn test_subprocess_timeout_enforcement_and_kill_on_drop() {
    use crate::updater::run_command_with_timeout;
    use std::time::Duration;
    use tokio::process::Command;

    let start = std::time::Instant::now();
    let mut cmd = Command::new("/usr/bin/sleep");
    cmd.arg("10");

    let res = run_command_with_timeout(cmd, Duration::from_millis(150)).await;
    let elapsed = start.elapsed();

    assert!(res.is_err(), "Command must fail due to timeout");
    let err = res.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    assert!(
        elapsed < Duration::from_secs(2),
        "Timeout must abort fast (elapsed: {elapsed:?})"
    );
}

#[test]
fn test_updater_offline_mode_resilience() {
    let _guard = TERMINAL_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::updater::{check_aur_updates, check_official_updates, check_standalone_updates};

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        unsafe {
            std::env::set_var("SYSPILOT_BIN_CHECKUPDATES", "/nonexistent/checkupdates/bin");
            std::env::set_var("SYSPILOT_BIN_YAY", "/nonexistent/yay/bin");
            std::env::set_var("SYSPILOT_BIN_SYS_HEALTH", "/nonexistent/sys-health/bin");
        }

        let (core, regular, pkgs) = check_official_updates().await;
        assert_eq!(core, 0);
        assert_eq!(regular, 0);
        assert!(pkgs.is_empty());

        let aur = check_aur_updates().await;
        assert_eq!(aur, 0);

        let standalone = check_standalone_updates().await;
        assert_eq!(standalone, 0);

        unsafe {
            std::env::remove_var("SYSPILOT_BIN_CHECKUPDATES");
            std::env::remove_var("SYSPILOT_BIN_YAY");
            std::env::remove_var("SYSPILOT_BIN_SYS_HEALTH");
        }
    });
}

#[test]
fn test_pacman_lock_toctou_resilience() {
    use crate::sentinels::check_pacman_lock_at;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("syspilot-lock-test-{nanos}"));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let lock_file = tmp_dir.join("db.lck");

    // 1. No lockfile present -> (false, false)
    assert_eq!(check_pacman_lock_at(&lock_file), (false, false));

    // 2. Lockfile with dead PID (99999999) -> (true, true) (orphaned)
    std::fs::write(&lock_file, "99999999\n").unwrap();
    assert_eq!(check_pacman_lock_at(&lock_file), (true, true));

    // 3. Clean up lockfile -> (false, false)
    std::fs::remove_file(&lock_file).unwrap();
    assert_eq!(check_pacman_lock_at(&lock_file), (false, false));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_fwupd_nvram_quirk_insufficient_space() {
    use crate::firmware::{FwupdDevice, parse_fwupd_devices};

    let dev = FwupdDevice {
        name: Some("System Firmware".into()),
        plugin: Some("uefi_capsule".into()),
        protocol: Some("org.uefi.capsule".into()),
        protocols: None,
        update_state: Some(3),
        update_error: Some("Not enough NVRAM space to stage update".into()),
    };

    // Even with Secure Boot ACTIVE (true), NVRAM space limit must be recognized as a quirk
    let (updates, monitored, pending, quirks) = parse_fwupd_devices(vec![dev], true);
    assert_eq!(monitored, 1);
    assert_eq!(updates, 0);
    assert!(pending.is_empty());
    assert_eq!(quirks.len(), 1);
    assert_eq!(quirks[0].name, "System Firmware");
    assert_eq!(
        quirks[0].reason,
        "NVRAM write limit (insufficient NVRAM storage)"
    );
}

#[test]
fn test_clock_skew_resilience() {
    use crate::models::{STALE_AFTER_SECS, SystemSnapshot, is_stale};
    use std::time::{Duration, SystemTime};

    let now = SystemTime::now();

    // 1. Clock skew <= 60s in future (e.g. +10s) -> Not stale, age 0s
    let future_fresh = now + Duration::from_secs(10);
    assert!(!is_stale(Some(future_fresh)));

    let snap_fresh = SystemSnapshot {
        modified: Some(future_fresh),
        ..Default::default()
    };
    assert_eq!(snap_fresh.age_seconds(), Some(0));
    assert_eq!(snap_fresh.age_label(), "0s");

    // 2. Clock skew <= 60s boundary (+59s) -> Not stale
    let future_fresh_59 = now + Duration::from_secs(59);
    assert!(!is_stale(Some(future_fresh_59)));
    let snap_fresh_59 = SystemSnapshot {
        modified: Some(future_fresh_59),
        ..Default::default()
    };
    assert_eq!(snap_fresh_59.age_seconds(), Some(0));

    // 3. Excessive clock skew > 60s (+120s) -> Stale, age unknown
    let future_excessive = now + Duration::from_secs(120);
    assert!(is_stale(Some(future_excessive)));

    let snap_excessive = SystemSnapshot {
        modified: Some(future_excessive),
        ..Default::default()
    };
    assert_eq!(snap_excessive.age_seconds(), None);
    assert_eq!(snap_excessive.age_label(), "unknown/clock-skew");

    // 4. Normal past (< STALE_AFTER_SECS) -> Not stale
    let past_valid = now - Duration::from_secs(120);
    assert!(!is_stale(Some(past_valid)));

    // 5. Expired past (> STALE_AFTER_SECS) -> Stale
    let past_expired = now - Duration::from_secs(STALE_AFTER_SECS + 60);
    assert!(is_stale(Some(past_expired)));
}

#[test]
fn test_concurrent_atomic_status_writes() {
    use crate::models::{StatusJson, StatusUpdates, read_system_snapshot};
    use crate::triage::write_status_json_atomic;
    use std::sync::Arc;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("syspilot-atomic-concurrent-{nanos}"));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let status_path = Arc::new(tmp_dir.join("status.json"));

    let mut handles = Vec::new();
    for thread_idx in 0..16 {
        let p = Arc::clone(&status_path);
        handles.push(thread::spawn(move || {
            for i in 0..10 {
                let payload = StatusJson {
                    schema_version: "1.0".into(),
                    status: "FLIGHT_READY".into(),
                    status_reasons: Vec::new(),
                    informational_reasons: Vec::new(),
                    timestamp: Some(format!("thread-{thread_idx}-iter-{i}")),
                    gaming_mode: false,
                    reboot_pending: false,
                    updates: StatusUpdates {
                        total: 0,
                        core_count: 0,
                        regular_count: 0,
                        aur_count: 0,
                    },
                    failed_services: None,
                    disk: None,
                    pacnew: None,
                    orphans: None,
                    sys_health: None,
                    security: None,
                    firmware: None,
                    firewall: None,
                    app_update: None,
                    reboot_safety: None,
                    gaming_readiness: None,
                };
                write_status_json_atomic(&p, &payload)
                    .expect("Concurrent atomic write must succeed");
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Verify destination exists and parses cleanly
    assert!(status_path.is_file());
    let snapshot = read_system_snapshot(&status_path)
        .expect("Snapshot written by concurrent threads must parse without corruption");
    assert_eq!(snapshot.status, "FLIGHT_READY");

    // Verify NO temporary files (.status.json.tmp.*) remain orphaned
    let entries = std::fs::read_dir(&tmp_dir).unwrap();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(
            !name.starts_with(".status.json.tmp"),
            "Dangling temporary file found: {name}"
        );
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_json_fuzz_property_resilience() {
    use crate::models::{read_system_snapshot, read_system_snapshot_or_fallback};
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_file = std::env::temp_dir().join(format!("syspilot-fuzz-{nanos}/status.json"));
    std::fs::create_dir_all(tmp_file.parent().unwrap()).unwrap();

    let valid_base = r#"{
        "schema_version": "1.0",
        "status": "FLIGHT_READY",
        "status_reasons": ["all good"],
        "updates": {"total": 3, "core_count": 1, "regular_count": 2, "aur_count": 0},
        "gaming_mode": false,
        "reboot_pending": false
    }"#;

    // Fuzz iteration seeds (500 variations)
    let mut prng_state: u64 = 0x853c49e6748fea9b;
    let mut next_u32 = || {
        prng_state = prng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (prng_state >> 32) as u32
    };

    for i in 0..500 {
        let mut bytes = valid_base.as_bytes().to_vec();
        match i % 5 {
            0 => {
                // Truncation at arbitrary cut points
                let cut = (next_u32() as usize) % (bytes.len() + 1);
                bytes.truncate(cut);
            }
            1 => {
                // Byte flip / corruption
                if !bytes.is_empty() {
                    let idx = (next_u32() as usize) % bytes.len();
                    bytes[idx] ^= (next_u32() & 0xFF) as u8;
                }
            }
            2 => {
                // Insertion of null bytes and special chars
                let idx = (next_u32() as usize) % (bytes.len() + 1);
                bytes.insert(idx, 0x00);
            }
            3 => {
                // Huge values / unexpected types
                let malformed = format!(
                    r#"{{"schema_version": {}, "status": null, "updates": {{"total": 99999999999}}}}"#,
                    i
                );
                bytes = malformed.into_bytes();
            }
            _ => {
                // Random junk
                let len = (next_u32() as usize) % 256;
                bytes = (0..len).map(|_| (next_u32() & 0xFF) as u8).collect();
            }
        }

        std::fs::write(&tmp_file, &bytes).unwrap();

        // 1. read_system_snapshot MUST NOT PANIC
        let _ = read_system_snapshot(&tmp_file);

        // 2. read_system_snapshot_or_fallback MUST NEVER PANIC AND ALWAYS RETURN A VALID SNAPSHOT
        let snapshot = read_system_snapshot_or_fallback(&tmp_file);
        if bytes.is_empty() || bytes != valid_base.as_bytes() {
            // Must have handled errors gracefully
            assert!(
                snapshot.source_error.is_some() || snapshot.status == "FLIGHT_READY",
                "Snapshot must report error or valid status"
            );
        }
    }

    let _ = std::fs::remove_dir_all(tmp_file.parent().unwrap());
}

#[test]
fn test_sys_health_ingests_mirrors_telemetry() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let state_dir = std::env::temp_dir().join(format!("syspilot-mirrors-test-{nanos}"));
    let summary_dir = state_dir.join("system-health");
    std::fs::create_dir_all(&summary_dir).unwrap();

    let summary_content = r#"{
        "schema_version": "2.0",
        "counts": {"errors": 1, "warnings": 2},
        "checks": {
            "mirrors": {
                "arch_primary_alive": true,
                "arch_age_days": 0.42
            }
        },
        "environment": {
            "os": "EndeavourOS",
            "kernel": "6.12.0-zen"
        }
    }"#;
    std::fs::write(summary_dir.join("summary.json"), summary_content).unwrap();

    unsafe {
        std::env::set_var("XDG_STATE_HOME", &state_dir);
    }

    let health = crate::triage::load_sys_health();

    unsafe {
        std::env::remove_var("XDG_STATE_HOME");
    }

    assert!(health.available);
    assert_eq!(health.errors, 1);
    assert_eq!(health.warnings, 2);
    let mirrors = health.mirrors.expect("Mirrors telemetry must be ingested");
    assert_eq!(mirrors.arch_primary_alive, Some(true));
    assert_eq!(mirrors.arch_age_days, Some(0.42));

    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn test_services_failed_units_differentiation() {
    use crate::models::FailedServicesStatus;

    let status = FailedServicesStatus::default();
    assert!(status.user_manager_available);
    assert!(status.system.is_empty());
    assert!(status.user.is_empty());

    let deserialized: FailedServicesStatus =
        serde_json::from_str(r#"{"system":[], "user":[]}"#).unwrap();
    assert!(deserialized.user_manager_available); // Backward compatible default
}

#[test]
fn test_semver_multi_component_comparison() {
    use crate::release::{is_newer_version, parse_version_components};

    assert_eq!(parse_version_components("2.0.2"), vec![2, 0, 2]);
    assert_eq!(parse_version_components("v2.0.1.1"), vec![2, 0, 1, 1]);
    assert_eq!(parse_version_components("V3.10.0-rc1"), vec![3, 10, 0]);

    assert!(is_newer_version("2.0.1", "2.0.2"));
    assert!(!is_newer_version("2.0.2", "2.0.2"));
    assert!(!is_newer_version("2.0.3", "2.0.2"));
    assert!(!is_newer_version("2.1.0", "2.0.9"));
    assert!(is_newer_version("2.0.9", "2.1.0"));
    assert!(is_newer_version("2.0.1", "2.0.1.1"));
    assert!(!is_newer_version("2.0.1.1", "2.0.1"));
    assert!(is_newer_version("v2.0.1", "v2.0.2"));
    assert!(!is_newer_version("v2.0.2", "2.0.2"));
}

#[test]
fn test_parse_github_release_json_sample() {
    use crate::release::GitHubRelease;

    let sample = r#"{
        "tag_name": "v2.0.2",
        "name": "SysPilot v2.0.2 Release",
        "html_url": "https://github.com/Gbur79/SysPilot/releases/tag/v2.0.2",
        "published_at": "2026-11-01T12:00:00Z",
        "body": "Zero-waste GitHub sync engine"
    }"#;

    let release: GitHubRelease = serde_json::from_str(sample).unwrap();
    assert_eq!(release.tag_name, "v2.0.2");
    assert_eq!(
        release.html_url,
        "https://github.com/Gbur79/SysPilot/releases/tag/v2.0.2"
    );
    assert_eq!(release.published_at, "2026-11-01T12:00:00Z");
    assert_eq!(release.name.as_deref(), Some("SysPilot v2.0.2 Release"));
}

#[test]
fn test_release_cache_ttl_logic() {
    use crate::release::{
        CachedReleaseInfo, GitHubRelease, RELEASE_CACHE_TTL_SECS, read_cached_release,
        write_cached_release,
    };

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_path = std::env::temp_dir().join(format!("syspilot-cache-test-{nanos}/release.json"));

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Fresh cache: timestamp = now_secs - 100s (< 24h)
    let fresh_info = CachedReleaseInfo {
        timestamp: now_secs - 100,
        release: GitHubRelease {
            tag_name: "v2.0.2".into(),
            html_url: "https://example.com/rel".into(),
            published_at: "2026-11-01T00:00:00Z".into(),
            name: Some("v2.0.2".into()),
            body: None,
        },
    };
    write_cached_release(&tmp_path, &fresh_info).unwrap();

    let read_back = read_cached_release(&tmp_path).expect("Failed to read cached release");
    assert_eq!(read_back.release.tag_name, "v2.0.2");
    assert!(now_secs.saturating_sub(read_back.timestamp) < RELEASE_CACHE_TTL_SECS);

    // 2. Expired cache: timestamp = now_secs - (RELEASE_CACHE_TTL_SECS + 500)
    let expired_info = CachedReleaseInfo {
        timestamp: now_secs.saturating_sub(RELEASE_CACHE_TTL_SECS + 500),
        release: GitHubRelease {
            tag_name: "v2.0.1".into(),
            html_url: "https://example.com/old".into(),
            published_at: "2026-10-01T00:00:00Z".into(),
            name: Some("v2.0.1".into()),
            body: None,
        },
    };
    write_cached_release(&tmp_path, &expired_info).unwrap();

    let read_expired = read_cached_release(&tmp_path).expect("Failed to read expired release");
    assert_eq!(read_expired.release.tag_name, "v2.0.1");
    assert!(now_secs.saturating_sub(read_expired.timestamp) >= RELEASE_CACHE_TTL_SECS);

    // Cleanup
    if let Some(parent) = tmp_path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
}

#[test]
fn test_skin_preference_persistence() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_path = std::env::temp_dir().join(format!("syspilot-skin-test-{nanos}/skin"));

    // 1. Non-existent path returns default true (LCARS)
    assert!(crate::skin::load_skin_preference_from(&tmp_path));

    // 2. Save classic, verify read back
    crate::skin::save_skin_preference_to(&tmp_path, false).unwrap();
    assert!(!crate::skin::load_skin_preference_from(&tmp_path));

    // 3. Save LCARS, verify read back
    crate::skin::save_skin_preference_to(&tmp_path, true).unwrap();
    assert!(crate::skin::load_skin_preference_from(&tmp_path));

    // Cleanup
    if let Some(parent) = tmp_path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
}

#[test]
fn test_detect_gpu_zero_fork() {
    let gpu = crate::models::detect_gpu();
    assert!(!gpu.is_empty(), "GPU descriptor must not be empty");
    assert!(
        gpu.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()),
        "GPU descriptor must be uppercase: {gpu}"
    );
}

#[test]
fn test_sub_path_prevents_absolute_path_replacement_trap() {
    use std::path::{Path, PathBuf};

    let mock_root = Path::new("/tmp/syspilot-mock-root");
    assert_eq!(
        crate::sys_root::sub_path(mock_root, "/etc/os-release"),
        PathBuf::from("/tmp/syspilot-mock-root/etc/os-release")
    );
    assert_eq!(
        crate::sys_root::sub_path(mock_root, "///proc/sys/kernel/osrelease"),
        PathBuf::from("/tmp/syspilot-mock-root/proc/sys/kernel/osrelease")
    );
    assert_eq!(
        crate::sys_root::sub_path(Path::new("/"), "/etc/os-release"),
        PathBuf::from("/etc/os-release")
    );
}

#[test]
fn test_mock_rig_five_archetypes_universalism() {
    use crate::mock_rig::MockRig;
    use crate::models::{detect_gpu_at, detect_kernel_at, detect_os_at};
    use crate::sentinels::check_reboot_pending_at;

    // 1. Arch + AMD Radeon
    let amd = MockRig::arch_amd_radeon();
    assert_eq!(detect_os_at(amd.root()), "Arch Linux");
    assert_eq!(detect_kernel_at(amd.root()), "6.12.1-arch1-1");
    assert_eq!(
        detect_gpu_at(amd.root()),
        "AMD/ATI GPU (0X1002:0X744C) • DRIVER: AMDGPU"
    );
    assert!(!check_reboot_pending_at(amd.root()));

    // 2. EndeavourOS + NVIDIA Maxwell DKMS
    let nv = MockRig::endeavouros_nvidia_dkms();
    assert_eq!(detect_os_at(nv.root()), "EndeavourOS");
    assert_eq!(detect_kernel_at(nv.root()), "7.2.14-zen1-1-zen");
    assert_eq!(
        detect_gpu_at(nv.root()),
        "NVIDIA GEFORCE GTX 970 • DRIVER 580.95.05"
    );
    assert!(!check_reboot_pending_at(nv.root()));

    // 3. CachyOS + Intel Iris Xe Hybrid Laptop
    let laptop = MockRig::cachyos_intel_hybrid_laptop();
    assert_eq!(detect_os_at(laptop.root()), "CachyOS");
    assert_eq!(detect_kernel_at(laptop.root()), "6.12.2-cachyos");
    assert_eq!(
        detect_gpu_at(laptop.root()),
        "INTEL GPU (0X8086:0X46A6) • DRIVER: I915"
    );
    assert!(!check_reboot_pending_at(laptop.root()));

    // 4. Broken Reboot State (running kernel 7.2.14 missing from /usr/lib/modules)
    let broken = MockRig::broken_reboot_state();
    assert!(
        check_reboot_pending_at(broken.root()),
        "Missing /usr/lib/modules/<running_kernel> must flag reboot_pending = true"
    );

    // 5. Headless Minimal VM (/usr/lib/os-release fallback, no 0x03 display controller)
    let vm = MockRig::headless_minimal_vm();
    assert_eq!(detect_os_at(vm.root()), "Arch Linux ARM / VM");
    assert_eq!(detect_kernel_at(vm.root()), "6.6.60-1-lts");
    assert_eq!(detect_gpu_at(vm.root()), "DEFAULT DISPLAY ADAPTER");
    assert!(!check_reboot_pending_at(vm.root()));
}

#[test]
fn test_golden_host_fixture_schema() {
    use crate::mock_rig::MockRig;
    use crate::models::{detect_gpu_at, detect_kernel_at, detect_os_at};
    use crate::sentinels::check_reboot_pending_at;

    let fixture_str = include_str!("../tests/fixtures/host_endeavouros_maxwell.json");
    let json: serde_json::Value =
        serde_json::from_str(fixture_str).expect("Valid JSON host fixture");

    assert!(
        json.get("os_release").and_then(|v| v.get("NAME")).is_some(),
        "Fixture must have os_release.NAME"
    );
    assert!(
        json.get("kernel").and_then(|v| v.as_str()).is_some(),
        "Fixture must have kernel version string"
    );
    let pci = json
        .get("pci_display_devices")
        .and_then(|v| v.as_array())
        .expect("pci_display_devices array");
    assert!(
        !pci.is_empty(),
        "Fixture must contain at least 1 PCI display device"
    );
    assert!(pci[0].get("vendor").is_some());
    assert!(pci[0].get("device").is_some());
    assert!(
        json.get("vulkan_icd").and_then(|v| v.as_array()).is_some(),
        "Fixture must contain vulkan_icd array"
    );
    assert!(
        json.get("libraries").is_some(),
        "Fixture must contain libraries map"
    );

    let rig = MockRig::from_host_fixture(fixture_str);
    crate::exec::reset_spawn_count();
    assert_eq!(detect_os_at(rig.root()), "EndeavourOS");
    let expected_kernel = json.get("kernel").and_then(|v| v.as_str()).unwrap();
    assert_eq!(detect_kernel_at(rig.root()), expected_kernel);
    let gpu_desc = detect_gpu_at(rig.root());
    assert!(gpu_desc.contains("NVIDIA GEFORCE GTX 970"));
    assert!(gpu_desc.contains("DRIVER"));
    assert!(!check_reboot_pending_at(rig.root()));

    let safety = crate::reboot_safety::check_reboot_safety_at(rig.root());
    assert!(!safety.black_screen_risk);

    let gaming = crate::gaming::check_gaming_readiness_at(rig.root());
    assert!(gaming.applicable);
    assert!(gaming.ready);
    assert!(gaming.vulkan_64_ready);
    assert!(gaming.vulkan_32_ready);

    assert_eq!(crate::exec::spawn_count(), 0);
}

#[test]
fn test_find_pacnew_files_at_survives_permission_denied_and_symlinks() {
    use crate::mock_rig::MockRig;
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt;

    let rig = MockRig::empty();
    rig.write("/etc/pacman.conf.pacnew", "mock");
    rig.write("/etc/pacman.d/mirrorlist.pacnew", "mock");
    rig.write("/etc/a/b/c/d/too_deep.pacnew", "mock");

    let sudoers = rig.sub("/etc/sudoers.d");
    fs::create_dir_all(&sudoers).unwrap();
    fs::set_permissions(&sudoers, Permissions::from_mode(0o000)).unwrap();

    rig.symlink("/etc", "/etc/loop_link");

    crate::exec::reset_spawn_count();
    let files = crate::triage::find_pacnew_files_at(rig.root());

    // Restore permissions so TempDir cleanup succeeds
    let _ = fs::set_permissions(&sudoers, Permissions::from_mode(0o700));

    assert_eq!(
        files,
        vec![
            "/etc/pacman.conf.pacnew".to_string(),
            "/etc/pacman.d/mirrorlist.pacnew".to_string()
        ]
    );
    assert_eq!(crate::exec::spawn_count(), 0);
}

#[test]
fn test_killer_features_reboot_safety_and_gaming_across_five_archetypes() {
    let _guard = TERMINAL_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::mock_rig::MockRig;

    crate::exec::reset_spawn_count();

    // 1. Arch AMD Radeon
    {
        let rig = MockRig::arch_amd_radeon();
        let safety = crate::reboot_safety::check_reboot_safety_at(rig.root());
        assert!(!safety.black_screen_risk);
        assert!(!safety.reboot_pending);
        assert_eq!(safety.active_gpu_drivers, vec!["amdgpu"]);

        let gaming = crate::gaming::check_gaming_readiness_at(rig.root());
        assert!(gaming.applicable);
        assert!(gaming.ready);
        assert!(gaming.vulkan_64_ready);
        assert!(gaming.vulkan_32_ready);
        assert!(!gaming.hybrid_gpu_detected);
    }

    // 2. EndeavourOS NVIDIA Maxwell DKMS
    {
        let rig = MockRig::endeavouros_nvidia_dkms();
        let safety = crate::reboot_safety::check_reboot_safety_at(rig.root());
        assert!(!safety.black_screen_risk);
        assert!(!safety.reboot_pending);
        assert_eq!(safety.active_gpu_drivers, vec!["nvidia"]);

        let gaming = crate::gaming::check_gaming_readiness_at(rig.root());
        assert!(gaming.applicable);
        assert!(gaming.ready);
        assert!(gaming.vulkan_64_ready);
        assert!(gaming.vulkan_32_ready);
    }

    // 3. CachyOS Intel Hybrid Laptop
    {
        let rig = MockRig::cachyos_intel_hybrid_laptop();
        let safety = crate::reboot_safety::check_reboot_safety_at(rig.root());
        assert!(!safety.black_screen_risk);
        assert!(!safety.reboot_pending);
        assert_eq!(safety.active_gpu_drivers, vec!["i915"]);

        let gaming = crate::gaming::check_gaming_readiness_at(rig.root());
        assert!(gaming.applicable);
        assert!(gaming.ready);
        assert!(gaming.vulkan_64_ready);
        assert!(gaming.vulkan_32_ready);
        assert!(gaming.hybrid_gpu_detected);
        assert_eq!(gaming.dgpu_runtime_status.as_deref(), Some("suspended"));
        assert!(gaming.on_battery);
        assert_eq!(gaming.cpu_governor.as_deref(), Some("powersave"));
    }

    // 4. Broken Reboot State
    {
        let rig = MockRig::broken_reboot_state();
        let safety = crate::reboot_safety::check_reboot_safety_at(rig.root());
        assert!(safety.black_screen_risk);
        assert!(safety.reboot_pending);
        assert!(safety.modules_dep_missing);
        assert!(safety.initramfs_empty);
        assert!(!safety.missing_gpu_modules.is_empty());

        let gaming = crate::gaming::check_gaming_readiness_at(rig.root());
        assert!(gaming.applicable);
        assert!(!gaming.ready);
        assert!(gaming.vulkan_64_ready);
        assert!(!gaming.vulkan_32_ready);
        assert!(!gaming.warnings.is_empty());
    }

    // 5. Headless Minimal VM
    {
        let rig = MockRig::headless_minimal_vm();
        let safety = crate::reboot_safety::check_reboot_safety_at(rig.root());
        assert!(!safety.black_screen_risk);
        assert!(!safety.reboot_pending);
        assert!(safety.active_gpu_drivers.is_empty());

        let gaming = crate::gaming::check_gaming_readiness_at(rig.root());
        assert!(!gaming.applicable);
        assert!(gaming.ready);
    }

    assert_eq!(crate::exec::spawn_count(), 0);
}

#[test]
fn test_adversarial_orphan_kernel_dir_builtin_module_and_foreign_icd() {
    use crate::gaming::check_gaming_readiness_at;
    use crate::mock_rig::MockRig;
    use crate::reboot_safety::check_reboot_safety_at;

    // 1. Start from MockRig::endeavouros_nvidia_dkms();
    //    Mark valid kernel 7.2.14-zen1-1-zen with vmlinuz.
    //    Create a dead leftover directory 6.1.0-old-orphan/extramodules with no vmlinuz/modules.dep/nvidia.ko.
    //    Assert safety ignores orphan and reports !black_screen_risk and installed_kernels == vec!["7.2.14-zen1-1-zen"].
    let rig = MockRig::endeavouros_nvidia_dkms();
    rig.write(
        "/usr/lib/modules/7.2.14-zen1-1-zen/vmlinuz",
        b"mock vmlinuz",
    );
    std::fs::create_dir_all(rig.sub("/usr/lib/modules/6.1.0-old-orphan/extramodules")).unwrap();

    let safety = check_reboot_safety_at(rig.root());
    assert!(!safety.black_screen_risk);
    assert_eq!(safety.installed_kernels, vec!["7.2.14-zen1-1-zen"]);

    // 2. Delete /usr/lib32/libGLX_nvidia.so.0 and inject foreign radeon_icd.i686.json + libvulkan_radeon.so.
    //    Assert check_gaming_readiness_at is NOT fooled by radeon_icd.i686.json on active NVIDIA rig.
    std::fs::remove_file(rig.sub("/usr/lib32/libGLX_nvidia.so.0")).unwrap();
    rig.write(
        "/usr/share/vulkan/icd.d/radeon_icd.i686.json",
        r#"{"ICD":{"api_version":"1.3.295","library_path":"/usr/lib32/libvulkan_radeon.so"}}"#,
    );
    rig.write("/usr/lib32/libvulkan_radeon.so", b"ELF32");

    let gaming = check_gaming_readiness_at(rig.root());
    assert!(gaming.vulkan_64_ready);
    assert!(!gaming.vulkan_32_ready);
    assert!(!gaming.ready);

    // 3. Create a custom kernel rig with i915 listed in modules.builtin and no separate .ko.zst file.
    //    Assert !check_reboot_safety_at(custom_rig.root()).black_screen_risk.
    let custom_rig = MockRig::empty();
    let kver = "6.13.0-custom";
    custom_rig.write("/proc/sys/kernel/osrelease", format!("{kver}\n"));
    custom_rig.write("/etc/os-release", "NAME=\"Arch Linux\"\n");
    custom_rig.write("/sys/bus/pci/devices/0000:00:02.0/class", "0x030000\n");
    std::fs::create_dir_all(custom_rig.sub("/sys/bus/pci/drivers/i915")).unwrap();
    custom_rig.symlink(
        "/sys/bus/pci/drivers/i915",
        "/sys/bus/pci/devices/0000:00:02.0/driver",
    );
    custom_rig.write(&format!("/usr/lib/modules/{kver}/modules.dep"), "");
    custom_rig.write(
        &format!("/usr/lib/modules/{kver}/modules.builtin"),
        "kernel/drivers/gpu/drm/i915/i915.ko\n",
    );

    let custom_safety = check_reboot_safety_at(custom_rig.root());
    assert!(!custom_safety.black_screen_risk);
    assert!(custom_safety.missing_gpu_modules.is_empty());
}

#[tokio::test]
async fn test_firewall_at_all_backends_and_archetypes_zero_subprocess() {
    use crate::firewall::{check_host_firewall, check_host_firewall_at, parse_ufw_conf_enabled};
    use crate::mock_rig::MockRig;

    crate::exec::reset_spawn_count();

    assert!(parse_ufw_conf_enabled("ENABLED=yes\n"));
    assert!(parse_ufw_conf_enabled("ENABLED=\"yes\"\n"));
    assert!(!parse_ufw_conf_enabled("ENABLED=no\n"));

    // 1. All 5 MockRig archetypes default to inactive perimeter-protected state with 0 subprocesses
    for rig in [
        MockRig::arch_amd_radeon(),
        MockRig::endeavouros_nvidia_dkms(),
        MockRig::cachyos_intel_hybrid_laptop(),
        MockRig::broken_reboot_state(),
        MockRig::headless_minimal_vm(),
    ] {
        let fw = check_host_firewall_at(rig.root());
        assert!(fw.available);
        assert!(!fw.active);
        assert_eq!(fw.backend, "none");
        assert_eq!(fw.default_zone, None);
        assert_eq!(fw.details, "protected by perimeter router/LAN");
    }

    // 2. firewalld via cgroup.procs + custom zone in /etc/firewalld/firewalld.conf
    {
        let rig = MockRig::empty();
        rig.write(
            "/sys/fs/cgroup/system.slice/firewalld.service/cgroup.procs",
            "812\n",
        );
        rig.write("/etc/firewalld/firewalld.conf", "DefaultZone=home\n");
        let fw = check_host_firewall_at(rig.root());
        assert!(fw.available);
        assert!(fw.active);
        assert_eq!(fw.backend, "firewalld");
        assert_eq!(fw.default_zone.as_deref(), Some("home"));
        assert_eq!(fw.details, "default zone: home");
    }

    // 3. firewalld via /run/firewalld/firewalld.pid fallback + default "public" zone (whitespace cgroup.procs ignored)
    {
        let rig = MockRig::empty();
        rig.write(
            "/sys/fs/cgroup/system.slice/firewalld.service/cgroup.procs",
            "   \n",
        );
        let fw_inactive = check_host_firewall_at(rig.root());
        assert!(!fw_inactive.active);

        rig.write("/run/firewalld/firewalld.pid", "915\n");
        let fw = check_host_firewall_at(rig.root());
        assert!(fw.active);
        assert_eq!(fw.backend, "firewalld");
        assert_eq!(fw.default_zone.as_deref(), Some("public"));
        assert_eq!(fw.details, "default zone: public");
    }

    // 4. ufw: requires BOTH ENABLED=yes AND (ufw.service dir || nf_tables dir || ip_tables_names)
    {
        let rig = MockRig::empty();
        rig.write("/etc/ufw/ufw.conf", "ENABLED=yes\n");
        assert!(!check_host_firewall_at(rig.root()).active);

        std::fs::create_dir_all(rig.sub("/sys/fs/cgroup/system.slice/ufw.service")).unwrap();
        let fw1 = check_host_firewall_at(rig.root());
        assert!(fw1.active);
        assert_eq!(fw1.backend, "ufw");
        assert_eq!(fw1.details, "active");
    }
    {
        let rig = MockRig::empty();
        std::fs::create_dir_all(rig.sub("/sys/module/nf_tables")).unwrap();
        assert!(!check_host_firewall_at(rig.root()).active);

        rig.write("/etc/ufw/ufw.conf", "ENABLED=yes\n");
        let fw2 = check_host_firewall_at(rig.root());
        assert!(fw2.active);
        assert_eq!(fw2.backend, "ufw");
    }
    {
        let rig = MockRig::empty();
        rig.write("/etc/ufw/ufw.conf", "ENABLED=yes\n");
        rig.write("/proc/net/ip_tables_names", "filter\n");
        let fw3 = check_host_firewall_at(rig.root());
        assert!(fw3.active);
        assert_eq!(fw3.backend, "ufw");
    }

    // 5. nftables: active cgroup service directory
    {
        let rig = MockRig::empty();
        std::fs::create_dir_all(rig.sub("/sys/fs/cgroup/system.slice/nftables.service")).unwrap();
        let fw = check_host_firewall_at(rig.root());
        assert!(fw.active);
        assert_eq!(fw.backend, "nftables");
        assert_eq!(fw.details, "active");
    }

    // 6. iptables: requires BOTH iptables.service dir AND non-empty /proc/net/ip_tables_names
    {
        let rig = MockRig::empty();
        std::fs::create_dir_all(rig.sub("/sys/fs/cgroup/system.slice/iptables.service")).unwrap();
        assert!(!check_host_firewall_at(rig.root()).active);

        let rig_only_proc = MockRig::empty();
        rig_only_proc.write("/proc/net/ip_tables_names", "filter\n");
        assert!(!check_host_firewall_at(rig_only_proc.root()).active);

        rig.write("/proc/net/ip_tables_names", "filter\n");
        let fw = check_host_firewall_at(rig.root());
        assert!(fw.active);
        assert_eq!(fw.backend, "iptables");
        assert_eq!(fw.details, "active");
    }

    assert_eq!(crate::exec::spawn_count(), 0);

    let live_fw = check_host_firewall().await;
    assert!(live_fw.available);
    assert!(!live_fw.backend.is_empty());
    assert_eq!(crate::exec::spawn_count(), 0);
}

#[tokio::test]
async fn test_release_install_origin_and_upstream_fetch_hermetic() {
    use crate::mock_rig::MockRig;
    use crate::release::{
        CachedReleaseInfo, GitHubRelease, InstallOrigin, RELEASE_CACHE_TTL_SECS,
        check_upstream_release, detect_install_origin, detect_install_origin_at,
        fetch_latest_github_release, get_release_cache_path, read_cached_release,
        set_test_release_overrides, write_cached_release,
    };
    use std::path::Path;

    set_test_release_overrides(None, None);
    assert!(
        get_release_cache_path()
            .to_string_lossy()
            .ends_with("syspilot/github_release.json")
    );
    assert!(read_cached_release(Path::new("/nonexistent/syspilot-missing-cache.json")).is_none());

    crate::exec::reset_spawn_count();

    // 1. Standalone when neither pacman DB entry nor .git exists
    let rig = MockRig::empty();
    assert_eq!(
        detect_install_origin_at(rig.root(), None, None),
        InstallOrigin::Standalone
    );
    let non_git_dir = rig.sub("/opt/syspilot_bin");
    std::fs::create_dir_all(&non_git_dir).unwrap();
    let non_git_exe = non_git_dir.join("syspilot");
    let non_git_cwd = non_git_dir.join("work");
    std::fs::create_dir_all(&non_git_cwd).unwrap();
    assert_eq!(
        detect_install_origin_at(rig.root(), Some(&non_git_exe), Some(&non_git_cwd)),
        InstallOrigin::Standalone
    );

    // 2. Negative cases for Pacman: only DB entry without /usr/bin, or only /usr/bin without DB entry
    rig.write("/usr/bin/syspilot", b"#!/bin/sh\n");
    assert_eq!(
        detect_install_origin_at(rig.root(), None, None),
        InstallOrigin::Standalone
    );

    let rig_db_only = MockRig::empty();
    std::fs::create_dir_all(rig_db_only.sub("/var/lib/pacman/local/syspilot-debug-2.1.1-1"))
        .unwrap();
    rig_db_only.write("/usr/bin/syspilot", b"#!/bin/sh\n");
    assert_eq!(
        detect_install_origin_at(rig_db_only.root(), None, None),
        InstallOrigin::Standalone
    );

    std::fs::create_dir_all(rig_db_only.sub("/var/lib/pacman/local/syspilot-2.1.1-1")).unwrap();
    std::fs::remove_file(rig_db_only.sub("/usr/bin/syspilot")).unwrap();
    assert_eq!(
        detect_install_origin_at(rig_db_only.root(), None, None),
        InstallOrigin::Standalone
    );

    // 3. Positive cases for Pacman: DB entry + /usr/bin/syspilot in root OR exe_override in /usr/bin
    assert_eq!(
        detect_install_origin_at(
            rig_db_only.root(),
            Some(Path::new("/usr/bin/syspilot-gui")),
            None
        ),
        InstallOrigin::Pacman
    );
    rig_db_only.write("/usr/bin/syspilot", b"#!/bin/sh\n");
    assert_eq!(
        detect_install_origin_at(rig_db_only.root(), None, None),
        InstallOrigin::Pacman
    );

    // 4. Positive cases for GitClone: via exe_override ancestor OR cwd_override ancestor
    let git_rig = MockRig::empty();
    std::fs::create_dir_all(git_rig.sub("/workspace/repo/.git")).unwrap();
    std::fs::create_dir_all(git_rig.sub("/workspace/repo/target/debug")).unwrap();
    let mock_exe = git_rig.sub("/workspace/repo/target/debug/syspilot-gui");
    let mock_cwd = git_rig.sub("/workspace/repo/target");

    assert_eq!(
        detect_install_origin_at(git_rig.root(), Some(&mock_exe), None),
        InstallOrigin::GitClone
    );
    assert_eq!(
        detect_install_origin_at(git_rig.root(), None, Some(&mock_cwd)),
        InstallOrigin::GitClone
    );
    assert_eq!(crate::exec::spawn_count(), 0);
    assert_eq!(detect_install_origin(), InstallOrigin::GitClone);

    // 5. Hermetic file:// test for fetch_latest_github_release and check_upstream_release
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let cache_file = tmp_dir.path().join("github_release.json");
    let feed_file = tmp_dir.path().join("latest.json");
    let feed_url = format!("file://{}", feed_file.display());

    std::fs::write(
        &feed_file,
        r#"{"tag_name":"v2.5.0","html_url":"https://example.com/v2.5.0","published_at":"2026-11-08T00:00:00Z"}"#,
    )
    .unwrap();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Exact TTL boundary (now - RELEASE_CACHE_TTL_SECS) must be treated as expired and trigger fetch
    let boundary_expired = CachedReleaseInfo {
        timestamp: now.saturating_sub(RELEASE_CACHE_TTL_SECS),
        release: GitHubRelease {
            tag_name: "v2.0.0".into(),
            ..Default::default()
        },
    };
    write_cached_release(&cache_file, &boundary_expired).unwrap();
    set_test_release_overrides(Some(cache_file.clone()), Some(feed_url.clone()));

    let fetched = check_upstream_release(false).await.unwrap();
    assert_eq!(fetched.tag_name, "v2.5.0");

    // Fresh cache hit must NOT re-fetch even if upstream file changes to v2.6.0
    std::fs::write(
        &feed_file,
        r#"{"tag_name":"v2.6.0","html_url":"https://example.com/v2.6.0","published_at":"2026-11-08T01:00:00Z"}"#,
    )
    .unwrap();
    let cached_hit = check_upstream_release(false).await.unwrap();
    assert_eq!(cached_hit.tag_name, "v2.5.0");

    // force_refresh = true must bypass fresh cache and fetch v2.6.0
    let forced = check_upstream_release(true).await.unwrap();
    assert_eq!(forced.tag_name, "v2.6.0");

    // Missing upstream file must return Err from fetch_latest_github_release and fall back to cache
    let missing_url = format!("file://{}/nonexistent.json", tmp_dir.path().display());
    set_test_release_overrides(Some(cache_file.clone()), Some(missing_url));
    assert!(fetch_latest_github_release().await.is_err());
    let fallback = check_upstream_release(true).await.unwrap();
    assert_eq!(fallback.tag_name, "v2.6.0");

    set_test_release_overrides(None, None);
}

#[cfg(unix)]
fn write_mock_executable(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

#[tokio::test]
async fn test_stage2_firmware_hermetic_and_mutants() {
    use crate::firmware::{
        FwupdDevice, check_firmware_updates, default_firmware_timeout, is_secure_boot_enabled,
        is_secure_boot_enabled_at, parse_fwupd_devices, parse_timeout_ms, set_test_fwupd_overrides,
    };
    use crate::mock_rig::MockRig;
    use std::time::Duration;

    assert_eq!(parse_timeout_ms(None, 3500), Duration::from_millis(3500));
    assert_eq!(
        parse_timeout_ms(Some("0"), 3500),
        Duration::from_millis(3500)
    );
    assert_eq!(
        parse_timeout_ms(Some("1200"), 3500),
        Duration::from_millis(1200)
    );
    assert_eq!(default_firmware_timeout(), Duration::from_millis(3500));

    // Secure Boot EFI variable tests across MockRig
    let rig = MockRig::empty();
    let sb_rel = "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
    assert!(!is_secure_boot_enabled_at(rig.root()));
    rig.write(sb_rel, [0x06, 0x00, 0x00, 0x00]); // truncated < 5 bytes
    assert!(!is_secure_boot_enabled_at(rig.root()));
    rig.write(sb_rel, [0x06, 0x00, 0x00, 0x00, 0x00]); // disabled
    set_test_fwupd_overrides(Some(rig.root().to_path_buf()), None);
    assert!(!is_secure_boot_enabled());
    rig.write(sb_rel, [0x06, 0x00, 0x00, 0x00, 0x01]); // enabled
    assert!(is_secure_boot_enabled());

    // Independent branches of is_sb_cert (plugin-only, protocols-only, name-only)
    let devs = vec![
        FwupdDevice {
            name: Some("Vendor Cert".into()),
            plugin: Some("uefi_db".into()),
            protocol: Some("other".into()),
            protocols: None,
            update_state: Some(1),
            update_error: None,
        },
        FwupdDevice {
            name: Some("Vendor Token".into()),
            plugin: Some("other".into()),
            protocol: Some("other".into()),
            protocols: Some(vec!["org.uefi.dbx.2".into()]),
            update_state: Some(1),
            update_error: None,
        },
        FwupdDevice {
            name: Some("UEFI Revocation List".into()),
            plugin: Some("other".into()),
            protocol: Some("other".into()),
            protocols: None,
            update_state: Some(1),
            update_error: None,
        },
    ];
    let (upd, mon, pend, quirks) = parse_fwupd_devices(devs, false);
    assert_eq!(mon, 3);
    assert_eq!(upd, 0);
    assert!(pend.is_empty());
    assert_eq!(quirks.len(), 3);

    // Runner branches: skip, not_installed, empty stdout, valid JSON, invalid JSON, exit 2, stderr no updatable, exit 3
    let skipped = check_firmware_updates(true).await;
    assert!(!skipped.available);
    assert_eq!(skipped.reason, "skipped");

    set_test_fwupd_overrides(Some(rig.root().to_path_buf()), Some(None));
    let missing = check_firmware_updates(false).await;
    assert!(!missing.available);
    assert_eq!(missing.reason, "not_installed");

    let tmp_dir = tempfile::TempDir::new().unwrap();
    let mode_file = tmp_dir.path().join("mode");
    let script = format!(
        "#!/bin/sh\nmode=$(cat \"{}\")\ncase \"$mode\" in\n  empty) exit 0 ;;\n  valid) printf '{{\"Devices\":[{{\"Name\":\"NVMe SSD\",\"Plugin\":\"nvme\"}}]}}'; exit 0 ;;\n  badjson) printf 'not-json'; exit 0 ;;\n  exit2) exit 2 ;;\n  stderr_noup) echo 'No updatable devices' >&2; exit 1 ;;\n  exit3) echo 'fatal error' >&2; exit 3 ;;\nesac\n",
        mode_file.display()
    );
    let bin_path = write_mock_executable(tmp_dir.path(), "fwupdmgr", &script);
    set_test_fwupd_overrides(Some(rig.root().to_path_buf()), Some(Some(bin_path)));

    std::fs::write(&mode_file, "empty").unwrap();
    let r_empty = check_firmware_updates(false).await;
    assert!(r_empty.available);
    assert_eq!(r_empty.updates_count, 0);

    std::fs::write(&mode_file, "valid").unwrap();
    let r_valid = check_firmware_updates(false).await;
    assert!(r_valid.available);
    assert_eq!(r_valid.updates_count, 1);
    assert_eq!(r_valid.devices_monitored, 1);
    assert_eq!(r_valid.pending_devices, vec!["NVMe SSD"]);

    std::fs::write(&mode_file, "badjson").unwrap();
    let r_bad = check_firmware_updates(false).await;
    assert!(!r_bad.available);
    assert!(r_bad.reason.starts_with("parse_error:"));

    std::fs::write(&mode_file, "exit2").unwrap();
    let r_exit2 = check_firmware_updates(false).await;
    assert!(r_exit2.available);

    std::fs::write(&mode_file, "stderr_noup").unwrap();
    let r_noup = check_firmware_updates(false).await;
    assert!(r_noup.available);

    std::fs::write(&mode_file, "exit3").unwrap();
    let r_exit3 = check_firmware_updates(false).await;
    assert!(!r_exit3.available);
    assert_eq!(r_exit3.reason, "exit_3");

    set_test_fwupd_overrides(None, None);
}

#[tokio::test]
async fn test_stage2_security_hermetic_and_mutants() {
    use crate::security::{
        check_security_vulnerabilities, default_security_timeout, set_test_arch_audit_bin,
    };
    use std::time::Duration;

    assert_eq!(default_security_timeout(), Duration::from_millis(3500));

    set_test_arch_audit_bin(Some(None));
    let missing = check_security_vulnerabilities(false).await;
    assert!(!missing.available);
    assert_eq!(missing.reason, "not_installed");

    let tmp_dir = tempfile::TempDir::new().unwrap();
    let mode_file = tmp_dir.path().join("mode");
    let script = format!(
        "#!/bin/sh\nmode=$(cat \"{}\")\ncase \"$mode\" in\n  empty) exit 0 ;;\n  valid) printf '[{{\"severity\":\"High\",\"packages\":[\"openssl\"]}}]'; exit 0 ;;\n  bad) printf '{{bad'; exit 0 ;;\n  exit2) exit 2 ;;\nesac\n",
        mode_file.display()
    );
    let bin_path = write_mock_executable(tmp_dir.path(), "arch-audit", &script);
    set_test_arch_audit_bin(Some(Some(bin_path)));

    std::fs::write(&mode_file, "empty").unwrap();
    let r_empty = check_security_vulnerabilities(false).await;
    assert!(r_empty.available);
    assert_eq!(r_empty.vulnerabilities_total, 0);

    std::fs::write(&mode_file, "valid").unwrap();
    let r_valid = check_security_vulnerabilities(false).await;
    assert!(r_valid.available);
    assert_eq!(r_valid.vulnerabilities_total, 1);
    assert_eq!(r_valid.high_risk_count, 1);
    assert_eq!(r_valid.packages, vec!["openssl"]);

    std::fs::write(&mode_file, "bad").unwrap();
    let r_bad = check_security_vulnerabilities(false).await;
    assert!(!r_bad.available);
    assert!(r_bad.reason.starts_with("parse_error:"));

    std::fs::write(&mode_file, "exit2").unwrap();
    let r_exit2 = check_security_vulnerabilities(false).await;
    assert!(!r_exit2.available);
    assert_eq!(r_exit2.reason, "exit_2");

    set_test_arch_audit_bin(None);
}

#[tokio::test(flavor = "current_thread")]
async fn test_stage2_orphans_hermetic_and_mutants() {
    use crate::mock_rig::MockRig;
    use crate::orphans::{
        check_orphan_packages, classify_orphans, glob_match, list_local_pacman_db_entries_at,
        query_pacman_package_names, set_test_pacman_bin,
    };

    assert!(!glob_match("linux*", "mesa"));
    assert!(!glob_match("*-dkms", "mesa"));
    assert!(!glob_match("*firmware*", "mesa"));

    let rig = MockRig::empty();
    assert!(list_local_pacman_db_entries_at(rig.root()).is_empty());
    std::fs::create_dir_all(rig.sub("/var/lib/pacman/local/mesa-24.2.1-1")).unwrap();
    std::fs::create_dir_all(rig.sub("/var/lib/pacman/local/alsa-lib-1.2.12-1")).unwrap();
    rig.write("/var/lib/pacman/local/ALPM_DB_VERSION", "9\n");
    assert_eq!(
        list_local_pacman_db_entries_at(rig.root()),
        vec!["alsa-lib-1.2.12-1".to_string(), "mesa-24.2.1-1".to_string()]
    );

    // Exhaustive classify_orphans state matrix
    let clean = classify_orphans(Ok(vec![]), Ok(vec![]));
    assert!(clean.checked);
    assert!(!clean.affects_flight_status);
    assert!(clean.tiering_complete);
    assert_eq!(clean.query_status, "CLEAN");
    assert_eq!(clean.severity, "INFO");
    assert_eq!(clean.message, "No orphan packages detected");

    let partial = classify_orphans(Ok(vec!["leaf-pkg".into()]), Err("timeout".into()));
    assert!(partial.checked);
    assert!(!partial.affects_flight_status);
    assert!(!partial.tiering_complete);
    assert_eq!(partial.query_status, "PARTIAL");
    assert_eq!(partial.optional_count, 0);
    assert_eq!(partial.tier1_packages, vec!["leaf-pkg"]);
    assert_eq!(
        partial.message,
        "1 strict orphan(s), 0 optional candidate(s), 0 sensitive"
    );

    let failed = classify_orphans(
        Err("db error".into()),
        Ok(vec!["leaf-pkg".into(), "linux-zen".into()]),
    );
    assert!(!failed.checked);
    assert!(failed.affects_flight_status);
    assert!(!failed.tiering_complete);
    assert_eq!(failed.optional_count, 0);
    assert_eq!(failed.query_status, "ERROR");
    assert_eq!(failed.severity, "WARN");
    assert_eq!(failed.unclassified_packages, vec!["leaf-pkg"]);
    assert_eq!(failed.tier3_packages, vec!["linux-zen"]);
    assert_eq!(
        failed.message,
        "Orphan query unavailable: pacman query error"
    );

    // Runner & query_pacman_package_names tests
    set_test_pacman_bin(Some(None));
    let no_bin = check_orphan_packages().await;
    assert!(!no_bin.checked);
    assert_eq!(
        no_bin.message,
        "Orphan query unavailable: pacman executable not found"
    );

    let tmp_dir = tempfile::TempDir::new().unwrap();
    let mode_file = tmp_dir.path().join("mode");
    let script = format!(
        "#!/bin/sh\nmode=$(cat \"{}\")\ncase \"$mode:$1\" in\n  ok:-Qdtq) printf '  leaf-a \\n\\nleaf-b\\n'; exit 0 ;;\n  ok:-Qdttq) printf 'leaf-a\\nleaf-b\\nopt-c\\n'; exit 0 ;;\n  none:*) exit 1 ;;\n  err_stderr:*) echo 'db locked' >&2; exit 1 ;;\n  err_stdout:*) printf 'unexpected\\n'; exit 1 ;;\n  err_empty:*) exit 2 ;;\nesac\n",
        mode_file.display()
    );
    let bin_path = write_mock_executable(tmp_dir.path(), "pacman", &script);
    set_test_pacman_bin(Some(Some(bin_path.clone())));

    std::fs::write(&mode_file, "ok").unwrap();
    let status = check_orphan_packages().await;
    assert!(status.checked);
    assert_eq!(status.count, 2);
    assert_eq!(status.optional_count, 1);

    std::fs::write(&mode_file, "none").unwrap();
    assert_eq!(
        query_pacman_package_names(&bin_path, "-Qdtq", 2)
            .await
            .unwrap(),
        Vec::<String>::new()
    );

    std::fs::write(&mode_file, "err_stderr").unwrap();
    assert!(
        query_pacman_package_names(&bin_path, "-Qdtq", 2)
            .await
            .unwrap_err()
            .contains("db locked")
    );

    std::fs::write(&mode_file, "err_stdout").unwrap();
    assert!(
        query_pacman_package_names(&bin_path, "-Qdtq", 2)
            .await
            .is_err()
    );

    std::fs::write(&mode_file, "err_empty").unwrap();
    assert!(
        query_pacman_package_names(&bin_path, "-Qdtq", 2)
            .await
            .unwrap_err()
            .contains("no diagnostic text")
    );

    set_test_pacman_bin(None);
}

#[tokio::test]
async fn test_stage2_services_hermetic_and_mutants() {
    use crate::mock_rig::MockRig;
    use crate::services::{
        check_failed_services, get_transient_desktop_app_units, is_candidate_app_unit,
        parse_systemctl_show_transient, query_failed_units, set_test_systemctl_bin,
        user_systemd_available_at,
    };
    use std::collections::HashSet;

    assert!(!is_candidate_app_unit("app-slice.slice"));

    let show_out = r#"
Id=app-not-candidate@1.service
Transient=yes
Slice=app.slice

Id=app-wrong-slice@2.service
Transient=yes
Slice=session.slice

Id=app-valid@3.service
Transient=yes
Slice=app.slice
"#;
    let mut cands = HashSet::new();
    cands.insert("app-wrong-slice@2.service".into());
    cands.insert("app-valid@3.service".into());
    let ignored = parse_systemctl_show_transient(show_out, &cands);
    assert_eq!(ignored.len(), 1);
    assert!(ignored.contains("app-valid@3.service"));

    // Headless / container & XDG_RUNTIME_DIR guard checks
    let rig = MockRig::headless_minimal_vm();
    let xdg_dir = rig.sub("/run/user/1000");
    std::fs::create_dir_all(&xdg_dir).unwrap();
    assert!(!user_systemd_available_at(rig.root(), Some(&xdg_dir)));

    std::fs::create_dir_all(rig.sub("/run/systemd/system")).unwrap();
    assert!(!user_systemd_available_at(rig.root(), None));
    assert!(!user_systemd_available_at(rig.root(), Some(&xdg_dir)));

    rig.write("/run/user/1000/bus", b"socket");
    assert!(user_systemd_available_at(rig.root(), Some(&xdg_dir)));
    std::fs::remove_file(rig.sub("/run/user/1000/bus")).unwrap();
    std::fs::create_dir_all(rig.sub("/run/user/1000/systemd/private")).unwrap();
    assert!(user_systemd_available_at(rig.root(), Some(&xdg_dir)));

    // Empty candidate fast-return
    assert!(get_transient_desktop_app_units(&[]).await.is_empty());

    // Mock systemctl runner tests
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let mode_file = tmp_dir.path().join("mode");
    let script = format!(
        "#!/bin/sh\nmode=$(cat \"{}\")\nif [ \"$1\" = \"--user\" ] && [ \"$2\" = \"show\" ]; then\n  if [ \"$mode\" = \"show_fail\" ]; then printf 'Id=app-java@1.service\\nTransient=yes\\nSlice=app.slice\\n'; exit 1; fi\n  printf 'Id=app-java@1.service\\nTransient=yes\\nSlice=app.slice\\n'\n  exit 0\nfi\nif [ \"$1\" = \"--user\" ]; then\n  if [ \"$mode\" = \"user_err\" ]; then echo 'no user bus' >&2; exit 1; fi\n  printf 'app-java@1.service loaded failed failed Java\\nsyncthing.service loaded failed failed Sync\\n'\n  exit 0\nfi\nif [ \"$mode\" = \"sys_err_empty\" ]; then exit 2; fi\nprintf 'ntpd.service loaded failed failed NTP\\n'\nexit 0\n",
        mode_file.display()
    );
    let bin_path = write_mock_executable(tmp_dir.path(), "systemctl", &script);
    set_test_systemctl_bin(Some(bin_path));

    std::fs::write(&mode_file, "ok").unwrap();
    let status = check_failed_services().await;
    assert!(status.user_manager_available);
    assert_eq!(status.system, vec!["ntpd.service"]);
    assert_eq!(status.user, vec!["syncthing.service"]);

    std::fs::write(&mode_file, "show_fail").unwrap();
    let tr_fail = get_transient_desktop_app_units(&["app-java@1.service".into()]).await;
    assert!(tr_fail.is_empty());

    std::fs::write(&mode_file, "user_err").unwrap();
    assert_eq!(query_failed_units(true).await.unwrap_err(), "no user bus");

    std::fs::write(&mode_file, "sys_err_empty").unwrap();
    assert!(
        query_failed_units(false)
            .await
            .unwrap_err()
            .contains("systemctl failed with exit code")
    );

    set_test_systemctl_bin(None);
}

#[test]
fn test_stage3_sentinels_mutants() {
    use crate::mock_rig::MockRig;
    use crate::sentinels::*;
    use std::fs;

    let rig = MockRig::headless_minimal_vm();

    // 1. check_reboot_pending_at:
    // osrelease exists, modules path exists -> false
    rig.write("/proc/sys/kernel/osrelease", b"6.12.1-arch1-1\n");
    let mod_dir = rig.sub("/usr/lib/modules/6.12.1-arch1-1");
    fs::create_dir_all(&mod_dir).unwrap();
    assert!(!check_reboot_pending_at(rig.root()));

    // osrelease exists, modules path missing -> true
    fs::remove_dir_all(&mod_dir).unwrap();
    assert!(check_reboot_pending_at(rig.root()));

    // osrelease empty or spaces -> skips modules check, checks /var/run/reboot-required
    rig.write("/proc/sys/kernel/osrelease", b"   \n");
    assert!(!check_reboot_pending_at(rig.root()));
    rig.write("/var/run/reboot-required", b"reboot\n");
    assert!(check_reboot_pending_at(rig.root()));

    // Reset osrelease & reboot-required
    let _ = fs::remove_file(rig.sub("/var/run/reboot-required"));
    let _ = fs::remove_file(rig.sub("/proc/sys/kernel/osrelease"));
    assert!(!check_reboot_pending_at(rig.root()));

    // 2. check_pacman_lock_at:
    let lock_path = rig.sub("/var/lib/pacman/db.lck");
    assert_eq!(check_pacman_lock_at(&lock_path), (false, false));

    // Create lockfile
    fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    rig.write("/var/lib/pacman/db.lck", b"12345\n");

    // /proc/12345/comm missing and /proc empty -> orphaned lock
    let proc_dir = rig.sub("/proc");
    fs::create_dir_all(&proc_dir).unwrap();
    assert_eq!(check_pacman_lock_at(&lock_path), (true, true));

    // /proc/12345/comm exists with "pacman" -> active!
    let p12345 = proc_dir.join("12345");
    fs::create_dir_all(&p12345).unwrap();
    fs::write(p12345.join("comm"), "pacman\n").unwrap();
    assert_eq!(check_pacman_lock_at(&lock_path), (true, false));

    // "yay", "paru", "pamac-daemon"
    for name in &["yay", "paru", "pamac-daemon"] {
        fs::write(p12345.join("comm"), format!("{name}\n")).unwrap();
        assert_eq!(check_pacman_lock_at(&lock_path), (true, false));
    }

    // comm is other process (e.g. bash) -> fallback scan
    fs::write(p12345.join("comm"), "bash\n").unwrap();
    // Non-digit entry skipped in fallback scan
    let p_net = proc_dir.join("net");
    fs::create_dir_all(&p_net).unwrap();
    fs::write(p_net.join("comm"), "pacman\n").unwrap();
    assert_eq!(check_pacman_lock_at(&lock_path), (true, true));

    // Digit entry in proc found with "yay"
    let p23456 = proc_dir.join("23456");
    fs::create_dir_all(&p23456).unwrap();
    fs::write(p23456.join("comm"), "yay\n").unwrap();
    assert_eq!(check_pacman_lock_at(&lock_path), (true, false));

    // Standalone lock path outside /var/lib/pacman
    let custom_lock = rig.sub("/custom/db.lck");
    fs::create_dir_all(custom_lock.parent().unwrap()).unwrap();
    fs::write(&custom_lock, "999\n").unwrap();
    assert_eq!(check_pacman_lock_at(&custom_lock), (true, true));

    // TOCTOU check: removing custom_lock -> (false, false)
    fs::remove_file(&custom_lock).unwrap();
    assert_eq!(check_pacman_lock_at(&custom_lock), (false, false));

    // 3. get_pacman_db_mtime_at:
    let local_db = rig.sub("/var/lib/pacman/local");
    fs::create_dir_all(&local_db).unwrap();
    assert!(get_pacman_db_mtime_at(rig.root()).is_some());
    fs::remove_dir_all(&local_db).unwrap();
    assert!(get_pacman_db_mtime_at(rig.root()).is_none());

    // 4. get_summary_mtime_at:
    let state_dir = rig.sub("/state");
    assert!(get_summary_mtime_at(&state_dir).is_none());
    let sum_file = state_dir.join("system-health/summary.json");
    fs::create_dir_all(sum_file.parent().unwrap()).unwrap();
    fs::write(&sum_file, "{}").unwrap();
    assert!(get_summary_mtime_at(&state_dir).is_some());

    // 5. get_disk_stats:
    assert!(get_disk_stats("/").is_some());
    assert!(get_disk_stats("/nonexistent_path_stage3_sentinels").is_none());

    // 6. collect_sentinels and parameterless functions with thread-local overrides:
    let local_db = rig.sub("/var/lib/pacman/local");
    fs::create_dir_all(&local_db).unwrap();

    set_test_sentinels_overrides(Some(rig.root().to_path_buf()), Some(state_dir));

    assert!(get_pacman_db_mtime().is_some());
    assert!(!check_reboot_pending());
    rig.write("/proc/sys/kernel/osrelease", b"missing-mod\n");
    assert!(check_reboot_pending());
    let _ = fs::remove_file(rig.sub("/proc/sys/kernel/osrelease"));

    let _ = fs::remove_file(rig.sub("/var/lib/pacman/db.lck"));
    assert_eq!(check_pacman_lock(), (false, false));
    fs::write(rig.sub("/var/lib/pacman/db.lck"), "12345\n").unwrap();
    assert_eq!(check_pacman_lock(), (true, false));
    fs::remove_file(rig.sub("/var/lib/pacman/db.lck")).unwrap();

    assert!(get_summary_mtime().is_some());

    let collected = collect_sentinels();
    assert!(collected.summary_mtime.is_some());
    assert!(!collected.reboot_pending);
    assert!(!collected.pacman_locked);

    set_test_sentinels_overrides(None, None);
}

#[tokio::test(flavor = "current_thread")]
async fn test_stage3_triage_mutants() {
    use crate::mock_rig::MockRig;
    use crate::models::*;
    use crate::triage::*;
    use std::fs;

    let rig = MockRig::headless_minimal_vm();

    // 1. find_pacnew_files_at & depth limits
    // Create depth 1..4 files and depth 5 file
    rig.write("/etc/a.pacnew", b"conf1");
    rig.write("/etc/d1/b.pacnew", b"conf2");
    rig.write("/etc/d1/d2/c.pacnew", b"conf3");
    rig.write("/etc/d1/d2/d3/d.pacnew", b"conf4");
    rig.write("/etc/d1/d2/d3/d4/e.pacnew", b"conf5"); // depth 5, must be skipped
    rig.write("/etc/regular.conf", b"ignore");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(rig.sub("/etc/a.pacnew"), rig.sub("/etc/symlink.pacnew"));
    }

    let found = find_pacnew_files_at(rig.root());
    assert_eq!(
        found,
        vec![
            "/etc/a.pacnew",
            "/etc/d1/b.pacnew",
            "/etc/d1/d2/c.pacnew",
            "/etc/d1/d2/d3/d.pacnew",
        ]
    );

    // check_pacnew_files with active_triage_root override
    set_test_triage_overrides(Some(rig.root().to_path_buf()), None);
    let check_found = check_pacnew_files().await;
    assert_eq!(check_found, found);
    set_test_triage_overrides(None, None);

    // 2. calc_disk_usage & get_disk_usage
    assert!(calc_disk_usage(0, 100, 50).is_none());
    assert!(calc_disk_usage(4096, 0, 50).is_none());

    let u_clean = calc_disk_usage(4096, 1_000_000, 1_000_000).unwrap();
    assert_eq!(u_clean.used_pct, 0);
    assert_eq!(u_clean.used_gb, 0.0);

    let u_full = calc_disk_usage(4096, 1_000_000, 0).unwrap();
    assert_eq!(u_full.used_pct, 100);

    let u_overflow = calc_disk_usage(4096, 100, 200).unwrap();
    assert_eq!(u_overflow.used_pct, 0);

    let u_quarter = calc_disk_usage(4096, 1_000_000, 250_000).unwrap();
    assert_eq!(u_quarter.used_pct, 75);
    let expected_total = (1_000_000.0 * 4096.0 / 1_073_741_824.0) as f32;
    assert_eq!(u_quarter.total_gb, expected_total);
    let expected_used = (750_000.0 * 4096.0 / 1_073_741_824.0) as f32;
    let expected_free = (250_000.0 * 4096.0 / 1_073_741_824.0) as f32;
    assert!((u_quarter.used_gb - expected_used).abs() < 0.001);
    assert!((u_quarter.free_gb - expected_free).abs() < 0.001);

    assert!(get_disk_usage("/").is_some());
    assert!(get_disk_usage("invalid\0path").is_none());
    assert!(get_disk_usage("/nonexistent_stage3_path").is_none());

    let d_status = check_disk_status();
    assert!(d_status.root.is_some());

    // 3. load_sys_health_at and load_sys_health
    let state_dir = rig.sub("/state");
    rig.write("/proc/sys/kernel/osrelease", b"6.12.1-arch1-1\n");
    rig.write("/etc/os-release", b"PRETTY_NAME=\"EndeavourOS Linux\"\n");

    // Missing summary.json -> default with environment fallbacks
    let h_missing = load_sys_health_at(&state_dir, rig.root());
    assert!(!h_missing.available);
    assert_eq!(
        h_missing.environment.as_ref().and_then(|e| e.os.as_deref()),
        Some("EndeavourOS Linux")
    );
    assert_eq!(
        h_missing
            .environment
            .as_ref()
            .and_then(|e| e.kernel.as_deref()),
        Some("6.12.1-arch1-1")
    );

    // Valid summary.json
    let summary_path = state_dir.join("system-health/summary.json");
    fs::create_dir_all(summary_path.parent().unwrap()).unwrap();
    let sum_json = r#"{
      "counts": { "errors": 2, "warnings": 3 },
      "checks": { "mirrors": { "arch_primary_alive": true, "arch_age_days": 1.5 } },
      "gaming": { "detected": true },
      "environment": { "os": "CustomOS", "kernel": "custom-kernel" }
    }"#;
    fs::write(&summary_path, sum_json).unwrap();

    let h_valid = load_sys_health_at(&state_dir, rig.root());
    assert!(h_valid.available);
    assert_eq!(h_valid.errors, 2);
    assert_eq!(h_valid.warnings, 3);
    assert_eq!(
        h_valid.mirrors.as_ref().and_then(|m| m.arch_primary_alive),
        Some(true)
    );
    assert_eq!(
        h_valid.mirrors.as_ref().and_then(|m| m.arch_age_days),
        Some(1.5)
    );
    assert_eq!(h_valid.gaming.as_ref().and_then(|g| g.detected), Some(true));
    assert_eq!(
        h_valid.environment.as_ref().and_then(|e| e.os.as_deref()),
        Some("CustomOS")
    );

    // Summary with unknown kernel and empty os
    let sum_fallback = r#"{
      "counts": { "errors": 0, "warnings": 0 },
      "environment": { "os": "", "kernel": "unknown" }
    }"#;
    fs::write(&summary_path, sum_fallback).unwrap();
    let h_fb = load_sys_health_at(&state_dir, rig.root());
    assert_eq!(
        h_fb.environment.as_ref().and_then(|e| e.os.as_deref()),
        Some("EndeavourOS Linux")
    );
    assert_eq!(
        h_fb.environment.as_ref().and_then(|e| e.kernel.as_deref()),
        Some("6.12.1-arch1-1")
    );

    // Corrupted summary
    fs::write(&summary_path, "not json").unwrap();
    let h_corrupt = load_sys_health_at(&state_dir, rig.root());
    assert!(!h_corrupt.available);

    // load_sys_health with overrides
    set_test_triage_overrides(Some(rig.root().to_path_buf()), Some(state_dir.clone()));
    let h_override = load_sys_health();
    assert_eq!(
        h_override
            .environment
            .as_ref()
            .and_then(|e| e.os.as_deref()),
        Some("EndeavourOS Linux")
    );
    set_test_triage_overrides(None, None);

    // 4. resolve_check_mode
    assert_eq!(resolve_check_mode(true, false), (true, false));
    assert_eq!(resolve_check_mode(true, true), (false, true));
    assert_eq!(resolve_check_mode(false, false), (false, true));
    assert_eq!(resolve_check_mode(false, true), (false, true));

    // 5. build_status_updates
    let u_b = build_status_updates(2, 5, 3);
    assert_eq!(u_b.total, 10);
    assert_eq!(u_b.core_count, 2);
    assert_eq!(u_b.regular_count, 5);
    assert_eq!(u_b.aur_count, 3);

    // 6. apply_hardware_and_gaming_gates
    let mut status = "FLIGHT_READY".to_string();
    let mut status_reasons = Vec::new();
    let mut info_reasons = Vec::new();

    let mut reboot_safety = crate::reboot_safety::RebootSafetyStatus::default();
    let mut gaming_readiness = crate::gaming::GamingReadinessStatus::default();

    // Black screen risk
    reboot_safety.black_screen_risk = true;
    reboot_safety.issues = vec!["issue1".into(), "issue2".into()];
    apply_hardware_and_gaming_gates(
        &mut status,
        &mut status_reasons,
        &mut info_reasons,
        &reboot_safety,
        &gaming_readiness,
    );
    assert_eq!(status, "ACTION_REQUIRED");
    assert_eq!(status_reasons[0], "CRITICAL REBOOT RISK: issue1");
    assert_eq!(status_reasons[1], "CRITICAL REBOOT RISK: issue2");

    // Gaming not ready
    status = "FLIGHT_READY".into();
    status_reasons.clear();
    reboot_safety.black_screen_risk = false;
    gaming_readiness.applicable = true;
    gaming_readiness.ready = false;
    gaming_readiness.warnings = vec!["wine missing".into()];
    apply_hardware_and_gaming_gates(
        &mut status,
        &mut status_reasons,
        &mut info_reasons,
        &reboot_safety,
        &gaming_readiness,
    );
    assert_eq!(status, "PRE_FLIGHT_ATTENTION");
    assert_eq!(status_reasons[0], "Gaming Readiness: wine missing");

    // ALL_CLEAR becomes PRE_FLIGHT_ATTENTION
    status = "ALL_CLEAR".into();
    status_reasons.clear();
    apply_hardware_and_gaming_gates(
        &mut status,
        &mut status_reasons,
        &mut info_reasons,
        &reboot_safety,
        &gaming_readiness,
    );
    assert_eq!(status, "PRE_FLIGHT_ATTENTION");

    // Gaming ready (applicable = true, ready = true) -> info reasons
    gaming_readiness.ready = true;
    gaming_readiness.warnings = vec!["optimum latency".into()];
    apply_hardware_and_gaming_gates(
        &mut status,
        &mut status_reasons,
        &mut info_reasons,
        &reboot_safety,
        &gaming_readiness,
    );
    assert_eq!(info_reasons[0], "Gaming Telemetry: optimum latency");

    // Gaming not applicable -> no changes
    gaming_readiness.applicable = false;
    let old_info_len = info_reasons.len();
    apply_hardware_and_gaming_gates(
        &mut status,
        &mut status_reasons,
        &mut info_reasons,
        &reboot_safety,
        &gaming_readiness,
    );
    assert_eq!(info_reasons.len(), old_info_len);

    // 7. compute_flight_status branches
    let base_failed = FailedServicesStatus::default();
    let base_disk = DiskStatus {
        root: Some(DiskUsage {
            total_gb: 100.0,
            used_gb: 50.0,
            free_gb: 50.0,
            used_pct: 50,
        }),
        home: None,
    };
    let base_orphans = OrphanStatus {
        checked: true,
        count: 0,
        tier3_count: 0,
        ..Default::default()
    };
    let base_updates = StatusUpdates::default();
    let base_security = StatusSecurity::default();
    let base_firmware = StatusFirmware {
        available: true,
        reason: "".into(),
        updates_count: 0,
        pending_devices: vec![],
        ..Default::default()
    };
    let base_firewall = StatusFirewall {
        available: true,
        active: true,
        backend: "nftables".into(),
        default_zone: None,
        details: "active filter".into(),
    };

    // Clean baseline
    let (st_clean, st_reasons, info_res) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_clean, "FLIGHT_READY");
    assert!(st_reasons.is_empty());
    assert!(info_res.iter().any(|r| r.contains("Host firewall active")));
    assert!(!info_res.iter().any(|r| r.contains("orphan")));

    // Failed system services -> ACTION_REQUIRED
    let mut f_sys = base_failed.clone();
    f_sys.system = vec!["sshd.service".into()];
    let (st_f, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &f_sys,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_f, "ACTION_REQUIRED");

    // Root disk used 92% -> ACTION_REQUIRED
    let mut d_92 = base_disk.clone();
    d_92.root.as_mut().unwrap().used_pct = 92;
    let (st_92, r_92, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &d_92,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_92, "ACTION_REQUIRED");
    assert!(r_92[0].contains("Root disk critically full (92%)"));

    // Root disk used 91% -> not ACTION_REQUIRED, but >= 80 -> PRE_FLIGHT_ATTENTION
    let mut d_91 = base_disk.clone();
    d_91.root.as_mut().unwrap().used_pct = 91;
    let (st_91, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &d_91,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_91, "PRE_FLIGHT_ATTENTION");

    // Root disk used 80% vs 79%
    let mut d_80 = base_disk.clone();
    d_80.root.as_mut().unwrap().used_pct = 80;
    let (st_80, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &d_80,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_80, "PRE_FLIGHT_ATTENTION");

    let mut d_79 = base_disk.clone();
    d_79.root.as_mut().unwrap().used_pct = 79;
    let (st_79, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &d_79,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_79, "FLIGHT_READY");

    // Failed user units -> PRE_FLIGHT_ATTENTION
    let mut f_usr = base_failed.clone();
    f_usr.user = vec!["app.service".into()];
    let (st_u, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &f_usr,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_u, "PRE_FLIGHT_ATTENTION");

    // Updates core > 0 vs regular > 0
    let mut u_core = base_updates.clone();
    u_core.core_count = 1;
    let (st_uc, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &u_core,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_uc, "PRE_FLIGHT_ATTENTION");

    let mut u_reg = base_updates.clone();
    u_reg.regular_count = 3;
    let (st_ur, r_ur, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &u_reg,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_ur, "PRE_FLIGHT_ATTENTION");
    assert_eq!(r_ur, vec!["3 package update(s) available"]);

    let mut u_aur_only = base_updates.clone();
    u_aur_only.aur_count = 2;
    let (st_ua, r_ua, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &u_aur_only,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_ua, "PRE_FLIGHT_ATTENTION");
    assert_eq!(r_ua, vec!["2 package update(s) available"]);

    let mut u_both = base_updates.clone();
    u_both.regular_count = 2;
    u_both.aur_count = 3;
    let (st_ub, r_ub, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &u_both,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_ub, "PRE_FLIGHT_ATTENTION");
    assert_eq!(r_ub, vec!["5 package update(s) available"]);

    // Reboot pending
    let (st_rb, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: true,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_rb, "PRE_FLIGHT_ATTENTION");

    // Pacnew files non-empty
    let pacnews = vec!["/etc/foo.pacnew".into()];
    let (st_pn, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &pacnews,
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_pn, "PRE_FLIGHT_ATTENTION");

    // High risk security CVEs
    let mut sec_high = base_security.clone();
    sec_high.high_risk_count = 2;
    sec_high.packages = vec!["linux".into()];
    let (st_sec, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &sec_high,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_sec, "PRE_FLIGHT_ATTENTION");

    // Orphans not checked
    let mut o_uncheck = base_orphans.clone();
    o_uncheck.checked = false;
    let (st_oc, _, _) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &o_uncheck,
        updates: &base_updates,
        security: &base_security,
        firmware: &base_firmware,
        firewall: &base_firewall,
    });
    assert_eq!(st_oc, "PRE_FLIGHT_ATTENTION");

    // Informational reasons: orphans, firmware updates, fwupd unavailable, firewall inactive
    let mut o_info = base_orphans.clone();
    o_info.count = 5;
    o_info.tier3_count = 2;
    let mut fw_up = base_firmware.clone();
    fw_up.updates_count = 1;
    fw_up.pending_devices = vec!["System BIOS".into()];
    let mut fw_skip = base_firmware.clone();
    fw_skip.available = false;
    fw_skip.reason = "skipped".into();
    let mut fw_unavail = base_firmware.clone();
    fw_unavail.available = false;
    fw_unavail.reason = "offline".into();
    let mut fw_inactive = base_firewall.clone();
    fw_inactive.active = false;

    let (_, _, info_all) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &o_info,
        updates: &base_updates,
        security: &base_security,
        firmware: &fw_up,
        firewall: &fw_inactive,
    });
    assert!(
        info_all
            .iter()
            .any(|r| r.contains("5 unrequired orphan package(s)"))
    );
    assert!(
        info_all
            .iter()
            .any(|r| r.contains("2 orphan candidate(s) are removal-sensitive"))
    );
    assert!(
        info_all
            .iter()
            .any(|r| r.contains("1 hardware firmware update(s) available via fwupd: System BIOS"))
    );
    assert!(
        info_all
            .iter()
            .any(|r| r.contains("Host firewall inactive (protected by perimeter router/LAN)"))
    );

    // fwupd skipped -> no informational reason
    let (_, _, info_skip) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &fw_skip,
        firewall: &base_firewall,
    });
    assert!(
        !info_skip
            .iter()
            .any(|r| r.contains("firmware telemetry unavailable"))
    );

    // fwupd unavailable non-skipped -> adds informational reason
    let (_, _, info_unavail) = compute_flight_status(FlightStatusInputs {
        failed: &base_failed,
        disk: &base_disk,
        reboot_pending: false,
        pacnew_files: &[],
        orphans: &base_orphans,
        updates: &base_updates,
        security: &base_security,
        firmware: &fw_unavail,
        firewall: &base_firewall,
    });
    assert!(
        info_unavail
            .iter()
            .any(|r| r.contains("Hardware firmware telemetry unavailable"))
    );

    // 8. write_status_json_atomic
    let out_json_path = rig.sub("/state/syspilot/status.json");
    let test_payload = StatusJson {
        schema_version: "1.0".into(),
        status: "FLIGHT_READY".into(),
        status_reasons: vec![],
        informational_reasons: vec![],
        timestamp: Some("1234567890".into()),
        gaming_mode: false,
        reboot_pending: false,
        updates: StatusUpdates::default(),
        failed_services: None,
        disk: None,
        pacnew: None,
        orphans: None,
        sys_health: None,
        security: None,
        firmware: None,
        firewall: None,
        app_update: None,
        reboot_safety: None,
        gaming_readiness: None,
    };
    write_status_json_atomic(&out_json_path, &test_payload).unwrap();
    let read_back = read_system_snapshot(&out_json_path).unwrap();
    assert_eq!(read_back.status, "FLIGHT_READY");
}

#[test]
fn test_stage3_models_mutants() {
    use crate::mock_rig::MockRig;
    use crate::models::*;
    use std::fs;
    use std::time::{Duration, SystemTime};

    // 1. Literal constants verification
    assert_eq!(MAX_STATUS_BYTES, 1_048_576);
    assert_eq!(SUPPORTED_SCHEMA_MAJOR, "1");
    assert_eq!(STALE_AFTER_SECS, 21_600);
    assert_eq!(CLOCK_SKEW_TOLERANCE_SECS, 60);

    // 2. valid_schema
    assert!(valid_schema("1.0"));
    assert!(valid_schema("1.1"));
    assert!(valid_schema("1.99"));
    assert!(!valid_schema("2.0"));
    assert!(!valid_schema("1"));
    assert!(!valid_schema("1.0.0"));
    assert!(!valid_schema("1.a"));
    assert!(!valid_schema(""));
    assert!(!valid_schema("1."));

    // 3. is_stale boundary tests
    assert!(is_stale(None));
    let now = SystemTime::now();
    assert!(!is_stale(Some(now)));
    assert!(!is_stale(Some(now - Duration::from_secs(21_600))));
    assert!(is_stale(Some(now - Duration::from_secs(21_601))));
    assert!(!is_stale(Some(
        SystemTime::now() + Duration::from_secs(60) + Duration::from_millis(500)
    )));
    assert!(is_stale(Some(SystemTime::now() + Duration::from_secs(65))));

    // 4. acquire_single_instance_lock
    let lock1 = acquire_single_instance_lock("test_stage3_lock1.lck");
    assert!(lock1);
    // Second lock on same file while held by process returns false
    let lock2 = acquire_single_instance_lock("test_stage3_lock1.lck");
    assert!(!lock2);

    // 5. read_system_snapshot and read_system_snapshot_or_fallback
    let rig = MockRig::headless_minimal_vm();
    let status_path = rig.sub("/state/syspilot/status.json");

    // File missing
    assert!(read_system_snapshot(&status_path).is_err());
    let fallback = read_system_snapshot_or_fallback(&status_path);
    assert_eq!(fallback.status, "UNKNOWN");
    assert!(fallback.cache_stale);
    assert!(fallback.source_error.is_some());

    // Directory path
    fs::create_dir_all(&status_path).unwrap();
    assert_eq!(
        read_system_snapshot(&status_path).unwrap_err(),
        "status is not a regular file"
    );
    fs::remove_dir(&status_path).unwrap();

    // Oversized status file boundary:
    // Exactly 1_048_576 bytes vs 1_048_577 bytes
    let valid_base = r#"{"schema_version":"1.0","status":"FLIGHT_READY","updates":{"total":0,"core_count":0,"regular_count":0,"aur_count":0}}"#;
    let pad_len_exact = (MAX_STATUS_BYTES as usize).saturating_sub(valid_base.len());
    let mut exact_buf = valid_base.as_bytes().to_vec();
    exact_buf.extend(vec![b' '; pad_len_exact]);
    assert_eq!(exact_buf.len(), MAX_STATUS_BYTES as usize);
    fs::create_dir_all(status_path.parent().unwrap()).unwrap();
    fs::write(&status_path, &exact_buf).unwrap();
    assert!(read_system_snapshot(&status_path).is_ok());

    let mut over_buf = exact_buf;
    over_buf.push(b' ');
    assert_eq!(over_buf.len(), (MAX_STATUS_BYTES + 1) as usize);
    fs::write(&status_path, &over_buf).unwrap();
    assert_eq!(
        read_system_snapshot(&status_path).unwrap_err(),
        "status exceeds 1048576 bytes"
    );

    // Invalid JSON
    fs::write(&status_path, "not-json").unwrap();
    assert!(
        read_system_snapshot(&status_path)
            .unwrap_err()
            .contains("parse status JSON")
    );

    // Unsupported schema
    let bad_schema = r#"{"schema_version":"2.0","status":"FLIGHT_READY","updates":{"total":0,"core_count":0,"regular_count":0,"aur_count":0}}"#;
    fs::write(&status_path, bad_schema).unwrap();
    assert!(
        read_system_snapshot(&status_path)
            .unwrap_err()
            .contains("unsupported schema_version")
    );

    // Unsupported status
    let bad_status = r#"{"schema_version":"1.0","status":"CUSTOM_STATUS","updates":{"total":0,"core_count":0,"regular_count":0,"aur_count":0}}"#;
    fs::write(&status_path, bad_status).unwrap();
    assert!(
        read_system_snapshot(&status_path)
            .unwrap_err()
            .contains("unsupported status")
    );

    // Update counter mismatch
    let bad_counters = r#"{"schema_version":"1.0","status":"FLIGHT_READY","updates":{"total":10,"core_count":1,"regular_count":1,"aur_count":1}}"#;
    fs::write(&status_path, bad_counters).unwrap();
    assert!(
        read_system_snapshot(&status_path)
            .unwrap_err()
            .contains("inconsistent update counters")
    );

    // Valid statuses
    for valid_st in [
        "ALL_CLEAR",
        "FLIGHT_READY",
        "REVIEW_WARNINGS",
        "PRE_FLIGHT_ATTENTION",
        "ACTION_REQUIRED",
    ] {
        let json = format!(
            r#"{{"schema_version":"1.0","status":"{valid_st}","updates":{{"total":0,"core_count":0,"regular_count":0,"aur_count":0}}}}"#
        );
        fs::write(&status_path, json).unwrap();
        let snap = read_system_snapshot(&status_path).unwrap();
        assert_eq!(snap.status, valid_st);
    }

    let env_custom = r#"{"schema_version":"1.0","status":"FLIGHT_READY","updates":{"total":0,"core_count":0,"regular_count":0,"aur_count":0},"sys_health":{"available":true,"environment":{"os":"CustomDistro","kernel":"6.99-custom"}}}"#;
    fs::write(&status_path, env_custom).unwrap();
    let snap_custom = read_system_snapshot(&status_path).unwrap();
    assert_eq!(snap_custom.os, "CustomDistro");
    assert_eq!(snap_custom.kernel, "6.99-custom");

    let env_empty = r#"{"schema_version":"1.0","status":"FLIGHT_READY","updates":{"total":0,"core_count":0,"regular_count":0,"aur_count":0},"sys_health":{"available":true,"environment":{"os":"","kernel":""}}}"#;
    fs::write(&status_path, env_empty).unwrap();
    let snap_empty = read_system_snapshot(&status_path).unwrap();
    assert_eq!(snap_empty.os, detect_os());
    assert_eq!(snap_empty.kernel, detect_kernel());

    let env_unk = r#"{"schema_version":"1.0","status":"FLIGHT_READY","updates":{"total":0,"core_count":0,"regular_count":0,"aur_count":0},"sys_health":{"available":true,"environment":{"os":"CustomDistro","kernel":"unknown"}}}"#;
    fs::write(&status_path, env_unk).unwrap();
    let snap_unk = read_system_snapshot(&status_path).unwrap();
    assert_eq!(snap_unk.os, "CustomDistro");
    assert_eq!(snap_unk.kernel, detect_kernel());

    // 6. detect_gpu_at
    // NVIDIA PCI display controller with procfs
    let pci_dev = rig.sub("/sys/bus/pci/devices/0000:01:00.0");
    fs::create_dir_all(&pci_dev).unwrap();
    fs::write(pci_dev.join("class"), "0x030000\n").unwrap();
    fs::write(pci_dev.join("vendor"), "0x10de\n").unwrap();
    fs::write(pci_dev.join("device"), "0x13c2\n").unwrap();

    let nv_gpu_info = rig.sub("/proc/driver/nvidia/gpus/0000:01:00.0/information");
    fs::create_dir_all(nv_gpu_info.parent().unwrap()).unwrap();
    fs::write(&nv_gpu_info, "Model:\tGeForce GTX 970\n").unwrap();
    let nv_ver = rig.sub("/proc/driver/nvidia/version");
    fs::create_dir_all(nv_ver.parent().unwrap()).unwrap();
    fs::write(
        &nv_ver,
        "NVRM version: NVIDIA UNIX x86_64 Kernel Module  580.82.07  Mon Oct 20 2026\n",
    )
    .unwrap();

    let gpu_proc = detect_gpu_at(rig.root());
    assert_eq!(gpu_proc, "GEFORCE GTX 970 • DRIVER 580.82.07");

    // NVIDIA PCI controller WITHOUT procfs (/proc/driver/nvidia removed)
    fs::remove_dir_all(rig.sub("/proc/driver/nvidia")).unwrap();
    let gpu_pci = detect_gpu_at(rig.root());
    assert!(gpu_pci.contains("NVIDIA GPU (0X10DE:0X13C2)"));

    // AMD PCI controller
    fs::write(pci_dev.join("vendor"), "0x1002\n").unwrap();
    fs::write(pci_dev.join("device"), "0x73df\n").unwrap();
    let gpu_amd = detect_gpu_at(rig.root());
    assert!(gpu_amd.contains("AMD/ATI GPU (0X1002:0X73DF)"));

    // Intel PCI controller
    fs::write(pci_dev.join("vendor"), "0x8086\n").unwrap();
    fs::write(pci_dev.join("device"), "0x9bc5\n").unwrap();
    let gpu_intel = detect_gpu_at(rig.root());
    assert!(gpu_intel.contains("INTEL GPU (0X8086:0X9BC5)"));

    // Unknown vendor PCI controller
    fs::write(pci_dev.join("vendor"), "0x1234\n").unwrap();
    fs::write(pci_dev.join("device"), "0x5678\n").unwrap();
    let gpu_unk = detect_gpu_at(rig.root());
    assert!(gpu_unk.contains("PCI DISPLAY GPU (0X1234:0X5678)"));

    // Non-display class ignored -> fallback default
    fs::write(pci_dev.join("class"), "0x020000\n").unwrap(); // Ethernet
    let gpu_default = detect_gpu_at(rig.root());
    assert_eq!(gpu_default, "DEFAULT DISPLAY ADAPTER");

    // 7. detect_os_at & detect_kernel_at
    rig.write("/etc/os-release", b"NAME=\"Arch Linux\"\n");
    assert_eq!(detect_os_at(rig.root()), "Arch Linux");
    rig.write(
        "/etc/os-release",
        b"PRETTY_NAME=\"EndeavourOS\"\nNAME=\"EndeavourOS\"\n",
    );
    assert_eq!(detect_os_at(rig.root()), "EndeavourOS");
    fs::remove_file(rig.sub("/etc/os-release")).unwrap();
    let _ = fs::remove_file(rig.sub("/usr/lib/os-release"));
    assert_eq!(detect_os_at(rig.root()), "Linux");

    rig.write("/proc/sys/kernel/osrelease", b"7.0.0-zen\n");
    assert_eq!(detect_kernel_at(rig.root()), "7.0.0-zen");
    fs::remove_file(rig.sub("/proc/sys/kernel/osrelease")).unwrap();
    assert_eq!(detect_kernel_at(rig.root()), "unknown");
    assert_eq!(detect_kernel(), detect_kernel_at(std::path::Path::new("/")));
    assert_eq!(detect_os(), detect_os_at(std::path::Path::new("/")));
    assert!(!get_status_path().as_os_str().is_empty());

    // 8. SystemSnapshot methods
    let mut snap = SystemSnapshot::default();
    assert_eq!(snap.age_label(), "unknown/clock-skew");
    snap.modified = Some(now - Duration::from_secs(30));
    assert_eq!(snap.age_label(), "30s");

    assert_eq!(snap.effective_status(), "UNKNOWN");
    snap.status = "ALL_CLEAR".into();
    snap.cache_stale = false;
    assert_eq!(snap.effective_status(), "ALL_CLEAR");
    snap.updates_total = 1;
    assert_eq!(snap.effective_status(), "PRE_FLIGHT_ATTENTION");
    snap.updates_total = 0;
    snap.errors = 1;
    assert_eq!(snap.effective_status(), "ACTION_REQUIRED");
    snap.errors = 0;
    snap.warnings = 1;
    assert_eq!(snap.effective_status(), "PRE_FLIGHT_ATTENTION");
    snap.warnings = 0;
    snap.security_high_risk = 1;
    assert_eq!(snap.effective_status(), "PRE_FLIGHT_ATTENTION");
    snap.security_high_risk = 0;

    assert!(!snap.has_data_problem());
    snap.cache_stale = true;
    assert!(snap.has_data_problem());
    snap.cache_stale = false;
    snap.source_error = Some("err".into());
    assert!(snap.has_data_problem());
}
