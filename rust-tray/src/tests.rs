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
        core_packages: vec!["linux".into(), "systemd".into()],
        last_checked: None,
    };
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
