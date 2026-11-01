use crate::models::FailedServicesStatus;
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub fn is_candidate_app_unit(unit: &str) -> bool {
    let lower = unit.trim().to_ascii_lowercase();
    lower.starts_with("app-") && (lower.ends_with(".service") || lower.ends_with(".scope"))
}

pub fn parse_systemctl_show_transient(
    stdout: &str,
    candidates: &HashSet<String>,
) -> HashSet<String> {
    let mut ignored = HashSet::new();
    let mut properties: HashMap<String, String> = HashMap::new();

    for line in stdout.lines().chain(std::iter::once("")) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let Some(unit_id) = properties.get("Id") {
                if candidates.contains(unit_id)
                    && properties.get("Transient").map(|s| s.as_str()) == Some("yes")
                    && properties.get("Slice").map(|s| s.as_str()) == Some("app.slice")
                {
                    ignored.insert(unit_id.clone());
                }
            }
            properties.clear();
            continue;
        }

        if let Some((k, v)) = trimmed.split_once('=') {
            properties.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    ignored
}

pub async fn get_transient_desktop_app_units(unit_names: &[String]) -> HashSet<String> {
    let candidates: HashSet<String> = unit_names
        .iter()
        .filter(|u| is_candidate_app_unit(u))
        .cloned()
        .collect();

    if candidates.is_empty() {
        return HashSet::new();
    }

    let mut args = vec![
        "--user".to_string(),
        "show".to_string(),
        "--no-pager".to_string(),
        "--property=Id".to_string(),
        "--property=Transient".to_string(),
        "--property=Slice".to_string(),
    ];
    args.extend(candidates.iter().cloned());

    let run_cmd = async {
        let mut cmd = Command::new("systemctl");
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        cmd.output().await
    };

    match timeout(Duration::from_millis(1000), run_cmd).await {
        Ok(Ok(output)) if output.status.success() => {
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            parse_systemctl_show_transient(&stdout_str, &candidates)
        }
        _ => HashSet::new(),
    }
}

pub async fn query_failed_units(user_mode: bool) -> Result<Vec<String>, String> {
    let mut args = vec!["--failed", "--no-legend", "--plain"];
    if user_mode {
        args.insert(0, "--user");
    }

    let run_cmd = async {
        let mut cmd = Command::new("systemctl");
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd.output().await
    };

    match timeout(Duration::from_millis(1500), run_cmd).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                let units = stdout_str
                    .lines()
                    .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
                    .collect();
                Ok(units)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let err_msg = if stderr.is_empty() {
                    format!("systemctl failed with exit code {:?}", output.status.code())
                } else {
                    stderr
                };
                Err(err_msg)
            }
        }
        Ok(Err(e)) => Err(format!("failed to spawn systemctl: {e}")),
        Err(_) => Err("systemctl query timed out".to_string()),
    }
}

pub async fn check_failed_services() -> FailedServicesStatus {
    let system = query_failed_units(false).await.unwrap_or_default();
    let (user, user_manager_available) = match query_failed_units(true).await {
        Ok(raw_user) => {
            let ignored_user = get_transient_desktop_app_units(&raw_user).await;
            let user = raw_user
                .into_iter()
                .filter(|u| !ignored_user.contains(u))
                .collect();
            (user, true)
        }
        Err(_) => (Vec::new(), false),
    };

    FailedServicesStatus {
        system,
        user,
        user_manager_available,
    }
}
