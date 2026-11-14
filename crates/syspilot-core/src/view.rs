use crate::gaming::GamingReadinessStatus;
use crate::models::{SystemSnapshot, UpdateCheckState};
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
