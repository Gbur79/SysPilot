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
                    info.custom_cmd = Some(val.to_string());
                    info.ready = true;
                    info.model = "Custom Command".into();
                    info.provider = "Custom".into();
                }
            } else if key == "COPILOT_LABEL" {
                if !val.is_empty() {
                    info.model = val.to_string();
                }
            } else if key == "SYSPILOT_GOOSE_RECIPE" && !val.is_empty() {
                let recipe_path = Path::new(val);
                if let Some(file_name) = recipe_path.file_name() {
                    info.recipe = file_name.to_string_lossy().to_string();
                } else {
                    info.recipe = val.to_string();
                }
            }
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

    // 3. Fallback: check if goose command is in PATH
    if !info.ready && crate::terminal::find_program("goose").is_some() {
        info.ready = true;
        info.model = "Goose SRE".into();
        info.provider = "Default".into();
    }

    info
}

pub fn execute_copilot_query(query: &str) -> Result<String, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("Query cannot be empty.".to_string());
    }

    let info = get_copilot_info();

    if let Some(cmd) = info.custom_cmd {
        let output = Command::new("/usr/bin/bash")
            .args(["-c", &format!("{cmd} \"$@\"", cmd = cmd), "--", trimmed])
            .output()
            .map_err(|e| format!("Failed to execute custom Copilot command: {e}"))?;

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

    // Check if goose is available
    if crate::terminal::find_program("goose").is_none() {
        return Err("Goose CLI is not installed or not in PATH. Install Goose or configure SYSPILOT_COPILOT_CMD in ~/.config/syspilot/config.env".to_string());
    }

    // Run goose with text query
    let mut cmd = Command::new("goose");
    cmd.args(["run", "--text", trimmed]);

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let recipe_path = PathBuf::from(&home).join(".config/goose/recipes/eos-admin.yaml");
    if recipe_path.is_file() {
        cmd.args(["--recipe", recipe_path.to_str().unwrap()]);
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
