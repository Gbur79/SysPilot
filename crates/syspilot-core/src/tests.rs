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
