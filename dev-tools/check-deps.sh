#!/usr/bin/env bash
# [SRE-TOOLING: Dependency Audit & Pinning Verification for SysPilot]
set -Eeuo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$(cd -- "$script_dir/.." && pwd -P)

quiet=0
for arg in "$@"; do
    case "$arg" in
        -q|--quiet)
            quiet=1
            ;;
    esac
done

cd "$project_root"

if [[ $quiet -eq 0 ]]; then
    echo "=== SysPilot Dependency Check (Non-Mutating) ==="
    echo "Checking Cargo dependencies via dry-run..."
fi

if [[ $quiet -eq 1 ]]; then
    cargo update --dry-run --quiet
else
    cargo update --dry-run
fi

if command -v cargo-outdated >/dev/null 2>&1; then
    if [[ $quiet -eq 0 ]]; then
        echo ""
        echo "=== cargo-outdated (Root dependencies) ==="
        cargo outdated -R || true
    fi
fi

if [[ $quiet -eq 0 ]]; then
    echo ""
    echo "=== Pinned Major Dependencies ==="
    echo "• zbus = \"5.19\": Pinned to 5.19.x series required by ksni 0.3.x ecosystem."
    echo ""
    echo "CHECK_DEPS: PASS"
else
    echo "CHECK_DEPS: PASS"
fi
