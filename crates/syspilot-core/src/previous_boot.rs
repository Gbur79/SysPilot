use crate::firmware::parse_timeout_ms;
use crate::sys_root::sub_path;
use crate::view::truncate_chars;
#[cfg(test)]
use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::time::timeout;

const DEFAULT_PREV_BOOT_TIMEOUT_MS: u64 = 2000;

const JOURNAL_FSCK_UNCLEAN_MARKERS: &[&str] = &[
    "corrupted or uncleanly shut down",
    "was not closed cleanly",
    "recovering journal",
    "dirty bit is set",
    "contains a file system with errors",
];

const KERNEL_PANIC_MARKERS: &[&str] = &[
    "kernel panic",
    "bug: unable to handle",
    "oops:",
    "watchdog: bug: soft lockup",
];

const PERMISSION_HINT_MARKERS: &[&str] = &[
    "not seeing messages from other users",
    "insufficient permissions",
];

const MISSING_PREV_BOOT_MARKERS: &[&str] = &[
    "no persistent journal",
    "failed to look up boot -1",
    "specifying boot id or boot offset has no effect",
    "does not exist",
    "is not available",
    "no such boot id",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviousBootVerdict {
    Clean,
    Unclean(String),
    Inconclusive(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalProbeOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

struct BootVerdictCache {
    entry: Option<(String, PreviousBootVerdict)>,
}

impl BootVerdictCache {
    fn get(&self, boot_id: &str) -> Option<PreviousBootVerdict> {
        self.entry
            .as_ref()
            .filter(|(id, _)| id == boot_id)
            .map(|(_, v)| v.clone())
    }

    fn set(&mut self, boot_id: String, verdict: PreviousBootVerdict) {
        self.entry = Some((boot_id, verdict));
    }
}

#[cfg(test)]
thread_local! {
    static TEST_JOURNALCTL_BIN: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
    static TEST_PREV_BOOT_CACHE: RefCell<BootVerdictCache> = const { RefCell::new(BootVerdictCache { entry: None }) };
}

#[cfg(test)]
pub fn set_test_journalctl_bin(bin: Option<Option<PathBuf>>) {
    TEST_JOURNALCTL_BIN.with(|b| *b.borrow_mut() = bin);
}

#[cfg(test)]
pub fn clear_test_previous_boot_cache() {
    TEST_PREV_BOOT_CACHE.with(|c| c.borrow_mut().entry = None);
}

#[cfg(not(test))]
static PREV_BOOT_CACHE: std::sync::Mutex<BootVerdictCache> =
    std::sync::Mutex::new(BootVerdictCache { entry: None });

fn get_cached_verdict(boot_id: &str) -> Option<PreviousBootVerdict> {
    #[cfg(test)]
    {
        TEST_PREV_BOOT_CACHE.with(|c| c.borrow().get(boot_id))
    }
    #[cfg(not(test))]
    {
        let guard = PREV_BOOT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        guard.get(boot_id)
    }
}

fn store_cached_verdict(boot_id: String, verdict: PreviousBootVerdict) {
    #[cfg(test)]
    {
        TEST_PREV_BOOT_CACHE.with(|c| c.borrow_mut().set(boot_id, verdict));
    }
    #[cfg(not(test))]
    {
        let mut guard = PREV_BOOT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        guard.set(boot_id, verdict);
    }
}

pub fn read_boot_id_at(root: &Path) -> Option<String> {
    let path = sub_path(root, "/proc/sys/kernel/random/boot_id");
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn find_matching_line(stdout: &str, markers: &[&str]) -> Option<String> {
    for line in stdout.lines() {
        let lower = line.to_ascii_lowercase();
        if markers.iter().any(|m| lower.contains(m)) {
            return Some(line.trim().to_string());
        }
    }
    None
}

pub fn parse_boot_journal_fsck_output(stdout: &str) -> Option<String> {
    find_matching_line(stdout, JOURNAL_FSCK_UNCLEAN_MARKERS)
}

pub fn parse_prev_kernel_panic_output(stdout: &str) -> Option<String> {
    find_matching_line(stdout, KERNEL_PANIC_MARKERS)
}

fn has_permission_blind_spot(probe: Result<&JournalProbeOutput, &str>) -> bool {
    let Ok(out) = probe else {
        return false;
    };
    let lower = out.stderr.to_ascii_lowercase();
    PERMISSION_HINT_MARKERS.iter().any(|m| lower.contains(m))
}

fn is_missing_prev_boot_stderr(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    MISSING_PREV_BOOT_MARKERS.iter().any(|m| lower.contains(m))
}

pub fn evaluate_previous_boot_probes(
    boot0: Result<&JournalProbeOutput, &str>,
    prev_kernel: Result<&JournalProbeOutput, &str>,
) -> PreviousBootVerdict {
    if let Ok(pk) = prev_kernel {
        if pk.success {
            if let Some(hit) = parse_prev_kernel_panic_output(&pk.stdout) {
                return PreviousBootVerdict::Unclean(format!(
                    "Kernel crash in previous boot: {}",
                    truncate_chars(&hit, 96)
                ));
            }
        }
    }

    if let Ok(b0) = boot0 {
        if b0.success {
            if let Some(hit) = parse_boot_journal_fsck_output(&b0.stdout) {
                return PreviousBootVerdict::Unclean(format!(
                    "Unclean shutdown / filesystem recovery: {}",
                    truncate_chars(&hit, 96)
                ));
            }
        }
    }

    if has_permission_blind_spot(boot0) || has_permission_blind_spot(prev_kernel) {
        return PreviousBootVerdict::Inconclusive(
            "Insufficient permissions to read system journal".to_string(),
        );
    }

    match (boot0, prev_kernel) {
        (Ok(b0), Ok(pk))
            if b0.success && (pk.success || is_missing_prev_boot_stderr(&pk.stderr)) =>
        {
            PreviousBootVerdict::Clean
        }
        (Err(e), _) | (_, Err(e)) => PreviousBootVerdict::Inconclusive(e.to_string()),
        (Ok(b0), _) if !b0.success => PreviousBootVerdict::Inconclusive(format!(
            "journalctl boot-0 probe failed: {}",
            truncate_chars(b0.stderr.trim(), 96)
        )),
        (_, Ok(pk)) => PreviousBootVerdict::Inconclusive(format!(
            "journalctl prev-boot kernel probe failed: {}",
            truncate_chars(pk.stderr.trim(), 96)
        )),
    }
}

fn resolve_journalctl_bin() -> Option<OsString> {
    #[cfg(test)]
    if let Some(override_bin) = TEST_JOURNALCTL_BIN.with(|b| b.borrow().clone()) {
        return override_bin.map(|p| p.into_os_string());
    }
    Some(OsString::from("journalctl"))
}

pub fn default_prev_boot_timeout() -> Duration {
    parse_timeout_ms(
        std::env::var("SYSPILOT_PREV_BOOT_TIMEOUT_MS")
            .ok()
            .as_deref(),
        DEFAULT_PREV_BOOT_TIMEOUT_MS,
    )
}

async fn run_journalctl_probe(
    bin: &OsStr,
    args: &[&str],
    timeout_dur: Duration,
) -> Result<JournalProbeOutput, String> {
    let run_cmd = async {
        let mut cmd = crate::exec::new_tokio_command(bin);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd.output().await
    };

    match timeout(timeout_dur, run_cmd).await {
        Ok(Ok(out)) => Ok(JournalProbeOutput {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }),
        Ok(Err(err)) => Err(format!("failed to spawn journalctl: {err}")),
        Err(_) => Err("journalctl probe timed out".to_string()),
    }
}

pub async fn check_previous_boot_integrity_at(_root: &Path) -> PreviousBootVerdict {
    let Some(bin) = resolve_journalctl_bin() else {
        return PreviousBootVerdict::Inconclusive("journalctl binary unavailable".to_string());
    };
    let timeout_dur = default_prev_boot_timeout();
    let boot0_args = [
        "-b",
        "0",
        "-u",
        "systemd-journald",
        "-u",
        "systemd-fsck*",
        "--output=cat",
        "--no-pager",
    ];
    let prev_k_args = [
        "-b",
        "-1",
        "-k",
        "-p",
        "0..2",
        "--output=cat",
        "--lines=200",
        "--no-pager",
    ];

    let (boot0_res, prev_k_res) = tokio::join!(
        run_journalctl_probe(&bin, &boot0_args, timeout_dur),
        run_journalctl_probe(&bin, &prev_k_args, timeout_dur)
    );

    evaluate_previous_boot_probes(
        boot0_res.as_ref().map_err(String::as_str),
        prev_k_res.as_ref().map_err(String::as_str),
    )
}

pub async fn check_previous_boot_cached_at(root: &Path) -> PreviousBootVerdict {
    let boot_id = read_boot_id_at(root);
    if let Some(ref id) = boot_id {
        if let Some(cached) = get_cached_verdict(id) {
            return cached;
        }
    }

    let verdict = check_previous_boot_integrity_at(root).await;
    if let Some(id) = boot_id {
        if !matches!(verdict, PreviousBootVerdict::Inconclusive(_)) {
            store_cached_verdict(id, verdict.clone());
        }
    }
    verdict
}
