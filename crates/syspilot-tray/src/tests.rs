use crate::sentinels::*;
use crate::state::*;
use crate::updater::*;
use std::fs::{self, File};
use std::io::Write;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn make_status_json(
    schema: &str,
    status: &str,
    total: u32,
    core: u32,
    reg: u32,
    aur: u32,
) -> String {
    format!(
        r#"{{
  "schema_version": "{}",
  "timestamp": "2026-10-19T18:21:48.557269+01:00",
  "status": "{}",
  "gaming_mode": false,
  "reboot_pending": false,
  "updates": {{
    "total": {},
    "core_count": {},
    "regular_count": {},
    "aur_count": {}
  }},
  "sys_health": {{
    "available": true,
    "status": "ALL_CLEAR",
    "errors": 0,
    "warnings": 0,
    "environment": {{
      "os": "EndeavourOS",
      "kernel": "7.2.11-zen1-1-zen"
    }}
  }}
}}"#,
        schema, status, total, core, reg, aur
    )
}

struct TempTestDir {
    path: std::path::PathBuf,
}

impl TempTestDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("syspilot-test-{}", nanos));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_valid_status_with_updates() {
    let dir = TempTestDir::new();
    let path = dir.path.join("status.json");
    let content = make_status_json("1.0", "PRE_FLIGHT_ATTENTION", 69, 1, 68, 0);
    File::create(&path)
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();

    let snapshot = read_system_snapshot(&path).unwrap();
    assert_eq!(snapshot.status, "PRE_FLIGHT_ATTENTION");
    assert_eq!(snapshot.effective_status(), "PRE_FLIGHT_ATTENTION");
    assert_eq!(snapshot.updates_total, 69);
    assert_eq!(snapshot.core_updates, 1);
    assert_eq!(snapshot.regular_updates, 68);
    assert_eq!(snapshot.aur_updates, 0);
    assert_eq!(snapshot.kernel, "7.2.11-zen1-1-zen");
    assert_eq!(snapshot.os, "EndeavourOS");
    assert!(!snapshot.cache_stale);
    assert!(!snapshot.has_data_problem());
}

#[test]
fn test_update_counter_mismatch() {
    let dir = TempTestDir::new();
    let path = dir.path.join("status.json");
    // total 70, but components sum to 69
    let content = make_status_json("1.0", "PRE_FLIGHT_ATTENTION", 70, 1, 68, 0);
    File::create(&path)
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();

    let err = read_system_snapshot(&path).unwrap_err();
    assert!(err.contains("inconsistent update counters"));
}

#[test]
fn test_unsupported_schema() {
    let dir = TempTestDir::new();
    let path = dir.path.join("status.json");
    let content = make_status_json("2.0", "ALL_CLEAR", 0, 0, 0, 0);
    File::create(&path)
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();

    let err = read_system_snapshot(&path).unwrap_err();
    assert!(err.contains("unsupported schema_version"));
}

#[test]
fn test_updates_override_contradictory_flight_ready() {
    let mut snapshot = SystemSnapshot {
        status: "FLIGHT_READY".into(),
        updates_total: 5,
        ..Default::default()
    };
    assert_eq!(snapshot.effective_status(), "PRE_FLIGHT_ATTENTION");

    snapshot.updates_total = 0;
    snapshot.reboot_pending = true;
    assert_eq!(snapshot.effective_status(), "PRE_FLIGHT_ATTENTION");

    snapshot.reboot_pending = false;
    snapshot.errors = 1;
    assert_eq!(snapshot.effective_status(), "ACTION_REQUIRED");
}

#[test]
fn test_stale_cache_detection() {
    let snapshot = SystemSnapshot {
        status: "ALL_CLEAR".into(),
        modified: Some(SystemTime::now() - Duration::from_secs(7 * 3600)),
        cache_stale: true,
        ..Default::default()
    };
    assert!(snapshot.age_seconds().unwrap() >= 7 * 3600);
    assert!(snapshot.has_data_problem());
}

#[test]
fn test_is_core_package() {
    assert!(is_core_package("linux"));
    assert!(is_core_package("linux-zen"));
    assert!(is_core_package("linux-lts"));
    assert!(is_core_package("linux-headers"));
    assert!(is_core_package("systemd"));
    assert!(is_core_package("systemd-libs"));
    assert!(is_core_package("nvidia-580xx-dkms"));
    assert!(is_core_package("grub"));
    assert!(is_core_package("dracut"));
    assert!(is_core_package("glibc"));
    assert!(is_core_package("pipewire"));
    assert!(is_core_package("wireplumber"));

    assert!(!is_core_package("htop"));
    assert!(!is_core_package("firefox"));
    assert!(!is_core_package("neovim"));
    assert!(!is_core_package("git"));
    assert!(!is_core_package("alacritty"));
}

#[test]
fn test_parse_checkupdates_stdout() {
    let output = b"linux-zen 7.2.11.zen1-1 -> 7.2.12.zen1-1\nsystemd 255.4-1 -> 255.5-1\nhtop 3.3.0-1 -> 3.3.1-1\nfirefox 125.0-1 -> 125.0.1-1\n";
    let (core_count, regular_count, core_pkgs) = parse_checkupdates_stdout(output);

    assert_eq!(core_count, 2);
    assert_eq!(regular_count, 2);
    assert_eq!(core_pkgs.len(), 2);
    assert!(core_pkgs[0].starts_with("linux-zen"));
    assert!(core_pkgs[1].starts_with("systemd"));
}

#[test]
fn test_update_summary_total() {
    let summary = UpdateSummary {
        core_count: 2,
        regular_count: 10,
        aur_count: 3,
        standalone_count: 1,
        standalone_tools: vec!["goose".into()],
        core_packages: vec!["linux".into(), "systemd".into()],
        last_checked: None,
        check_state: syspilot_core::UpdateCheckState::Ok,
    };
    assert_eq!(summary.package_total(), 15);
    assert_eq!(summary.total(), 16);
}

#[test]
fn test_disk_stats_valid_and_invalid() {
    let root_stats = get_disk_stats("/");
    assert!(root_stats.is_some());
    let (pct, avail_gb) = root_stats.unwrap();
    assert!(pct <= 100);
    assert!(avail_gb > 0.0);

    let invalid_stats = get_disk_stats("/non_existent_mount_path_12345");
    assert!(invalid_stats.is_none());
}

#[test]
fn test_collect_sentinels() {
    let status = collect_sentinels();
    assert!(status.root_avail_gb > 0.0);
    assert!(status.root_used_pct <= 100);
}

#[test]
fn test_effective_updates_cache_invalidation() {
    use ksni::Tray;

    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let mut tray = crate::tray::SysPilotTray::new(
        SystemSnapshot {
            updates_total: 0,
            core_updates: 0,
            status: "ALL_CLEAR".into(),
            cache_stale: false,
            updates_check_state: syspilot_core::UpdateCheckState::Ok,
            ..Default::default()
        },
        SentinelStatus::default(),
        Some(UpdateSummary {
            regular_count: 1,
            ..Default::default()
        }),
        false,
        None,
        tx,
    );

    // 1. Snapshot says 0 updates -> effective_updates MUST return (0, 0) despite stale cache
    assert_eq!(tray.effective_updates(), (0, 0));
    assert_eq!(tray.icon_name(), "security-high");
    assert_eq!(tray.status(), ksni::Status::Active);

    // 2. Snapshot has updates and self.updates is None -> returns snapshot values
    tray.snapshot.updates_total = 3;
    tray.snapshot.core_updates = 1;
    tray.updates = None;
    assert_eq!(tray.effective_updates(), (3, 1));
    assert_eq!(tray.icon_name(), "dialog-warning");
    assert_eq!(tray.status(), ksni::Status::NeedsAttention);

    // 3. Both have updates -> self.updates takes precedence
    tray.updates = Some(UpdateSummary {
        regular_count: 2,
        core_count: 2,
        aur_count: 1,
        ..Default::default()
    });
    assert_eq!(tray.effective_updates(), (5, 2));
}

