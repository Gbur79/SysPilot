#[cfg(test)]
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use tokio::time::timeout;

const MAX_ALPM_DESC_BYTES: u64 = 8192;
const MAX_BUNDLE_BYTES: usize = 65_536;
const BUNDLE_TRUNCATED_SUFFIX: &str = "\n\n[TRUNCATED TO 64 KiB LIMIT]\n";

#[cfg(test)]
thread_local! {
    static TEST_COPILOT_JOURNALCTL_BIN: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub fn set_test_copilot_journalctl_bin(bin: Option<Option<PathBuf>>) {
    TEST_COPILOT_JOURNALCTL_BIN.with(|b| *b.borrow_mut() = bin);
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CopilotInfo {
    pub ready: bool,
    pub model: String,
    pub provider: String,
    pub recipe: String,
    pub custom_cmd: Option<String>,
}

impl CopilotInfo {
    pub fn badge_label(&self) -> String {
        if !self.ready {
            "🤖 Copilot: Not Configured".to_string()
        } else if self.provider == "Antigravity CLI" || self.provider == "Hermes Agent" {
            format!("🤖 {} ({})", self.model, self.provider)
        } else if let Some(cmd) = &self.custom_cmd {
            format!("🤖 Custom: {cmd}")
        } else {
            format!("🤖 {} ({})", self.model, self.provider)
        }
    }

    pub fn status_line(&self) -> String {
        if !self.ready {
            "⚠ Copilot CLI not configured. Set up ~/.config/goose/config.yaml or SYSPILOT_COPILOT_CMD in ~/.config/syspilot/config.env".to_string()
        } else {
            let mut line = format!("🟢 Connected: {} | Provider: {}", self.model, self.provider);
            if !self.recipe.is_empty() {
                line.push_str(&format!(" | Recipe: {}", self.recipe));
            }
            line.push_str(" | Token-Lean Mode Active");
            line
        }
    }
}

pub fn is_agy_command(cmd: &str) -> bool {
    cmd.split_whitespace().any(|token| {
        let p = Path::new(token);
        p.file_name().is_some_and(|name| name == "agy")
    })
}

pub fn adapt_agy_cmd_noninteractive(cmd: &str) -> String {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    let has_interactive = tokens
        .iter()
        .any(|&t| t == "-i" || t == "--prompt-interactive");
    let has_print = tokens
        .iter()
        .any(|&t| t == "-p" || t == "--print" || t == "--prompt");

    if has_interactive {
        tokens
            .into_iter()
            .map(|t| {
                if t == "-i" || t == "--prompt-interactive" {
                    "-p"
                } else {
                    t
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else if !has_print {
        format!("{cmd} -p")
    } else {
        cmd.to_string()
    }
}

pub fn get_copilot_info() -> CopilotInfo {
    let mut info = CopilotInfo {
        ready: false,
        model: "Unknown".into(),
        provider: "Unknown".into(),
        recipe: "".into(),
        custom_cmd: None,
    };

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());

    // 1. Check ~/.config/syspilot/config.env
    let env_path = PathBuf::from(&home).join(".config/syspilot/config.env");
    if let Ok(content) = fs::read_to_string(&env_path) {
        let mut parsed_cmd: Option<String> = None;
        let mut custom_label: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || !trimmed.contains('=') {
                continue;
            }
            let mut parts = trimmed.splitn(2, '=');
            let key = parts.next().unwrap_or("").trim();
            let val = parts
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .trim_matches('\'');

            if key == "SYSPILOT_COPILOT_CMD" || key == "COPILOT_CMD" {
                if !val.is_empty() {
                    parsed_cmd = Some(val.to_string());
                }
            } else if key == "COPILOT_LABEL" && !val.is_empty() {
                custom_label = Some(val.to_string());
            } else if key == "SYSPILOT_GOOSE_RECIPE" && !val.is_empty() {
                let recipe_path = Path::new(val);
                if let Some(file_name) = recipe_path.file_name() {
                    info.recipe = file_name.to_string_lossy().to_string();
                } else {
                    info.recipe = val.to_string();
                }
            }
        }

        if let Some(val) = parsed_cmd {
            info.custom_cmd = Some(val.clone());
            info.ready = true;
            if is_agy_command(&val) {
                info.provider = "Antigravity CLI".into();
                info.model = "AGY Agent".into();
            } else if val.contains("hermes") {
                info.provider = "Hermes Agent".into();
                info.model = "Hermes".into();
            } else {
                info.provider = "Custom".into();
                info.model = "Custom Command".into();
            }
        }
        if let Some(label) = custom_label {
            info.model = label;
        }
    }

    // 2. Process environment variables override config file
    if let Ok(val) = std::env::var("SYSPILOT_COPILOT_CMD") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            info.custom_cmd = Some(trimmed.to_string());
            info.ready = true;
            if is_agy_command(trimmed) {
                info.provider = "Antigravity CLI".into();
                info.model = "AGY Agent".into();
            } else if trimmed.contains("hermes") {
                info.provider = "Hermes Agent".into();
                info.model = "Hermes".into();
            } else {
                info.provider = "Custom".into();
                info.model = "Custom Command".into();
            }
        }
    }
    if let Ok(val) = std::env::var("COPILOT_LABEL") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            info.model = trimmed.to_string();
        }
    }

    if info.custom_cmd.is_some() {
        return info;
    }

    // 2. Check ~/.config/goose/config.yaml
    let goose_cfg = PathBuf::from(&home).join(".config/goose/config.yaml");
    if let Ok(content) = fs::read_to_string(&goose_cfg) {
        let mut active_provider = String::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("active_provider:") {
                let parts: Vec<&str> = trimmed.split(':').collect();
                if parts.len() > 1 {
                    active_provider = parts[1].trim().to_string();
                }
            }
        }

        if !active_provider.is_empty() {
            info.provider = match active_provider.as_str() {
                "google" => "Google Gemini".into(),
                "openai" => "OpenAI".into(),
                "anthropic" => "Anthropic".into(),
                "ollama" => "Ollama (Local)".into(),
                other => other.to_string(),
            };

            // Find model under active provider section
            let mut in_provider_section = false;
            let provider_header = format!("{active_provider}:");
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed == provider_header {
                    in_provider_section = true;
                    continue;
                }
                if in_provider_section {
                    if trimmed.starts_with("model:") {
                        let parts: Vec<&str> = trimmed.split(':').collect();
                        if parts.len() > 1 {
                            info.model = parts[1].trim().to_string();
                            info.ready = true;
                            break;
                        }
                    } else if !line.starts_with(' ')
                        && !line.starts_with('\t')
                        && trimmed.ends_with(':')
                    {
                        // Left provider section
                        break;
                    }
                }
            }
        }
    }

    // 3. Fallback: check if goose or agy command is in PATH
    if !info.ready {
        if crate::terminal::find_program("goose").is_some() {
            info.ready = true;
            info.model = "Goose SRE".into();
            info.provider = "Default".into();
        } else if crate::terminal::find_program("agy").is_some() {
            info.ready = true;
            info.model = "Antigravity".into();
            info.provider = "Antigravity CLI".into();
            info.custom_cmd = Some("agy".into());
        }
    }

    if info.recipe.is_empty() {
        if let Some(p) = resolve_goose_recipe() {
            if let Some(name) = p.file_name() {
                info.recipe = name.to_string_lossy().to_string();
            }
        }
    }

    info
}

