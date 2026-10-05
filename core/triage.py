#!/usr/bin/env python3
"""
SysPilot Core Triage Engine
High-speed, zero-sudo system diagnostics & state evaluator.
Portability: 100% dynamic, multi-hardware, Arch / EndeavourOS / CachyOS / Manjaro.
"""

import os
import sys
import json
import re
import shutil
import subprocess
from datetime import datetime
from typing import Dict, Any, List

# Core system packages regex that warrant elevated upgrade precautions
CORE_PKG_PATTERN = re.compile(
    r'^(linux([-_].*)?|systemd([-_].*)?|glibc|dracut([-_].*)?|mkinitcpio([-_].*)?|'
    r'booster([-_].*)?|grub([-_].*)?|systemd-boot|limine([-_].*)?|refind([-_].*)?|'
    r'nvidia([-_].*)?|amdgpu([-_].*)?|mesa([-_].*)?|vulkan([-_].*)?|wayland([-_].*)?|'
    r'xorg([-_].*)?|pipewire([-_].*)?|wireplumber([-_].*)?|dkms([-_].*)?)$',
    re.IGNORECASE
)

STATE_DIR = os.path.expanduser("~/.local/state/syspilot")
STATUS_FILE = os.path.join(STATE_DIR, "status.json")
SYS_HEALTH_SUMMARY = os.path.expanduser("~/.local/state/system-health/summary.json")


def is_gamemode_active() -> bool:
    """Check if Feral GameMode is actively running or a game is holding lock."""
    # 1. Try D-Bus property query (0 token, 1ms)
    try:
        res = subprocess.run(
            [
                "busctl", "--user", "get-property",
                "com.feralinteractive.GameMode",
                "/com/feralinteractive/GameMode",
                "com.feralinteractive.GameMode",
                "ClientCount"
            ],
            capture_output=True, text=True, timeout=0.8
        )
        if res.returncode == 0 and "i " in res.stdout:
            parts = res.stdout.strip().split()
            if len(parts) >= 2 and parts[1].isdigit() and int(parts[1]) > 0:
                return True
    except Exception:
        pass

    # 2. Fallback to gamemoded -s
    if shutil.which("gamemoded"):
        try:
            res = subprocess.run(["gamemoded", "-s"], capture_output=True, text=True, timeout=0.8)
            if "gamemode is active" in res.stdout.lower():
                return True
        except Exception:
            pass

    return False


def check_failed_services() -> Dict[str, List[str]]:
    """Query system and user systemd managers for failed units."""
    result = {"system": [], "user": []}
    
    # System services
    try:
        res = subprocess.run(
            ["systemctl", "--failed", "--no-legend", "--plain"],
            capture_output=True, text=True, timeout=1.5
        )
        if res.returncode == 0:
            for line in res.stdout.strip().splitlines():
                parts = line.split()
                if parts:
                    result["system"].append(parts[0])
    except Exception:
        pass

    # User services
    try:
        res = subprocess.run(
            ["systemctl", "--user", "--failed", "--no-legend", "--plain"],
            capture_output=True, text=True, timeout=1.5
        )
        if res.returncode == 0:
            for line in res.stdout.strip().splitlines():
                parts = line.split()
                if parts:
                    result["user"].append(parts[0])
    except Exception:
        pass

    return result


def check_disk_space() -> Dict[str, Any]:
    """Check root and home mountpoints usage."""
    disks = {}
    for mount in ["/", "/home"]:
        try:
            res = subprocess.run(["df", "-Pk", mount], capture_output=True, text=True, timeout=1.0)
            lines = res.stdout.strip().splitlines()
            if len(lines) > 1:
                parts = lines[1].split()
                total_gb = round(int(parts[1]) / (1024 * 1024), 1)
                avail_gb = round(int(parts[3]) / (1024 * 1024), 1)
                used_pct = int(parts[4].replace("%", ""))
                mount_name = "root" if mount == "/" else "home"
                disks[mount_name] = {
                    "path": mount,
                    "total_gb": total_gb,
                    "avail_gb": avail_gb,
                    "used_pct": used_pct
                }
        except Exception:
            pass
    return disks


def check_pacnew_files() -> List[str]:
    """Search /etc for pacnew files without root."""
    pacnew = []
    try:
        res = subprocess.run(
            ["find", "/etc", "-maxdepth", "4", "-name", "*.pacnew"],
            capture_output=True, text=True, timeout=1.5
        )
        if res.returncode == 0:
            pacnew = [f for f in res.stdout.strip().splitlines() if f]
    except Exception:
        pass
    return pacnew


def check_reboot_pending() -> bool:
    """Check if the currently running kernel was uninstalled/replaced by an update."""
    running_kernel = os.uname().release
    module_dir = f"/usr/lib/modules/{running_kernel}"
    if not os.path.exists(module_dir):
        return True
    return False


