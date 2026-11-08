use crate::models::StatusFirewall;
use crate::sys_root::sub_path;
use std::fs;
use std::path::Path;

pub fn parse_firewalld_conf_zone(conf_content: &str) -> Option<String> {
    for line in conf_content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("DefaultZone=") {
            let zone = rest.trim();
            if !zone.is_empty() {
                return Some(zone.to_string());
            }
        }
    }
    None
}

pub fn parse_ufw_conf_enabled(conf_content: &str) -> bool {
    conf_content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.eq_ignore_ascii_case("ENABLED=yes")
            || trimmed.eq_ignore_ascii_case("ENABLED=\"yes\"")
    })
}

fn has_non_empty_file(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

pub fn check_host_firewall_at(root: &Path) -> StatusFirewall {
    // 1. firewalld: active cgroup.procs or /run/firewalld/firewalld.pid (0ms, 0 forks)
    let firewalld_active =
        has_non_empty_file(&sub_path(
            root,
            "/sys/fs/cgroup/system.slice/firewalld.service/cgroup.procs",
        )) || has_non_empty_file(&sub_path(root, "/run/firewalld/firewalld.pid"));

    if firewalld_active {
        let conf_path = sub_path(root, "/etc/firewalld/firewalld.conf");
        let zone = fs::read_to_string(conf_path)
            .ok()
            .and_then(|c| parse_firewalld_conf_zone(&c))
            .unwrap_or_else(|| "public".to_string());
        return StatusFirewall {
            available: true,
            active: true,
            backend: "firewalld".into(),
            default_zone: Some(zone.clone()),
            details: format!("default zone: {zone}"),
        };
    }

    // 2. ufw: /etc/ufw/ufw.conf ENABLED=yes + active service or kernel netfilter
    let ufw_enabled = fs::read_to_string(sub_path(root, "/etc/ufw/ufw.conf"))
        .ok()
        .is_some_and(|c| parse_ufw_conf_enabled(&c));
    let ufw_kernel_or_service = sub_path(root, "/sys/fs/cgroup/system.slice/ufw.service").is_dir()
        || sub_path(root, "/sys/module/nf_tables").is_dir()
        || has_non_empty_file(&sub_path(root, "/proc/net/ip_tables_names"));
    if ufw_enabled && ufw_kernel_or_service {
        return StatusFirewall {
            available: true,
            active: true,
            backend: "ufw".into(),
            default_zone: None,
            details: "active".into(),
        };
    }

    // 3. nftables: active systemd cgroup unit directory
    if sub_path(root, "/sys/fs/cgroup/system.slice/nftables.service").is_dir() {
        return StatusFirewall {
            available: true,
            active: true,
            backend: "nftables".into(),
            default_zone: None,
            details: "active".into(),
        };
    }

    // 4. iptables: active systemd cgroup unit directory + active kernel ip_tables_names
    if sub_path(root, "/sys/fs/cgroup/system.slice/iptables.service").is_dir()
        && has_non_empty_file(&sub_path(root, "/proc/net/ip_tables_names"))
    {
        return StatusFirewall {
            available: true,
            active: true,
            backend: "iptables".into(),
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

pub async fn check_host_firewall() -> StatusFirewall {
    check_host_firewall_at(Path::new("/"))
}
