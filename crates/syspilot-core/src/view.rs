use crate::gaming::GamingReadinessStatus;
use crate::models::{SysHealthMirrors, SystemSnapshot, UpdateCheckState};
use crate::reboot_safety::RebootSafetyStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowSeverity {
    Pass,
    Update,
    Warn,
    Alert,
    Info,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowDisplayState {
    pub status: String,
    pub severity: RowSeverity,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionView {
    pub visible: bool,
    pub badge_text: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardAnomaly {
    pub label: String,
    pub details: String,
    pub status: String,
    pub severity: RowSeverity,
}

fn format_remediation_label(check: &str, code: &str) -> String {
    if code.eq_ignore_ascii_case("SYS_UNCLEAN_SHUTDOWN")
        || code.eq_ignore_ascii_case("SYS_SPURIOUS_WAKEUP")
    {
        return "PREVIOUS BOOT".to_string();
    }
    let raw = if !check.is_empty() {
        check
    } else if !code.is_empty() {
        code
    } else {
        "SYSTEM ALERT"
    };
    truncate_chars(&raw.replace('_', " ").to_ascii_uppercase(), 24)
}

pub fn build_dashboard_anomalies(snapshot: &SystemSnapshot) -> Vec<DashboardAnomaly> {
    let mut rows = Vec::new();
    let mut has_pacnew_remediation = false;

    for rem in &snapshot.actionable_remediations {
        let summary = rem.summary.as_deref().map(str::trim).unwrap_or("");
        let code = rem.code.as_deref().map(str::trim).unwrap_or("");
        let check = rem.check.as_deref().map(str::trim).unwrap_or("");
        if summary.is_empty() && code.is_empty() {
            continue;
        }
        if check.eq_ignore_ascii_case("pacnew") || code.to_ascii_uppercase().contains("PACNEW") {
            has_pacnew_remediation = true;
        }
        let label = format_remediation_label(check, code);
        let (status, severity) = match rem
            .severity
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_ascii_uppercase()
            .as_str()
        {
            "ERROR" | "CRITICAL" => ("ALERT", RowSeverity::Alert),
            "INFO" => ("INFO", RowSeverity::Info),
            _ => ("WARN", RowSeverity::Warn),
        };
        let details = if summary.is_empty() {
            code.to_ascii_uppercase()
        } else if code.is_empty() {
            summary.to_ascii_uppercase()
        } else {
            format!(
                "{} • {}",
                summary.to_ascii_uppercase(),
                code.to_ascii_uppercase()
            )
        };
        rows.push(DashboardAnomaly {
            label,
            details: truncate_chars(&details, 96),
            status: status.to_string(),
            severity,
        });
    }

    if !has_pacnew_remediation {
        let valid_pacnew: Vec<&str> = snapshot
            .pacnew_files
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if !valid_pacnew.is_empty() {
            let count = valid_pacnew.len();
            let first = valid_pacnew[0].to_ascii_uppercase();
            let details = if count == 1 {
                format!("1 UNMERGED .PACNEW ({first}) — RUN PACDIFF")
            } else {
                format!("{count} UNMERGED .PACNEW FILES (FIRST: {first}) — RUN PACDIFF")
            };
            rows.push(DashboardAnomaly {
                label: "PACNEW DRIFT".to_string(),
                details: truncate_chars(&details, 96),
                status: "WARN".to_string(),
                severity: RowSeverity::Warn,
            });
        }
    }

    rows
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        let prefix: String = s.chars().take(max).collect();
        format!("{prefix}…")
    }
}

pub fn format_repo_updates_state(
    count: u32,
    check_state: UpdateCheckState,
    consecutive_failures: u32,
) -> RowDisplayState {
    if check_state == UpdateCheckState::Failed && consecutive_failures >= 2 {
        let details = if count == 1 {
            "1 OFFICIAL PACKAGE PENDING (CACHED)".to_string()
        } else if count > 1 {
            format!("{count} OFFICIAL PACKAGES PENDING (CACHED)")
        } else {
            "OFFICIAL REPO CHECK FAILED (NETWORK/DB LOCK)".to_string()
        };
        RowDisplayState {
            status: "WARN".to_string(),
            severity: RowSeverity::Warn,
            details,
        }
    } else if count == 1 {
        RowDisplayState {
            status: "UPDATE".to_string(),
            severity: RowSeverity::Update,
            details: "1 OFFICIAL PACKAGE PENDING".to_string(),
        }
    } else if count > 1 {
        RowDisplayState {
            status: "UPDATE".to_string(),
            severity: RowSeverity::Update,
            details: format!("{count} OFFICIAL PACKAGES PENDING"),
        }
    } else {
        match check_state {
            UpdateCheckState::Ok => RowDisplayState {
                status: "PASS".to_string(),
                severity: RowSeverity::Pass,
                details: "0 OFFICIAL PACKAGES PENDING".to_string(),
            },
            UpdateCheckState::Failed => RowDisplayState {
                status: "INFO".to_string(),
                severity: RowSeverity::Info,
                details: "0 OFFICIAL PACKAGES PENDING (TRANSIENT RETRY)".to_string(),
            },
            UpdateCheckState::Unavailable => RowDisplayState {
                status: "INFO".to_string(),
                severity: RowSeverity::Info,
                details: "CHECKUPDATES UTILITY NOT INSTALLED".to_string(),
            },
            UpdateCheckState::Unknown => RowDisplayState {
                status: "INFO".to_string(),
                severity: RowSeverity::Info,
                details: "UPDATE CHECK PENDING".to_string(),
            },
        }
    }
}

pub fn format_standalone_updates_state(count: u32, tools: &[String]) -> RowDisplayState {
    let cleaned_tools: Vec<String> = tools
        .iter()
        .map(|t| t.trim().to_ascii_uppercase())
        .filter(|t| !t.is_empty())
        .collect();
    let effective_count = count.max(cleaned_tools.len() as u32);

    if effective_count == 0 {
        RowDisplayState {
            status: "PASS".to_string(),
            severity: RowSeverity::Pass,
            details: "ALL STANDALONE TOOLS UP TO DATE".to_string(),
        }
    } else if effective_count == 1 {
        let details = if let Some(tool) = cleaned_tools.first() {
            format!("1 UPDATE PENDING ({tool})")
        } else {
            "1 STANDALONE TOOL UPDATE PENDING".to_string()
        };
        RowDisplayState {
            status: "UPDATE".to_string(),
            severity: RowSeverity::Update,
            details,
        }
    } else {
        let details = if !cleaned_tools.is_empty() {
            let tools_str = cleaned_tools.join(", ");
            format!("{effective_count} UPDATES PENDING ({tools_str})")
        } else {
            format!("{effective_count} STANDALONE TOOL UPDATES PENDING")
        };
        RowDisplayState {
            status: "UPDATE".to_string(),
            severity: RowSeverity::Update,
            details,
        }
    }
}

fn valid_mirror_message(m: &SysHealthMirrors) -> Option<&str> {
    m.message
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("unknown"))
}

pub fn is_primary_mirror_dead(mirrors: Option<&SysHealthMirrors>) -> bool {
    let Some(m) = mirrors else {
        return false;
    };
    if m.status
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("unknown"))
        .unwrap_or(false)
    {
        return false;
    }
    if let Some(msg) = valid_mirror_message(m) {
        return msg.to_ascii_lowercase().contains("primary dead");
    }
    m.arch_primary_alive == Some(false)
        && !matches!(m.status.as_deref(), Some("PASS" | "pass" | "INFO" | "info"))
}

