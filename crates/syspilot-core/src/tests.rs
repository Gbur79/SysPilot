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

    let raw = json!([
        {
            "Name": "UEFI dbx",
            "Plugin": "uefi_dbx",
            "Protocol": "org.uefi.dbx",
            "UpdateState": 3,
            "UpdateError": "efivarsfs: Error writing to file descriptor: Invalid argument"
        },
        {
            "Name": "ADATA SX8200PNP SSD",
            "Plugin": "nvme",
            "Protocol": "org.nvmexpress",
            "UpdateState": 1
        }
    ]);

    let devices = serde_json::from_value(raw).unwrap();
    // Secure Boot is DISABLED (false)
    let (updates_count, total_monitored, pending, quirks) = parse_fwupd_devices(devices, false);

    assert_eq!(total_monitored, 2);
    assert_eq!(updates_count, 1);
    assert_eq!(pending, vec!["ADATA SX8200PNP SSD"]);
    assert_eq!(quirks.len(), 1);
    assert_eq!(quirks[0].name, "UEFI dbx");
    assert_eq!(quirks[0].plugin, "uefi_dbx");
    assert_eq!(quirks[0].reason, "NVRAM write limit (Secure Boot disabled)");
}

#[test]
fn test_parse_fwupd_devices_nvram_quirk_not_bypassed_when_secure_boot_enabled() {
    use serde_json::json;

    let raw = json!([
        {
            "Name": "UEFI dbx",
            "Plugin": "uefi_dbx",
            "Protocol": "org.uefi.dbx",
            "UpdateState": 3,
            "UpdateError": "efivarsfs: Error writing to file descriptor: Invalid argument"
        }
    ]);

    let devices = serde_json::from_value(raw).unwrap();
    // Secure Boot is ENABLED (true) -> must flag as real alert, not bypass
    let (updates_count, total_monitored, pending, quirks) = parse_fwupd_devices(devices, true);

    assert_eq!(total_monitored, 1);
    assert_eq!(updates_count, 1);
    assert_eq!(pending, vec!["UEFI dbx"]);
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
    };

    write_status_json_atomic(&tmp_file, &payload).unwrap();
    assert!(tmp_file.is_file());

    let snapshot = read_system_snapshot(&tmp_file).unwrap();
    assert_eq!(snapshot.status, "FLIGHT_READY");
    assert_eq!(snapshot.updates_total, 2);
    assert_eq!(snapshot.core_updates, 1);

    let _ = std::fs::remove_dir_all(tmp_file.parent().unwrap());
}