def check_orphan_packages() -> Dict[str, Any]:
    """Query pacman for unrequired orphan dependency packages (strict -Qtdq and optional -Qdttq)."""
    strict_orphans = []
    optional_candidates = []
    if shutil.which("pacman"):
        try:
            # 1. Strict orphans (neither required nor optionally required)
            res1 = subprocess.run(
                ["pacman", "-Qtdq"],
                capture_output=True, text=True, timeout=3.0
            )
            if res1.returncode == 0 and res1.stdout.strip():
                strict_orphans = [p.strip() for p in res1.stdout.strip().splitlines() if p.strip()]

            # 2. Extended candidate dependencies (-Qdttq)
            res2 = subprocess.run(
                ["pacman", "-Qdttq"],
                capture_output=True, text=True, timeout=3.0
            )
            if res2.returncode == 0 and res2.stdout.strip():
                all_candidates = [p.strip() for p in res2.stdout.strip().splitlines() if p.strip()]
                optional_candidates = [p for p in all_candidates if p not in strict_orphans]
        except Exception:
            pass

    return {
        "count": len(strict_orphans),
        "packages": strict_orphans,
        "optional_count": len(optional_candidates),
        "optional_packages": optional_candidates
    }


def check_updates(skip_network: bool = False) -> Dict[str, Any]:
    """Check repository and AUR updates without root."""
    updates = {
        "total": 0,
        "core_count": 0,
        "regular_count": 0,
        "aur_count": 0,
        "core_packages": [],
        "regular_packages": [],
        "aur_packages": []
    }

    if skip_network:
        return updates

    # 1. Official repositories (checkupdates)
    if shutil.which("checkupdates"):
        try:
            res = subprocess.run(["checkupdates"], capture_output=True, text=True, timeout=12.0)
            # Auto-heal stale checkup-db lock if checkupdates failed due to lock
            if res.returncode == 1 and "database is locked" in (res.stderr or "").lower():
                uid = os.getuid()
                db_lck = f"/tmp/checkup-db-{uid}/db.lck"
                pgrep = subprocess.run(["pgrep", "-x", "checkupdates"], capture_output=True)
                if pgrep.returncode != 0 and os.path.exists(db_lck):
                    try:
                        os.remove(db_lck)
                        res = subprocess.run(["checkupdates"], capture_output=True, text=True, timeout=12.0)
                    except Exception:
                        pass

            if res.returncode == 0 and res.stdout.strip():
                for line in res.stdout.strip().splitlines():
                    parts = line.split()
                    if parts:
                        pkg_name = parts[0]
                        if CORE_PKG_PATTERN.match(pkg_name):
                            updates["core_packages"].append(line)
                            updates["core_count"] += 1
                        else:
                            updates["regular_packages"].append(line)
                            updates["regular_count"] += 1
        except Exception:
            pass

    # 2. AUR updates (yay or paru)
    aur_helper = None
    if shutil.which("yay"):
        aur_helper = ["yay", "-Qua"]
    elif shutil.which("paru"):
        aur_helper = ["paru", "-Qua"]

    if aur_helper:
        try:
            res = subprocess.run(aur_helper, capture_output=True, text=True, timeout=5.0)
            if res.returncode == 0 and res.stdout.strip():
                for line in res.stdout.strip().splitlines():
                    if line.strip():
                        updates["aur_packages"].append(line)
                        updates["aur_count"] += 1
        except Exception:
            pass

    updates["total"] = updates["core_count"] + updates["regular_count"] + updates["aur_count"]
    return updates


def check_standalone_software(skip: bool = False) -> Dict[str, Any]:
    """Check standalone & third-party software updates (AUR, Flatpak, UV, Goose, Steam)."""
    default_summary = {
        "checked": False,
        "total_updates": 0,
        "aur_pending": 0,
        "flatpak_pending": 0,
        "goose_update": False,
        "uv_update": False,
        "details": {}
    }
    if skip:
        return default_summary

    script_path = os.path.join(os.path.dirname(os.path.dirname(__file__)), "bin", "sys-health.sh")
    if not os.path.isfile(script_path):
        return default_summary

    try:
        res = subprocess.run([script_path, "--software", "--json"], capture_output=True, text=True, timeout=8.0)
        if res.returncode == 0 and res.stdout.strip():
            data = json.loads(res.stdout.strip())
            aur_p = data.get("aur", {}).get("pending_count", 0)
            goose_u = data.get("goose", {}).get("update_available", False)
            uv_u = data.get("uv", {}).get("update_available", False)
            flatpak_p = 1 if data.get("flatpak", {}).get("update_available", False) else 0

            tot = aur_p + flatpak_p + (1 if goose_u else 0) + (1 if uv_u else 0)
            return {
                "checked": True,
                "total_updates": tot,
                "aur_pending": aur_p,
                "flatpak_pending": flatpak_p,
                "goose_update": goose_u,
                "uv_update": uv_u,
                "details": data
            }
    except Exception:
        pass

    return default_summary


