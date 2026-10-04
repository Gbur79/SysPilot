"""
SysPilot Copilot Configuration & Onboarding Manager
Handles seamless, zero-friction setup of Goose AI, API keys, and SRE skills for laymen.
"""

import os
import sys
import re
import yaml
import shutil
import subprocess
from typing import Dict, Any, Tuple, Optional, List

GOOSE_CONFIG_DIR = os.path.expanduser("~/.config/goose")
GOOSE_CONFIG_FILE = os.path.join(GOOSE_CONFIG_DIR, "config.yaml")
GOOSE_SECRETS_FILE = os.path.join(GOOSE_CONFIG_DIR, "secrets.yaml")
GOOSE_SKILLS_DIR = os.path.join(GOOSE_CONFIG_DIR, "skills")

PROJECT_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
SYS_PILOT_SKILL_SRC = os.path.join(PROJECT_ROOT, "copilot", "skills", "sys-pilot-admin")
SYS_PILOT_SKILL_FILE = os.path.join(SYS_PILOT_SKILL_SRC, "SKILL.md")
PLAYBOOKS_DIR = os.path.join(PROJECT_ROOT, "copilot", "playbooks")

USER_CONFIG_DIR = os.path.expanduser("~/.config/syspilot")
USER_DIRECTIVES_FILE = os.path.join(USER_CONFIG_DIR, "user_directives.md")

DIRECTIVES_REGEX = re.compile(
    r'<!-- USER_CUSTOM_DIRECTIVES_START -->(.*?)<!-- USER_CUSTOM_DIRECTIVES_END -->',
    re.DOTALL
)


def find_goose_binary() -> Optional[str]:
    """Locate goose binary in PATH or ~/.local/bin."""
    bin_path = shutil.which("goose")
    if bin_path:
        return bin_path
    user_bin = os.path.expanduser("~/.local/bin/goose")
    if os.path.isfile(user_bin) and os.access(user_bin, os.X_OK):
        return user_bin
    return None


def is_goose_installed() -> bool:
    """Return True if goose binary exists and is executable."""
    return find_goose_binary() is not None


def ensure_syspilot_skill_linked() -> bool:
    """Ensure the sys-pilot-admin skill is symlinked into ~/.config/goose/skills."""
    try:
        os.makedirs(GOOSE_SKILLS_DIR, exist_ok=True)
        target = os.path.join(GOOSE_SKILLS_DIR, "sys-pilot-admin")
        if not os.path.exists(target) or not os.path.islink(target):
            if os.path.exists(target):
                os.remove(target)
            os.symlink(SYS_PILOT_SKILL_SRC, target)
        return True
    except Exception:
        return False


def get_configured_providers() -> Dict[str, bool]:
    """Check which providers have API keys configured in secrets.yaml or config.yaml."""
    results = {
        "google": False,
        "openai": False,
        "anthropic": False,
        "ollama": False
    }

    # 1. Check secrets.yaml
    if os.path.isfile(GOOSE_SECRETS_FILE):
        try:
            with open(GOOSE_SECRETS_FILE, "r", encoding="utf-8") as f:
                secrets = yaml.safe_load(f) or {}
                if secrets.get("GOOGLE_API_KEY") or secrets.get("GEMINI_API_KEY"):
                    results["google"] = True
                if secrets.get("OPENAI_API_KEY"):
                    results["openai"] = True
                if secrets.get("ANTHROPIC_API_KEY"):
                    results["anthropic"] = True
        except Exception:
            pass

    # 2. Check config.yaml for local ollama or custom hosts
    if os.path.isfile(GOOSE_CONFIG_FILE):
        try:
            with open(GOOSE_CONFIG_FILE, "r", encoding="utf-8") as f:
                cfg = yaml.safe_load(f) or {}
                providers = cfg.get("providers", {})
                if "ollama" in providers and providers["ollama"].get("enabled", False):
                    results["ollama"] = True
        except Exception:
            pass

    # 3. Check environment variables as fallback
    if os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY"):
        results["google"] = True
    if os.environ.get("OPENAI_API_KEY"):
        results["openai"] = True
    if os.environ.get("ANTHROPIC_API_KEY"):
        results["anthropic"] = True

    return results