#[test]
fn test_security_cve_escalation_and_telemetry() {
    use ksni::Tray;

    let dir = TempTestDir::new();
    let path = dir.path.join("status.json");
    let json_content = r#"{
  "schema_version": "1.0",
  "status": "FLIGHT_READY",
  "gaming_mode": false,
  "reboot_pending": false,
  "updates": {
    "total": 0,
    "core_count": 0,
    "regular_count": 0,
    "aur_count": 0
  },
  "security": {
    "available": true,
    "tool": "arch-audit",
    "vulnerabilities_total": 2,
    "high_risk_count": 1,
    "packages": ["chromium"]
  },
  "firmware": {
    "available": true,
    "tool": "fwupd",
    "updates_count": 1,
    "devices_monitored": 5,
    "pending_devices": ["KEK CA"]
  },
  "firewall": {
    "available": true,
    "active": true,
    "backend": "firewalld",
    "details": "default zone: public"
  }
}"#;
    File::create(&path)
        .unwrap()
        .write_all(json_content.as_bytes())
        .unwrap();

    let snapshot = read_system_snapshot(&path).unwrap();
    assert_eq!(snapshot.security_high_risk, 1);
    assert_eq!(snapshot.firmware_updates, 1);
    assert!(snapshot.firewall_active);

    // High-risk CVE must escalate effective_status to PRE_FLIGHT_ATTENTION even if updates_total is 0
    assert_eq!(snapshot.effective_status(), "PRE_FLIGHT_ATTENTION");

    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let mut tray =
        crate::tray::SysPilotTray::new(snapshot, SentinelStatus::default(), None, false, None, tx);
    tray.snapshot.cache_stale = false;

    // High risk CVE triggers warning icon and attention status
    assert_eq!(tray.icon_name(), "dialog-warning");
    assert_eq!(tray.status(), ksni::Status::NeedsAttention);
    assert!(tray.tool_tip().title.contains("high-risk security CVE"));
}

#[test]
fn test_tray_menu_telemetry_error_rendering() {
    use ksni::Tray;

    let (exit_tx, _exit_rx) = tokio::sync::mpsc::channel(1);
    let snapshot = SystemSnapshot {
        source_error: Some("Corrupt status JSON payload".into()),
        ..Default::default()
    };

    let tray = crate::tray::SysPilotTray::new(
        snapshot,
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx,
    );

    let menu = tray.menu();
    assert!(!menu.is_empty());
    if let ksni::MenuItem::Standard(header) = &menu[0] {
        assert!(header.label.contains("Telemetry unavailable"));
        assert!(!header.enabled);
    } else {
        panic!("Expected standard item for menu header");
    }

    let tooltip = tray.tool_tip();
    assert!(tooltip.title.contains("telemetry unavailable"));
    assert_eq!(tray.status(), ksni::Status::NeedsAttention);
    assert_eq!(tray.icon_name(), "dialog-warning");
}

#[test]
fn test_tray_menu_critical_sentinel_states() {
    use ksni::Tray;

    let (exit_tx, _exit_rx) = tokio::sync::mpsc::channel(1);

    // 1. Orphaned pacman lock state
    let sentinels_lock = SentinelStatus {
        pacman_orphaned_lock: true,
        ..Default::default()
    };
    let tray_lock = crate::tray::SysPilotTray::new(
        SystemSnapshot::default(),
        sentinels_lock,
        None,
        false,
        None,
        exit_tx.clone(),
    );
    let menu_lock = tray_lock.menu();
    if let ksni::MenuItem::Standard(header) = &menu_lock[0] {
        assert!(
            header
                .label
                .starts_with("CRITICAL — Pacman database locked")
        );
        assert!(!header.enabled);
    } else {
        panic!("Expected standard item for menu header");
    }
    assert_eq!(tray_lock.status(), ksni::Status::NeedsAttention);
    assert_eq!(tray_lock.icon_name(), "dialog-error");

    // 2. Reboot pending state
    let sentinels_reboot = SentinelStatus {
        reboot_pending: true,
        ..Default::default()
    };
    let tray_reboot = crate::tray::SysPilotTray::new(
        SystemSnapshot::default(),
        sentinels_reboot,
        None,
        false,
        None,
        exit_tx,
    );
    let menu_reboot = tray_reboot.menu();
    if let ksni::MenuItem::Standard(header) = &menu_reboot[0] {
        assert!(header.label.starts_with("REBOOT REQUIRED"));
        assert!(!header.enabled);
    } else {
        panic!("Expected standard item for menu header");
    }
    assert_eq!(tray_reboot.status(), ksni::Status::NeedsAttention);
    assert_eq!(tray_reboot.icon_name(), "dialog-warning");
}

#[test]
fn test_triage_deduplication_atomic_flag() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let flag = Arc::new(AtomicBool::new(false));

    // First acquisition must succeed
    let guard1 = crate::try_acquire_triage_lock(&flag);
    assert!(guard1.is_some());
    assert!(flag.load(Ordering::Acquire));

    // Second concurrent acquisition while guard1 is held must fail (deduplicated)
    let guard2 = crate::try_acquire_triage_lock(&flag);
    assert!(guard2.is_none());
    assert!(flag.load(Ordering::Acquire));

    // Dropping guard1 releases the flag via RAII
    drop(guard1);
    assert!(!flag.load(Ordering::Acquire));

    // Subsequent acquisition succeeds
    let guard3 = crate::try_acquire_triage_lock(&flag);
    assert!(guard3.is_some());
    assert!(flag.load(Ordering::Acquire));
    drop(guard3);
    assert!(!flag.load(Ordering::Acquire));
}

#[test]
fn test_gamemode_resilience_without_dbus() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    rt.block_on(async {
        // Standalone query with None connection returns false without panic
        let active = crate::gamemode::is_gamemode_active(None).await;
        assert!(!active);

        // Persistent watcher initializes and handles absent D-Bus cleanly
        let mut watcher = crate::gamemode::GameModeWatcher::new();
        let is_active = watcher.is_active().await;
        // On test/headless CI environments, this must be false and non-panicking
        assert!(!is_active);
    });
}

