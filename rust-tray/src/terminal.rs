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

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn find_program(name: &str) -> Option<PathBuf> {
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
    }
    candidates.push(PathBuf::from("/usr/bin/syspilot"));
    candidates.push(PathBuf::from("/usr/local/bin/syspilot"));
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin/syspilot"));
    }
    candidates.into_iter().find(|p| is_executable(p))
}

#[allow(deprecated)]
fn reserve_slot() -> bool {
    ACTIVE_TERMINALS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < MAX_TERMINALS).then_some(n + 1)
        })
        .is_ok()
}

fn release_slot() {
    ACTIVE_TERMINALS.fetch_sub(1, Ordering::AcqRel);
}

fn reaper() -> &'static mpsc::SyncSender<Child> {
    static REAPER: OnceLock<mpsc::SyncSender<Child>> = OnceLock::new();
    REAPER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Child>(MAX_TERMINALS);
        let _ = thread::Builder::new()
            .name("syspilot-child-reaper".into())
            .spawn(move || {
                let mut children = Vec::new();
                loop {
                    match rx.recv_timeout(Duration::from_millis(250)) {
                        Ok(child) => children.push(child),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) if children.is_empty() => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {}
                    }
                    while let Ok(child) = rx.try_recv() {
                        children.push(child);
                    }
                    children.retain_mut(|child| match child.try_wait() {
                        Ok(None) => true,
                        _ => {
                            release_slot();
                            false
                        }
                    });
                }
            });
        tx
    })
}

fn build_terminal(
    name: &str,
    terminal: &Path,
    executable: &Path,
    args: &[&str],
    title: &str,
) -> Command {
    let mut command = Command::new(terminal);
    match name {
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
        "xterm" => {
            command.args(["-T", title, "-e"]);
        }
        _ => unreachable!(),
    }
    command
        .args(["/usr/bin/bash", "-c", WRAPPER, "syspilot-tray"])
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
            let _ = reaper().try_send(child);
            Ok(())
        }
        Err(err) => Err(format!("Failed to launch SysPilot GUI: {err}")),
    }
}

pub fn spawn_terminal_command(executable: &Path, args: &[&str], title: &str) -> Result<(), String> {
    if !is_executable(executable) {
        return Err(format!(
            "SysPilot executable is unavailable: {}",
            executable.display()
        ));
    }
    if !reserve_slot() {
        return Err(format!("terminal launch limit ({MAX_TERMINALS}) reached"));
    }

    for name in [
        "konsole",
        "ghostty",
        "kitty",
        "alacritty",
        "gnome-terminal",
        "foot",
        "xterm",
    ] {
        let Some(terminal) = find_program(name) else {
            continue;
        };
        if let Ok(child) = build_terminal(name, &terminal, executable, args, title).spawn() {
            if let Err(error) = reaper().try_send(child) {
                let mut child = match error {
                    mpsc::TrySendError::Full(c) | mpsc::TrySendError::Disconnected(c) => c,
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