pub fn is_mirrorlist_warning(mirrors: Option<&SysHealthMirrors>) -> bool {
    let Some(m) = mirrors else {
        return false;
    };
    if is_primary_mirror_dead(Some(m)) {
        return true;
    }
    if m.status
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("warn"))
        .unwrap_or(false)
    {
        return true;
    }
    m.status.is_none() && m.arch_age_days.is_some_and(|d| d > 90.0)
}

pub fn mirror_upgrade_warning_message(mirrors: Option<&SysHealthMirrors>) -> Option<String> {
    if !is_primary_mirror_dead(mirrors) {
        return None;
    }
    let m = mirrors?;
    let detail = valid_mirror_message(m)
        .map(|msg| format!(" ({msg})"))
        .unwrap_or_default();
    Some(format!(
        "Primary Arch repository mirror is unreachable (primary DEAD){detail}. It is strongly recommended to refresh and rank mirrors before running Guarded System Upgrade."
    ))
}

pub fn format_mirrorlist_state(mirrors: Option<&SysHealthMirrors>) -> RowDisplayState {
    let Some(m) = mirrors else {
        return RowDisplayState {
            status: "INFO".to_string(),
            severity: RowSeverity::Info,
            details: "MIRROR TELEMETRY PENDING — RUN DIAGNOSTIC AUDIT".to_string(),
        };
    };

    let is_status_unknown = m
        .status
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("unknown"))
        .unwrap_or(false);
    let valid_msg = valid_mirror_message(m);

    if is_status_unknown
        && valid_msg.is_none()
        && m.arch_age_days.is_none()
        && m.arch_primary_alive != Some(true)
    {
        return RowDisplayState {
            status: "INFO".to_string(),
            severity: RowSeverity::Info,
            details: "MIRROR TELEMETRY PENDING — RUN DIAGNOSTIC AUDIT".to_string(),
        };
    }

    if is_primary_mirror_dead(Some(m)) {
        let details = if let Some(msg) = valid_msg {
            format!("PRIMARY MIRROR DEAD • {}", msg.to_ascii_uppercase())
        } else if let Some(days) = m.arch_age_days {
            format!("PRIMARY ARCH MIRROR DEAD ({days:.0}D) — REFRESH MIRRORS")
        } else {
            "PRIMARY ARCH MIRROR DEAD — REFRESH MIRRORS".to_string()
        };
        return RowDisplayState {
            status: "WARN".to_string(),
            severity: RowSeverity::Warn,
            details,
        };
    }

    if is_mirrorlist_warning(Some(m)) {
        let details = if let Some(msg) = valid_msg {
            msg.to_ascii_uppercase()
        } else if let Some(days) = m.arch_age_days {
            format!("ARCH MIRRORLIST STALE ({days:.0}D) — REFRESH RECOMMENDED")
        } else {
            "MIRRORLIST WARNING — RUN DIAGNOSTIC AUDIT".to_string()
        };
        return RowDisplayState {
            status: "WARN".to_string(),
            severity: RowSeverity::Warn,
            details,
        };
    }

    let is_info = m
        .status
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("info"))
        .unwrap_or(false)
        || (m.status.is_none() && m.arch_age_days.is_some_and(|d| d > 45.0));

    if is_info {
        let details = if let Some(msg) = valid_msg {
            msg.to_ascii_uppercase()
        } else if let Some(days) = m.arch_age_days {
            format!("ARCH MIRRORLIST AGING ({days:.0}D)")
        } else {
            "MIRRORLIST ADVISORY — REVIEW MIRRORS".to_string()
        };
        return RowDisplayState {
            status: "INFO".to_string(),
            severity: RowSeverity::Info,
            details,
        };
    }

    let details = if let Some(msg) = valid_msg {
        msg.to_ascii_uppercase()
    } else if let (Some(days), Some(srv)) = (m.arch_age_days, m.arch_servers) {
        format!("ARCH MIRRORS HEALTHY ({days:.0}D, {srv} SRV)")
    } else if let Some(days) = m.arch_age_days {
        format!("ARCH MIRRORS HEALTHY ({days:.0}D)")
    } else {
        "REPOSITORY MIRRORS ONLINE & VERIFIED".to_string()
    };
    RowDisplayState {
        status: "PASS".to_string(),
        severity: RowSeverity::Pass,
        details,
    }
}

