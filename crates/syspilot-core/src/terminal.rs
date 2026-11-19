use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{OnceLock, mpsc};
use std::thread;
use std::time::Duration;

const MAX_TERMINALS: usize = 8;
const WRAPPER: &str =
    r#""$@"; rc=$?; printf '\n'; read -r -p 'Press Enter to close...' _; exit "$rc""#;
static ACTIVE_TERMINALS: AtomicUsize = AtomicUsize::new(0);

pub enum TrackedProcess {
    Terminal(Child),
    Detached(Child),
    TerminalWithCallback(Child, Option<Box<dyn FnOnce() + Send + 'static>>),
}

impl std::fmt::Debug for TrackedProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Terminal(c) => f.debug_tuple("Terminal").field(c).finish(),
            Self::Detached(c) => f.debug_tuple("Detached").field(c).finish(),
            Self::TerminalWithCallback(c, _) => {
                f.debug_tuple("TerminalWithCallback").field(c).finish()
            }
        }
    }
}

pub fn is_executable(path: &Path) -> bool {
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

pub fn find_program_at(
    root: &Path,
    name: &str,
    path_env: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if name.contains('/') {
        let direct = Path::new(name);
        return (direct.is_absolute() && is_executable(direct)).then(|| direct.to_path_buf());
    }
    let system = crate::sys_root::sub_path(root, "usr/bin").join(name);
    if is_executable(&system) {
        return Some(system);
    }
    path_env
        .into_iter()
        .flat_map(|v| std::env::split_paths(v).collect::<Vec<_>>())
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
}

pub fn find_program(name: &str) -> Option<PathBuf> {
    find_program_at(Path::new("/"), name, std::env::var_os("PATH").as_deref())
}

fn find_bash() -> PathBuf {
    find_program("bash").unwrap_or_else(|| PathBuf::from("/usr/bin/bash"))
}

pub fn resolve_syspilot_at(
    root: &Path,
    current_exe: Option<&Path>,
    syspilot_bin_env: Option<&std::ffi::OsStr>,
    home_env: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if let Some(p) = syspilot_bin_env.map(PathBuf::from) {
        if p.is_absolute() && is_executable(&p) {
            return Some(p);
        }
    }

    let mut candidates = Vec::new();
    if let Some(current) = current_exe {
        if let Some(parent) = current.parent() {
            candidates.push(parent.join("syspilot"));
        }
        for ancestor in current.ancestors().take(5) {
            candidates.push(ancestor.join("bin/syspilot"));
        }
    }
    candidates.push(crate::sys_root::sub_path(root, "usr/bin/syspilot"));
    candidates.push(crate::sys_root::sub_path(root, "usr/local/bin/syspilot"));
    if let Some(home) = home_env {
        candidates.push(PathBuf::from(home).join(".local/bin/syspilot"));
    }
    candidates.into_iter().find(|p| is_executable(p))
}

pub fn resolve_syspilot() -> Option<PathBuf> {
    resolve_syspilot_at(
        Path::new("/"),
        std::env::current_exe().ok().as_deref(),
        std::env::var_os("SYSPILOT_BIN").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

#[allow(deprecated)]
pub(crate) fn reserve_slot() -> bool {
    ACTIVE_TERMINALS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < MAX_TERMINALS).then_some(n + 1)
        })
        .is_ok()
}

#[allow(deprecated)]
pub(crate) fn release_slot() {
    let _ = ACTIVE_TERMINALS.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
        Some(n.saturating_sub(1))
    });
}

#[cfg(test)]
pub fn active_terminals_count() -> usize {
    ACTIVE_TERMINALS.load(Ordering::Acquire)
}

fn extract_child_from_send_error(error: mpsc::TrySendError<TrackedProcess>) -> Child {
    match error {
        mpsc::TrySendError::Full(TrackedProcess::Terminal(c))
        | mpsc::TrySendError::Disconnected(TrackedProcess::Terminal(c))
        | mpsc::TrySendError::Full(TrackedProcess::Detached(c))
        | mpsc::TrySendError::Disconnected(TrackedProcess::Detached(c))
        | mpsc::TrySendError::Full(TrackedProcess::TerminalWithCallback(c, _))
        | mpsc::TrySendError::Disconnected(TrackedProcess::TerminalWithCallback(c, _)) => c,
    }
}

