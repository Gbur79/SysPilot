use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpdateSummary {
    pub core_count: u32,
    pub regular_count: u32,
    pub aur_count: u32,
    pub standalone_count: u32,
    pub standalone_tools: Vec<String>,
    pub core_packages: Vec<String>,
    pub last_checked: Option<Instant>,
    pub check_state: crate::models::UpdateCheckState,
}

impl UpdateSummary {
    pub fn package_total(&self) -> u32 {
        self.core_count + self.regular_count + self.aur_count
    }

    pub fn total(&self) -> u32 {
        self.package_total() + self.standalone_count
    }
}

pub fn default_checkupdates_timeout() -> Duration {
    if let Ok(val) = std::env::var("SYSPILOT_UPDATE_TIMEOUT_SECS") {
        if let Ok(secs) = val.trim().parse::<u64>() {
            if secs > 0 {
                return Duration::from_secs(secs);
            }
        }
    }
    Duration::from_secs(15)
}

pub fn default_standalone_timeout() -> Duration {
    if let Ok(val) = std::env::var("SYSPILOT_STANDALONE_TIMEOUT_SECS") {
        if let Ok(secs) = val.trim().parse::<u64>() {
            if secs > 0 {
                return Duration::from_secs(secs);
            }
        }
    }
    Duration::from_secs(10)
}

pub async fn run_command_with_timeout(
    mut cmd: Command,
    timeout_dur: Duration,
) -> Result<std::process::Output, std::io::Error> {
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let child = cmd.spawn()?;
    match timeout(timeout_dur, child.wait_with_output()).await {
        Ok(res) => res,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("subprocess timed out after {}s", timeout_dur.as_secs()),
        )),
    }
}

pub fn is_core_package(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("linux")
        || lower.starts_with("systemd")
        || lower.starts_with("nvidia")
        || matches!(
            lower.as_str(),
            "grub"
                | "mkinitcpio"
                | "dracut"
                | "booster"
                | "glibc"
                | "cryptsetup"
                | "mesa"
                | "pipewire"
                | "wireplumber"
        )
}

pub fn find_executable(name: &str) -> Option<PathBuf> {
    let env_key = format!(
        "SYSPILOT_BIN_{}",
        name.to_ascii_uppercase().replace('-', "_")
    );
    if let Ok(path_str) = std::env::var(env_key) {
        let trimmed = path_str.trim();
        if trimmed.is_empty() || trimmed == "none" || trimmed == "disabled" {
            return None;
        }
        let p = PathBuf::from(trimmed);
        return if p.is_file() { Some(p) } else { None };
    }
    for dir in ["/usr/bin", "/usr/local/bin"] {
        let p = Path::new(dir).join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_bin = PathBuf::from(home).join(".local/bin").join(name);
        if user_bin.is_file() {
            return Some(user_bin);
        }
    }
    None
}

pub fn is_process_running(proc_name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let comm_path = entry.path().join("comm");
        if let Ok(comm) = std::fs::read_to_string(comm_path) {
            if comm.trim() == proc_name {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckError {
    NotFound,
    Busy,
    Timeout(Duration),
    ExecutionFailed(String),
    ParseFailed(String),
}

pub fn map_checkupdates_exit(
    code: Option<i32>,
    stdout: &[u8],
) -> Result<CheckupdatesReport, UpdateCheckError> {
    match code {
        Some(0) => {
            let text = std::str::from_utf8(stdout)
                .map_err(|e| UpdateCheckError::ParseFailed(format!("invalid utf-8 output: {e}")))?;
            parse_checkupdates_str(text).map_err(UpdateCheckError::ParseFailed)
        }
        Some(2) => Ok(CheckupdatesReport {
            core_count: 0,
            regular_count: 0,
            core_packages: Vec::new(),
            all_packages: Vec::new(),
        }),
        Some(75) => Err(UpdateCheckError::Busy),
        Some(c) => Err(UpdateCheckError::ExecutionFailed(format!(
            "checkupdates exited with status {c}"
        ))),
        None => Err(UpdateCheckError::ExecutionFailed(
            "checkupdates terminated by signal".to_string(),
        )),
    }
}

pub fn validate_secure_runtime_dir_meta(
    is_dir: bool,
    is_symlink: bool,
    owner_uid: u32,
    expected_uid: u32,
) -> bool {
    !is_symlink && is_dir && owner_uid == expected_uid
}

pub fn checkupdates_runtime_dir() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    let uid = unsafe { libc::getuid() };
    let base = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val).join("syspilot/checkupdates"),
        _ => std::env::temp_dir().join(format!("syspilot-{uid}/checkupdates")),
    };

    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    builder.mode(0o700);

    let db_dir = base.join("db");
    builder.create(&db_dir)?;

    for dir in [&base, &db_dir] {
        let meta = std::fs::symlink_metadata(dir)?;
        if !validate_secure_runtime_dir_meta(
            meta.is_dir(),
            meta.file_type().is_symlink(),
            meta.uid(),
            uid,
        ) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("insecure checkupdates runtime dir {:?}", dir),
            ));
        }
    }

    Ok(base)
}