#[test]
fn test_stage3_tray_state_machine_and_mutants() {
    use crate::tray::SysPilotTray;
    use ksni::{Category, MenuItem, Status, Tray};
    use syspilot_core::models::SystemSnapshot;
    use syspilot_core::sentinels::SentinelStatus;
    use syspilot_core::updater::UpdateSummary;

    let (exit_tx, mut exit_rx) = tokio::sync::mpsc::channel(1);

    let mut tray = SysPilotTray::new(
        SystemSnapshot {
            status: "ALL_CLEAR".into(),
            cache_stale: false,
            ..Default::default()
        },
        SentinelStatus::default(),
        None,
        false,
        Some(std::path::PathBuf::from("/bin/true")),
        exit_tx.clone(),
    );

    // 1. Tray identity & category
    assert_eq!(tray.id(), "syspilot-sentinel");
    assert_eq!(tray.title(), "SysPilot SRE Copilot");
    assert_eq!(tray.category(), Category::SystemServices);

    // 2. effective_updates
    // updates = None -> (snapshot.updates_total, snapshot.core_updates)
    tray.snapshot.updates_total = 5;
    tray.snapshot.core_updates = 2;
    assert_eq!(tray.effective_updates(), (5, 2));

    // updates = Some(...), but snapshot.updates_total == 0 -> (0, 0)
    tray.snapshot.updates_total = 0;
    tray.snapshot.core_updates = 0;
    tray.snapshot.updates_check_state = syspilot_core::UpdateCheckState::Ok;
    tray.updates = Some(UpdateSummary {
        core_count: 3,
        regular_count: 5,
        aur_count: 2,
        standalone_count: 0,
        standalone_tools: vec![],
        core_packages: vec![],
        last_checked: None,
        check_state: syspilot_core::UpdateCheckState::Ok,
    });
    assert_eq!(tray.effective_updates(), (0, 0));

    // updates = Some(...), snapshot.updates_total > 0 -> (u.total(), u.core_count)
    tray.snapshot.updates_total = 10;
    assert_eq!(tray.effective_updates(), (10, 3));

    // 3. status() state machine
    tray.snapshot.updates_total = 0;
    tray.updates = None;
    assert_eq!(tray.status(), Status::Active);

    // pacman_orphaned_lock -> NeedsAttention
    tray.sentinels.pacman_orphaned_lock = true;
    assert_eq!(tray.status(), Status::NeedsAttention);
    tray.sentinels.pacman_orphaned_lock = false;

    // reboot_pending -> NeedsAttention
    tray.sentinels.reboot_pending = true;
    assert_eq!(tray.status(), Status::NeedsAttention);
    tray.sentinels.reboot_pending = false;

    // has_data_problem (source_error or cache_stale) -> NeedsAttention
    tray.snapshot.cache_stale = true;
    assert_eq!(tray.status(), Status::NeedsAttention);
    tray.snapshot.cache_stale = false;

    // total_updates > 0 -> NeedsAttention
    tray.snapshot.updates_total = 1;
    assert_eq!(tray.status(), Status::NeedsAttention);
    tray.snapshot.updates_total = 0;

    // status not ALL_CLEAR/FLIGHT_READY -> NeedsAttention
    tray.snapshot.status = "PRE_FLIGHT_ATTENTION".into();
    assert_eq!(tray.status(), Status::NeedsAttention);
    tray.snapshot.status = "ACTION_REQUIRED".into();
    assert_eq!(tray.status(), Status::NeedsAttention);
    tray.snapshot.status = "FLIGHT_READY".into();
    assert_eq!(tray.status(), Status::Active);

    // 4. icon_name()
    tray.sentinels.pacman_orphaned_lock = true;
    assert_eq!(tray.icon_name(), "dialog-error");
    tray.sentinels.pacman_orphaned_lock = false;

    tray.sentinels.reboot_pending = true;
    assert_eq!(tray.icon_name(), "dialog-warning");
    tray.sentinels.reboot_pending = false;

    tray.snapshot.reboot_pending = true;
    assert_eq!(tray.icon_name(), "dialog-warning");
    tray.snapshot.reboot_pending = false;

    tray.snapshot.updates_total = 2;
    assert_eq!(tray.icon_name(), "dialog-warning");
    tray.snapshot.updates_total = 0;

    tray.snapshot.source_error = Some("error".into());
    assert_eq!(tray.icon_name(), "dialog-warning");
    tray.snapshot.source_error = None;

    tray.snapshot.status = "ACTION_REQUIRED".into();
    assert_eq!(tray.icon_name(), "dialog-error");

    tray.snapshot.status = "REVIEW_WARNINGS".into();
    assert_eq!(tray.icon_name(), "dialog-warning");

    tray.snapshot.status = "PRE_FLIGHT_ATTENTION".into();
    assert_eq!(tray.icon_name(), "dialog-warning");

    tray.snapshot.status = "FLIGHT_READY".into();
    tray.gamemode_active = true;
    assert_eq!(tray.icon_name(), "input-gaming");

    tray.gamemode_active = false;
    assert_eq!(tray.icon_name(), "security-high");

    tray.snapshot.status = "UNKNOWN_STATUS".into();
    assert_eq!(tray.icon_name(), "utilities-system-monitor");
    tray.snapshot.status = "FLIGHT_READY".into();

    // 5. tool_tip()
    tray.sentinels.pacman_orphaned_lock = true;
    assert!(tray.tool_tip().title.contains("Pacman database locked"));
    tray.sentinels.pacman_orphaned_lock = false;

    tray.sentinels.reboot_pending = true;
    assert!(tray.tool_tip().title.contains("Reboot required"));
    assert!(tray.tool_tip().description.contains("REQUIRED ⚠"));
    tray.sentinels.reboot_pending = false;

    tray.snapshot.source_error = Some("corrupted data".into());
    assert!(
        tray.tool_tip()
            .title
            .contains("telemetry unavailable: corrupted data")
    );
    tray.snapshot.source_error = None;

    tray.snapshot.cache_stale = true;
    assert!(tray.tool_tip().title.contains("telemetry stale"));
    tray.snapshot.cache_stale = false;

    tray.snapshot.status = "ACTION_REQUIRED".into();
    assert!(tray.tool_tip().title.contains("action required"));
    tray.snapshot.status = "FLIGHT_READY".into();

    tray.snapshot.security_high_risk = 3;
    assert!(
        tray.tool_tip()
            .title
            .contains("3 high-risk security CVE(s)")
    );
    tray.snapshot.security_high_risk = 0;

    tray.snapshot.updates_total = 5;
    tray.snapshot.core_updates = 2;
    assert!(
        tray.tool_tip()
            .title
            .contains("5 updates available (2 core)")
    );
    tray.snapshot.updates_total = 0;
    tray.snapshot.core_updates = 0;

    tray.gamemode_active = true;
    assert!(tray.tool_tip().title.contains("GameMode active"));
    tray.gamemode_active = false;

    assert!(tray.tool_tip().title.contains("Status: FLIGHT_READY"));
    assert!(tray.tool_tip().description.contains("clean"));

    // 6. activate debounce
    let t0 = std::time::Instant::now();
    assert!(!crate::tray::is_gui_spawn_debounced(None, t0));
    assert!(crate::tray::is_gui_spawn_debounced(Some(t0), t0));
    assert!(crate::tray::is_gui_spawn_debounced(
        Some(t0),
        t0 + std::time::Duration::from_millis(1499)
    ));
    assert!(!crate::tray::is_gui_spawn_debounced(
        Some(t0),
        t0 + std::time::Duration::from_millis(1500)
    ));

    let mut tray_no_bin = SysPilotTray::new(
        SystemSnapshot::default(),
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx.clone(),
    );
    tray_no_bin.activate(0, 0); // Logs error, does not crash
    assert!(tray_no_bin.last_gui_spawn.is_none());

    tray.last_gui_spawn = None;
    tray.activate(0, 0); // First call spawns
    assert!(tray.last_gui_spawn.is_some());
    let spawn_time = tray.last_gui_spawn.unwrap();

    tray.activate(0, 0); // Immediate second activate is debounced (< 1500ms)
    assert_eq!(tray.last_gui_spawn.unwrap(), spawn_time);

    // 7. menu() items and activation closures
    tray.sentinels.pacman_orphaned_lock = true;
    let menu_orphaned = tray.menu();
    if let MenuItem::Standard(header) = &menu_orphaned[0] {
        assert!(
            header
                .label
                .starts_with("CRITICAL — Pacman database locked")
        );
    }
    tray.sentinels.pacman_orphaned_lock = false;

    tray.snapshot.source_error = Some("fail".into());
    let menu_err = tray.menu();
    if let MenuItem::Standard(header) = &menu_err[0] {
        assert!(header.label.starts_with("Telemetry unavailable"));
    }
    tray.snapshot.source_error = None;

    tray.snapshot.cache_stale = true;
    let menu_stale = tray.menu();
    if let MenuItem::Standard(header) = &menu_stale[0] {
        assert!(header.label.starts_with("Telemetry stale"));
    }
    tray.snapshot.cache_stale = false;

    tray.snapshot.updates_total = 4;
    tray.snapshot.core_updates = 1;
    let menu_updates = tray.menu();
    if let MenuItem::Standard(header) = &menu_updates[0] {
        assert!(
            header
                .label
                .starts_with("Updates Pending — 4 packages (1 core)")
        );
    }
    tray.snapshot.updates_total = 0;
    tray.snapshot.core_updates = 0;

    tray.snapshot.status = "FLIGHT_READY".into();
    let menu_healthy = tray.menu();
    if let MenuItem::Standard(header) = &menu_healthy[0] {
        assert!(header.label.contains("System primed and healthy"));
    }

    let items = tray.menu();
    assert_eq!(items.len(), 8);

    // Test activate on action_gui (item index 2)
    if let MenuItem::Standard(ref gui_item) = items[2] {
        assert!(gui_item.label.contains("Open SysPilot Dashboard"));
        assert!(gui_item.enabled);
        (gui_item.activate)(&mut tray);
    } else {
        panic!("Expected StandardItem for gui");
    }

    // Test activate on action terminal (item index 3)
    if let MenuItem::Standard(ref audit_item) = items[3] {
        assert!(audit_item.label.contains("Full System Diagnostic"));
        assert!(audit_item.enabled);
        (audit_item.activate)(&mut tray);
    } else {
        panic!("Expected StandardItem for audit");
    }

    // Test activate on action terminal (item index 4)
    if let MenuItem::Standard(ref up_item) = items[4] {
        assert!(up_item.label.contains("Guarded System Upgrade"));
        assert!(up_item.enabled);
        (up_item.activate)(&mut tray);
    } else {
        panic!("Expected StandardItem for upgrade");
    }

    // Test activate on action terminal (item index 5)
    if let MenuItem::Standard(ref copilot_item) = items[5] {
        assert!(copilot_item.label.contains("Ask SysPilot Copilot"));
        assert!(copilot_item.enabled);
        (copilot_item.activate)(&mut tray);
    } else {
        panic!("Expected StandardItem for copilot");
    }

    // Test activate on Exit (item index 7)
    if let MenuItem::Standard(ref exit_item) = items[7] {
        assert_eq!(exit_item.label, "Exit SysPilot");
        (exit_item.activate)(&mut tray);
        assert_eq!(exit_rx.try_recv().unwrap(), ());
    } else {
        panic!("Expected StandardItem for exit");
    }

    // Repeat actions when syspilot is None (covers the else branch)
    let no_bin_items = tray_no_bin.menu();
    if let MenuItem::Standard(ref gui_item) = no_bin_items[2] {
        assert!(!gui_item.enabled);
        (gui_item.activate)(&mut tray_no_bin);
    }
    if let MenuItem::Standard(ref audit_item) = no_bin_items[3] {
        assert!(!audit_item.enabled);
        (audit_item.activate)(&mut tray_no_bin);
    }
}

