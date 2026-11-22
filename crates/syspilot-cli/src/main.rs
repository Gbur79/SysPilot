use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};
use syspilot_core::models::{
    CanonicalSummary, MAX_STATUS_BYTES, STALE_AFTER_SECS, is_pre_boot_modified,
    parse_canonical_summary,
};
use syspilot_core::sys_root::sub_path;
use syspilot_core::terminal::{find_program_at, is_executable};
use syspilot_core::triage::{parse_proc_stat_btime, read_boot_epoch_secs_at};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_CONFIG_ENV_BYTES: usize = 65_536;
const READ_NOFOLLOW_NONBLOCK: libc::c_int = 133_120;
const _: () = assert!(READ_NOFOLLOW_NONBLOCK == (libc::O_NOFOLLOW | libc::O_NONBLOCK));

const USAGE_TEXT: &str = "\
Usage: syspilot [OPTION]
  --gui                       Launch the SysPilot Desktop Dashboard
  -s, --status                Read and format cached sys-health summary
  -j, --json                  Print cached canonical sys-health JSON
  -a, --audit                 Explicit foreground audit
  -u, --upgrade               Explicit guarded upgrade
  --software                  Explicit software audit
  -o, --orphans               Explicit orphan review
  --mirrors                   Explicit mirror workflow
  -m, --maintenance           Explicit maintenance
  -d, --deep-clean            Explicit deep-clean workflow
  --check-update              Check upstream GitHub release for updates
  --self-update               Perform safe self-update and atomic build
  -g, --gaming                Explicit gaming readiness audit
  --copilot QUESTION          Start terminal-native Goose SRE session
  --copilot-shell             Open an interactive Goose SRE session
  --tray                      Start the StatusNotifierItem tray
  --faf-repair, --faf-setup   Setup or repair Forged Alliance Forever runner
  --enable-autostart          Enable packaged systemd user service
  --disable-autostart         Disable packaged systemd user service
  -v, --version               Print version information and exit
  -h, --help                  Show help

Environment / Configuration (~/.config/syspilot/config.env):
  SYSPILOT_SYS_HEALTH         Development/test engine override
  SYSPILOT_TERMINAL           Preferred supported terminal executable
  SYSPILOT_COPILOT_CMD        Custom copilot executable override
  SYSPILOT_GOOSE_RECIPE       Optional Goose recipe path override
  SYSPILOT_MAX_STATUS_AGE     Cache freshness limit in seconds (default 21600)
";

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliAction {
    Exit(i32),
    SpawnDetached {
        bin: PathBuf,
    },
    ExecReplace {
        bin: PathBuf,
        args: Vec<String>,
        set_fallback_guard: bool,
    },
}

#[derive(Debug, Clone)]
struct CliEnv {
    root: PathBuf,
    state_home: PathBuf,
    config_home: PathBuf,
    xdg_bin_home: Option<PathBuf>,
    home: Option<PathBuf>,
    path_env: Option<OsString>,
    current_exe: Option<PathBuf>,
    sys_health_env: Option<String>,
    proc_stat_env: Option<PathBuf>,
    max_status_age_env: Option<String>,
    shell_fallback_env: Option<PathBuf>,
    now: SystemTime,
}

impl CliEnv {
    fn from_lookup<F>(get_var: F, current_exe: Option<PathBuf>, now: SystemTime) -> Self
    where
        F: Fn(&str) -> Option<OsString>,
    {
        let non_empty = |name: &str| get_var(name).filter(|v| !v.is_empty());
        let non_empty_str = |name: &str| {
            non_empty(name)
                .and_then(|v| v.into_string().ok())
                .filter(|s| !s.is_empty())
        };
        let home = non_empty("HOME").map(PathBuf::from);
        let state_home = non_empty("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".local/state")))
            .unwrap_or_else(|| PathBuf::from(".local/state"));
        let config_home = non_empty("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"));
        let xdg_bin_home = non_empty("XDG_BIN_HOME").map(PathBuf::from);
        Self {
            root: PathBuf::from("/"),
            state_home,
            config_home,
            xdg_bin_home,
            home,
            path_env: get_var("PATH"),
            current_exe,
            sys_health_env: non_empty_str("SYSPILOT_SYS_HEALTH"),
            proc_stat_env: non_empty("SYSPILOT_PROC_STAT").map(PathBuf::from),
            max_status_age_env: non_empty_str("SYSPILOT_MAX_STATUS_AGE"),
            shell_fallback_env: non_empty("SYSPILOT_SHELL_FALLBACK_BIN").map(PathBuf::from),
            now,
        }
    }