def is_copilot_ready() -> Tuple[bool, str]:
    """Verify if Goose is installed and at least one provider is configured."""
    if not is_goose_installed():
        return False, "Goose CLI is not installed."
    
    providers = get_configured_providers()
    if not any(providers.values()):
        return False, "No AI provider API key found."

    ensure_syspilot_skill_linked()
    return True, "Ready"


def get_active_model_details() -> Dict[str, Any]:
    """
    Retrieve currently active LLM provider and model from Goose configuration (~/.config/goose/config.yaml).
    Returns a dictionary with keys:
      - provider: str (e.g. 'google', 'openai', 'anthropic', 'ollama')
      - provider_display: str (e.g. 'Google Gemini', 'OpenAI', 'Anthropic Claude', 'Ollama')
      - model: str (e.g. 'gemini-3.8-flash', 'gpt-5.6-sol')
      - full_label: str (e.g. 'gemini-3.8-flash (Google Gemini)')
      - thinking_effort: str (e.g. 'high', 'medium', 'low')
    """
    provider = "google"
    model = "gemini-3.8-flash"
    thinking = "default"

    if os.path.isfile(GOOSE_CONFIG_FILE):
        try:
            with open(GOOSE_CONFIG_FILE, "r", encoding="utf-8") as f:
                cfg = yaml.safe_load(f) or {}
                provider = cfg.get("active_provider") or "google"
                thinking = cfg.get("GOOSE_THINKING_EFFORT") or "default"
                providers_dict = cfg.get("providers", {})
                if provider in providers_dict and isinstance(providers_dict[provider], dict):
                    model = providers_dict[provider].get("model") or model
        except Exception:
            pass

    display_map = {
        "google": "Google Gemini",
        "openai": "OpenAI",
        "anthropic": "Anthropic Claude",
        "ollama": "Local Ollama"
    }
    p_disp = display_map.get(provider.lower(), provider.capitalize())

    return {
        "provider": provider,
        "provider_display": p_disp,
        "model": model,
        "full_label": f"{model} ({p_disp})",
        "thinking_effort": thinking
    }


def configure_provider(provider_type: str, api_key: str, model: Optional[str] = None) -> Tuple[bool, str]:
    """
    Save provider key and enable it in Goose configuration safely.
    provider_type: 'google' (default), 'openai', 'anthropic', or 'ollama'
    """
    os.makedirs(GOOSE_CONFIG_DIR, exist_ok=True)
    api_key = api_key.strip()

    # Determine key name and default model
    key_env_var = "GOOGLE_API_KEY"
    default_model = "gemini-3.8-flash"

    if provider_type == "google":
        key_env_var = "GOOGLE_API_KEY"
        default_model = model or "gemini-3.8-flash"
    elif provider_type == "openai":
        key_env_var = "OPENAI_API_KEY"
        default_model = model or "gpt-4o-mini"
    elif provider_type == "anthropic":
        key_env_var = "ANTHROPIC_API_KEY"
        default_model = model or "claude-3-5-haiku-20241022"
    elif provider_type == "ollama":
        key_env_var = None
        default_model = model or "qwen2.5-coder:7b"

    # 1. Update secrets.yaml
    if key_env_var:
        secrets_data = {}
        if os.path.isfile(GOOSE_SECRETS_FILE):
            try:
                with open(GOOSE_SECRETS_FILE, "r", encoding="utf-8") as f:
                    secrets_data = yaml.safe_load(f) or {}
            except Exception:
                secrets_data = {}

        secrets_data[key_env_var] = api_key

        # Save with strict 0600 permissions
        try:
            with open(GOOSE_SECRETS_FILE, "w", encoding="utf-8") as f:
                yaml.dump(secrets_data, f, default_flow_style=False)
            os.chmod(GOOSE_SECRETS_FILE, 0o600)
        except Exception as e:
            return False, f"Failed to save secrets: {e}"

    # 2. Update config.yaml
    config_data = {}
    if os.path.isfile(GOOSE_CONFIG_FILE):
        try:
            with open(GOOSE_CONFIG_FILE, "r", encoding="utf-8") as f:
                config_data = yaml.safe_load(f) or {}
        except Exception:
            config_data = {}

    if not config_data:
        # Base minimal config
        config_data = {
            "GOOSE_TELEMETRY_ENABLED": False,
            "active_provider": provider_type,
            "providers": {},
            "extensions": {
                "developer": {"enabled": True, "type": "platform", "name": "developer", "bundled": True},
                "skills": {"enabled": True, "type": "platform", "name": "skills", "bundled": True}
            }
        }

    config_data["active_provider"] = provider_type
    if "providers" not in config_data or not isinstance(config_data["providers"], dict):
        config_data["providers"] = {}

    config_data["providers"][provider_type] = {
        "enabled": True,
        "configured": True,
        "model": default_model
    }

    try:
        with open(GOOSE_CONFIG_FILE, "w", encoding="utf-8") as f:
            yaml.dump(config_data, f, default_flow_style=False)
    except Exception as e:
        return False, f"Failed to update config: {e}"

    # 3. Ensure skill is symlinked
    ensure_syspilot_skill_linked()

    return True, f"Successfully configured {provider_type} ({default_model})!"