#[test]
fn test_effective_updates_check_state_distinction() {
    use ksni::Tray;
    use syspilot_core::models::{SystemSnapshot, UpdateCheckState};
    use syspilot_core::updater::UpdateSummary;

    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let mut tray = crate::tray::SysPilotTray::new(
        SystemSnapshot {
            updates_total: 0,
            core_updates: 0,
            status: "ALL_CLEAR".into(),
            cache_stale: false,
            updates_check_state: UpdateCheckState::Failed,
            updates_consecutive_failures: 2,
            ..Default::default()
        },
        crate::sentinels::SentinelStatus::default(),
        Some(UpdateSummary {
            core_count: 1,
            regular_count: 2,
            aur_count: 0,
            standalone_count: 0,
            standalone_tools: vec![],
            core_packages: vec!["linux".into()],
            last_checked: None,
            check_state: UpdateCheckState::Failed,
        }),
        false,
        None,
        tx,
    );

    // When check_state is Failed (not Ok), snapshot's 0 updates does NOT override cached updates!
    assert_eq!(tray.effective_updates(), (3, 1));

    // Tooltip and menu surface failure notice
    let tt = tray.tool_tip();
    assert_eq!(tt.title, "SysPilot — Official update check failed");

    // Header in menu surfaces failure notice
    let menu = tray.menu();
    if let ksni::MenuItem::Standard(ref header) = menu[0] {
        assert_eq!(header.label, "SysPilot — Official update check failed");
    } else {
        panic!("Expected StandardItem for menu header");
    }

    // Verify (Failed, 1) does NOT show failure notice
    tray.snapshot.updates_check_state = UpdateCheckState::Failed;
    tray.snapshot.updates_consecutive_failures = 1;
    assert_ne!(
        tray.tool_tip().title,
        "SysPilot — Official update check failed"
    );
    if let ksni::MenuItem::Standard(ref header) = tray.menu()[0] {
        assert_ne!(header.label, "SysPilot — Official update check failed");
    }

    // Verify (Ok, 2) does NOT show failure notice
    tray.snapshot.updates_check_state = UpdateCheckState::Ok;
    tray.snapshot.updates_consecutive_failures = 2;
    assert_ne!(
        tray.tool_tip().title,
        "SysPilot — Official update check failed"
    );
    if let ksni::MenuItem::Standard(ref header) = tray.menu()[0] {
        assert_ne!(header.label, "SysPilot — Official update check failed");
    }

    // Now set check_state to Ok: authoritative 0 updates overrides cached updates
    tray.snapshot.updates_check_state = UpdateCheckState::Ok;
    tray.snapshot.updates_consecutive_failures = 0;
    assert_eq!(tray.effective_updates(), (0, 0));
}