pub fn get_compact_health() -> String {
    let status_path = crate::models::get_status_path();
    match crate::models::read_system_snapshot(&status_path) {
        Ok(s) => format!(
            "status={};age_s={};errors={};warnings={};updates={};reboot_pending={}",
            s.effective_status(),
            s.age_seconds().unwrap_or(0),
            s.errors,
            s.warnings,
            s.updates_total,
            s.reboot_pending
        ),
        Err(_) => "cache=unavailable".to_string(),
    }
}

pub fn resolve_goose_recipe() -> Option<PathBuf> {
    // 1. Explicit environment override
    if let Some(p) = std::env::var_os("SYSPILOT_GOOSE_RECIPE").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());

    // 2. SysPilot user config (~/.config/syspilot/config.env)
    let env_path = PathBuf::from(&home).join(".config/syspilot/config.env");
    if let Ok(content) = fs::read_to_string(&env_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(val) = trimmed.strip_prefix("SYSPILOT_GOOSE_RECIPE=") {
                let val = val.trim().trim_matches('"').trim_matches('\'');
                let p = PathBuf::from(val);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    // 3. User recipe directory
    for name in ["eos-admin.yaml", "copilot-admin.yaml"] {
        let p = PathBuf::from(&home)
            .join(".config/goose/recipes")
            .join(name);
        if p.is_file() {
            return Some(p);
        }
    }

    // 4. System packaged recipe
    let sys_recipe = PathBuf::from("/usr/share/syspilot/goose/recipes/copilot-admin.yaml");
    if sys_recipe.is_file() {
        return Some(sys_recipe);
    }

    // 5. Development project root
    if let Ok(current) = std::env::current_exe() {
        for ancestor in current.ancestors().take(5) {
            let candidate = ancestor.join("packaging/goose/recipes/copilot-admin.yaml");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

pub fn default_copilot_timeout() -> Duration {
    if let Ok(val) = std::env::var("SYSPILOT_COPILOT_TIMEOUT_SECS") {
        if let Ok(secs) = val.trim().parse::<u64>() {
            if secs > 0 {
                return Duration::from_secs(secs);
            }
        }
    }
    Duration::from_secs(90)
}

async fn run_command_with_timeout(
    mut cmd: Command,
    timeout_dur: Duration,
    desc: &str,
) -> Result<std::process::Output, String> {
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {desc}: {e}"))?;

    match timeout(timeout_dur, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(format!("Failed to execute {desc}: {e}")),
        Err(_) => {
            // Note: Since kill_on_drop(true) is enabled, dropping wait_with_output
            // (which owns child) immediately dispatches SIGKILL to the child process.
            Err(format!(
                "Copilot query timed out after {}s (process terminated)",
                timeout_dur.as_secs()
            ))
        }
    }
}

pub fn execute_copilot_query(query: &str) -> Result<String, String> {
    execute_copilot_query_with_timeout(query, default_copilot_timeout())
}

pub fn execute_copilot_query_with_timeout(
    query: &str,
    timeout_dur: Duration,
) -> Result<String, String> {
    let query_owned = query.to_string();
    let run_async = move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to build tokio runtime: {e}"))?;
        rt.block_on(execute_copilot_query_async(&query_owned, timeout_dur))
    };

    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::spawn(run_async)
            .join()
            .map_err(|_| "Copilot worker thread panicked".to_string())?
    } else {
        run_async()
    }
}

pub async fn execute_copilot_query_async(
    query: &str,
    timeout_dur: Duration,
) -> Result<String, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("Query cannot be empty.".to_string());
    }

    let info = get_copilot_info();

    if let Some(cmd) = info.custom_cmd {
        // Adapt interactive flags for non-interactive execution if agy is used
        let effective_cmd = if is_agy_command(&cmd) {
            adapt_agy_cmd_noninteractive(&cmd)
        } else {
            cmd
        };

        let bash_bin = crate::terminal::find_program("bash")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "bash".to_string());

        let mut command = crate::exec::new_tokio_command_user_locale(&bash_bin);
        command.args(["-c", &format!("{effective_cmd} \"$@\""), "--", trimmed]);

        let output = run_command_with_timeout(
            command,
            timeout_dur,
            &format!("custom Copilot command via {bash_bin}"),
        )
        .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if !output.status.success() {
            return Err(if !stderr.is_empty() {
                stderr
            } else {
                format!("Process exited with status {}", output.status)
            });
        }
        return Ok(if !stdout.is_empty() {
            stdout
        } else {
            "Copilot completed with no output.".to_string()
        });
    }

    // Check if goose is available, fallback to agy
    if crate::terminal::find_program("goose").is_none() {
        if crate::terminal::find_program("agy").is_some() {
            let mut command = crate::exec::new_tokio_command_user_locale("agy");
            command.args(["-p", trimmed]);

            let output = run_command_with_timeout(command, timeout_dur, "agy CLI").await?;

            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if !output.status.success() {
                let err_msg = if !stderr.is_empty() { stderr } else { stdout };
                return Err(err_msg);
            }
            return Ok(if stdout.is_empty() {
                "Query completed successfully. (No stdout returned)".to_string()
            } else {
                stdout
            });
        }
        return Err("Neither Goose CLI nor agy CLI found. Install Goose or configure SYSPILOT_COPILOT_CMD in ~/.config/syspilot/config.env".to_string());
    }

    // Run goose with text query or recipe
    let mut cmd = crate::exec::new_tokio_command_user_locale("goose");
    cmd.arg("run");

    if let Some(recipe_path) = resolve_goose_recipe() {
        let health = get_compact_health();
        let pwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".into());

        cmd.args(["--recipe", recipe_path.to_str().unwrap()]);
        cmd.args(["--params", &format!("user_query={trimmed}")]);
        cmd.args(["--params", &format!("health_context={health}")]);
        cmd.args(["--params", &format!("working_directory={pwd}")]);
    } else {
        cmd.args(["--text", trimmed]);
    }

    cmd.env("GOOSE_MODE", "approve");
    cmd.env("SECURITY_PROMPT_ENABLED", "true");
    cmd.env("GOOSE_DISABLE_SESSION_NAMING", "true");
    cmd.env("GOOSE_CLI_SHOW_COST", "true");

    let output = run_command_with_timeout(cmd, timeout_dur, "Goose CLI").await?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let err_msg = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("Goose exited with code {:?}", output.status.code())
        };
        return Err(err_msg);
    }

    if stdout.is_empty() {
        Ok("Query completed successfully. (No stdout returned)".to_string())
    } else {
        Ok(stdout)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPlaybook {
    pub name: String,
    pub filename: String,
    pub path: PathBuf,
    pub description: String,
}

pub fn discover_user_playbooks_at(dir: &Path) -> Vec<UserPlaybook> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut playbooks = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();
        if !filename.ends_with(".sh") {
            continue;
        }

        if !crate::terminal::is_executable(&path) {
            continue;
        }

        let name = filename
            .strip_suffix(".sh")
            .unwrap_or(&filename)
            .to_string();

        let mut description = String::new();
        if let Ok(content) = fs::read_to_string(&path) {
            for line in content.lines().take(5) {
                let trimmed = line.trim();
                if let Some(desc) = trimmed.strip_prefix("# Description:") {
                    description = desc.trim().to_string();
                    break;
                }
            }
        }

        playbooks.push(UserPlaybook {
            name,
            filename,
            path,
            description,
        });
    }

    playbooks.sort_by(|a, b| a.filename.cmp(&b.filename));
    playbooks.truncate(16);
    playbooks
}

