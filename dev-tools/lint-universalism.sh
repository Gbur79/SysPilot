#!/usr/bin/env bash
# [SRE-TOOLING: Universalism & Zero-Root Invariant Linter for SysPilot]
set -Eeuo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$(cd -- "$script_dir/.." && pwd -P)
export PROJECT_ROOT="$project_root"

python3 - "$@" << 'EOF'
import os
import sys

quiet = "-q" in sys.argv or "--quiet" in sys.argv

project_root = os.environ.get("PROJECT_ROOT", os.path.realpath("."))
crates_dir = os.path.join(project_root, "crates")
check_roots_guards = True
for i, arg in enumerate(sys.argv):
    if arg == "--target-dir" and i + 1 < len(sys.argv):
        crates_dir = os.path.realpath(sys.argv[i + 1])
        check_roots_guards = False


forbidden_subprocesses = [
    "glxinfo", "vulkaninfo", "lspci", "xrandr",
    "nvidia-settings", "pkexec", "sudo", "busctl", "find"
]

forbidden_rig_bias = [
    "192.168.1.1", "/home/gbur", "MSI Z97", "/boot/efi/EFI"
]

def is_excluded_file(rel_path):
    base = os.path.basename(rel_path)
    if base in ["tests.rs", "mock_rig.rs"]:
        return True
    if "/tests/" in rel_path or rel_path.endswith("_test.rs"):
        return True
    return False

violations = []

for root, dirs, files in os.walk(crates_dir):
    for f in sorted(files):
        if not f.endswith(".rs"):
            continue
        full_path = os.path.join(root, f)
        rel_path = os.path.relpath(full_path, project_root)
        if is_excluded_file(rel_path):
            continue
        try:
            with open(full_path, "r", encoding="utf-8", errors="replace") as fh:
                for line_no, line in enumerate(fh, 1):
                    if "syspilot-lint:allow" in line:
                        continue
                    # Check rig bias
                    for bias in forbidden_rig_bias:
                        if bias in line:
                            violations.append(f"{rel_path}:{line_no}: Forbidden rig-bias pattern '{bias}'")
                    # Check subprocesses
                    for proc in forbidden_subprocesses:
                        if f'Command::new("{proc}")' in line or f'Command::new(&"{proc}")' in line:
                            violations.append(f"{rel_path}:{line_no}: Forbidden subprocess invocation '{proc}'")
                        elif proc != "sudo" and f'"{proc}"' in line:
                            violations.append(f"{rel_path}:{line_no}: Forbidden heavy subprocess pattern '{proc}'")
                    if os.path.basename(rel_path) != "exec.rs" and "Command::new(" in line:
                        violations.append(f"{rel_path}:{line_no}: Direct Command::new is forbidden outside exec.rs; use syspilot_core::exec::{{new_tokio_command, new_std_command}}")
        except Exception as e:
            violations.append(f"{rel_path}: read error: {e}")

# Check getuid() == 0 guard in tray and gui main.rs
if check_roots_guards:
    for main_rel in ["crates/syspilot-tray/src/main.rs", "crates/syspilot-gui/src/main.rs"]:
        main_path = os.path.join(project_root, main_rel)
        if not os.path.exists(main_path):
            violations.append(f"{main_rel}: file not found")
            continue
        with open(main_path, "r", encoding="utf-8") as fh:
            content = fh.read()
        if "libc::getuid() == 0" not in content and "getuid() == 0" not in content:
            violations.append(f"{main_rel}: missing getuid() == 0 zero-root guard")

    # Version Synchronization Gate (Cargo.toml == CHANGELOG.md == bin/syspilot == packaging/PKGBUILD)
    import re
    cargo_toml = os.path.join(project_root, "Cargo.toml")
    if os.path.exists(cargo_toml):
        with open(cargo_toml, "r", encoding="utf-8") as fh:
            m = re.search(r'^version\s*=\s*"([^"]+)"', fh.read(), re.M)
        cargo_ver = m.group(1) if m else None
        if cargo_ver:
            changelog_path = os.path.join(project_root, "CHANGELOG.md")
            if os.path.exists(changelog_path):
                with open(changelog_path, "r", encoding="utf-8") as fh:
                    cm = re.search(r'^## \[([^\]]+)\]', fh.read(), re.M)
                if not cm or cm.group(1) != cargo_ver:
                    violations.append(f"CHANGELOG.md: top release header '{cm.group(1) if cm else 'NONE'}' does not match Cargo.toml version '{cargo_ver}'")
            cli_path = os.path.join(project_root, "bin/syspilot")
            if os.path.exists(cli_path):
                with open(cli_path, "r", encoding="utf-8") as fh:
                    bm = re.search(r'^readonly VERSION="([^"]+)"', fh.read(), re.M)
                if not bm or bm.group(1) != cargo_ver:
                    violations.append(f"bin/syspilot: VERSION '{bm.group(1) if bm else 'NONE'}' does not match Cargo.toml version '{cargo_ver}'")
            pkgbuild_path = os.path.join(project_root, "packaging/PKGBUILD")
            if os.path.exists(pkgbuild_path):
                with open(pkgbuild_path, "r", encoding="utf-8") as fh:
                    pm = re.search(r'^pkgver=(\S+)', fh.read(), re.M)
                if not pm or pm.group(1) != cargo_ver:
                    violations.append(f"packaging/PKGBUILD: pkgver '{pm.group(1) if pm else 'NONE'}' does not match Cargo.toml version '{cargo_ver}'")

    # Repository Hygiene Gate (Forbidden internal agent governance files in git index)
    try:
        import subprocess
        tracked_out = subprocess.check_output(["git", "ls-files"], cwd=project_root, text=True, errors="replace")
        forbidden_exact = {
            "AUDIT_MATRIX.md", "DEVELOPMENT_PROPOSALS.md", "pending-patches.md",
            "DECOUPLING_PLAN.md", "RECONSTRUCTION_PLAN.md", "ROADMAP_STATUS.md",
            "SRE_AUDIT_PLAN.md", "OBSERVED_ISSUES.md"
        }
        for item in tracked_out.splitlines():
            item_base = os.path.basename(item)
            if item_base in forbidden_exact or item.endswith("/OBSERVED_ISSUES.md") or (item.startswith("dev-tools/") and item.endswith(".json")):
                violations.append(f"Git tracking violation: '{item}' is an internal SRE/agent file and must not be tracked in git (add to .gitignore).")
    except Exception as e:
        pass

if violations:
    first_viol = violations[0].replace("|", "_")
    if quiet:
        print(f"LINT_UNIVERSALISM: FAIL | {first_viol}")
    else:
        print("LINT_UNIVERSALISM: FAIL")
        for v in violations:
            print(f"  {v}")
    sys.exit(1)
else:
    if quiet:
        print("LINT_UNIVERSALISM: PASS")
    else:
        print("LINT_UNIVERSALISM: PASS | All crates pass universalism and zero-root checks.")
    sys.exit(0)
EOF
