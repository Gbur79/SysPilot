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

pub fn find_program(name: &str) -> Option<PathBuf> {
    let system = PathBuf::from("/usr/bin").join(name);
    if is_executable(&system) {
        return Some(system);
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
}

pub fn resolve_syspilot() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SYSPILOT_BIN").map(PathBuf::from) {
        if p.is_absolute() && is_executable(&p) {
            return Some(p);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            candidates.push(parent.join("syspilot"));
        }
        for ancestor in current.ancestors().take(5) {
            let candidate = ancestor.join("bin/syspilot");
            if is_executable(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates.push(PathBuf::from("/usr/bin/syspilot"));
    candidates.push(PathBuf::from("/usr/local/bin/syspilot"));
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin/syspilot"));
    }
    candidates.into_iter().find(|p| is_executable(p))
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

pub(crate) fn reaper() -> &'static mpsc::SyncSender<TrackedProcess> {
    static REAPER: OnceLock<mpsc::SyncSender<TrackedProcess>> = OnceLock::new();
    REAPER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<TrackedProcess>(64);
        let _ = thread::Builder::new()
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
                                        callback();
                                    }
                                }
                                false
                            }
                        }
                    });
                }
            });
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
    let mut command = Command::new(terminal);
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
            // Universal fallback for custom terminals (wezterm, urxvt, terminator, etc.)
            command.args(["--title", title, "-e"]);
        }
    }
    command
        .args(["/usr/bin/bash", "-c", WRAPPER, "syspilot-runner"])
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
    match Command::new(executable)
        .arg("--gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            let _ = reaper().try_send(TrackedProcess::Detached(child));
            Ok(())
        }
        Err(err) => Err(format!("Failed to launch SysPilot GUI: {err}")),
    }
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

    // Check user preference from environment
    let preferred = std::env::var("SYSPILOT_TERMINAL").ok();

    let candidates: Vec<&str> = if let Some(pref) = preferred.as_deref() {
        vec![
            pref,
            "konsole",
            "ghostty",
            "kitty",
            "alacritty",
            "gnome-terminal",
            "foot",
            "wezterm",
            "xterm",
        ]
    } else {
        vec![
            "konsole",
            "ghostty",
            "kitty",
            "alacritty",
            "gnome-terminal",
            "foot",
            "wezterm",
            "xterm",
        ]
    };

    for name in candidates {
        let Some(terminal) = find_program(name) else {
            continue;
        };
        if let Ok(child) = build_terminal(name, &terminal, executable, args, title).spawn() {
            let proc = match callback {
                Some(cb) => TrackedProcess::TerminalWithCallback(child, Some(cb)),
                None => TrackedProcess::Terminal(child),
            };
            if let Err(error) = reaper().try_send(proc) {
                let mut child = match error {
                    mpsc::TrySendError::Full(TrackedProcess::Terminal(c))
                    | mpsc::TrySendError::Disconnected(TrackedProcess::Terminal(c))
                    | mpsc::TrySendError::Full(TrackedProcess::Detached(c))
                    | mpsc::TrySendError::Disconnected(TrackedProcess::Detached(c))
                    | mpsc::TrySendError::Full(TrackedProcess::TerminalWithCallback(c, _))
                    | mpsc::TrySendError::Disconnected(TrackedProcess::TerminalWithCallback(
                        c,
                        _,
                    )) => c,
                };
                let _ = child.kill();
                let _ = child.wait();
                release_slot();
                return Err("child reaper unavailable".into());
            }
            return Ok(());
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

pub fn spawn_syspilot_task(args: &[&str], title: &str) -> Result<(), String> {
    if let Some(syspilot_bin) = resolve_syspilot() {
        spawn_terminal_command(&syspilot_bin, args, title)
    } else {
        let bash = PathBuf::from("/usr/bin/bash");
        let mut bash_args = vec!["-c", r#"exec syspilot "$@""#, "--"];
        bash_args.extend_from_slice(args);
        spawn_terminal_command(&bash, &bash_args, title)
    }
}

pub fn spawn_syspilot_task_with_callback<F>(
    args: &[&str],
    title: &str,
    callback: F,
) -> Result<(), String>
where
    F: FnOnce() + Send + 'static,
{
    if let Some(syspilot_bin) = resolve_syspilot() {
        spawn_terminal_command_with_callback(&syspilot_bin, args, title, callback)
    } else {
        let bash = PathBuf::from("/usr/bin/bash");
        let mut bash_args = vec!["-c", r#"exec syspilot "$@""#, "--"];
        bash_args.extend_from_slice(args);
        spawn_terminal_command_with_callback(&bash, &bash_args, title, callback)
    }
}

pub fn spawn_shell_task(command: &str, title: &str) -> Result<(), String> {
    let bash = PathBuf::from("/usr/bin/bash");
    spawn_terminal_command(&bash, &["-c", command], title)
}
