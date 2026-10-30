use crate::models::{FirmwareQuirk, StatusFirmware};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub fn is_secure_boot_enabled() -> bool {
    let sb_var =
        Path::new("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c");
    if !sb_var.is_file() {
        return false;
    }
    match fs::read(sb_var) {
        Ok(data) => data.len() >= 5 && data[4] == 1,
        Err(_) => false,
    }
}

#[derive(Debug, Deserialize)]
struct FwupdOutput {
    #[serde(default, rename = "Devices")]
    devices: Vec<FwupdDevice>,
}

#[derive(Debug, Deserialize)]
pub struct FwupdDevice {
    #[serde(default, rename = "Name")]
    pub name: Option<String>,
    #[serde(default, rename = "Plugin")]
    pub plugin: Option<String>,
    #[serde(default, rename = "Protocol")]
    pub protocol: Option<String>,
    #[serde(default, rename = "Protocols")]
    pub protocols: Option<Vec<String>>,
    #[serde(default, rename = "UpdateState")]
    pub update_state: Option<i64>,
    #[serde(default, rename = "UpdateError")]
    pub update_error: Option<String>,
}

pub fn parse_fwupd_devices(
    devices: Vec<FwupdDevice>,
    secure_boot_active: bool,
) -> (u32, u32, Vec<String>, Vec<FirmwareQuirk>) {
    let devices_monitored = devices.len() as u32;
    let mut pending_names = Vec::new();
    let mut bypassed = Vec::new();

    for d in devices {
        let dev_name = d.name.unwrap_or_else(|| "Unknown".into());
        let dev_name_lower = dev_name.to_ascii_lowercase();
        let plugin = d.plugin.unwrap_or_default().to_ascii_lowercase();
        let protocol = d.protocol.unwrap_or_default().to_ascii_lowercase();
        let protocols: Vec<String> = d
            .protocols
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_ascii_lowercase())
            .collect();
        let update_err = d.update_error.unwrap_or_default().to_ascii_lowercase();

        let is_sb_cert = matches!(
            plugin.as_str(),
            "uefi_kek" | "uefi_dbx" | "uefi_db" | "uefi_pk"
        ) || protocol.starts_with("org.uefi.dbx")
            || protocols.iter().any(|p| p.starts_with("org.uefi.dbx"))
            || dev_name_lower.contains("kek")
            || dev_name_lower.contains("dbx")
            || dev_name_lower.contains("uefi ca");

        let has_nvram_error = d.update_state == Some(3)
            || [
                "efivarsfs",
                "invalid argument",
                "no space left",
                "resource busy",
            ]
            .iter()
            .any(|err| update_err.contains(err));

        if is_sb_cert && has_nvram_error && !secure_boot_active {
            bypassed.push(FirmwareQuirk {
                name: dev_name,
                plugin,
                reason: "NVRAM write limit (Secure Boot disabled)".into(),
            });
        } else {
            pending_names.push(dev_name);
        }
    }

    let updates_count = pending_names.len() as u32;
    (updates_count, devices_monitored, pending_names, bypassed)
}

pub async fn check_firmware_updates(skip: bool) -> StatusFirmware {
    if skip {
        return StatusFirmware {
            available: false,
            tool: "fwupd".into(),
            reason: "skipped".into(),
            updates_count: 0,
            devices_monitored: 0,
            pending_devices: Vec::new(),
            bypassed_quirks: Vec::new(),
        };
    }

    let fwupd_bin = match crate::updater::find_executable("fwupdmgr") {
        Some(bin) => bin,
        None => {
            return StatusFirmware {
                available: false,
                tool: "fwupd".into(),
                reason: "not_installed".into(),
                updates_count: 0,
                devices_monitored: 0,
                pending_devices: Vec::new(),
                bypassed_quirks: Vec::new(),
            };
        }
    };

    let run_cmd = async {
        Command::new(fwupd_bin)
            .args(["get-updates", "--no-unreported-check", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
    };

    match timeout(Duration::from_millis(3000), run_cmd).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
            let trimmed = stdout.trim();

            if output.status.success() && !trimmed.is_empty() {
                let parsed: Result<FwupdOutput, _> = serde_json::from_str(trimmed);
                match parsed {
                    Ok(data) => {
                        let sb_active = is_secure_boot_enabled();
                        let (updates_count, devices_monitored, pending_devices, bypassed_quirks) =
                            parse_fwupd_devices(data.devices, sb_active);

                        StatusFirmware {
                            available: true,
                            tool: "fwupd".into(),
                            reason: String::new(),
                            updates_count,
                            devices_monitored,
                            pending_devices,
                            bypassed_quirks,
                        }
                    }
                    Err(e) => StatusFirmware {
                        available: false,
                        tool: "fwupd".into(),
                        reason: format!("parse_error: {e}"),
                        updates_count: 0,
                        devices_monitored: 0,
                        pending_devices: Vec::new(),
                        bypassed_quirks: Vec::new(),
                    },
                }
            } else if output.status.code() == Some(2)
                || trimmed.is_empty()
                || stderr.contains("no updatable devices")
            {
                StatusFirmware {
                    available: true,
                    tool: "fwupd".into(),
                    reason: String::new(),
                    updates_count: 0,
                    devices_monitored: 0,
                    pending_devices: Vec::new(),
                    bypassed_quirks: Vec::new(),
                }
            } else {
                StatusFirmware {
                    available: false,
                    tool: "fwupd".into(),
                    reason: format!("exit_{}", output.status.code().unwrap_or(-1)),
                    updates_count: 0,
                    devices_monitored: 0,
                    pending_devices: Vec::new(),
                    bypassed_quirks: Vec::new(),
                }
            }
        }
        Ok(Err(e)) => StatusFirmware {
            available: false,
            tool: "fwupd".into(),
            reason: format!("io_error: {e}"),
            updates_count: 0,
            devices_monitored: 0,
            pending_devices: Vec::new(),
            bypassed_quirks: Vec::new(),
        },
        Err(_) => StatusFirmware {
            available: false,
            tool: "fwupd".into(),
            reason: "timeout".into(),
            updates_count: 0,
            devices_monitored: 0,
            pending_devices: Vec::new(),
            bypassed_quirks: Vec::new(),
        },
    }
}
