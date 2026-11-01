use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_STATUS_BYTES: u64 = 1024 * 1024;
pub const SUPPORTED_SCHEMA_MAJOR: &str = "1";
pub const STALE_AFTER_SECS: u64 = 6 * 60 * 60;
pub const CLOCK_SKEW_TOLERANCE_SECS: u64 = 60;

use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailedServicesStatus {
    #[serde(default)]
    pub system: Vec<String>,
    #[serde(default)]
    pub user: Vec<String>,
    #[serde(default = "default_true")]
    pub user_manager_available: bool,
}

const fn default_true() -> bool {
    true
}

impl Default for FailedServicesStatus {
    fn default() -> Self {
        Self {
            system: Vec::new(),
            user: Vec::new(),
            user_manager_available: true,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiskUsage {
    #[serde(default)]
    pub total_gb: f32,
    #[serde(default)]
    pub used_gb: f32,
    #[serde(default, alias = "avail_gb")]
    pub free_gb: f32,
    #[serde(default)]
    pub used_pct: u8,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiskStatus {
    #[serde(default)]
    pub root: Option<DiskUsage>,
    #[serde(default)]
    pub home: Option<DiskUsage>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct PacnewStatus {
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrphanStatus {
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub query_status: String,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub affects_flight_status: bool,
    #[serde(default)]
    pub tiering_complete: bool,
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub optional_count: usize,
    #[serde(default)]
    pub optional_packages: Vec<String>,
    #[serde(default)]
    pub candidate_count: usize,
    #[serde(default)]
    pub tier1_count: usize,
    #[serde(default)]
    pub tier1_packages: Vec<String>,
    #[serde(default)]
    pub tier2_count: usize,
    #[serde(default)]
    pub tier2_packages: Vec<String>,
    #[serde(default)]
    pub tier3_count: usize,
    #[serde(default)]
    pub tier3_packages: Vec<String>,
    #[serde(default)]
    pub tier3_tags: BTreeMap<String, String>,
    #[serde(default)]
    pub unclassified_packages: Vec<String>,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusJson {
    pub schema_version: String,
    pub status: String,
    #[serde(default)]
    pub status_reasons: Vec<String>,
    #[serde(default)]
    pub informational_reasons: Vec<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub gaming_mode: bool,
    #[serde(default)]
    pub reboot_pending: bool,
    #[serde(default)]
    pub updates: StatusUpdates,
    #[serde(default)]
    pub failed_services: Option<FailedServicesStatus>,
    #[serde(default)]
    pub disk: Option<DiskStatus>,
    #[serde(default)]
    pub pacnew: Option<PacnewStatus>,
    #[serde(default)]
    pub orphans: Option<OrphanStatus>,
    #[serde(default)]
    pub sys_health: Option<SysHealth>,
    #[serde(default)]
    pub security: Option<StatusSecurity>,
    #[serde(default)]
    pub firmware: Option<StatusFirmware>,
    #[serde(default)]
    pub firewall: Option<StatusFirewall>,
    #[serde(default)]
    pub app_update: Option<StatusAppUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StatusUpdates {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub core_count: u32,
    #[serde(default)]
    pub regular_count: u32,
    #[serde(default)]
    pub aur_count: u32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusSecurity {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub vulnerabilities_total: u32,
    #[serde(default)]
    pub high_risk_count: u32,
    #[serde(default)]
    pub packages: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct FirmwareQuirk {
    pub name: String,
    pub plugin: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusFirmware {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub updates_count: u32,
    #[serde(default)]
    pub devices_monitored: u32,
    #[serde(default)]
    pub pending_devices: Vec<String>,
    #[serde(default)]
    pub bypassed_quirks: Vec<FirmwareQuirk>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusFirewall {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub default_zone: Option<String>,
    #[serde(default)]
    pub details: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusAppUpdate {
    #[serde(default)]
    pub update_available: bool,
    #[serde(default)]
    pub current_version: String,
    #[serde(default)]
    pub latest_version: String,
    #[serde(default)]
    pub release_url: String,
    #[serde(default)]
    pub published_at: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct SysHealthMirrors {
    #[serde(default)]
    pub arch_primary_alive: Option<bool>,
    #[serde(default)]
    pub arch_age_days: Option<f64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct SysHealth {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub errors: u32,
    #[serde(default)]
    pub warnings: u32,
    #[serde(default)]
    pub environment: Option<SysHealthEnvironment>,
    #[serde(default)]
    pub gaming: Option<SysHealthGaming>,
    #[serde(default)]
    pub mirrors: Option<SysHealthMirrors>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct SysHealthEnvironment {
    pub os: Option<String>,
    pub kernel: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct SysHealthGaming {
    pub detected: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemSnapshot {
    pub status: String,
    pub errors: u32,
    pub warnings: u32,
    pub updates_total: u32,
    pub core_updates: u32,
    pub regular_updates: u32,
    pub aur_updates: u32,
    pub reboot_pending: bool,
    pub kernel: String,
    pub os: String,
    pub gaming_detected: bool,
    pub gaming_mode: bool,
    pub modified: Option<SystemTime>,
    pub cache_stale: bool,
    pub source_error: Option<String>,
    pub security_high_risk: u32,
    pub firmware_updates: u32,
    pub firewall_active: bool,
    pub app_update_available: bool,
    pub app_current_version: String,
    pub app_latest_version: String,
    pub app_release_url: String,
}

impl Default for SystemSnapshot {
    fn default() -> Self {
        Self {
            status: "UNKNOWN".into(),
            errors: 0,
            warnings: 0,
            updates_total: 0,
            core_updates: 0,
            regular_updates: 0,
            aur_updates: 0,
            reboot_pending: false,
            kernel: "unknown".into(),
            os: "Linux".into(),
            gaming_detected: false,
            gaming_mode: false,
            modified: None,
            cache_stale: true,
            source_error: None,
            security_high_risk: 0,
            firmware_updates: 0,
            firewall_active: false,
            app_update_available: false,
            app_current_version: "2.0.2".into(),
            app_latest_version: String::new(),
            app_release_url: String::new(),
        }
    }
}

impl SystemSnapshot {
    pub fn age_seconds(&self) -> Option<u64> {
        let time = self.modified?;
        let now = SystemTime::now();
        match now.duration_since(time) {
            Ok(duration) => Some(duration.as_secs()),
            Err(err) => {
                if err.duration().as_secs() <= CLOCK_SKEW_TOLERANCE_SECS {
                    Some(0)
                } else {
                    None
                }
            }
        }
    }

    pub fn age_label(&self) -> String {
        match self.age_seconds() {
            Some(seconds) => format!("{seconds}s"),
            None => "unknown/clock-skew".into(),
        }
    }

    pub fn effective_status(&self) -> &str {
        if self.status == "ACTION_REQUIRED" || self.errors > 0 {
            "ACTION_REQUIRED"
        } else if matches!(
            self.status.as_str(),
            "REVIEW_WARNINGS" | "PRE_FLIGHT_ATTENTION"
        ) || self.warnings > 0
            || self.updates_total > 0
            || self.reboot_pending
            || self.security_high_risk > 0
        {
            "PRE_FLIGHT_ATTENTION"
        } else {
            self.status.as_str()
        }
    }

    pub fn has_data_problem(&self) -> bool {
        self.source_error.is_some() || self.cache_stale
    }
}

pub fn detect_kernel() -> String {
    if let Ok(content) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "unknown".to_string()
}

pub fn detect_os() -> String {
    for path in ["/etc/os-release", "/usr/lib/os-release"] {
        if let Ok(content) = std::fs::read_to_string(path) {
            let mut name = None;
            let mut pretty_name = None;
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(val) = trimmed.strip_prefix("PRETTY_NAME=") {
                    pretty_name = Some(val.trim_matches('"').trim_matches('\'').to_string());
                } else if let Some(val) = trimmed.strip_prefix("NAME=") {
                    name = Some(val.trim_matches('"').trim_matches('\'').to_string());
                }
            }
            if let Some(p) = pretty_name {
                if !p.is_empty() {
                    return p;
                }
            }
            if let Some(n) = name {
                if !n.is_empty() {
                    return n;
                }
            }
        }
    }
    "Linux".to_string()
}

pub fn acquire_single_instance_lock(lock_name: &str) -> bool {
    use std::os::unix::io::AsRawFd;

    let state_dir = state_base().join("syspilot");
    let _ = std::fs::create_dir_all(&state_dir);
    let lock_path = state_dir.join(lock_name);

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

    std::mem::forget(file);
    true
}

pub fn state_base() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
}

pub fn get_status_path() -> PathBuf {
    let path = std::env::var_os("SYSPILOT_STATUS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| state_base().join("syspilot/status.json"));

    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub fn valid_schema(schema: &str) -> bool {
    let mut components = schema.split('.');
    matches!(
        (components.next(), components.next(), components.next()),
        (Some(SUPPORTED_SCHEMA_MAJOR), Some(minor), None)
            if !minor.is_empty() && minor.bytes().all(|byte| byte.is_ascii_digit())
    )
}

pub fn is_stale(modified: Option<SystemTime>) -> bool {
    let Some(time) = modified else {
        return true;
    };
    let now = SystemTime::now();
    match now.duration_since(time) {
        Ok(age) => age.as_secs() > STALE_AFTER_SECS,
        Err(err) => {
            // now < time: Clock skew detected.
            // If future timestamp is within tolerance (<= 60s), treat as fresh.
            err.duration().as_secs() > CLOCK_SKEW_TOLERANCE_SECS
        }
    }
}

pub fn read_system_snapshot_or_fallback(path: &Path) -> SystemSnapshot {
    match read_system_snapshot(path) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            let modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
            SystemSnapshot {
                status: "UNKNOWN".into(),
                cache_stale: is_stale(modified),
                modified,
                source_error: Some(err),
                os: detect_os(),
                kernel: detect_kernel(),
                ..Default::default()
            }
        }
    }
}

pub fn read_system_snapshot(path: &Path) -> Result<SystemSnapshot, String> {
    let file = File::open(path).map_err(|error| format!("open status: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("stat status: {error}"))?;

    if !metadata.is_file() {
        return Err("status is not a regular file".into());
    }
    if metadata.len() > MAX_STATUS_BYTES {
        return Err(format!("status exceeds {MAX_STATUS_BYTES} bytes"));
    }

    let mut bytes = Vec::with_capacity(metadata.len().min(MAX_STATUS_BYTES) as usize);
    file.take(MAX_STATUS_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read status: {error}"))?;
    if bytes.len() as u64 > MAX_STATUS_BYTES {
        return Err(format!("status exceeds {MAX_STATUS_BYTES} bytes"));
    }

    let parsed: StatusJson =
        serde_json::from_slice(&bytes).map_err(|error| format!("parse status JSON: {error}"))?;

    if !valid_schema(&parsed.schema_version) {
        return Err(format!(
            "unsupported schema_version {}",
            parsed.schema_version
        ));
    }

    let status = match parsed.status.as_str() {
        "ALL_CLEAR"
        | "FLIGHT_READY"
        | "REVIEW_WARNINGS"
        | "PRE_FLIGHT_ATTENTION"
        | "ACTION_REQUIRED" => parsed.status,
        other => return Err(format!("unsupported status {other}")),
    };

    let calculated_total = parsed
        .updates
        .core_count
        .checked_add(parsed.updates.regular_count)
        .and_then(|value| value.checked_add(parsed.updates.aur_count))
        .ok_or_else(|| "update counter overflow".to_string())?;
    if calculated_total != parsed.updates.total {
        return Err(format!(
            "inconsistent update counters: total={}, components={calculated_total}",
            parsed.updates.total
        ));
    }

    let health = parsed.sys_health.unwrap_or_default();
    let (errors, warnings, environment, gaming_detected) = if health.available {
        (
            health.errors,
            health.warnings,
            health.environment,
            health
                .gaming
                .and_then(|gaming| gaming.detected)
                .unwrap_or(false),
        )
    } else {
        (0, 0, health.environment, false)
    };

    let (os, kernel) = environment.map_or_else(
        || (detect_os(), detect_kernel()),
        |environment| {
            (
                environment
                    .os
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(detect_os),
                environment
                    .kernel
                    .filter(|s| !s.is_empty() && s != "unknown")
                    .unwrap_or_else(detect_kernel),
            )
        },
    );

    let modified = metadata.modified().ok().or_else(|| {
        metadata
            .created()
            .ok()
            .filter(|time| time.duration_since(UNIX_EPOCH).is_ok())
    });

    let security = parsed.security.unwrap_or_default();
    let firmware = parsed.firmware.unwrap_or_default();
    let firewall = parsed.firewall.unwrap_or_default();
    let app_update = parsed.app_update.unwrap_or_default();

    Ok(SystemSnapshot {
        status,
        errors,
        warnings,
        updates_total: parsed.updates.total,
        core_updates: parsed.updates.core_count,
        regular_updates: parsed.updates.regular_count,
        aur_updates: parsed.updates.aur_count,
        reboot_pending: parsed.reboot_pending,
        kernel,
        os,
        gaming_detected,
        gaming_mode: parsed.gaming_mode,
        modified,
        cache_stale: is_stale(modified),
        source_error: None,
        security_high_risk: security.high_risk_count,
        firmware_updates: firmware.updates_count,
        firewall_active: firewall.active,
        app_update_available: app_update.update_available,
        app_current_version: if app_update.current_version.is_empty() {
            "2.0.2".into()
        } else {
            app_update.current_version
        },
        app_latest_version: app_update.latest_version,
        app_release_url: app_update.release_url,
    })
}