fn invoke_reaper_callback(callback: Box<dyn FnOnce() + Send + 'static>) {
    if let Err(err) = thread::Builder::new()
        .name("syspilot-reaper-cb".into())
        .spawn(move || {
            if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback)) {
                let msg = payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("non-string panic payload");
                eprintln!("SysPilot: terminal exit callback panicked: {msg}");
            }
        })
    {
        eprintln!("SysPilot: failed to spawn terminal callback thread: {err}");
    }
}

pub(crate) fn reaper() -> &'static mpsc::SyncSender<TrackedProcess> {
    static REAPER: OnceLock<mpsc::SyncSender<TrackedProcess>> = OnceLock::new();
    REAPER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<TrackedProcess>(64);
        if let Err(err) = thread::Builder::new()
            .name("syspilot-child-reaper".into())
            .spawn(move || {
                let mut children: Vec<TrackedProcess> = Vec::new();
                loop {
                    if children.is_empty() {
                        match rx.recv() {
                            Ok(proc) => children.push(proc),
                            Err(_) => break,
                        }
                    } else {
                        match rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(proc) => children.push(proc),
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => {}
                        }
                    }
                    while let Ok(proc) = rx.try_recv() {
                        children.push(proc);
                    }
                    children.retain_mut(|proc| {
                        let (child, is_terminal) = match proc {
                            TrackedProcess::Terminal(c) => (c, true),
                            TrackedProcess::Detached(c) => (c, false),
                            TrackedProcess::TerminalWithCallback(c, _) => (c, true),
                        };
                        match child.try_wait() {
                            Ok(None) => true,
                            _ => {
                                if is_terminal {
                                    release_slot();
                                }
                                if let TrackedProcess::TerminalWithCallback(_, cb) = proc {
                                    if let Some(callback) = cb.take() {
                                        invoke_reaper_callback(callback);
                                    }
                                }
                                false
                            }
                        }
                    });
                }
            })
        {
            eprintln!("SysPilot: failed to spawn child reaper thread: {err}");
        }
        tx
    })
}

pub fn build_terminal(
    name: &str,
    terminal: &Path,
    executable: &Path,
    args: &[&str],
    title: &str,
) -> Command {
    let mut command = crate::exec::new_std_command_user_locale(terminal);
    let stem = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);

    match stem {
        "konsole" => {
            command
                .args(["--nofork", "-p"])
                .arg(format!("tabtitle={title}"))
                .arg("-e");
        }
        "ghostty" => {
            command.arg(format!("--title={title}")).arg("-e");
        }
        "kitty" => {
            command.args(["--title", title]);
        }
        "alacritty" => {
            command.args(["--title", title, "-e"]);
        }
        "gnome-terminal" => {
            command.args(["--title", title, "--"]);
        }
        "foot" => {
            command.arg(format!("--title={title}"));
        }
        "wezterm" => {
            command.args(["start", "--"]);
        }
        "xterm" => {
            command.args(["-T", title, "-e"]);
        }
        _ => {
            // Universal fallback for custom terminals (urxvt, terminator, etc.)
            command.args(["--title", title, "-e"]);
        }
    }
    let bash = find_bash();
    command
        .arg(&bash)
        .args(["-c", WRAPPER, "syspilot-runner"])
        .arg(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

pub fn spawn_detached_gui(executable: &Path) -> Result<(), String> {
    if !is_executable(executable) {
        return Err(format!(
            "SysPilot executable is unavailable: {}",
            executable.display()
        ));
    }
    match crate::exec::new_std_command_user_locale(executable)
        .arg("--gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            if let Err(err) = reaper().try_send(TrackedProcess::Detached(child)) {
                eprintln!(
                    "SysPilot: child reaper unavailable for detached GUI ({err}); waiting on fallback thread"
                );
                let mut child = extract_child_from_send_error(err);
                if let Err(spawn_err) = thread::Builder::new()
                    .name("syspilot-detached-wait".into())
                    .spawn(move || {
                        let _ = child.wait();
                    })
                {
                    eprintln!("SysPilot: failed to spawn fallback waiter thread: {spawn_err}");
                }
            }
            Ok(())
        }
        Err(err) => Err(format!("Failed to launch SysPilot GUI: {err}")),
    }
}