#[test]
fn test_apply_tray_snapshot_update_cache_invalidation() {
    use syspilot_core::models::{SystemSnapshot, UpdateCheckState};
    use syspilot_core::updater::UpdateSummary;

    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let sample_summary = Some(UpdateSummary {
        core_count: 1,
        regular_count: 1,
        aur_count: 0,
        standalone_count: 0,
        standalone_tools: vec![],
        core_packages: vec!["linux".into()],
        last_checked: None,
        check_state: UpdateCheckState::Ok,
    });

    let mut tray = crate::tray::SysPilotTray::new(
        SystemSnapshot::default(),
        crate::sentinels::SentinelStatus::default(),
        sample_summary,
        false,
        None,
        tx,
    );

    // 1. (0, Failed) -> preserves tray.updates
    let snap_failed = SystemSnapshot {
        updates_total: 0,
        updates_check_state: UpdateCheckState::Failed,
        ..Default::default()
    };
    crate::apply_tray_snapshot_update(&mut tray, snap_failed);
    assert!(tray.updates.is_some());

    // 2. (2, Ok) -> preserves tray.updates
    let snap_ok_with_updates = SystemSnapshot {
        updates_total: 2,
        updates_check_state: UpdateCheckState::Ok,
        ..Default::default()
    };
    crate::apply_tray_snapshot_update(&mut tray, snap_ok_with_updates);
    assert!(tray.updates.is_some());

    // 3. (0, Ok) -> clears tray.updates to None
    let snap_ok_zero = SystemSnapshot {
        updates_total: 0,
        updates_check_state: UpdateCheckState::Ok,
        ..Default::default()
    };
    crate::apply_tray_snapshot_update(&mut tray, snap_ok_zero);
    assert!(tray.updates.is_none());
}

#[test]
fn test_tray_mirrors_permutations_status_icon_tooltip_menu() {
    use ksni::Tray;
    use syspilot_core::{SysHealthMirrors, SystemSnapshot};

    let (exit_tx, _exit_rx) = tokio::sync::mpsc::channel(1);

    // 1. mirrors: None (nominal)
    let snap_none = SystemSnapshot {
        status: "ALL_CLEAR".into(),
        cache_stale: false,
        ..Default::default()
    };
    let tray_none = crate::tray::SysPilotTray::new(
        snap_none,
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx.clone(),
    );
    assert_eq!(tray_none.status(), ksni::Status::Active);
    assert_eq!(tray_none.icon_name(), "security-high");
    assert!(!tray_none.tool_tip().description.contains("Mirrors:"));

    // 2. mirrors: status "unknown" + arch_primary_alive: false -> stays Active!
    let snap_unknown = SystemSnapshot {
        status: "ALL_CLEAR".into(),
        cache_stale: false,
        mirrors: Some(SysHealthMirrors {
            status: Some("unknown".into()),
            arch_primary_alive: Some(false),
            ..Default::default()
        }),
        ..Default::default()
    };
    let tray_unknown = crate::tray::SysPilotTray::new(
        snap_unknown,
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx.clone(),
    );
    assert_eq!(tray_unknown.status(), ksni::Status::Active);
    assert_eq!(tray_unknown.icon_name(), "security-high");
    assert!(
        tray_unknown
            .tool_tip()
            .description
            .contains("Mirrors: INFO (MIRROR TELEMETRY PENDING")
    );

    // 3. mirrors: healthy PASS
    let snap_healthy = SystemSnapshot {
        status: "ALL_CLEAR".into(),
        cache_stale: false,
        mirrors: Some(SysHealthMirrors {
            status: Some("PASS".into()),
            arch_primary_alive: Some(true),
            arch_age_days: Some(3.0),
            arch_servers: Some(12),
            ..Default::default()
        }),
        ..Default::default()
    };
    let tray_healthy = crate::tray::SysPilotTray::new(
        snap_healthy,
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx.clone(),
    );
    assert_eq!(tray_healthy.status(), ksni::Status::Active);
    assert_eq!(tray_healthy.icon_name(), "security-high");
    assert!(
        tray_healthy
            .tool_tip()
            .description
            .contains("Mirrors: PASS (ARCH MIRRORS HEALTHY (3D, 12 SRV))")
    );

    // 4. mirrors: primary DEAD
    let snap_dead = SystemSnapshot {
        status: "ALL_CLEAR".into(),
        cache_stale: false,
        mirrors: Some(SysHealthMirrors {
            status: Some("WARN".into()),
            message: Some("Arch mirror status: primary DEAD".into()),
            arch_primary_alive: Some(false),
            arch_age_days: Some(8.0),
            ..Default::default()
        }),
        ..Default::default()
    };
    let tray_dead = crate::tray::SysPilotTray::new(
        snap_dead,
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx,
    );
    assert_eq!(tray_dead.status(), ksni::Status::NeedsAttention);
    assert_eq!(tray_dead.icon_name(), "dialog-warning");
    let tt = tray_dead.tool_tip();
    assert_eq!(
        tt.title,
        "SysPilot — Primary Arch mirror DEAD (refresh mirrors)"
    );
    assert!(tt.description.contains("Mirrors: WARN"));
    assert!(tt.description.contains("PRIMARY MIRROR DEAD"));

    let menu = tray_dead.menu();
    assert!(!menu.is_empty());
    if let ksni::MenuItem::Standard(header) = &menu[0] {
        assert_eq!(
            header.label,
            "Primary mirror DEAD — refresh mirrors before upgrading"
        );
    } else {
        panic!("Expected standard item for menu header");
    }
}

#[test]
fn test_effective_updates_excludes_standalone_count() {
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let tray = crate::tray::SysPilotTray::new(
        SystemSnapshot {
            updates_total: 6,
            core_updates: 2,
            ..Default::default()
        },
        SentinelStatus::default(),
        Some(UpdateSummary {
            core_count: 2,
            regular_count: 3,
            aur_count: 1,
            standalone_count: 5,
            standalone_tools: vec!["goose".into(), "uv".into()],
            ..Default::default()
        }),
        false,
        None,
        tx,
    );

    // Must return (package_total, core_count) = (6, 2), NOT (11, 2)
    assert_eq!(tray.effective_updates(), (6, 2));
}

#[test]
fn test_effective_standalone_updates_matrix() {
    let (tx, _rx) = tokio::sync::mpsc::channel(1);

    // 1. Standalone count 0
    let mut tray = crate::tray::SysPilotTray::new(
        SystemSnapshot::default(),
        SentinelStatus::default(),
        Some(UpdateSummary {
            standalone_count: 0,
            standalone_tools: vec![],
            ..Default::default()
        }),
        false,
        None,
        tx.clone(),
    );
    let (count0, tools0) = tray.effective_standalone_updates();
    assert_eq!(count0, 0);
    assert!(tools0.is_empty());

    // 2. Standalone count 1
    tray.updates = Some(UpdateSummary {
        standalone_count: 1,
        standalone_tools: vec!["goose".into()],
        ..Default::default()
    });
    let (count1, tools1) = tray.effective_standalone_updates();
    assert_eq!(count1, 1);
    assert_eq!(tools1, &["goose".to_string()]);

    // 3. Standalone count N (2)
    tray.updates = Some(UpdateSummary {
        standalone_count: 2,
        standalone_tools: vec!["goose".into(), "uv".into()],
        ..Default::default()
    });
    let (count_n, tools_n) = tray.effective_standalone_updates();
    assert_eq!(count_n, 2);
    assert_eq!(tools_n, &["goose".to_string(), "uv".to_string()]);

    // 4. Fallback when updates is None
    tray.updates = None;
    tray.snapshot.standalone_updates = 3;
    tray.snapshot.standalone_tools = vec!["flatpak".into(), "goose".into(), "uv".into()];
    let (count_snap, tools_snap) = tray.effective_standalone_updates();
    assert_eq!(count_snap, 3);
    assert_eq!(
        tools_snap,
        &["flatpak".to_string(), "goose".to_string(), "uv".to_string()]
    );
}