def get_sys_health_status() -> Dict[str, Any]:
    """Read latest sys-health.sh audit summary if available."""
    if os.path.isfile(SYS_HEALTH_SUMMARY):
        try:
            with open(SYS_HEALTH_SUMMARY, "r", encoding="utf-8") as f:
                data = json.load(f)
                return {
                    "available": True,
                    "timestamp": data.get("timestamp"),
                    "status": data.get("status", "UNKNOWN"),
                    "errors": data.get("counts", {}).get("errors", 0),
                    "warnings": data.get("counts", {}).get("warnings", 0),
                    "environment": data.get("environment", {}),
                    "gaming": data.get("gaming", {})
                }
        except Exception:
            pass
    return {"available": False}


def run_triage(check_pkgs: bool = True) -> Dict[str, Any]:
    """Execute complete triage and compute overall flight readiness."""
    os.makedirs(STATE_DIR, exist_ok=True)
    
    # Read existing status file to preserve package & standalone state during light checks
    cached_payload = {}
    if os.path.isfile(STATUS_FILE):
        try:
            with open(STATUS_FILE, "r", encoding="utf-8") as f:
                cached_payload = json.load(f)
        except Exception:
            cached_payload = {}

    gamemode_on = is_gamemode_active()
    
    # If in active game, inhibit heavy package checks
    if gamemode_on and check_pkgs:
        check_pkgs = False

    failed_units = check_failed_services()
    disks = check_disk_space()
    pacnew = check_pacnew_files()
    orphans = check_orphan_packages()
    reboot_pending = check_reboot_pending()

    # If skipping packages, preserve existing cached package telemetry instead of wiping to 0
    if not check_pkgs and cached_payload.get("updates"):
        updates = cached_payload.get("updates", {})
        standalone = cached_payload.get("standalone_software", {})
    else:
        updates = check_updates(skip_network=not check_pkgs)
        standalone = check_standalone_software(skip=not check_pkgs)

    sys_health = get_sys_health_status()

    # Determine Overall Flight Status
    # 🔴 ACTION_REQUIRED
    # 🟡 PRE_FLIGHT_ATTENTION
    # 🟢 FLIGHT_READY
    status = "FLIGHT_READY"
    status_reasons = []

    has_failed_system = len(failed_units.get("system", [])) > 0
    has_failed_user = len(failed_units.get("user", [])) > 0
    root_disk_used = disks.get("root", {}).get("used_pct", 0)

    if has_failed_system:
        status = "ACTION_REQUIRED"
        status_reasons.append(f"Failed systemd system services: {', '.join(failed_units['system'])}")
    
    if root_disk_used >= 92:
        status = "ACTION_REQUIRED"
        status_reasons.append(f"Root disk critically full ({root_disk_used}%)")

    if status != "ACTION_REQUIRED":
        if has_failed_user:
            status = "PRE_FLIGHT_ATTENTION"
            status_reasons.append(f"Failed user units: {', '.join(failed_units['user'])}")
        if updates.get("core_count", 0) > 0:
            status = "PRE_FLIGHT_ATTENTION"
            status_reasons.append(f"{updates['core_count']} core system update(s) available")
        elif updates.get("regular_count", 0) > 0 or updates.get("aur_count", 0) > 0:
            status = "PRE_FLIGHT_ATTENTION"
            reg_tot = updates.get("regular_count", 0) + updates.get("aur_count", 0)
            status_reasons.append(f"{reg_tot} package update(s) available")
        if reboot_pending:
            status = "PRE_FLIGHT_ATTENTION"
            status_reasons.append("Running kernel updated - system reboot pending")
        if orphans.get("count", 0) > 0:
            status = "PRE_FLIGHT_ATTENTION"
            status_reasons.append(f"{orphans['count']} unrequired orphan package(s) detected")
        if len(pacnew) > 0:
            status = "PRE_FLIGHT_ATTENTION"
            status_reasons.append(f"{len(pacnew)} .pacnew configuration file(s) require review")
        if root_disk_used >= 80:
            status = "PRE_FLIGHT_ATTENTION"
            status_reasons.append(f"Root disk usage elevated ({root_disk_used}%)")

    payload = {
        "schema_version": "1.0",
        "timestamp": datetime.now().astimezone().isoformat(),
        "status": status,
        "status_reasons": status_reasons,
        "gaming_mode": gamemode_on,
        "reboot_pending": reboot_pending,
        "failed_services": failed_units,
        "disk": disks,
        "pacnew": {
            "count": len(pacnew),
            "files": pacnew
        },
        "orphans": orphans,
        "updates": updates,
        "standalone_software": standalone,
        "sys_health": sys_health
    }

    # Save to status file
    try:
        with open(STATUS_FILE, "w", encoding="utf-8") as f:
            json.dump(payload, f, indent=2)
    except Exception:
        pass

    return payload


if __name__ == "__main__":
    check_pkgs_flag = True
    if "--no-pkg" in sys.argv:
        check_pkgs_flag = False
    
    data = run_triage(check_pkgs=check_pkgs_flag)
    print(json.dumps(data, indent=2))
