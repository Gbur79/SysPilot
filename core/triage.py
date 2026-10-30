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
from fnmatch import fnmatchcase
from typing import Dict, Any, List, Set

# Core system packages regex that warrant elevated upgrade precautions
CORE_PKG_PATTERN = re.compile(
    r'^(linux([-_].*)?|systemd([-_].*)?|glibc|dracut([-_].*)?|mkinitcpio([-_].*)?|'
    r'booster([-_].*)?|grub([-_].*)?|systemd-boot|limine([-_].*)?|refind([-_].*)?|'
    r'nvidia([-_].*)?|amdgpu([-_].*)?|mesa([-_].*)?|vulkan([-_].*)?|wayland([-_].*)?|'
    r'xorg([-_].*)?|pipewire([-_].*)?|wireplumber([-_].*)?|dkms([-_].*)?)$',
    re.IGNORECASE
)

# Candidate names used by desktop launchers for user application units.
APP_UNIT_PATTERN = re.compile(r"^app-.*\.(?:service|scope)$")

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


def get_transient_desktop_app_units(unit_names: List[str]) -> Set[str]:
    """
    Return failed units confirmed as transient desktop application launches.

    Unit names are only candidates: suppressing by name alone could hide a
    legitimate persistent user service (for example, app-backup.service).
    Metadata-query failure intentionally returns no matches so diagnostics
    fail open and preserve visibility.
    """
    candidates = [unit for unit in unit_names if APP_UNIT_PATTERN.match(unit)]
    if not candidates:
        return set()

    try:
        res = subprocess.run(
            [
                "systemctl", "--user", "show", "--no-pager",
                "--property=Id", "--property=Transient", "--property=Slice",
                *candidates,
            ],
            capture_output=True, text=True, timeout=1.0
        )
        if res.returncode != 0:
            return set()
    except Exception:
        return set()

    ignored = set()
    candidate_set = set(candidates)
    properties = {}

    # systemctl show separates each requested unit's property block by a
    # blank line. Add a final delimiter so the last block is processed too.
    for line in [*res.stdout.splitlines(), ""]:
        if not line:
            unit_id = properties.get("Id")
            if (
                unit_id in candidate_set
                and properties.get("Transient") == "yes"
                and properties.get("Slice") == "app.slice"
            ):
                ignored.add(unit_id)
            properties = {}
            continue

        key, separator, value = line.partition("=")
        if separator:
            properties[key] = value

    return ignored


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

    # Filter only systemd-confirmed transient desktop application launches.
    # System units and non-app transient units remain intentionally visible.
    ignored_user_units = get_transient_desktop_app_units(result["user"])
    if ignored_user_units:
        result["user"] = [
            unit for unit in result["user"] if unit not in ignored_user_units
        ]

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


# Must remain synchronized with sys-health.sh classify_orphan_tier().
# Tier 3 is removal sensitivity, not health severity.
ORPHAN_SENSITIVE_PATTERNS = (
    (
        "kernel / bootloader",
        (
            "linux*", "*-headers", "grub*", "systemd*", "dracut*",
            "mkinitcpio*", "booster*", "limine*", "refind*",
            "efibootmgr*", "syslinux*",
        ),
    ),
    (
        "driver / firmware / graphics",
        (
            "*firmware*", "*-ucode", "*-dkms", "nvidia*", "mesa*",
            "vulkan*", "xf86-video*", "intel-media-driver", "libva*",
            "libvdpau*", "xorg-server*", "xorg-xwayland*",
        ),
    ),
    (
        "audio subsystem",
        ("pipewire*", "wireplumber*", "alsa-*", "pulseaudio*", "jack*"),
    ),
    (
        "development toolchain",
        (
            "base-devel", "rust", "cargo", "go", "gcc*", "clang*",
            "llvm*", "make", "cmake", "patch", "git", "fakeroot",
            "binutils",
        ),
    ),
    (
        "desktop / compositor / display manager",
        (
            "wayland*", "hyprland*", "kwin*", "plasma-*", "sway*",
            "mutter*", "weston*", "sddm*", "gdm*", "lightdm*",
        ),
    ),
    (
        "storage / filesystem / crypto",
        (
            "btrfs-progs*", "dosfstools", "e2fsprogs", "xfsprogs*",
            "zfs*", "cryptsetup*", "lvm2*", "mdadm*", "device-mapper*",
        ),
    ),
    (
        "core security / auth / network",
        (
            "polkit*", "shadow", "sudo", "pam", "networkmanager*",
            "iwd*", "wpa_supplicant*",
        ),
    ),
    ("multilib / 32-bit runtime", ("lib32-*",)),
)