    fn from_process() -> Self {
        Self::from_lookup(
            |name| std::env::var_os(name),
            std::env::current_exe().ok(),
            SystemTime::now(),
        )
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ConfigOverrides {
    max_status_age: Option<u64>,
    sys_health: Option<String>,
}

fn parse_config_env(content: &str) -> ConfigOverrides {
    let mut cfg = ConfigOverrides::default();
    for line in content.lines() {
        if let Some((raw_key, raw_val)) = line.split_once('=') {
            let trimmed_key = raw_key.trim();
            let key = trimmed_key
                .strip_prefix("export ")
                .unwrap_or(trimmed_key)
                .trim();
            let val = raw_val.trim().trim_matches('"').trim_matches('\'');
            match key {
                "SYSPILOT_MAX_STATUS_AGE" => {
                    if let Ok(age) = val.parse::<u64>() {
                        cfg.max_status_age = Some(age);
                    }
                }
                "SYSPILOT_SYS_HEALTH" if !val.is_empty() => {
                    cfg.sys_health = Some(val.to_string());
                }
                _ => {}
            }
        }
    }
    cfg
}

fn read_bounded_regular_file(path: &Path, max_bytes: usize) -> Result<(Vec<u8>, SystemTime), ()> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(READ_NOFOLLOW_NONBLOCK)
        .open(path)
        .map_err(|_| ())?;
    let meta = file.metadata().map_err(|_| ())?;
    if !meta.is_file() {
        return Err(());
    }
    let mtime = meta.modified().map_err(|_| ())?;
    let mut bytes = Vec::with_capacity(meta.len().min(max_bytes as u64) as usize);
    file.take((max_bytes as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() > max_bytes {
        return Err(());
    }
    Ok((bytes, mtime))
}

fn load_effective_config(env: &CliEnv) -> (u64, Option<String>) {
    let config_path = env.config_home.join("syspilot/config.env");
    let file_cfg = read_bounded_regular_file(&config_path, MAX_CONFIG_ENV_BYTES)
        .ok()
        .and_then(|(bytes, _)| String::from_utf8(bytes).ok())
        .map(|text| parse_config_env(&text))
        .unwrap_or_default();
    let max_age = env
        .max_status_age_env
        .as_deref()
        .and_then(|v| v.parse::<u64>().ok())
        .or(file_cfg.max_status_age)
        .unwrap_or(STALE_AFTER_SECS);
    let sys_health = env
        .sys_health_env
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
        .or(file_cfg.sys_health);
    (max_age, sys_health)
}

fn is_embedded_engine_path(resolved: &Path) -> bool {
    let s = resolved.to_string_lossy();
    s.ends_with("/sysPilot/bin/sys-health.sh") || s.ends_with("/syspilot/bin/sys-health.sh")
}

fn have_engine_at(env: &CliEnv, sys_health_override: Option<&str>) -> bool {
    let candidate = match sys_health_override.filter(|v| !v.is_empty()) {
        Some(raw) => {
            let p = PathBuf::from(raw);
            if !is_executable(&p) {
                return false;
            }
            p
        }
        None => match find_program_at(&env.root, "sys-health", env.path_env.as_deref()) {
            Some(p) => p,
            None => return false,
        },
    };
    let Ok(resolved) = std::fs::canonicalize(&candidate) else {
        return false;
    };
    !is_embedded_engine_path(&resolved)
}

fn read_btime_at(env: &CliEnv) -> Option<u64> {
    if let Some(proc_stat) = env.proc_stat_env.as_deref() {
        let (bytes, _) = read_bounded_regular_file(proc_stat, MAX_CONFIG_ENV_BYTES).ok()?;
        let text = String::from_utf8(bytes).ok()?;
        return parse_proc_stat_btime(&text);
    }
    read_boot_epoch_secs_at(&env.root)
}

fn read_validated_summary(
    env: &CliEnv,
    sys_health_override: Option<&str>,
    stderr: &mut dyn Write,
) -> Option<(Vec<u8>, CanonicalSummary, SystemTime)> {
    let status_file = env.state_home.join("system-health/summary.json");
    let Ok((bytes, mtime)) = read_bounded_regular_file(&status_file, MAX_STATUS_BYTES as usize)
    else {
        if !have_engine_at(env, sys_health_override) {
            let _ = writeln!(
                stderr,
                "syspilot: no readable canonical summary; sys-health is not installed. Install sys-health or configure SYSPILOT_SYS_HEALTH to generate telemetry."
            );
        } else {
            let _ = writeln!(
                stderr,
                "syspilot: no readable canonical summary; run: syspilot --audit"
            );
        }
        return None;
    };
    let Ok(summary) = parse_canonical_summary(&bytes) else {
        let _ = writeln!(stderr, "syspilot: invalid or incompatible summary schema");
        return None;
    };
    Some((bytes, summary, mtime))
}

fn status_exit_code(status: &str) -> i32 {
    match status {
        "ALL_CLEAR" | "FLIGHT_READY" => 0,
        "ACTION_REQUIRED" => 1,
        _ => 2,
    }
}

fn is_heavy_fallback_flag(arg: &str) -> bool {
    matches!(
        arg,
        "-a" | "--audit"
            | "-u"
            | "--upgrade"
            | "--software"
            | "-o"
            | "--orphans"
            | "--mirrors"
            | "-m"
            | "--maintenance"
            | "-d"
            | "--deep-clean"
            | "--check-update"
            | "--self-update"
            | "-g"
            | "--gaming"
            | "--copilot"
            | "--copilot-shell"
            | "--faf-repair"
            | "--faf-setup"
            | "--enable-autostart"
            | "--disable-autostart"
    )
}

fn resolve_companion_bin_at(env: &CliEnv, bin_name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(current) = env.current_exe.as_deref() {
        if let Some(parent) = current.parent() {
            candidates.push(parent.join(bin_name));
        }
        for ancestor in current.ancestors().take(5) {
            candidates.push(ancestor.join("target/release").join(bin_name));
            candidates.push(ancestor.join("target/debug").join(bin_name));
        }
    }
    if let Some(bin_home) = env.xdg_bin_home.as_deref() {
        candidates.push(bin_home.join(bin_name));
    } else if let Some(home) = env.home.as_deref() {
        candidates.push(home.join(".local/bin").join(bin_name));
    }
    candidates.push(sub_path(&env.root, "usr/bin").join(bin_name));
    candidates.push(sub_path(&env.root, "usr/local/bin").join(bin_name));
    if let Some(found) = find_program_at(&env.root, bin_name, env.path_env.as_deref()) {
        candidates.push(found);
    }
    candidates.into_iter().find(|p| is_executable(p))
}

fn same_canonical_file(candidate: &Path, current_exe: Option<&Path>) -> bool {
    let Some(exe) = current_exe else {
        return false;
    };
    match (std::fs::canonicalize(candidate), std::fs::canonicalize(exe)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn resolve_shell_fallback_at(env: &CliEnv) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(explicit) = env.shell_fallback_env.as_deref() {
        candidates.push(explicit.to_path_buf());
    }
    if let Some(current) = env.current_exe.as_deref() {
        for ancestor in current.ancestors().take(5) {
            candidates.push(ancestor.join("bin/syspilot"));
        }
        if let Some(parent) = current.parent().and_then(Path::parent) {
            candidates.push(parent.join("lib/syspilot/bin/syspilot"));
        }
    }
    if let Some(bin_home) = env.xdg_bin_home.as_deref() {
        if let Some(parent) = bin_home.parent() {
            candidates.push(parent.join("lib/syspilot/bin/syspilot"));
        }
    } else if let Some(home) = env.home.as_deref() {
        candidates.push(home.join(".local/lib/syspilot/bin/syspilot"));
    }
    candidates.push(sub_path(&env.root, "usr/lib/syspilot/bin/syspilot"));
    candidates.push(sub_path(&env.root, "usr/local/lib/syspilot/bin/syspilot"));
    candidates
        .into_iter()
        .find(|p| is_executable(p) && !same_canonical_file(p, env.current_exe.as_deref()))
}

fn plan_cli(
    args: &[String],
    env: &CliEnv,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliAction {
    let first = args.first().map(String::as_str).unwrap_or("");
    match first {
        "-h" | "--help" => {
            let _ = stdout.write_all(USAGE_TEXT.as_bytes());
            CliAction::Exit(0)
        }
        "-v" | "--version" => {
            let _ = writeln!(stdout, "syspilot {VERSION}");
            CliAction::Exit(0)
        }
        "--gui" => match resolve_companion_bin_at(env, "syspilot-gui") {
            Some(bin) => CliAction::SpawnDetached { bin },
            None => {
                let _ = writeln!(stderr, "syspilot: syspilot-gui is not installed or built");
                CliAction::Exit(1)
            }
        },
        "--tray" | "" => match resolve_companion_bin_at(env, "syspilot-tray") {
            Some(bin) => CliAction::ExecReplace {
                bin,
                args: Vec::new(),
                set_fallback_guard: false,
            },
            None => {
                let _ = writeln!(stderr, "syspilot: syspilot-tray is not installed or built");
                CliAction::Exit(1)
            }
        },
        "-j" | "--json" => {
            let (max_age, sys_health) = load_effective_config(env);
            let Some((bytes, _, mtime)) =
                read_validated_summary(env, sys_health.as_deref(), stderr)
            else {
                return CliAction::Exit(1);
            };
            let _ = stdout.write_all(&bytes);
            if !bytes.ends_with(b"\n") {
                let _ = stdout.write_all(b"\n");
            }
            let age_secs = env
                .now
                .duration_since(mtime)
                .unwrap_or(Duration::ZERO)
                .as_secs();
            CliAction::Exit(if age_secs > max_age { 3 } else { 0 })
        }
        "-s" | "--status" => {
            let (max_age, sys_health) = load_effective_config(env);
            let Some((_, summary, mtime)) =
                read_validated_summary(env, sys_health.as_deref(), stderr)
            else {
                return CliAction::Exit(1);
            };
            let age_secs = env
                .now
                .duration_since(mtime)
                .unwrap_or(Duration::ZERO)
                .as_secs();
            let is_stale = age_secs > max_age;
            let pre_boot = is_pre_boot_modified(Some(mtime), read_btime_at(env));
            let stale_tag = if is_stale { " (STALE)" } else { "" };
            let pre_boot_tag = if pre_boot { " (PRE-BOOT)" } else { "" };
            let game_label = if summary.gaming_detected {
                "detected"
            } else {
                "not reported"
            };
            let _ = writeln!(stdout, "SysPilot — cached canonical health summary");
            let _ = writeln!(stdout, "Status:       {}", summary.status);
            let _ = writeln!(stdout, "Observed:     {}", summary.timestamp);
            let _ = writeln!(stdout, "Cache age:    {age_secs}s{stale_tag}{pre_boot_tag}");
            let _ = writeln!(stdout, "Errors:       {}", summary.errors);
            let _ = writeln!(stdout, "Warnings:     {}", summary.warnings);
            let _ = writeln!(stdout, "Kernel:       {}", summary.kernel);
            let _ = writeln!(stdout, "Game profile: {game_label}");
            if pre_boot {
                let _ = writeln!(
                    stdout,
                    "Info:         cached summary predates current boot (run: syspilot --audit)"
                );
            }
            if is_stale {
                let _ = writeln!(
                    stderr,
                    "syspilot: cached data is stale; an audit will run only when explicitly requested"
                );
                return CliAction::Exit(3);
            }
            CliAction::Exit(status_exit_code(&summary.status))
        }
        other if is_heavy_fallback_flag(other) => match resolve_shell_fallback_at(env) {
            Some(bin) => CliAction::ExecReplace {
                bin,
                args: args.to_vec(),
                set_fallback_guard: true,
            },
            None => {
                let _ = writeln!(
                    stderr,
                    "syspilot: fallback shell router is not installed for {other}"
                );
                CliAction::Exit(1)
            }
        },
        other => {
            let _ = writeln!(stderr, "syspilot: unknown option: {other} (use --help)");
            CliAction::Exit(1)
        }
    }
}

fn launch_gui_bin(bin: &Path) -> Result<(), String> {
    let mut cmd = syspilot_core::exec::new_std_command_user_locale(bin);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    cmd.spawn()
        .map(|_| ())
        .map_err(|err| format!("failed to launch {}: {err}", bin.display()))
}

fn build_exec_command(bin: &Path, args: &[String], set_fallback_guard: bool) -> Command {
    let mut cmd = syspilot_core::exec::new_std_command_user_locale(bin);
    cmd.args(args);
    if set_fallback_guard {
        cmd.env("SYSPILOT_SHELL_FALLBACK", "1");
    }
    cmd
}

fn execute_or_plan<F>(
    args: &[String],
    env: &CliEnv,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    exec_fn: F,
) -> i32
where
    F: FnOnce(&Path, &[String], bool) -> Result<(), String>,
{
    match plan_cli(args, env, stdout, stderr) {
        CliAction::Exit(code) => code,
        CliAction::SpawnDetached { bin } => match launch_gui_bin(&bin) {
            Ok(()) => 0,
            Err(err) => {
                let _ = writeln!(stderr, "syspilot: {err}");
                1
            }
        },
        CliAction::ExecReplace {
            bin,
            args,
            set_fallback_guard,
        } => match exec_fn(&bin, &args, set_fallback_guard) {
            Ok(()) => 0,
            Err(err) => {
                let _ = writeln!(stderr, "syspilot: failed to exec {}: {err}", bin.display());
                1
            }
        },
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = CliEnv::from_process();
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    let code = execute_or_plan(&args, &env, &mut out, &mut err, |bin, fwd, guard| {
        let mut cmd = build_exec_command(bin, fwd, guard);
        let exec_err = cmd.exec();
        Err(exec_err.to_string())
    });
    std::process::exit(code);
}

#[cfg(test)]
mod tests;
