use crate::models::{FirmwareQuirk, StatusFirmware};
use crate::sys_root::sub_path;
use serde::Deserialize;
#[cfg(test)]
use std::cell::RefCell;
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::time::timeout;

#[cfg(test)]
thread_local! {
    static TEST_FWUPD_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static TEST_FWUPD_BIN: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub fn set_test_fwupd_overrides(root: Option<PathBuf>, bin: Option<Option<PathBuf>>) {
    TEST_FWUPD_ROOT.with(|r| *r.borrow_mut() = root);
    TEST_FWUPD_BIN.with(|b| *b.borrow_mut() = bin);
}

pub fn parse_timeout_ms(raw: Option<&str>, default_ms: u64) -> Duration {
    let ms = raw
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&ms| ms > 0)
        .unwrap_or(default_ms);
    Duration::from_millis(ms)
}

pub fn default_firmware_timeout() -> Duration {
    parse_timeout_ms(
        std::env::var("SYSPILOT_FIRMWARE_TIMEOUT_MS")
            .ok()
            .as_deref(),
        3500,
    )
}

pub fn is_secure_boot_enabled_at(root: &Path) -> bool {
    let sb_var = sub_path(
        root,
        "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c",
    );
    match fs::read(sb_var) {
        Ok(data) => data.get(4).copied() == Some(1),
        Err(_) => false,
    }
}

pub fn is_secure_boot_enabled() -> bool {
    #[cfg(test)]
    if let Some(root) = TEST_FWUPD_ROOT.with(|r| r.borrow().clone()) {
        return is_secure_boot_enabled_at(&root);
    }
    is_secure_boot_enabled_at(Path::new("/"))
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
        let err_lower = d
            .update_error
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();

        let has_nvram_error = ["nvram", "no space left", "insufficient space"]
            .iter()
            .any(|pat| err_lower.contains(pat));

        let is_sb_plugin =
            ["uefi_kek", "uefi_dbx", "uefi_db", "uefi_pk"].contains(&plugin.as_str());
        let is_sb_proto = std::iter::once(protocol.as_str())
            .chain(protocols.iter().map(String::as_str))
            .any(|p| p.starts_with("org.uefi.dbx"));
        let is_sb_name = ["kek", "dbx", "uefi ca", "revocation"]
            .iter()
            .any(|pat| dev_name_lower.contains(pat));

        let is_sb_cert = is_sb_plugin || is_sb_proto || is_sb_name;

        if (is_sb_cert && !secure_boot_active) || has_nvram_error {
            let reason = if has_nvram_error {
                "NVRAM write limit (insufficient NVRAM storage)".into()
            } else {
                "NVRAM write limit (Secure Boot disabled)".into()
            };
            bypassed.push(FirmwareQuirk {
                name: dev_name,
                plugin,
                reason,
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

    let fwupd_opt = {
        #[cfg(test)]
        {
            TEST_FWUPD_BIN
                .with(|b| b.borrow().clone())
                .unwrap_or_else(|| crate::updater::find_executable("fwupdmgr"))
        }
        #[cfg(not(test))]
        {
            crate::updater::find_executable("fwupdmgr")
        }
    };

    let Some(fwupd_bin) = fwupd_opt else {
        return StatusFirmware {
            available: false,
            tool: "fwupd".into(),
            reason: "not_installed".into(),
            updates_count: 0,
            devices_monitored: 0,
            pending_devices: Vec::new(),
            bypassed_quirks: Vec::new(),
        };
    };

    let run_cmd = async {
        let mut cmd = crate::exec::new_tokio_command(fwupd_bin);
        cmd.args(["get-updates", "--no-unreported-check", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = cmd.spawn()?;
        child.wait_with_output().await
    };

    match timeout(default_firmware_timeout(), run_cmd).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
            let trimmed = stdout.trim();

            if output.status.success() {
                if trimmed.is_empty() {
                    return StatusFirmware {
                        available: true,
                        tool: "fwupd".into(),
                        reason: String::new(),
                        updates_count: 0,
                        devices_monitored: 0,
                        pending_devices: Vec::new(),
                        bypassed_quirks: Vec::new(),
                    };
                }
                match serde_json::from_str::<FwupdOutput>(trimmed) {
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
            } else if output.status.code() == Some(2) || stderr.contains("no updatable devices") {
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