pub fn discover_user_playbooks() -> Vec<UserPlaybook> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let default_dir = home.join(".config/syspilot/playbooks");
    discover_user_playbooks_at(&default_dir)
}

fn match_uuid_at(bytes: &[u8], i: usize) -> bool {
    let Some(s) = bytes.get(i..i + 36) else {
        return false;
    };
    let is_uuid_char = |b: u8| b.is_ascii_alphanumeric() || b == b'-';
    if (i > 0 && is_uuid_char(bytes[i - 1])) || bytes.get(i + 36).is_some_and(|b| is_uuid_char(*b))
    {
        return false;
    }
    let mut pos = 0;
    for (idx, len) in [8usize, 4, 4, 4, 12].into_iter().enumerate() {
        if !s[pos..pos + len].iter().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
        pos += len;
        if idx < 4 {
            if s[pos] != b'-' {
                return false;
            }
            pos += 1;
        }
    }
    true
}

fn match_mac_at(bytes: &[u8], i: usize) -> bool {
    let Some(s) = bytes.get(i..i + 17) else {
        return false;
    };
    let is_mac_char = |b: u8| b.is_ascii_alphanumeric() || b == b':';
    if (i > 0 && is_mac_char(bytes[i - 1])) || bytes.get(i + 17).is_some_and(|b| is_mac_char(*b)) {
        return false;
    }
    (0..6).all(|oct| {
        let base = oct * 3;
        s[base].is_ascii_hexdigit()
            && s[base + 1].is_ascii_hexdigit()
            && (oct == 5 || s[base + 2] == b':')
    })
}