# ------------------------------------------------------------------------------
# SRE Skill Inspection & User Directives
# ------------------------------------------------------------------------------

def get_skill_info() -> Dict[str, Any]:
    """Retrieve full details of the sys-pilot-admin skill."""
    ensure_syspilot_skill_linked()
    
    model_details = get_active_model_details()
    info = {
        "name": "sys-pilot-admin",
        "description": "Token-Lean Autonomous SRE Systems Copilot for Arch Linux & derivatives",
        "model": model_details.get("full_label", "gemini-3.8-flash (Google Gemini)"),
        "skill_path": SYS_PILOT_SKILL_FILE,
        "is_linked": os.path.exists(os.path.join(GOOSE_SKILLS_DIR, "sys-pilot-admin")),
        "raw_content": "",
        "custom_directives": "",
        "playbooks": list_available_playbooks()
    }

    if os.path.isfile(SYS_PILOT_SKILL_FILE):
        try:
            with open(SYS_PILOT_SKILL_FILE, "r", encoding="utf-8") as f:
                info["raw_content"] = f.read()
        except Exception:
            pass

    # Read custom directives from ~/.config/syspilot/user_directives.md (isolated from git)
    if os.path.isfile(USER_DIRECTIVES_FILE):
        try:
            with open(USER_DIRECTIVES_FILE, "r", encoding="utf-8") as f:
                info["custom_directives"] = f.read().strip()
        except Exception:
            info["custom_directives"] = ""
    elif info["raw_content"]:
        # Fallback to reading legacy block inside skill file if present
        match = DIRECTIVES_REGEX.search(info["raw_content"])
        if match:
            info["custom_directives"] = match.group(1).strip()

    return info


def save_custom_directives(directives_text: str) -> Tuple[bool, str]:
    """Save user-custom directives into ~/.config/syspilot/user_directives.md safely."""
    try:
        os.makedirs(USER_CONFIG_DIR, exist_ok=True)
        with open(USER_DIRECTIVES_FILE, "w", encoding="utf-8") as f:
            f.write(directives_text.strip() + "\n")
        return True, "User custom directives saved to ~/.config/syspilot/user_directives.md!"
    except Exception as e:
        return False, f"Failed to save directives: {str(e)}"


def list_available_playbooks() -> List[Dict[str, str]]:
    """List all available surgical playbooks in copilot/playbooks/."""
    playbooks = []
    if os.path.isdir(PLAYBOOKS_DIR):
        for fname in sorted(os.listdir(PLAYBOOKS_DIR)):
            if fname.endswith(".md"):
                fpath = os.path.join(PLAYBOOKS_DIR, fname)
                title = fname.replace("_", " ").replace(".md", "").capitalize()
                try:
                    with open(fpath, "r", encoding="utf-8") as f:
                        first_line = f.readline().strip()
                        if first_line.startswith("#"):
                            title = first_line.lstrip("#").strip()
                except Exception:
                    pass
                playbooks.append({
                    "filename": fname,
                    "title": title,
                    "path": fpath
                })
    return playbooks


def get_playbook_content(filename: str) -> str:
    """Read the markdown content of a playbook."""
    fpath = os.path.join(PLAYBOOKS_DIR, filename)
    if os.path.isfile(fpath):
        try:
            with open(fpath, "r", encoding="utf-8") as f:
                return f.read()
        except Exception as e:
            return f"Error reading playbook: {e}"
    return f"Playbook not found: {filename}"