pub async fn check_official_updates_at(
    checkupdates_bin: Option<&Path>,
    runtime_dir: &Path,
    timeout_dur: Duration,
) -> Result<CheckupdatesReport, UpdateCheckError> {
    static CHECK_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = CHECK_LOCK.lock().await;

    let Some(checkupdates_path) = checkupdates_bin else {
        return Err(UpdateCheckError::NotFound);
    };

    const FLOCK_EX_NB: libc::c_int = 6;
    const _: () = assert!(FLOCK_EX_NB == (libc::LOCK_EX | libc::LOCK_NB));
    const FLOCK_UN: libc::c_int = 8;
    const _: () = assert!(FLOCK_UN == libc::LOCK_UN);

    use std::os::unix::fs::DirBuilderExt;
    use std::os::unix::io::AsRawFd;

    let db_dir = runtime_dir.join("db");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    builder.mode(0o700);
    if let Err(e) = builder.create(&db_dir) {
        return Err(UpdateCheckError::ExecutionFailed(format!(
            "failed to create runtime dir: {e}"
        )));
    }

    let lock_path = runtime_dir.join(".lock");
    let lock_file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            return Err(UpdateCheckError::ExecutionFailed(format!(
                "failed to open lock file: {e}"
            )));
        }
    };

    let lock_res = unsafe { libc::flock(lock_file.as_raw_fd(), FLOCK_EX_NB) };
    if lock_res != 0 {
        return Err(UpdateCheckError::Busy);
    }

    let db_lck = db_dir.join("db.lck");
    if db_lck.exists() {
        let _ = std::fs::remove_file(&db_lck);
    }

    let flock_bin_opt = find_executable("flock");
    let mut cmd = if let Some(ref flock_bin) = flock_bin_opt {
        unsafe { libc::flock(lock_file.as_raw_fd(), FLOCK_UN) };
        let mut c = crate::exec::new_tokio_command(flock_bin);
        c.arg("-n")
            .arg("-E")
            .arg("75")
            .arg(&lock_path)
            .arg(checkupdates_path);
        c
    } else {
        crate::exec::new_tokio_command(checkupdates_path)
    };

    cmd.env("CHECKUPDATES_DB", &db_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err(UpdateCheckError::ExecutionFailed(format!(
                "failed to spawn checkupdates: {e}"
            )));
        }
    };

    let pgid = child.id().map(|p| p as i32);

    let stdout_pipe = child.stdout.take();
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut pipe, &mut buf).await;
        }
        buf
    });

    let status_res = tokio::select! {
        res = child.wait() => res,
        _ = tokio::time::sleep(timeout_dur) => {
            if let Some(pid) = pgid {
                unsafe {
                    libc::kill(-pid, libc::SIGTERM);
                }
            }
            let wait_res = tokio::time::timeout(Duration::from_millis(500), child.wait()).await;
            if wait_res.is_err() {
                if let Some(pid) = pgid {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                }
            }
            let _ = child.wait().await;
            eprintln!("[syspilot-core::updater] checkupdates timed out after {}s (pgid: {:?})", timeout_dur.as_secs(), pgid);
            return Err(UpdateCheckError::Timeout(timeout_dur));
        }
    };

    let status = match status_res {
        Ok(s) => s,
        Err(e) => {
            return Err(UpdateCheckError::ExecutionFailed(format!(
                "failed to wait on checkupdates: {e}"
            )));
        }
    };

    let stdout = match tokio::time::timeout(Duration::from_secs(1), stdout_task).await {
        Ok(Ok(bytes)) => bytes,
        _ => Vec::new(),
    };

    map_checkupdates_exit(status.code(), &stdout)
}