#[test]
fn test_tool_tip_standalone_tools_line_matrix() {
    use ksni::Tray;
    let (tx, _rx) = tokio::sync::mpsc::channel(1);

    // 1. 0 standalone updates -> no "Tools:" line
    let mut tray = crate::tray::SysPilotTray::new(
        SystemSnapshot::default(),
        SentinelStatus::default(),
        Some(UpdateSummary {
            standalone_count: 0,
            standalone_tools: vec![],
            ..Default::default()
        }),
        false,
        None,
        tx.clone(),
    );
    let tip0 = tray.tool_tip();
    assert!(!tip0.description.contains("Tools:"));

    // 2. 1 standalone update -> singular "1 standalone update (goose)"
    tray.updates = Some(UpdateSummary {
        standalone_count: 1,
        standalone_tools: vec!["goose".into()],
        ..Default::default()
    });
    let tip1 = tray.tool_tip();
    assert!(
        tip1.description
            .contains("Tools: 1 standalone update (goose)")
    );

    // 3. N standalone updates -> plural "2 standalone updates (goose, uv)"
    tray.updates = Some(UpdateSummary {
        standalone_count: 2,
        standalone_tools: vec!["goose".into(), "uv".into()],
        ..Default::default()
    });
    let tip_n = tray.tool_tip();
    assert!(
        tip_n
            .description
            .contains("Tools: 2 standalone updates (goose, uv)")
    );
}

#[test]
fn test_apply_tray_snapshot_update_syncs_standalone_and_package_diff() {
    use syspilot_core::models::{SystemSnapshot, UpdateCheckState};
    use syspilot_core::updater::UpdateSummary;

    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let mut tray = crate::tray::SysPilotTray::new(
        SystemSnapshot::default(),
        SentinelStatus::default(),
        Some(UpdateSummary {
            core_count: 2,
            regular_count: 3,
            aur_count: 0,
            standalone_count: 1,
            standalone_tools: vec!["goose".into()],
            ..Default::default()
        }),
        false,
        None,
        tx,
    );

    assert_eq!(tray.updates.as_ref().unwrap().package_total(), 5);

    // Snapshot transitions to 3 packages (1 core, 1 regular, 1 aur) and 2 standalone tools
    let snap = SystemSnapshot {
        updates_total: 3,
        core_updates: 1,
        regular_updates: 1,
        aur_updates: 1,
        standalone_updates: 2,
        standalone_tools: vec!["goose".into(), "uv".into()],
        updates_check_state: UpdateCheckState::Ok,
        ..Default::default()
    };

    crate::apply_tray_snapshot_update(&mut tray, snap);

    let u = tray
        .updates
        .as_ref()
        .expect("updates cache should be updated, not None");
    assert_eq!(u.package_total(), 3);
    assert_eq!(u.core_count, 1);
    assert_eq!(u.regular_count, 1);
    assert_eq!(u.aur_count, 1);
    assert_eq!(u.standalone_count, 2);
    assert_eq!(
        u.standalone_tools,
        vec!["goose".to_string(), "uv".to_string()]
    );

    // Snapshot transitions to same total (3 -> 3) but changed component split (core 1 -> 2, regular 1 -> 0, aur 1 -> 1)
    let snap2 = SystemSnapshot {
        updates_total: 3,
        core_updates: 2,
        regular_updates: 0,
        aur_updates: 1,
        standalone_updates: 2,
        standalone_tools: vec!["goose".into(), "uv".into()],
        updates_check_state: UpdateCheckState::Ok,
        ..Default::default()
    };
    crate::apply_tray_snapshot_update(&mut tray, snap2);

    let u2 = tray
        .updates
        .as_ref()
        .expect("updates cache should be updated, not None");
    assert_eq!(u2.package_total(), 3);
    assert_eq!(u2.core_count, 2);
    assert_eq!(u2.regular_count, 0);
    assert_eq!(u2.aur_count, 1);

    // When tray.updates is None and snapshot has updates_total > 0, tray.updates stays None
    tray.updates = None;
    let snap3 = SystemSnapshot {
        updates_total: 4,
        core_updates: 1,
        regular_updates: 3,
        updates_check_state: UpdateCheckState::Ok,
        ..Default::default()
    };
    crate::apply_tray_snapshot_update(&mut tray, snap3);
    assert!(tray.updates.is_none());
    assert_eq!(tray.effective_updates(), (4, 1));
}

#[test]
fn test_relevant_event_filtering() {
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};
    use notify::{Event, EventKind};
    use std::path::PathBuf;

    let target = PathBuf::from("/tmp/syspilot-test/status.json");
    let other = PathBuf::from("/tmp/syspilot-test/other.json");

    for kind in [
        EventKind::Any,
        EventKind::Create(CreateKind::File),
        EventKind::Modify(ModifyKind::Any),
        EventKind::Remove(RemoveKind::File),
    ] {
        let ev = Event::new(kind).add_path(target.clone());
        assert!(crate::relevant(&ev, &target));
    }

    let multi = Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(other.clone())
        .add_path(target.clone());
    assert!(crate::relevant(&multi, &target));

    let access_ev = Event::new(EventKind::Access(AccessKind::Read)).add_path(target.clone());
    assert!(!crate::relevant(&access_ev, &target));

    let wrong_path = Event::new(EventKind::Create(CreateKind::File)).add_path(other);
    assert!(!crate::relevant(&wrong_path, &target));

    let empty_paths = Event::new(EventKind::Modify(ModifyKind::Any));
    assert!(!crate::relevant(&empty_paths, &target));
}

#[test]
fn test_bounded_error_truncation_and_utf8() {
    assert_eq!(crate::bounded_error("short error"), "short error");

    let long_utf8: String = std::iter::repeat_n('ą', 300).collect();
    let bounded = crate::bounded_error(&long_utf8);
    assert_eq!(bounded.chars().count(), 240);
    assert!(bounded.chars().all(|c| c == 'ą'));
}

#[test]
fn test_read_with_retry_valid_delayed_and_backoff() {
    let dir = TempTestDir::new();
    let valid_path = dir.path.join("status.json");
    let content = make_status_json("1.0", "FLIGHT_READY", 2, 1, 1, 0);
    fs::write(&valid_path, &content).unwrap();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    rt.block_on(async {
        let snap = crate::read_with_retry(valid_path.clone()).await.unwrap();
        assert_eq!(snap.status, "FLIGHT_READY");
        assert_eq!(snap.updates_total, 2);
        assert_eq!(snap.core_updates, 1);

        let missing_path = dir.path.join("missing.json");
        let start = std::time::Instant::now();
        let err = crate::read_with_retry(missing_path).await.unwrap_err();
        assert!(start.elapsed() >= Duration::from_millis(120));
        assert!(err.contains("open status"));

        let delayed_path = dir.path.join("delayed.json");
        let delayed_writer = delayed_path.clone();
        let writer_content = content.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = fs::write(&delayed_writer, writer_content);
        });
        let delayed_snap = crate::read_with_retry(delayed_path).await.unwrap();
        assert_eq!(delayed_snap.status, "FLIGHT_READY");
    });
}

