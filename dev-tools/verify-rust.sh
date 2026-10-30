#!/usr/bin/env bash
# [SRE-TOOLING: Rust Verification Engine for SysPilot]
# Supports quiet 1-line NO_CHATTER mode for AI Hands / CI (-q)
set -Eeuo pipefail
IFS=$'\n\t'

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$(cd -- "$script_dir/.." && pwd -P)
crate="$project_root/rust-tray"

quiet=0
if [[ "${1:-}" == "-q" || "${1:-}" == "--quiet" ]]; then
    quiet=1
fi

if [[ ! -d "$crate" ]]; then
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: 0/0 | CLIPPY: SKIP | FMT: SKIP | TARGET: syspilot-tray"
        echo "DIAG_1: STEP=init;EXIT=1;TARGET=$crate"
        echo "DIAG_2: SUMMARY=crate directory not found"
    else
        echo "Error: Crate directory '$crate' not found." >&2
    fi
    exit 1
fi

cd -- "$crate"

# 1. Formatting
fmt_out=""
if ! fmt_out=$(cargo fmt --check 2>&1); then
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: UNTESTED | CLIPPY: UNTESTED | FMT: FAIL | TARGET: syspilot-tray"
        echo "DIAG_1: STEP=fmt;EXIT=1;TARGET=syspilot-tray"
        first_line=$(echo "$fmt_out" | head -n 1 | tr '|' '_')
        echo "DIAG_2: SUMMARY=${first_line:-cargo fmt mismatch}"
    else
        echo "cargo fmt failed:" >&2
        echo "$fmt_out" >&2
    fi
    exit 1
fi

# 2. Clippy
clippy_out=""
if ! clippy_out=$(cargo clippy --locked --all-targets -- -D warnings 2>&1); then
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: UNTESTED | CLIPPY: FAIL | FMT: PASS | TARGET: syspilot-tray"
        echo "DIAG_1: STEP=clippy;EXIT=1;TARGET=syspilot-tray"
        first_line=$(echo "$clippy_out" | grep -m1 "error:" | tr '|' '_')
        echo "DIAG_2: SUMMARY=${first_line:-cargo clippy failed}"
    else
        echo "cargo clippy failed:" >&2
        echo "$clippy_out" >&2
    fi
    exit 1
fi

# 3. Tests
test_out=""
if ! test_out=$(cargo test --locked 2>&1); then
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: FAIL | CLIPPY: PASS | FMT: PASS | TARGET: syspilot-tray"
        echo "DIAG_1: STEP=test;EXIT=1;TARGET=syspilot-tray"
        failed_test=$(echo "$test_out" | grep -m1 "FAILED" | tr '|' '_')
        echo "DIAG_2: SUMMARY=${failed_test:-cargo test assertions failed}"
    else
        echo "cargo test failed:" >&2
        echo "$test_out" >&2
    fi
    exit 1
fi

test_count=$(echo "$test_out" | grep -oE '[0-9]+ passed' | awk '{print $1}' | tail -n 1)
test_count=${test_count:-0}

if (( quiet )); then
    echo "RUST_VERIFY: PASS | TESTS: ${test_count}/${test_count} | CLIPPY: CLEAN | FMT: CLEAN | TARGET: syspilot-tray"
else
    echo "✔ All Rust checks passed: fmt, clippy, and ${test_count} unit tests."
fi
exit 0
