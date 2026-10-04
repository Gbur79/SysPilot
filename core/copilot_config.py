"""
SysPilot Copilot Configuration & Onboarding Manager
Handles seamless, zero-friction setup of Goose AI, API keys, and SRE skills for laymen.
"""

import os
import sys
import yaml
import shutil
import subprocess
from typing import Dict, Any, Tuple, Optional

GOOSE_CONFIG_DIR = os.path.expanduser("~/.config/goose")
GOOSE_CONFIG_FILE = os.path.join(GOOSE_CONFIG_DIR, "config.yaml")
GOOSE_SECRETS_FILE = os.path.join(GOOSE_CONFIG_DIR, "secrets.yaml")
GOOSE_SKILLS_DIR = os.path.join(GOOSE_CONFIG_DIR, "skills")

PROJECT_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
SYS_PILOT_SKILL_SRC = os.path.join(PROJECT_ROOT, "copilot", "skills", "sys-pilot-admin")


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


def configure_provider(provider_type: str, api_key: str, model: Optional[str] = None) -> Tuple[bool, str]:
    """
    Save provider key and enable it in Goose configuration safely.
    provider_type: 'google' (default), 'openai', 'anthropic', or 'ollama'
    """
    os.makedirs(GOOSE_CONFIG_DIR, exist_ok=True)
    api_key = api_key.strip()

    # Determine key name and default model
    key_env_var = "GOOGLE_API_KEY"
    default_model = "gemini-2.5-flash"

    if provider_type == "google":
        key_env_var = "GOOGLE_API_KEY"
        default_model = model or "gemini-2.5-flash"
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