pub(crate) fn resolve_terminal_candidates(preferred: Option<&str>) -> Vec<&str> {
    const DEFAULTS: [&str; 8] = [
        "konsole",
        "ghostty",
        "kitty",
        "alacritty",
        "gnome-terminal",
        "foot",
        "wezterm",
        "xterm",
    ];
    let mut out = Vec::new();
    if let Some(pref) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
        out.push(pref);
    }
    for term in DEFAULTS {
        if !out.contains(&term) {
            out.push(term);
        }
    }
    out
}

fn spawn_terminal_command_internal(
    executable: &Path,
    args: &[&str],
    title: &str,
    callback: Option<Box<dyn FnOnce() + Send + 'static>>,
) -> Result<(), String> {
    if !is_executable(executable) {
        return Err(format!(
            "Executable is unavailable: {}",
            executable.display()
        ));
    }
    if !reserve_slot() {
        return Err(format!("terminal launch limit ({MAX_TERMINALS}) reached"));
    }

    let preferred = std::env::var("SYSPILOT_TERMINAL").ok();
    let candidates = resolve_terminal_candidates(preferred.as_deref());

    for name in candidates {
        let Some(terminal) = find_program(name) else {
            continue;
        };
        match build_terminal(name, &terminal, executable, args, title).spawn() {
            Ok(child) => {
                let proc = match callback {
                    Some(cb) => TrackedProcess::TerminalWithCallback(child, Some(cb)),
                    None => TrackedProcess::Terminal(child),
                };
                if let Err(error) = reaper().try_send(proc) {
                    let mut child = extract_child_from_send_error(error);
                    let _ = child.kill();
                    let _ = child.wait();
                    release_slot();
                    return Err("child reaper unavailable".into());
                }
                return Ok(());
            }
            Err(err) => {
                eprintln!(
                    "SysPilot: failed to spawn terminal '{name}' ({}): {err}",
                    terminal.display()
                );
            }
        }
    }

    release_slot();
    Err("no supported terminal emulator could be started".into())
}

pub fn spawn_terminal_command(executable: &Path, args: &[&str], title: &str) -> Result<(), String> {
    spawn_terminal_command_internal(executable, args, title, None)
}

pub fn spawn_terminal_command_with_callback<F>(
    executable: &Path,
    args: &[&str],
    title: &str,
    callback: F,
) -> Result<(), String>
where
    F: FnOnce() + Send + 'static,
{
    spawn_terminal_command_internal(executable, args, title, Some(Box::new(callback)))
}

fn spawn_syspilot_task_internal(
    args: &[&str],
    title: &str,
    callback: Option<Box<dyn FnOnce() + Send + 'static>>,
) -> Result<(), String> {
    if let Some(syspilot_bin) = resolve_syspilot() {
        spawn_terminal_command_internal(&syspilot_bin, args, title, callback)
    } else {
        let bash = find_bash();
        let mut bash_args = vec!["-c", r#"exec syspilot "$@""#, "--"];
        bash_args.extend_from_slice(args);
        spawn_terminal_command_internal(&bash, &bash_args, title, callback)
    }
}

pub fn spawn_syspilot_task(args: &[&str], title: &str) -> Result<(), String> {
    spawn_syspilot_task_internal(args, title, None)
}

pub fn spawn_syspilot_task_with_callback<F>(
    args: &[&str],
    title: &str,
    callback: F,
) -> Result<(), String>
where
    F: FnOnce() + Send + 'static,
{
    spawn_syspilot_task_internal(args, title, Some(Box::new(callback)))
}

pub fn spawn_shell_task(command: &str, title: &str) -> Result<(), String> {
    let bash = find_bash();
    spawn_terminal_command_internal(&bash, &["-c", command], title, None)
}

pub fn spawn_shell_task_with_callback<F>(
    command: &str,
    title: &str,
    callback: F,
) -> Result<(), String>
where
    F: FnOnce() + Send + 'static,
{
    let bash = find_bash();
    spawn_terminal_command_internal(&bash, &["-c", command], title, Some(Box::new(callback)))
}
