use crate::models::StatusSecurity;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::process::Stdio;
use std::time::Duration;
use tokio::time::timeout;

#[derive(Debug, Deserialize)]
struct ArchAuditItem {
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    packages: Vec<String>,
}

pub fn default_security_timeout() -> Duration {
    if let Ok(val) = std::env::var("SYSPILOT_SECURITY_TIMEOUT_MS") {
        if let Ok(ms) = val.trim().parse::<u64>() {
            if ms > 0 {
                return Duration::from_millis(ms);
            }
        }
    }
    Duration::from_millis(3500)
}

pub fn parse_arch_audit_json(json_str: &str) -> Result<(u32, u32, Vec<String>), String> {
    let items: Vec<ArchAuditItem> =
        serde_json::from_str(json_str).map_err(|e| format!("invalid arch-audit JSON: {e}"))?;

    let total = items.len() as u32;
    let mut high_risk = 0;
    let mut vuln_pkgs = BTreeSet::new();

    for item in items {
        if let Some(ref sev) = item.severity {
            let lower = sev.to_ascii_lowercase();
            if lower == "high" || lower == "critical" {
                high_risk += 1;
            }
        }
        for p in item.packages {
            vuln_pkgs.insert(p);
        }
    }

    Ok((total, high_risk, vuln_pkgs.into_iter().collect()))
}

pub async fn check_security_vulnerabilities(skip_network: bool) -> StatusSecurity {
    if skip_network {
        return StatusSecurity {
            available: false,
            tool: "arch-audit".into(),
            reason: "skipped".into(),
            vulnerabilities_total: 0,
            high_risk_count: 0,
            packages: Vec::new(),
        };
    }

    let audit_bin = match crate::updater::find_executable("arch-audit") {
        Some(bin) => bin,
        None => {
            return StatusSecurity {
                available: false,
                tool: "arch-audit".into(),
                reason: "not_installed".into(),
                vulnerabilities_total: 0,
                high_risk_count: 0,
                packages: Vec::new(),
            };
        }
    };

    let run_cmd = async {
        let mut cmd = crate::exec::new_tokio_command(audit_bin);
        cmd.args(["-u", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = cmd.spawn()?;
        child.wait_with_output().await
    };

    match timeout(default_security_timeout(), run_cmd).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let trimmed = stdout.trim();
                let (total, high_risk, pkgs) = if trimmed.is_empty() {
                    (0, 0, Vec::new())
                } else {
                    match parse_arch_audit_json(trimmed) {
                        Ok(res) => res,
                        Err(e) => {
                            return StatusSecurity {
                                available: false,
                                tool: "arch-audit".into(),
                                reason: format!("parse_error: {e}"),
                                vulnerabilities_total: 0,
                                high_risk_count: 0,
                                packages: Vec::new(),
                            };
                        }
                    }
                };

                StatusSecurity {
                    available: true,
                    tool: "arch-audit".into(),
                    reason: String::new(),
                    vulnerabilities_total: total,
                    high_risk_count: high_risk,
                    packages: pkgs,
                }
            } else {
                StatusSecurity {
                    available: false,
                    tool: "arch-audit".into(),
                    reason: format!("exit_{}", output.status.code().unwrap_or(-1)),
                    vulnerabilities_total: 0,
                    high_risk_count: 0,
                    packages: Vec::new(),
                }
            }
        }
        Ok(Err(e)) => StatusSecurity {
            available: false,
            tool: "arch-audit".into(),
            reason: format!("io_error: {e}"),
            vulnerabilities_total: 0,
            high_risk_count: 0,
            packages: Vec::new(),
        },
        Err(_) => StatusSecurity {
            available: false,
            tool: "arch-audit".into(),
            reason: "timeout".into(),
            vulnerabilities_total: 0,
            high_risk_count: 0,
            packages: Vec::new(),
        },
    }
}