pub fn compute_attention_view(
    snapshot: &SystemSnapshot,
    reboot_safety: &RebootSafetyStatus,
    gaming: &GamingReadinessStatus,
) -> AttentionView {
    let has_error =
        snapshot.source_error.is_some() || snapshot.cache_stale || snapshot.status == "DEGRADED";

    let attention_triggered = reboot_safety.black_screen_risk
        || snapshot.effective_status() == "ACTION_REQUIRED"
        || snapshot.effective_status() == "PRE_FLIGHT_ATTENTION"
        || (gaming.applicable && !gaming.ready);

    let visible = !has_error && attention_triggered;
    if !visible {
        return AttentionView {
            visible: false,
            badge_text: String::new(),
            message: String::new(),
        };
    }

    let is_red_alert =
        reboot_safety.black_screen_risk || snapshot.effective_status() == "ACTION_REQUIRED";
    let badge_text = if is_red_alert {
        "RED ALERT".to_string()
    } else {
        "ATTENTION".to_string()
    };

    let reason = if let Some(issue) = reboot_safety
        .issues
        .first()
        .filter(|s| !s.trim().is_empty())
    {
        issue.clone()
    } else if let Some(first_reason) = snapshot
        .status_reasons
        .iter()
        .find(|s| !s.trim().is_empty())
    {
        first_reason.clone()
    } else if let Some(parse_err) = snapshot
        .sys_health_parse_error
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        parse_err.to_string()
    } else if snapshot.errors > 0 {
        format!(
            "sys-health reported {} error(s) — run sys-health --audit",
            snapshot.errors
        )
    } else if is_primary_mirror_dead(snapshot.mirrors.as_ref()) {
        "Primary Arch repository mirror is DEAD — refresh mirrors before upgrading".to_string()
    } else if snapshot.warnings > 0 {
        format!(
            "sys-health reported {} warning(s) — run sys-health --audit",
            snapshot.warnings
        )
    } else if snapshot.updates_check_state == UpdateCheckState::Failed
        && snapshot.updates_consecutive_failures >= 2
    {
        "Official repository update check failed repeatedly (checkupdates)".to_string()
    } else if snapshot.security_high_risk > 0 {
        format!(
            "{} high-risk security vulnerability patch(es) pending",
            snapshot.security_high_risk
        )
    } else if snapshot.reboot_pending {
        "Running kernel updated - system reboot pending".to_string()
    } else if snapshot.updates_total > 0 {
        if snapshot.core_updates > 0 {
            format!("{} core system update(s) available", snapshot.core_updates)
        } else {
            format!("{} package update(s) available", snapshot.updates_total)
        }
    } else if let Some(warn) = gaming.warnings.first().filter(|s| !s.trim().is_empty()) {
        format!("Gaming Readiness: {warn}")
    } else {
        "System requires pre-flight review — run Diagnostic Audit".to_string()
    };

    let message = truncate_chars(&reason, 120);

    AttentionView {
        visible: true,
        badge_text,
        message,
    }
}