def _orphan_sensitive_tag(package: str):
    """Return the shared Tier-3 sensitivity tag, or None."""
    normalized = package.strip().lower()
    for tag, patterns in ORPHAN_SENSITIVE_PATTERNS:
        if any(fnmatchcase(normalized, pattern) for pattern in patterns):
            return tag
    return None


def _query_pacman_package_names(
    pacman_path: str, query_flag: str, timeout: float = 3.0
) -> Dict[str, Any]:
    """Run a read-only pacman query while distinguishing empty from failed."""
    env = os.environ.copy()
    env["LC_ALL"] = "C"

    try:
        res = subprocess.run(
            [pacman_path, query_flag],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
            env=env,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return {
            "ok": False,
            "packages": [],
            "error": f"{query_flag} timed out after {timeout:.1f}s",
        }
    except OSError as exc:
        return {
            "ok": False,
            "packages": [],
            "error": f"{query_flag} could not execute: {exc}",
        }

    packages = sorted({line.strip() for line in res.stdout.splitlines() if line.strip()})
    stderr = " ".join(res.stderr.split())[:240]

    if res.returncode == 0:
        return {"ok": True, "packages": packages, "error": None}

    # pacman query filters commonly return 1 when there are no matches.
    # Accept that only when there is no output and no diagnostic text.
    if res.returncode == 1 and not packages and not stderr:
        return {"ok": True, "packages": [], "error": None}

    detail = stderr or "no diagnostic text"
    return {
        "ok": False,
        "packages": [],
        "error": f"{query_flag} exited {res.returncode}: {detail}",
    }


def check_orphan_packages() -> Dict[str, Any]:
    """
    Report orphan telemetry without treating orphan presence as a health fault.

    Compatibility:
      count/packages retain the strict pacman -Qdtq meaning.
      optional_count/optional_packages retain the -Qdttq minus -Qdtq meaning.

    Tier model shared with sys-health.sh:
      Tier 1: strict and not heuristically sensitive.
      Tier 2: optional-only and not heuristically sensitive.
      Tier 3: heuristically sensitive, regardless of reference class.
    """
    pacman_path = shutil.which("pacman")
    if not pacman_path:
        return {
            "checked": False,
            "query_status": "ERROR",
            "severity": "WARN",
            "affects_flight_status": True,
            "tiering_complete": False,
            "count": 0,
            "packages": [],
            "optional_count": 0,
            "optional_packages": [],
            "candidate_count": 0,
            "tier1_count": 0,
            "tier1_packages": [],
            "tier2_count": 0,
            "tier2_packages": [],
            "tier3_count": 0,
            "tier3_packages": [],
            "tier3_tags": {},
            "unclassified_packages": [],
            "query_errors": {"strict": "pacman executable not found"},
            "message": "Orphan query unavailable: pacman executable not found",
        }

    strict_query = _query_pacman_package_names(pacman_path, "-Qdtq")
    extended_query = _query_pacman_package_names(pacman_path, "-Qdttq")

    strict_ok = bool(strict_query["ok"])
    extended_ok = bool(extended_query["ok"])
    strict_set = set(strict_query["packages"]) if strict_ok else set()
    extended_set = set(extended_query["packages"]) if extended_ok else set()

    # A stable -Qdttq result must include every -Qdtq result. Preserve the
    # union and expose degradation if a concurrent transaction breaks that rule.
    snapshots_consistent = not (
        strict_ok and extended_ok and not strict_set.issubset(extended_set)
    )
    candidate_set = strict_set | extended_set

    optional_set = (
        extended_set - strict_set if strict_ok and extended_ok else set()
    )

    tier1 = []
    tier2 = []
    tier3 = []
    tier3_tags = {}
    unclassified = []

    for package in sorted(candidate_set):
        sensitive_tag = _orphan_sensitive_tag(package)
        if sensitive_tag is not None:
            tier3.append(package)
            tier3_tags[package] = sensitive_tag
        elif not strict_ok:
            # Without the strict snapshot, an ordinary expanded candidate
            # cannot safely be called Tier 1 or Tier 2.
            unclassified.append(package)
        elif package in strict_set:
            tier1.append(package)
        else:
            tier2.append(package)

    query_errors = {}
    if not strict_ok:
        query_errors["strict"] = strict_query["error"]
    if not extended_ok:
        query_errors["extended"] = extended_query["error"]
    if not snapshots_consistent:
        query_errors["consistency"] = (
            "-Qdttq did not contain every -Qdtq package; "
            "a concurrent package transaction may have occurred"
        )

    tiering_complete = strict_ok and extended_ok and snapshots_consistent
    if not strict_ok:
        query_status = "ERROR"
        severity = "WARN"
        affects_flight_status = True
        message = "Strict orphan query failed; orphan telemetry is unavailable"
    elif not tiering_complete:
        query_status = "DEGRADED"
        severity = "INFO" if strict_set else "PASS"
        affects_flight_status = False
        message = (
            f"{len(strict_set)} strict orphan package(s); "
            "expanded tier telemetry is incomplete"
        )
    elif strict_set:
        query_status = "OK"
        severity = "INFO"
        affects_flight_status = False
        message = (
            f"{len(strict_set)} strict orphan package(s); review recommended"
        )
    else:
        query_status = "OK"
        severity = "PASS"
        affects_flight_status = False
        message = "No strict orphan packages detected"

    return {
        "checked": strict_ok,
        "query_status": query_status,
        "severity": severity,
        "affects_flight_status": affects_flight_status,
        "tiering_complete": tiering_complete,
        # Backward-compatible strict orphan fields.
        "count": len(strict_set),
        "packages": sorted(strict_set),
        # Backward-compatible optional-only candidate fields.
        "optional_count": len(optional_set),
        "optional_packages": sorted(optional_set),
        # Explicit tier telemetry.
        "candidate_count": len(candidate_set),
        "tier1_count": len(tier1),
        "tier1_packages": tier1,
        "tier2_count": len(tier2),
        "tier2_packages": tier2,
        "tier3_count": len(tier3),
        "tier3_packages": tier3,
        "tier3_tags": tier3_tags,
        "unclassified_packages": unclassified,
        "query_errors": query_errors,
        "message": message,
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

    engine_bin = shutil.which("sys-health")
    if not engine_bin:
        user_bin = os.path.expanduser("~/.local/bin/sys-health")
        if os.path.isfile(user_bin) and os.access(user_bin, os.X_OK):
            engine_bin = user_bin

    if not engine_bin:
        return default_summary

    try:
        res = subprocess.run([engine_bin, "--software", "--json"], capture_output=True, text=True, timeout=8.0)
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
                    "gaming": data.get("gaming", {}),
                    "checks": data.get("checks", {})
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
    informational_reasons = []

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
        if len(pacnew) > 0:
            status = "PRE_FLIGHT_ATTENTION"
            status_reasons.append(f"{len(pacnew)} .pacnew configuration file(s) require review")
        if root_disk_used >= 80:
            status = "PRE_FLIGHT_ATTENTION"
            status_reasons.append(f"Root disk usage elevated ({root_disk_used}%)")

    # Orphan presence is an INFO maintenance advisory. Only inability to perform
    # the strict query is WARN-equivalent and may degrade readiness.
    if not orphans.get("checked", False):
        if status != "ACTION_REQUIRED":
            status = "PRE_FLIGHT_ATTENTION"
        status_reasons.append(
            "Unable to query strict orphan packages; package telemetry unavailable"
        )
    elif orphans.get("count", 0) > 0:
        informational_reasons.append(
            f"{orphans['count']} unrequired orphan package(s); review recommended"
        )

    if orphans.get("tier3_count", 0) > 0:
        informational_reasons.append(
            f"{orphans['tier3_count']} orphan candidate(s) are removal-sensitive; "
            "review before removal"
        )

    payload = {
        "schema_version": "1.0",
        "timestamp": datetime.now().astimezone().isoformat(),
        "status": status,
        "status_reasons": status_reasons,
        "informational_reasons": informational_reasons,
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

    # Save to status file atomically
    try:
        os.makedirs(STATE_DIR, exist_ok=True)
        tmp_status = os.path.join(STATE_DIR, f".status.json.tmp.{os.getpid()}")
        with open(tmp_status, "w", encoding="utf-8") as f:
            json.dump(payload, f, indent=2)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp_status, STATUS_FILE)
    except Exception:
        pass

    return payload


if __name__ == "__main__":
    check_pkgs_flag = True
    if "--no-pkg" in sys.argv:
        check_pkgs_flag = False
    
    data = run_triage(check_pkgs=check_pkgs_flag)
    print(json.dumps(data, indent=2))