#[test]
fn test_evaluate_sentinel_transition_truth_table() {
    let base_t = UNIX_EPOCH + Duration::from_secs(1_000);
    let later_t = UNIX_EPOCH + Duration::from_secs(2_000);

    let prev = SentinelStatus {
        pacman_locked: false,
        root_used_pct: 40,
        pacman_db_mtime: Some(base_t),
        summary_mtime: Some(base_t),
        ..Default::default()
    };

    assert_eq!(
        crate::evaluate_sentinel_transition(&prev, &prev),
        (false, false)
    );

    let mut disk_only = prev.clone();
    disk_only.root_used_pct = 45;
    assert_eq!(
        crate::evaluate_sentinel_transition(&prev, &disk_only),
        (true, false)
    );

    let mut locked = prev.clone();
    locked.pacman_locked = true;
    assert_eq!(
        crate::evaluate_sentinel_transition(&prev, &locked),
        (true, false)
    );
    assert_eq!(
        crate::evaluate_sentinel_transition(&locked, &locked),
        (false, false)
    );
    assert_eq!(
        crate::evaluate_sentinel_transition(&locked, &prev),
        (true, true)
    );

    let mut db_changed = prev.clone();
    db_changed.pacman_db_mtime = Some(later_t);
    assert_eq!(
        crate::evaluate_sentinel_transition(&prev, &db_changed),
        (true, true)
    );

    let mut summary_changed = prev.clone();
    summary_changed.summary_mtime = Some(later_t);
    assert_eq!(
        crate::evaluate_sentinel_transition(&prev, &summary_changed),
        (true, true)
    );
}

#[test]
fn test_try_spawn_triage_with_gamemode_dedup_and_panic_release() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    rt.block_on(async {
        let in_flight = Arc::new(AtomicBool::new(false));
        let runs = Arc::new(AtomicUsize::new(0));

        // 1. GameMode active -> rejected without acquiring lock or running
        let runs_skip = Arc::clone(&runs);
        assert!(!crate::try_spawn_triage_with(
            &in_flight,
            true,
            true,
            move |_, _| async move {
                runs_skip.fetch_add(1, Ordering::Relaxed);
            }
        ));
        assert!(!in_flight.load(Ordering::Acquire));

        // 2. Normal spawn -> acquires lock, blocks second concurrent spawn, releases on finish
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel::<()>();
        let runs_ok = Arc::clone(&runs);
        assert!(crate::try_spawn_triage_with(
            &in_flight,
            true,
            false,
            move |pkgs, gm| async move {
                assert!(pkgs);
                assert!(!gm);
                runs_ok.fetch_add(1, Ordering::Relaxed);
                let _ = ack_tx.send(());
                let _ = done_rx.await;
            }
        ));
        assert!(in_flight.load(Ordering::Acquire));

        assert!(!crate::try_spawn_triage_with(
            &in_flight,
            false,
            false,
            |_, _| async {}
        ));

        let _ = ack_rx.await;
        let _ = done_tx.send(());
        tokio::task::yield_now().await;
        for _ in 0..20 {
            if !in_flight.load(Ordering::Acquire) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(!in_flight.load(Ordering::Acquire));
        assert_eq!(runs.load(Ordering::Relaxed), 1);

        // 3. Panic in spawned worker still releases in_flight via RAII Drop guard
        assert!(crate::try_spawn_triage_with(
            &in_flight,
            false,
            false,
            |_, _| async move {
                panic!("simulated worker panic");
            }
        ));
        for _ in 0..20 {
            if !in_flight.load(Ordering::Acquire) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(!in_flight.load(Ordering::Acquire));
    });
}

#[test]
fn test_drain_fs_signals_preserves_fault() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    assert!(crate::drain_fs_signals(&mut rx).is_none());

    tx.try_send(crate::FsSignal::Dirty).unwrap();
    tx.try_send(crate::FsSignal::Dirty).unwrap();
    assert!(crate::drain_fs_signals(&mut rx).is_none());

    tx.try_send(crate::FsSignal::Dirty).unwrap();
    tx.try_send(crate::FsSignal::Fault("inotify overflow".into()))
        .unwrap();
    tx.try_send(crate::FsSignal::Dirty).unwrap();
    assert_eq!(
        crate::drain_fs_signals(&mut rx).as_deref(),
        Some("inotify overflow")
    );
}

#[test]
fn test_make_interval_delays_first_tick_and_sets_delay_behavior() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    rt.block_on(async {
        let mut interval = crate::make_interval(Duration::from_millis(40));
        assert_eq!(
            interval.missed_tick_behavior(),
            tokio::time::MissedTickBehavior::Delay
        );
        // Must NOT fire immediately at t=0
        assert!(
            tokio::time::timeout(Duration::from_millis(10), interval.tick())
                .await
                .is_err()
        );
        // Fires after period elapses
        assert!(
            tokio::time::timeout(Duration::from_millis(150), interval.tick())
                .await
                .is_ok()
        );
    });
}

#[test]
fn test_status_watcher_late_dir_inotify_and_fault_recovery() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    rt.block_on(async {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let root_sw = crate::StatusWatcher::new(std::path::PathBuf::from("/"), tx.clone());
        assert!(!root_sw.is_watching());

        let dir = TempTestDir::new();
        let late_dir = dir.path.join("late");
        let target = late_dir.join("status.json");

        let mut sw = crate::StatusWatcher::new(target.clone(), tx);
        assert!(!sw.is_watching());
        assert!(!sw.ensure_watching());

        fs::create_dir_all(&late_dir).unwrap();
        assert!(sw.ensure_watching());
        assert!(sw.is_watching());
        // Idempotent when already watching
        assert!(sw.ensure_watching());

        fs::write(late_dir.join("unrelated.txt"), "ignored").unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(rx.try_recv().is_err());

        fs::write(&target, "{}").unwrap();
        let sig = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("inotify signal timeout")
            .expect("fs channel closed");
        assert!(matches!(sig, crate::FsSignal::Dirty));

        sw.handle_fault("simulated error");
        assert!(!sw.is_watching());
        assert!(sw.ensure_watching());
        assert!(sw.is_watching());
        drop(sw);
    });
}

#[test]
fn test_dispatch_notify_result_dirty_ignore_and_fault() {
    use notify::event::CreateKind;
    use notify::{Event, EventKind};

    let target = std::path::PathBuf::from("/tmp/syspilot-dispatch/status.json");
    let other = std::path::PathBuf::from("/tmp/syspilot-dispatch/other.json");
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);

    // Unrelated file is ignored
    let other_ev = Event::new(EventKind::Create(CreateKind::File)).add_path(other);
    crate::dispatch_notify_result(Ok(other_ev), &target, &tx);
    assert!(rx.try_recv().is_err());

    // Target file produces Dirty
    let target_ev = Event::new(EventKind::Create(CreateKind::File)).add_path(target.clone());
    crate::dispatch_notify_result(Ok(target_ev), &target, &tx);
    assert!(matches!(rx.try_recv().unwrap(), crate::FsSignal::Dirty));

    // Error produces Fault
    crate::dispatch_notify_result(Err(notify::Error::generic("watch fault")), &target, &tx);
    match rx.try_recv().unwrap() {
        crate::FsSignal::Fault(msg) => assert!(msg.contains("watch fault")),
        crate::FsSignal::Dirty => panic!("expected Fault signal"),
    }

    // Full channel drops Fault with log (no panic)
    let (full_tx, _full_rx) = tokio::sync::mpsc::channel(1);
    full_tx.try_send(crate::FsSignal::Dirty).unwrap();
    crate::dispatch_notify_result(
        Err(notify::Error::generic("dropped fault")),
        &target,
        &full_tx,
    );
}

