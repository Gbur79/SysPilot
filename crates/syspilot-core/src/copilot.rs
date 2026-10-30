use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

pub fn execute_copilot_query(query: &str) -> Result<String, String> {
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

        let output = Command::new(&bash_bin)
            .args(["-c", &format!("{effective_cmd} \"$@\""), "--", trimmed])
            .output()
            .map_err(|e| format!("Failed to execute custom Copilot command via {bash_bin}: {e}"))?;

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
            let output = Command::new("agy")
                .args(["-p", trimmed])
                .output()
                .map_err(|e| format!("Failed to execute agy CLI: {e}"))?;
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
    let mut cmd = Command::new("goose");
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

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to launch Goose CLI: {e}"))?;
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
