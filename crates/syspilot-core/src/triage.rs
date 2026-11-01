use crate::firewall::check_host_firewall;
use crate::firmware::check_firmware_updates;
use crate::models::*;
use crate::orphans::check_orphan_packages;
use crate::security::check_security_vulnerabilities;
use crate::services::check_failed_services;
use crate::updater::{check_aur_updates, check_official_updates};
use std::ffi::CString;
use std::fs::{self, File};
use std::io::Write;
use std::mem::MaybeUninit;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use tokio::time::timeout;

pub async fn check_pacnew_files() -> Vec<String> {
    let run_cmd = async {
        let mut cmd = Command::new("find");
        cmd.args(["/etc", "-maxdepth", "4", "-name", "*.pacnew"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        cmd.output().await
    };

    match timeout(Duration::from_millis(1500), run_cmd).await {
        Ok(Ok(output)) if output.status.success() => {
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            stdout_str
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        }
        _ => Vec::new(),
    }
}

pub fn get_disk_usage(path: &str) -> Option<DiskUsage> {
    let c_path = CString::new(path).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();

    let res = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if res != 0 {
        return None;
    }

    let stat = unsafe { stat.assume_init() };
    let block_size = stat.f_frsize as f64;
    let total_bytes = stat.f_blocks as f64 * block_size;
    let avail_bytes = stat.f_bavail as f64 * block_size;

    if total_bytes <= 0.0 {
        return None;
    }

    let used_bytes = total_bytes - avail_bytes;
    let used_pct = ((used_bytes / total_bytes) * 100.0).clamp(0.0, 100.0) as u8;
    let total_gb = (total_bytes / (1024.0 * 1024.0 * 1024.0)) as f32;
    let used_gb = (used_bytes / (1024.0 * 1024.0 * 1024.0)) as f32;
    let free_gb = (avail_bytes / (1024.0 * 1024.0 * 1024.0)) as f32;

    Some(DiskUsage {
        total_gb,
        used_gb,
        free_gb,
        used_pct,
    })
}

pub fn check_disk_status() -> DiskStatus {
    let root = get_disk_usage("/");
    let home = get_disk_usage("/home");
    DiskStatus { root, home }
}

#[derive(Debug, Clone)]
pub struct FlightStatusInputs<'a> {
    pub failed: &'a FailedServicesStatus,
    pub disk: &'a DiskStatus,
    pub reboot_pending: bool,
    pub pacnew_files: &'a [String],
    pub orphans: &'a OrphanStatus,
    pub updates: &'a StatusUpdates,
    pub security: &'a StatusSecurity,
    pub firmware: &'a StatusFirmware,
    pub firewall: &'a StatusFirewall,
}

pub fn compute_flight_status(inputs: FlightStatusInputs) -> (String, Vec<String>, Vec<String>) {
    let mut status = "FLIGHT_READY".to_string();
    let mut status_reasons = Vec::new();
    let mut informational_reasons = Vec::new();

    let has_failed_system = !inputs.failed.system.is_empty();
    let has_failed_user = !inputs.failed.user.is_empty();
    let root_used = inputs.disk.root.as_ref().map(|d| d.used_pct).unwrap_or(0);

    if has_failed_system {
        status = "ACTION_REQUIRED".into();
        status_reasons.push(format!(
            "Failed systemd system services: {}",
            inputs.failed.system.join(", ")
        ));
    }

    if root_used >= 92 {
        status = "ACTION_REQUIRED".into();
        status_reasons.push(format!("Root disk critically full ({root_used}%)"));
    }

    if status != "ACTION_REQUIRED" {
        if has_failed_user {
            status = "PRE_FLIGHT_ATTENTION".into();
            status_reasons.push(format!(
                "Failed user units: {}",
                inputs.failed.user.join(", ")
            ));
        }
        if inputs.updates.core_count > 0 {
            status = "PRE_FLIGHT_ATTENTION".into();
            status_reasons.push(format!(
                "{} core system update(s) available",
                inputs.updates.core_count
            ));
        } else if inputs.updates.regular_count > 0 || inputs.updates.aur_count > 0 {
            status = "PRE_FLIGHT_ATTENTION".into();
            let sum = inputs.updates.regular_count + inputs.updates.aur_count;
            status_reasons.push(format!("{sum} package update(s) available"));
        }
        if inputs.reboot_pending {
            status = "PRE_FLIGHT_ATTENTION".into();
            status_reasons.push("Running kernel updated - system reboot pending".into());
        }
        if !inputs.pacnew_files.is_empty() {
            status = "PRE_FLIGHT_ATTENTION".into();
            status_reasons.push(format!(
                "{} .pacnew configuration file(s) require review",
                inputs.pacnew_files.len()
            ));
        }
        if root_used >= 80 {
            status = "PRE_FLIGHT_ATTENTION".into();
            status_reasons.push(format!("Root disk usage elevated ({root_used}%)"));
        }
        if inputs.security.high_risk_count > 0 {
            status = "PRE_FLIGHT_ATTENTION".into();
            let pkgs = inputs.security.packages.join(", ");
            status_reasons.push(format!(
                "{} high-risk security vulnerability patch(es) pending ({pkgs})",
                inputs.security.high_risk_count
            ));
        }
        if !inputs.orphans.checked {
            status = "PRE_FLIGHT_ATTENTION".into();
            status_reasons.push(
                "Unable to query strict orphan packages; package telemetry unavailable".into(),
            );
        }
    }

    if inputs.orphans.count > 0 {
        informational_reasons.push(format!(
            "{} unrequired orphan package(s); review recommended",
            inputs.orphans.count
        ));
    }
    if inputs.orphans.tier3_count > 0 {
        informational_reasons.push(format!(
            "{} orphan candidate(s) are removal-sensitive; review before removal",
            inputs.orphans.tier3_count
        ));
    }
    if inputs.firmware.updates_count > 0 {
        let devs = inputs.firmware.pending_devices.join(", ");
        informational_reasons.push(format!(
            "{} hardware firmware update(s) available via fwupd: {devs}",
            inputs.firmware.updates_count
        ));
    } else if !inputs.firmware.available && inputs.firmware.reason != "skipped" {
        informational_reasons
            .push("Hardware firmware telemetry unavailable (fwupd inactive or missing)".into());
    }

    if inputs.firewall.active {
        informational_reasons.push(format!(
            "Host firewall active ({}: {})",
            inputs.firewall.backend, inputs.firewall.details
        ));
    } else {
        informational_reasons
            .push("Host firewall inactive (protected by perimeter router/LAN)".into());
    }

    (status, status_reasons, informational_reasons)
}

static ATOMIC_WRITE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn write_status_json_atomic(dest_path: &Path, payload: &StatusJson) -> std::io::Result<()> {
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let pid = unsafe { libc::getpid() };
    let seq = ATOMIC_WRITE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_name = dest_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("status.json");
    let tmp_path = dest_path.with_file_name(format!(".{file_name}.tmp.{pid}.{seq}.{nanos}"));

    let write_res = (|| -> std::io::Result<()> {
        let mut file = File::create(&tmp_path)?;
        let json_bytes = serde_json::to_vec_pretty(payload)?;
        file.write_all(&json_bytes)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    if let Err(e) = fs::rename(&tmp_path, dest_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    // Fsync the parent directory to guarantee atomic directory entry persistence
    if let Some(parent) = dest_path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct SummaryCounts {
    #[serde(default)]
    errors: u32,
    #[serde(default)]
    warnings: u32,
}

#[derive(Debug, serde::Deserialize)]
struct SummaryChecksMirrors {
    #[serde(default)]
    arch_primary_alive: Option<bool>,
    #[serde(default)]
    arch_age_days: Option<f64>,
}

#[derive(Debug, serde::Deserialize)]
struct SummaryChecks {
    #[serde(default)]
    mirrors: Option<SummaryChecksMirrors>,
}

#[derive(Debug, serde::Deserialize)]
struct SummaryJson {
    #[serde(default)]
    counts: Option<SummaryCounts>,
    #[serde(default)]
    checks: Option<SummaryChecks>,
    #[serde(default)]
    environment: Option<SysHealthEnvironment>,
    #[serde(default)]
    gaming: Option<SysHealthGaming>,
}

pub fn load_sys_health() -> SysHealth {
    let summary_path = state_base().join("system-health/summary.json");
    let mut health = if summary_path.is_file() {
        match std::fs::File::open(&summary_path) {
            Ok(file) => {
                let reader = std::io::BufReader::new(file);
                match serde_json::from_reader::<_, SummaryJson>(reader) {
                    Ok(summary) => {
                        let (errors, warnings) = summary
                            .counts
                            .map(|c| (c.errors, c.warnings))
                            .unwrap_or((0, 0));
                        let mirrors =
                            summary
                                .checks
                                .and_then(|c| c.mirrors)
                                .map(|m| SysHealthMirrors {
                                    arch_primary_alive: m.arch_primary_alive,
                                    arch_age_days: m.arch_age_days,
                                });
                        SysHealth {
                            available: true,
                            errors,
                            warnings,
                            environment: summary.environment,
                            gaming: summary.gaming,
                            mirrors,
                        }
                    }
                    Err(_) => SysHealth::default(),
                }
            }
            Err(_) => SysHealth::default(),
        }
    } else {
        SysHealth::default()
    };

    let mut env = health.environment.unwrap_or_default();

    if env.kernel.as_deref().unwrap_or("").is_empty() || env.kernel.as_deref() == Some("unknown") {
        env.kernel = Some(detect_kernel());
    }

    if env.os.as_deref().unwrap_or("").is_empty() {
        env.os = Some(detect_os());
    }

    health.environment = Some(env);
    health
}

pub async fn run_triage(check_pkgs: bool, gamemode_active: bool) -> StatusJson {
    let status_path = get_status_path();
    let cached = if status_path.is_file() {
        read_system_snapshot(&status_path).ok()
    } else {
        None
    };

    let effective_check_pkgs = check_pkgs && !gamemode_active;

    let failed_services = check_failed_services().await;
    let disk = check_disk_status();
    let pacnew_list = check_pacnew_files().await;
    let orphans = check_orphan_packages().await;
    let reboot_pending = crate::sentinels::check_reboot_pending();

    let updates = if effective_check_pkgs {
        let (core, regular, _pkgs) = check_official_updates().await;
        let aur = check_aur_updates().await;
        StatusUpdates {
            total: core + regular + aur,
            core_count: core,
            regular_count: regular,
            aur_count: aur,
        }
    } else if let Some(ref c) = cached {
        StatusUpdates {
            total: c.updates_total,
            core_count: c.core_updates,
            regular_count: c.regular_updates,
            aur_count: c.aur_updates,
        }
    } else {
        StatusUpdates::default()
    };

    let security = check_security_vulnerabilities(!effective_check_pkgs).await;
    let firmware = check_firmware_updates(!effective_check_pkgs).await;
    let firewall = check_host_firewall().await;

    let (status, status_reasons, informational_reasons) =
        compute_flight_status(FlightStatusInputs {
            failed: &failed_services,
            disk: &disk,
            reboot_pending,
            pacnew_files: &pacnew_list,
            orphans: &orphans,
            updates: &updates,
            security: &security,
            firmware: &firmware,
            firewall: &firewall,
        });

    let pacnew = PacnewStatus {
        count: pacnew_list.len(),
        files: pacnew_list,
    };

    let app_update = if let Some(upstream_rel) = crate::release::check_upstream_release(false).await
    {
        let is_newer = crate::release::is_newer_version(
            crate::release::CURRENT_VERSION,
            &upstream_rel.tag_name,
        );
        Some(StatusAppUpdate {
            update_available: is_newer,
            current_version: crate::release::CURRENT_VERSION.to_string(),
            latest_version: upstream_rel
                .tag_name
                .trim_start_matches(['v', 'V'])
                .to_string(),
            release_url: upstream_rel.html_url,
            published_at: upstream_rel.published_at,
        })
    } else {
        None
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into());

    let payload = StatusJson {
        schema_version: "1.0".into(),
        status,
        status_reasons,
        informational_reasons,
        timestamp: Some(now),
        gaming_mode: gamemode_active,
        reboot_pending,
        updates,
        failed_services: Some(failed_services),
        disk: Some(disk),
        pacnew: Some(pacnew),
        orphans: Some(orphans),
        sys_health: Some(load_sys_health()),
        security: Some(security),
        firmware: Some(firmware),
        firewall: Some(firewall),
        app_update,
    };

    let _ = write_status_json_atomic(&status_path, &payload);

    payload
}