#[test]
fn test_build_initial_state_and_trigger_startup_triage() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let dir = TempTestDir::new();
    let missing_path = dir.path.join("missing.json");
    let mut err_state = crate::build_initial_state(&missing_path, SentinelStatus::default(), true);
    assert!(err_state.last_error.is_some());
    assert!(err_state.snapshot.source_error.is_some());
    assert!(err_state.gamemode_active);

    let valid_path = dir.path.join("status.json");
    fs::write(
        &valid_path,
        make_status_json("1.0", "ALL_CLEAR", 0, 0, 0, 0),
    )
    .unwrap();
    let ok_state = crate::build_initial_state(&valid_path, SentinelStatus::default(), false);
    assert!(ok_state.last_error.is_none());
    assert!(ok_state.snapshot.source_error.is_none());
    assert_eq!(ok_state.snapshot.status, "ALL_CLEAR");

    let (fs_tx, _fs_rx) = tokio::sync::mpsc::channel(4);
    let allow = Arc::new(AtomicBool::new(false));
    let allow_c = Arc::clone(&allow);
    let trigger: crate::TriageTrigger = Arc::new(move |_, _, _| allow_c.load(Ordering::Acquire));

    crate::trigger_startup_triage(&trigger, &mut err_state, &fs_tx);
    assert!(err_state.pending_pkg_check);

    allow.store(true, Ordering::Release);
    crate::trigger_startup_triage(&trigger, &mut err_state, &fs_tx);
    assert!(!err_state.pending_pkg_check);
}

#[test]
fn test_tray_anomalies_tooltip_and_menu_matrix() {
    use crate::tray::SysPilotTray;
    use ksni::Tray;
    use syspilot_core::models::{ActionableRemediation, SystemSnapshot};

    let (tx, _rx) = tokio::sync::mpsc::channel(1);

    // 1. 0 anomalies + FLIGHT_READY -> healthy message, no Alerts overflow
    let snap_ok = SystemSnapshot {
        status: "FLIGHT_READY".into(),
        cache_stale: false,
        ..Default::default()
    };
    let tray_ok = SysPilotTray::new(
        snap_ok,
        SentinelStatus::default(),
        None,
        false,
        None,
        tx.clone(),
    );
    assert_eq!(tray_ok.tool_tip().title, "SysPilot — Status: FLIGHT_READY");
    assert!(!tray_ok.tool_tip().description.contains("Alerts:"));
    if let ksni::MenuItem::Standard(header) = &tray_ok.menu()[0] {
        assert!(header.label.contains("System primed and healthy"));
    } else {
        panic!("Expected StandardItem header");
    }

    // 2. PRE_FLIGHT_ATTENTION without structured anomalies -> never claims "primed and healthy"
    let snap_attn_empty = SystemSnapshot {
        status: "PRE_FLIGHT_ATTENTION".into(),
        cache_stale: false,
        ..Default::default()
    };
    let tray_attn = SysPilotTray::new(
        snap_attn_empty,
        SentinelStatus::default(),
        None,
        false,
        None,
        tx.clone(),
    );
    if let ksni::MenuItem::Standard(header) = &tray_attn.menu()[0] {
        assert_eq!(
            header.label,
            "PRE_FLIGHT_ATTENTION — Pre-flight review recommended"
        );
    } else {
        panic!("Expected StandardItem header");
    }

    // 3. 1 anomaly (SYS_UNCLEAN_SHUTDOWN) -> surfaced in tooltip title, description, and menu header
    let snap_one = SystemSnapshot {
        status: "PRE_FLIGHT_ATTENTION".into(),
        cache_stale: false,
        actionable_remediations: vec![ActionableRemediation {
            check: Some("previous_boot".into()),
            severity: Some("WARN".into()),
            code: Some("SYS_UNCLEAN_SHUTDOWN".into()),
            summary: Some("Previous session crashed".into()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let tray_one = SysPilotTray::new(
        snap_one,
        SentinelStatus::default(),
        None,
        false,
        None,
        tx.clone(),
    );
    let tt_one = tray_one.tool_tip();
    assert_eq!(
        tt_one.title,
        "SysPilot — PRE_FLIGHT_ATTENTION (PREVIOUS BOOT)"
    );
    assert!(
        tt_one
            .description
            .contains("PREVIOUS BOOT: PREVIOUS SESSION CRASHED • SYS_UNCLEAN_SHUTDOWN")
    );
    assert!(!tt_one.description.contains("Alerts:"));
    if let ksni::MenuItem::Standard(header) = &tray_one.menu()[0] {
        assert_eq!(
            header.label,
            "PRE_FLIGHT_ATTENTION — PREVIOUS BOOT: PREVIOUS SESSION CRASHED • SYS_UNCLEAN_SHUTDOWN"
        );
    } else {
        panic!("Expected StandardItem header");
    }

    // 4. 5 anomalies (4 remediations + 1 pacnew_files) -> capped at 3 lines + "+2 more"
    let snap_five = SystemSnapshot {
        status: "PRE_FLIGHT_ATTENTION".into(),
        cache_stale: false,
        actionable_remediations: vec![
            ActionableRemediation {
                check: Some("previous_boot".into()),
                severity: Some("WARN".into()),
                code: Some("SYS_UNCLEAN_SHUTDOWN".into()),
                summary: Some("Unclean stop".into()),
                ..Default::default()
            },
            ActionableRemediation {
                check: Some("journal_rate_limit".into()),
                severity: Some("WARN".into()),
                code: Some("JOURNAL_RL".into()),
                summary: Some("Rate limit".into()),
                ..Default::default()
            },
            ActionableRemediation {
                check: Some("boot_capacity".into()),
                severity: Some("WARN".into()),
                code: Some("BOOT_LOW".into()),
                summary: Some("Low space".into()),
                ..Default::default()
            },
            ActionableRemediation {
                check: Some("thermal_zone".into()),
                severity: Some("WARN".into()),
                code: Some("TEMP_HIGH".into()),
                summary: Some("High temp".into()),
                ..Default::default()
            },
        ],
        pacnew_files: vec!["/etc/pacman.conf.pacnew".into()],
        ..Default::default()
    };
    let tray_five = SysPilotTray::new(snap_five, SentinelStatus::default(), None, false, None, tx);
    let tt_five = tray_five.tool_tip();
    assert_eq!(tt_five.title, "SysPilot — PRE_FLIGHT_ATTENTION (5 alerts)");
    assert!(
        tt_five
            .description
            .contains("PREVIOUS BOOT: UNCLEAN STOP • SYS_UNCLEAN_SHUTDOWN")
    );
    assert!(
        tt_five
            .description
            .contains("JOURNAL RATE LIMIT: RATE LIMIT • JOURNAL_RL")
    );
    assert!(
        tt_five
            .description
            .contains("BOOT CAPACITY: LOW SPACE • BOOT_LOW")
    );
    assert!(!tt_five.description.contains("THERMAL ZONE:"));
    assert!(
        tt_five
            .description
            .contains("Alerts: +2 more (open Dashboard)")
    );
    if let ksni::MenuItem::Standard(header) = &tray_five.menu()[0] {
        assert_eq!(
            header.label,
            "PRE_FLIGHT_ATTENTION — 5 exceptions (first: PREVIOUS BOOT)"
        );
    } else {
        panic!("Expected StandardItem header");
    }
}