fn match_ipv4_at(bytes: &[u8], i: usize) -> Option<usize> {
    if i > 0 && (bytes[i - 1].is_ascii_digit() || bytes[i - 1] == b'.') {
        return None;
    }
    let mut idx = i;
    let mut octets = [0u16; 4];
    for (slot, octet) in octets.iter_mut().enumerate() {
        let mut val = 0u16;
        let mut count = 0usize;
        while let Some(&b) = bytes.get(idx) {
            if !b.is_ascii_digit() || count >= 3 {
                break;
            }
            val = val * 10 + u16::from(b - b'0');
            idx += 1;
            count += 1;
        }
        if count == 0 || val > 255 {
            return None;
        }
        *octet = val;
        if slot < 3 {
            if bytes.get(idx) != Some(&b'.') {
                return None;
            }
            idx += 1;
        }
    }
    if bytes
        .get(idx)
        .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
    {
        return None;
    }
    if octets == [127, 0, 0, 1] || octets == [0, 0, 0, 0] {
        return None;
    }
    Some(idx - i)
}

fn redact_home_paths(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find("/home/") {
        out.push_str(&rest[..pos + 6]);
        let after = &rest[pos + 6..];
        let user_len = after
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-' || *b == b'.')
            .count();
        if user_len > 0 {
            out.push_str("<USER>");
            rest = &after[user_len..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn redact_hostname_tokens(input: &str, hostname: &str) -> String {
    let host = hostname.trim();
    let bytes = input.as_bytes();
    let host_bytes = host.as_bytes();
    if host.len() < 3 || !host.is_ascii() || bytes.len() < host_bytes.len() {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut skip_to = 0;
    let mut last_copy = 0;
    for i in 0..=bytes.len() - host_bytes.len() {
        if i < skip_to {
            continue;
        }
        let left_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let right_ok = bytes
            .get(i + host_bytes.len())
            .is_none_or(|b| !b.is_ascii_alphanumeric());
        if left_ok && right_ok && bytes[i..i + host_bytes.len()].eq_ignore_ascii_case(host_bytes) {
            out.push_str(&input[last_copy..i]);
            out.push_str("<HOST>");
            skip_to = i + host_bytes.len();
            last_copy = skip_to;
        }
    }
    out.push_str(&input[last_copy..]);
    out
}

fn redact_tokens_and_lines(input: &str) -> String {
    let mut lines_out = Vec::new();
    for line in input.split_inclusive('\n') {
        let has_nl = line.ends_with('\n');
        let body = line.strip_suffix('\n').unwrap_or(line);
        let lower = body.to_ascii_lowercase();
        if lower.contains("machine-id") || lower.contains("serial") {
            if let Some(sep) = body.find([':', '=']) {
                let mut redacted = format!("{} <REDACTED>", &body[..=sep]);
                if has_nl {
                    redacted.push('\n');
                }
                lines_out.push(redacted);
                continue;
            }
        }
        let bytes = body.as_bytes();
        let mut out = String::with_capacity(body.len());
        let mut last_copy = 0;
        for i in 0..bytes.len() {
            if match_uuid_at(bytes, i) {
                out.push_str(&body[last_copy..i]);
                out.push_str("<UUID>");
                last_copy = i + 36;
            } else if match_mac_at(bytes, i) {
                out.push_str(&body[last_copy..i]);
                out.push_str("<MAC>");
                last_copy = i + 17;
            } else if let Some(ip_len) = match_ipv4_at(bytes, i) {
                out.push_str(&body[last_copy..i]);
                out.push_str("<IP>");
                last_copy = i + ip_len;
            }
        }
        out.push_str(&body[last_copy..]);
        if has_nl {
            out.push('\n');
        }
        lines_out.push(out);
    }
    lines_out.concat()
}

pub fn sanitize_telemetry_text_at(root: &Path, input: &str) -> String {
    let hostname = fs::read_to_string(crate::sys_root::sub_path(root, "/etc/hostname"))
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .unwrap_or_default();
    let step1 = redact_home_paths(input);
    let step2 = redact_hostname_tokens(&step1, &hostname);
    redact_tokens_and_lines(&step2)
}

fn is_gpu_kernel_pkg(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "linux",
        "nvidia",
        "lib32-nvidia",
        "mesa",
        "lib32-mesa",
        "vulkan-",
        "xf86-video-",
        "dkms",
    ];
    let lower = name.to_ascii_lowercase();
    PREFIXES.iter().any(|p| lower.starts_with(p))
}

fn parse_alpm_desc_name_version(content: &str) -> Option<(String, String)> {
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed == "%NAME%" {
            name = lines
                .next()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
        } else if trimmed == "%VERSION%" {
            version = lines
                .next()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
        }
    }
    Some((name?, version?))
}

pub fn collect_gpu_kernel_packages_at(root: &Path) -> Vec<String> {
    let db_dir = crate::sys_root::sub_path(root, "/var/lib/pacman/local");
    let Ok(entries) = fs::read_dir(db_dir) else {
        return Vec::new();
    };
    let mut found = BTreeSet::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let desc_path = entry.path().join("desc");
        let Ok(file) = File::open(&desc_path) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_ALPM_DESC_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_ALPM_DESC_BYTES
        {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        if let Some((name, ver)) = parse_alpm_desc_name_version(&text)
            && is_gpu_kernel_pkg(&name)
        {
            found.insert(format!("{name}={ver}"));
        }
    }
    found.into_iter().take(24).collect()
}

pub fn build_ai_telemetry_markdown_at(root: &Path, status_path: &Path) -> String {
    let snap = crate::models::read_system_snapshot_or_fallback(status_path);
    let gpu = crate::models::detect_gpu_at(root);
    let safety = crate::reboot_safety::check_reboot_safety_at(root);
    let gaming = crate::gaming::check_gaming_readiness_at(root);
    let pacnew = crate::triage::find_pacnew_files_at(root);
    let gpu_pkgs = collect_gpu_kernel_packages_at(root);

    let mut md = String::new();
    md.push_str("### SysPilot AI Telemetry\n");
    md.push_str(&format!(
        "- **Flight Status**: {}\n",
        snap.effective_status()
    ));
    md.push_str(&format!(
        "- **OS**: {} | **Kernel**: {}\n",
        snap.os, snap.kernel
    ));
    md.push_str(&format!("- **GPU**: {}\n", gpu));
    if let Some(bios) = &safety.bios_info {
        md.push_str(&format!(
            "- **DMI BIOS**: {} ({})\n",
            bios.version, bios.date
        ));
    }
    for link in &safety.pcie_links {
        let w = match (link.current_width, link.effective_max_width) {
            (Some(c), Some(m)) => format!("x{c}/x{m}"),
            _ => "unknown".to_string(),
        };
        let state = if link.runtime_suspended {
            "D3cold suspended"
        } else if link.degraded {
            link.detail.as_deref().unwrap_or("DEGRADED")
        } else {
            "nominal"
        };
        md.push_str(&format!(
            "- **PCIe Link ({})**: width={w}, speed={} / {}, state={state}\n",
            link.bdf, link.current_speed, link.max_speed
        ));
    }
    if !gpu_pkgs.is_empty() {
        md.push_str(&format!(
            "- **Kernel/GPU Packages**: {}\n",
            gpu_pkgs.join(", ")
        ));
    }
    md.push_str(&format!(
        "- **Updates**: {} total ({} core, {} regular, {} AUR) [State: {:?}]\n",
        snap.updates_total,
        snap.core_updates,
        snap.regular_updates,
        snap.aur_updates,
        snap.updates_check_state
    ));
    if snap.standalone_updates > 0 {
        let tools = if snap.standalone_tools.is_empty() {
            "standalone tools".to_string()
        } else {
            snap.standalone_tools.join(", ")
        };
        md.push_str(&format!(
            "- **Standalone Tools**: {} pending ({tools})\n",
            snap.standalone_updates
        ));
    }
    md.push_str(&format!(
        "- **Reboot Safety**: Pending={}, BlackScreenRisk={}, PcieDegraded={}\n",
        snap.reboot_pending, safety.black_screen_risk, safety.pcie_degraded
    ));
    if !safety.issues.is_empty() {
        md.push_str(&format!(
            "  - Safety Issues: {}\n",
            safety.issues.join("; ")
        ));
    }
    md.push_str(&format!(
        "- **Gaming Readiness**: Applicable={}, Ready={}\n",
        gaming.applicable, gaming.ready
    ));
    if !gaming.warnings.is_empty() {
        md.push_str(&format!(
            "  - Gaming Warnings: {}\n",
            gaming.warnings.join("; ")
        ));
    }
    md.push_str(&format!("- **Pacnew Count**: {}\n", pacnew.len()));
    let gpu_alerts: Vec<String> = snap
        .actionable_remediations
        .iter()
        .filter(|r| {
            r.code.as_deref().is_some_and(|c| {
                c.starts_with("GPU_") || c.starts_with("PCIE_") || c.starts_with("PREV_BOOT_GPU")
            })
        })
        .map(|r| {
            let code = r.code.as_deref().unwrap_or("");
            let sum = r.summary.as_deref().unwrap_or("");
            format!("{code}: {sum}")
        })
        .collect();
    if !gpu_alerts.is_empty() {
        md.push_str(&format!(
            "- **GPU/PCIe Alerts**: {}\n",
            gpu_alerts.join("; ")
        ));
    }
    if !snap.status_reasons.is_empty() {
        md.push_str(&format!(
            "- **Status Reasons**: {}\n",
            snap.status_reasons.join("; ")
        ));
    }
    if let Some(err) = &snap.sys_health_parse_error {
        md.push_str(&format!("- **SysHealth Parse Error**: {}\n", err));
    }
    if snap.mirrors.is_some() {
        let m = crate::view::format_mirrorlist_state(snap.mirrors.as_ref());
        md.push_str(&format!("- **Mirrors**: {} ({})\n", m.status, m.details));
    }
    sanitize_telemetry_text_at(root, &md)
}

pub fn build_ai_telemetry_markdown() -> String {
    let status_path = crate::models::get_status_path();
    build_ai_telemetry_markdown_at(Path::new("/"), &status_path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuCrashBundleReport {
    pub bundle_path: PathBuf,
    pub markdown: String,
    pub fault_count: usize,
}

fn resolve_copilot_journalctl_bin() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(override_bin) = TEST_COPILOT_JOURNALCTL_BIN.with(|b| b.borrow().clone()) {
        return override_bin;
    }
    Some(PathBuf::from("journalctl"))
}

fn is_relevant_gpu_kernel_line(line: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "nvrm",
        "nvidia",
        "amdgpu",
        "i915",
        "pcieport",
        "drm",
        "kernel panic",
        "oops:",
    ];
    if crate::previous_boot::classify_gpu_pcie_fault(line).is_some() {
        return true;
    }
    let lower = line.to_ascii_lowercase();
    KEYWORDS.iter().any(|kw| lower.contains(kw))
}

pub(crate) fn cap_bundle_markdown(mut text: String) -> String {
    if text.len() <= MAX_BUNDLE_BYTES {
        return text;
    }
    let max_cut = MAX_BUNDLE_BYTES - BUNDLE_TRUNCATED_SUFFIX.len();
    let cut = text
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|&idx| idx <= max_cut)
        .last()
        .unwrap_or(0);
    text.truncate(cut);
    text.push_str(BUNDLE_TRUNCATED_SUFFIX);
    text
}

fn write_bundle_atomic_0600(dest_path: &Path, content: &str) -> Result<(), String> {
    let parent = dest_path
        .parent()
        .ok_or_else(|| "invalid bundle destination path".to_string())?;
    let mut dir_builder = fs::DirBuilder::new();
    dir_builder.recursive(true).mode(0o700);
    dir_builder
        .create(parent)
        .map_err(|e| format!("failed to create bundle directory: {e}"))?;

    let pid = unsafe { libc::getpid() };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path = parent.join(format!(".gpu-crash-bundle.md.tmp.{pid}.{nanos}"));
    let write_res = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp_path)?;
        f.write_all(content.as_bytes())?;
        f.flush()?;
        f.sync_all()?;
        Ok(())
    })();
    if let Err(e) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("failed to write bundle: {e}"));
    }
    if let Err(e) = fs::rename(&tmp_path, dest_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("failed to finalize bundle: {e}"));
    }
    Ok(())
}

