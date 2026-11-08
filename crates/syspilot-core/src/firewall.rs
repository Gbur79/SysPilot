use crate::models::StatusFirewall;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::time::timeout;

pub fn parse_firewalld_conf_zone(conf_content: &str) -> Option<String> {
    for line in conf_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("DefaultZone=") {
            let zone = trimmed.trim_start_matches("DefaultZone=").trim();
            if !zone.is_empty() {
                return Some(zone.to_string());
            }
        }
    }
    None
}

pub async fn check_systemd_service_active(service: &str) -> bool {
    let run_cmd = async {
        let mut cmd = crate::exec::new_tokio_command("systemctl");
        cmd.args(["is-active", service])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        cmd.output().await
    };

    match timeout(Duration::from_millis(800), run_cmd).await {
        Ok(Ok(output)) => {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "active"
        }
        _ => false,
    }
}

pub async fn check_host_firewall() -> StatusFirewall {
    // 1. Probe firewalld via systemctl & local config (instant, zero D-Bus hang)
    if check_systemd_service_active("firewalld").await {
        let mut zone = "public".to_string();
        let conf_path = Path::new("/etc/firewalld/firewalld.conf");
        if conf_path.is_file() {
            if let Ok(content) = fs::read_to_string(conf_path) {
                if let Some(z) = parse_firewalld_conf_zone(&content) {
                    zone = z;
                }
            }
        }
        return StatusFirewall {
            available: true,
            active: true,
            backend: "firewalld".into(),
            default_zone: Some(zone.clone()),
            details: format!("default zone: {zone}"),
        };
    }

    // 2. Probe ufw
    if crate::updater::find_executable("ufw").is_some() && check_systemd_service_active("ufw").await
    {
        return StatusFirewall {
            available: true,
            active: true,
            backend: "ufw".into(),
            default_zone: None,
            details: "active".into(),
        };
    }

    // 3. Probe nftables
    if check_systemd_service_active("nftables").await {
        return StatusFirewall {
            available: true,
            active: true,
            backend: "nftables".into(),
            default_zone: None,
            details: "active".into(),
        };
    }

    StatusFirewall {
        available: true,
        active: false,
        backend: "none".into(),
        default_zone: None,
        details: "protected by perimeter router/LAN".into(),
    }
}