pub async fn check_official_updates_detailed() -> Result<CheckupdatesReport, UpdateCheckError> {
    let dir = match checkupdates_runtime_dir() {
        Ok(d) => d,
        Err(e) => {
            return Err(UpdateCheckError::ExecutionFailed(format!(
                "checkupdates_runtime_dir failed: {e}"
            )));
        }
    };
    let bin = find_executable("checkupdates");
    check_official_updates_at(bin.as_deref(), &dir, default_checkupdates_timeout()).await
}

pub async fn check_official_updates() -> (u32, u32, Vec<String>) {
    match check_official_updates_detailed().await {
        Ok(report) => (
            report.core_count,
            report.regular_count,
            report.core_packages,
        ),
        Err(e) => {
            eprintln!("[syspilot-core::updater] check_official_updates failed: {e:?}");
            (0, 0, Vec::new())
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CheckupdatesReport {
    pub core_count: u32,
    pub regular_count: u32,
    pub core_packages: Vec<String>,
    pub all_packages: Vec<String>,
}

pub fn parse_checkupdates_str(stdout: &str) -> Result<CheckupdatesReport, String> {
    let mut core_count = 0;
    let mut regular_count = 0;
    let mut core_packages = Vec::new();
    let mut all_packages = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("==> error:") || lower.starts_with("error:") {
            return Err(format!("checkupdates emitted error on stdout: {trimmed}"));
        }
        let Some((left, right)) = trimmed.split_once(" -> ") else {
            return Err(format!("malformed checkupdates line: {trimmed}"));
        };
        let mut left_parts = left.split_whitespace();
        let Some(pkg_name) = left_parts.next() else {
            return Err(format!("malformed checkupdates line: {trimmed}"));
        };
        let Some(_old_ver) = left_parts.next() else {
            return Err(format!("malformed checkupdates line: {trimmed}"));
        };
        if right.trim().is_empty() {
            return Err(format!("malformed checkupdates line: {trimmed}"));
        }

        if is_core_package(pkg_name) {
            core_count += 1;
            core_packages.push(trimmed.to_string());
        } else {
            regular_count += 1;
        }
        all_packages.push(trimmed.to_string());
    }

    Ok(CheckupdatesReport {
        core_count,
        regular_count,
        core_packages,
        all_packages,
    })
}

pub fn parse_checkupdates_stdout(stdout: &[u8]) -> (u32, u32, Vec<String>) {
    let text = String::from_utf8_lossy(stdout);
    match parse_checkupdates_str(&text) {
        Ok(r) => (r.core_count, r.regular_count, r.core_packages),
        Err(e) => {
            eprintln!("[syspilot-core::updater] parse_checkupdates_str failed: {e}");
            (0, 0, Vec::new())
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AurUpdatesReport {
    pub count: u32,
    pub packages: Vec<String>,
}

pub fn parse_aur_updates_str(stdout: &str) -> Result<AurUpdatesReport, String> {
    let mut count = 0;
    let mut packages = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("==> error:") || lower.starts_with("error:") {
            return Err(format!("AUR update emitted error on stdout: {trimmed}"));
        }
        if trimmed.split_whitespace().count() < 2 {
            return Err(format!("malformed AUR update line: {trimmed}"));
        }
        packages.push(trimmed.to_string());
        count += 1;
    }

    Ok(AurUpdatesReport { count, packages })
}

#[cfg(test)]
thread_local! {
    static TEST_AUR_HELPER_BIN: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static TEST_SYS_HEALTH_BIN: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub fn set_test_updater_bins(helper: Option<PathBuf>, sys_health: Option<PathBuf>) {
    TEST_AUR_HELPER_BIN.with(|b| *b.borrow_mut() = helper);
    TEST_SYS_HEALTH_BIN.with(|b| *b.borrow_mut() = sys_health);
}

pub async fn check_aur_updates() -> u32 {
    let (helper, args) = {
        #[cfg(test)]
        {
            if let Some(p) = TEST_AUR_HELPER_BIN.with(|b| b.borrow().clone()) {
                (p, ["-Qua"])
            } else if let Some(p) = find_executable("yay") {
                (p, ["-Qua"])
            } else if let Some(p) = find_executable("paru") {
                (p, ["-Qua"])
            } else {
                return 0;
            }
        }
        #[cfg(not(test))]
        {
            if let Some(p) = find_executable("yay") {
                (p, ["-Qua"])
            } else if let Some(p) = find_executable("paru") {
                (p, ["-Qua"])
            } else {
                return 0;
            }
        }
    };

    let mut cmd = crate::exec::new_tokio_command(helper);
    cmd.args(args);
    let output = match run_command_with_timeout(cmd, default_checkupdates_timeout()).await {
        Ok(out) => out,
        Err(_) => return 0,
    };

    if !output.status.success() {
        return 0;
    }

    match parse_aur_updates_str(&String::from_utf8_lossy(&output.stdout)) {
        Ok(r) => r.count,
        Err(e) => {
            eprintln!("[syspilot-core::updater] parse_aur_updates_str failed: {e}");
            0
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SoftwareJson {
    #[serde(default)]
    aur: Option<SoftwareAur>,
    #[serde(default)]
    goose: Option<SoftwareTool>,
    #[serde(default)]
    uv: Option<SoftwareTool>,
    #[serde(default)]
    flatpak: Option<SoftwareTool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SoftwareAur {
    #[serde(default)]
    pending_count: u32,
}

#[derive(Debug, Deserialize)]
struct SoftwareTool {
    #[serde(default)]
    update_available: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StandaloneUpdatesReport {
    pub count: u32,
    pub pending_tools: Vec<String>,
}

pub fn parse_standalone_updates_str(stdout: &str) -> Result<StandaloneUpdatesReport, String> {
    if stdout.trim().is_empty() {
        return Err("empty standalone software JSON".to_string());
    }

    let parsed: SoftwareJson =
        serde_json::from_str(stdout).map_err(|e| format!("invalid standalone JSON: {e}"))?;

    let mut count = 0;
    let mut pending_tools = Vec::new();
    if let Some(tool) = &parsed.goose {
        if tool.update_available {
            count += 1;
            pending_tools.push("goose".to_string());
        }
    }
    if let Some(tool) = &parsed.uv {
        if tool.update_available {
            count += 1;
            pending_tools.push("uv".to_string());
        }
    }
    if let Some(tool) = &parsed.flatpak {
        if tool.update_available {
            count += 1;
            pending_tools.push("flatpak".to_string());
        }
    }

    Ok(StandaloneUpdatesReport {
        count,
        pending_tools,
    })
}

pub async fn check_standalone_updates_detailed() -> StandaloneUpdatesReport {
    let sys_health_opt = {
        #[cfg(test)]
        {
            TEST_SYS_HEALTH_BIN
                .with(|b| b.borrow().clone())
                .or_else(|| find_executable("sys-health"))
        }
        #[cfg(not(test))]
        {
            find_executable("sys-health")
        }
    };
    let Some(sys_health) = sys_health_opt else {
        return StandaloneUpdatesReport::default();
    };

    let mut cmd = crate::exec::new_tokio_command(sys_health);
    cmd.args(["--software", "--json"]);
    let output = match run_command_with_timeout(cmd, default_standalone_timeout()).await {
        Ok(out) => out,
        Err(_) => return StandaloneUpdatesReport::default(),
    };

    if !output.status.success() {
        return StandaloneUpdatesReport::default();
    }

    match parse_standalone_updates_str(&String::from_utf8_lossy(&output.stdout)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[syspilot-core::updater] parse_standalone_updates_str failed: {e}");
            StandaloneUpdatesReport::default()
        }
    }
}

pub async fn check_standalone_updates() -> u32 {
    check_standalone_updates_detailed().await.count
}

pub async fn run_full_update_check(gamemode_active: bool) -> Option<UpdateSummary> {
    if gamemode_active {
        // Strict SRE Invariant: Never perform background disk/network scans during gaming!
        return None;
    }

    let official_res = check_official_updates_detailed().await;
    let (core_count, regular_count, core_packages, check_state) = match official_res {
        Ok(report) => (
            report.core_count,
            report.regular_count,
            report.core_packages,
            crate::models::UpdateCheckState::Ok,
        ),
        Err(UpdateCheckError::NotFound) => (
            0,
            0,
            Vec::new(),
            crate::models::UpdateCheckState::Unavailable,
        ),
        Err(_) => (0, 0, Vec::new(), crate::models::UpdateCheckState::Failed),
    };

    let aur_count = check_aur_updates().await;
    let standalone_report = check_standalone_updates_detailed().await;

    Some(UpdateSummary {
        core_count,
        regular_count,
        aur_count,
        standalone_count: standalone_report.count,
        standalone_tools: standalone_report.pending_tools,
        core_packages,
        last_checked: Some(Instant::now()),
        check_state,
    })
}