async fn capture_kernel_journal_lines(bin: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let mut cmd = crate::exec::new_tokio_command(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let timeout_dur = Duration::from_millis(2500);
    let out = match timeout(timeout_dur, cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(format!("spawn failed: {e}")),
        Err(_) => return Err("journalctl timed out".to_string()),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("exit {}", out.status)
        } else {
            err
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout
        .lines()
        .filter(|l| is_relevant_gpu_kernel_line(l))
        .map(|l| l.trim().to_string())
        .collect())
}

pub fn export_gpu_crash_bundle_at(
    root: &Path,
    status_path: &Path,
    state_dir: &Path,
) -> Result<GpuCrashBundleReport, String> {
    let mut md = build_ai_telemetry_markdown_at(root, status_path);
    md.push_str("\n### Kernel & Previous-Boot GPU/PCIe Forensics\n");

    let mut fault_count = 0usize;
    if let Some(bin) = resolve_copilot_journalctl_bin() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("failed to build tokio runtime: {e}"))?;
        let (b0_res, b1_res) = rt.block_on(async {
            tokio::join!(
                capture_kernel_journal_lines(
                    &bin,
                    &[
                        "-b",
                        "0",
                        "-k",
                        "-p",
                        "0..4",
                        "--output=cat",
                        "--lines=120",
                        "--no-pager"
                    ]
                ),
                capture_kernel_journal_lines(
                    &bin,
                    &[
                        "-b",
                        "-1",
                        "-k",
                        "-p",
                        "0..4",
                        "--output=cat",
                        "--lines=120",
                        "--no-pager"
                    ]
                )
            )
        });
        for (label, res) in [
            ("Current Boot (-b 0)", b0_res),
            ("Previous Boot (-b -1)", b1_res),
        ] {
            match res {
                Ok(lines) if lines.is_empty() => {
                    md.push_str(&format!("- **{label}**: no GPU/PCIe fault lines\n"));
                }
                Ok(lines) => {
                    md.push_str(&format!("- **{label}**:\n"));
                    for line in lines {
                        if crate::previous_boot::classify_gpu_pcie_fault(&line).is_some() {
                            fault_count += 1;
                        }
                        md.push_str(&format!(
                            "  - {}\n",
                            sanitize_telemetry_text_at(root, &line)
                        ));
                    }
                }
                Err(err) => {
                    md.push_str(&format!(
                        "- **{label}**: unavailable ({})\n",
                        sanitize_telemetry_text_at(root, &err)
                    ));
                }
            }
        }
    } else {
        md.push_str("- **Journal Probe**: journalctl binary unavailable\n");
    }

    let final_md = cap_bundle_markdown(sanitize_telemetry_text_at(root, &md));
    let dest_path = state_dir.join("syspilot/gpu-crash-bundle.md");
    write_bundle_atomic_0600(&dest_path, &final_md)?;

    Ok(GpuCrashBundleReport {
        bundle_path: dest_path,
        markdown: final_md,
        fault_count,
    })
}

pub fn export_gpu_crash_bundle() -> Result<GpuCrashBundleReport, String> {
    let status_path = crate::models::get_status_path();
    let state_dir = crate::models::state_base();
    export_gpu_crash_bundle_at(Path::new("/"), &status_path, &state_dir)
}
