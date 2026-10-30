use crate::firewall::parse_firewalld_conf_zone;
use crate::firmware::parse_fwupd_devices;
use crate::security::{check_security_vulnerabilities, parse_arch_audit_json};

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
