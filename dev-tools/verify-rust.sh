#!/usr/bin/env bash
# [SRE-TOOLING: Rust Verification Engine for SysPilot]
# Supports quiet 1-line NO_CHATTER mode for AI Hands / CI (-q)
set -Eeuo pipefail
IFS=$'\n\t'

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$(cd -- "$script_dir/.." && pwd -P)

if [[ -f "$project_root/Cargo.toml" ]]; then
    target_dir="$project_root"
elif [[ -d "$project_root/crates/syspilot-tray" ]]; then
    target_dir="$project_root/crates/syspilot-tray"
elif [[ -d "$project_root/rust-tray" ]]; then
    target_dir="$project_root/rust-tray"
else
    target_dir="$project_root"
fi

quiet=0
mutants_target=""
mutants_diff=0
while (( $# > 0 )); do
    case "$1" in
        -q|--quiet)
            quiet=1
            shift
            ;;
        --mutants)
            mutants_target="${2:?--mutants requires a file path (e.g. crates/syspilot-core/src/firewall.rs)}"
            shift 2
            ;;
        --mutants-diff)
            mutants_diff=1
            shift
            ;;
        *)
            shift
            ;;
    esac
done

if [[ ! -d "$target_dir" || (! -f "$target_dir/Cargo.toml" && ! -d "$target_dir/crates") ]]; then
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: 0/0 | CLIPPY: SKIP | FMT: SKIP | TARGET: syspilot-workspace"
        echo "DIAG_1: STEP=init;EXIT=1;TARGET=$target_dir"
        echo "DIAG_2: SUMMARY=crate directory not found"
    else
        echo "Error: Target directory '$target_dir' not found or invalid." >&2
    fi
    exit 1
fi

cd -- "$target_dir"
err_log="/tmp/syspilot-build-err.log"

# 1. Formatting
fmt_out=""
if ! fmt_out=$(cargo fmt --check 2>&1); then
    echo "$fmt_out" > "$err_log"
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: UNTESTED | CLIPPY: UNTESTED | FMT: FAIL | TARGET: syspilot-workspace"
        echo "DIAG_1: STEP=fmt;EXIT=1;TARGET=syspilot-workspace"
        first_line=$(echo "$fmt_out" | head -n 1 | tr '|' '_')
        echo "DIAG_2: SUMMARY=${first_line:-cargo fmt mismatch}"
        echo "FULL_LOG: $err_log"
    else
        echo "cargo fmt failed:" >&2
        echo "$fmt_out" >&2
    fi
    exit 1
fi

# 2. Clippy
clippy_out=""
if ! clippy_out=$(cargo clippy --locked --all-targets -- -D warnings 2>&1); then
    echo "$clippy_out" > "$err_log"
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: UNTESTED | CLIPPY: FAIL | FMT: PASS | TARGET: syspilot-workspace"
        echo "DIAG_1: STEP=clippy;EXIT=1;TARGET=syspilot-workspace"
        first_line=$(echo "$clippy_out" | grep -m1 "error:" | tr '|' '_')
        echo "DIAG_2: SUMMARY=${first_line:-cargo clippy failed}"
        echo "FULL_LOG: $err_log"
    else
        echo "cargo clippy failed:" >&2
        echo "$clippy_out" >&2
    fi
    exit 1
fi

# 2b. Universalism Linter
lint_out=""
if ! lint_out=$("$script_dir/lint-universalism.sh" -q 2>&1); then
    echo "$lint_out" > "$err_log"
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: UNTESTED | CLIPPY: PASS | FMT: PASS | LINT: FAIL | TARGET: syspilot-workspace"
        echo "DIAG_1: STEP=lint;EXIT=1;TARGET=syspilot-workspace"
        first_line=$(echo "$lint_out" | head -n 1 | tr '|' '_')
        echo "DIAG_2: SUMMARY=${first_line:-lint-universalism failed}"
        echo "FULL_LOG: $err_log"
    else
        echo "lint-universalism failed:" >&2
        echo "$lint_out" >&2
    fi
    exit 1
fi

# 3. Tests
test_out=""
if ! test_out=$(cargo test --locked 2>&1); then
    echo "$test_out" > "$err_log"
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: FAIL | CLIPPY: PASS | FMT: PASS | TARGET: syspilot-workspace"
        echo "DIAG_1: STEP=test;EXIT=1;TARGET=syspilot-workspace"
        failed_test=$(echo "$test_out" | grep -m1 "FAILED" | tr '|' '_')
        echo "DIAG_2: SUMMARY=${failed_test:-cargo test assertions failed}"
        echo "FULL_LOG: $err_log"
    else
        echo "cargo test failed:" >&2
        echo "$test_out" >&2
    fi
    exit 1
fi

core_count=0
tray_count=0
gui_count=0
eval "$(echo "$test_out" | awk '
/Running/ {
    cur=""
    if ($0 ~ /syspilot_core/) cur="core"
    else if ($0 ~ /syspilot_tray/) cur="tray"
    else if ($0 ~ /(syspilot_gui|watcher_smoke)/) cur="gui"
}
/test result: ok\./ {
    match($0, /([0-9]+) passed/, m)
    cnt = m[1] + 0
    if (cur == "core") core += cnt
    else if (cur == "tray") tray += cnt
    else if (cur == "gui") gui += cnt
    cur=""
}
END {
    print "core_count=" core+0 "; tray_count=" tray+0 "; gui_count=" gui+0
}')"

total_tests=$((core_count + tray_count + gui_count))

if (( gui_count == 0 )); then
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: ZERO_GUI | CLIPPY: PASS | FMT: PASS | TARGET: syspilot-gui"
        echo "DIAG_1: STEP=test;EXIT=1;TARGET=syspilot-gui"
        echo "DIAG_2: SUMMARY=crates/syspilot-gui has 0 tests (Anti-Green-Test-Illusion Gate violated)"
    else
        echo "Error: crates/syspilot-gui has 0 passing tests. Anti-Green-Test-Illusion Gate violated." >&2
    fi
    exit 1
fi

# 4. Footprint Benchmark
footprint_out=""
if ! footprint_out=$("$script_dir/bench-footprint.sh" --smoke -q 2>&1); then
    echo "$footprint_out" > "$err_log"
    if (( quiet )); then
        echo "RUST_VERIFY: FAIL | TESTS: PASS | CLIPPY: PASS | FMT: PASS | FOOTPRINT: FAIL | TARGET: syspilot-workspace"
        echo "DIAG_1: STEP=footprint;EXIT=1;TARGET=syspilot-workspace"
        first_line=$(echo "$footprint_out" | head -n 1 | tr '|' '_')
        echo "DIAG_2: SUMMARY=${first_line:-bench-footprint failed}"
        echo "FULL_LOG: $err_log"
    else
        echo "bench-footprint failed:" >&2
        echo "$footprint_out" >&2
    fi
    exit 1
fi

# 5. Optional Surgical Mutation Testing (--mutants <file> or --mutants-diff)
mutants_tag=""
if [[ -n "$mutants_target" ]] || (( mutants_diff )); then
    export PATH="$HOME/.local/bin:$PATH"
    mut_out=""
    mut_rc=0
    if [[ -n "$mutants_target" ]]; then
        mut_pkg="syspilot-core"
        mut_test_args=(--lib)
        if [[ "$mutants_target" == *"syspilot-tray"* ]]; then
            mut_pkg="syspilot-tray"
            mut_test_args=(--bin syspilot-tray)
        elif [[ "$mutants_target" == *"syspilot-gui"* ]]; then
            mut_pkg="syspilot-gui"
            mut_test_args=(--bin syspilot-gui)
        fi
        mut_out=$(cargo mutants --in-place -p "$mut_pkg" -f "$mutants_target" -- "${mut_test_args[@]}" 2>&1) || mut_rc=$?
    else
        diff_tmp=$(mktemp "/tmp/syspilot-mutants-diff.XXXXXX")
        git diff HEAD > "$diff_tmp"
        mut_out=$(cargo mutants --in-place --in-diff "$diff_tmp" -p syspilot-core -- --lib 2>&1) || mut_rc=$?
        rm -f "$diff_tmp"
    fi
    rm -rf "$target_dir/mutants.out" "$target_dir/mutants.out.old"
    summary_line=$(echo "$mut_out" | grep -E "mutants tested|Found 0 mutants" | tail -n 1 | tr '|' '_' || true)
    if (( mut_rc != 0 )); then
        echo "$mut_out" > "$err_log"
        if (( quiet )); then
            echo "RUST_VERIFY: FAIL | TESTS: PASS | MUTANTS: FAIL (${summary_line:-missed mutants}) | TARGET: ${mutants_target:-git-diff}"
            echo "FULL_LOG: $err_log"
        else
            echo "cargo mutants failed (survived mutants detected):" >&2
            echo "$mut_out" >&2
        fi
        exit 1
    fi
    mutants_tag=" | MUTANTS: PASS (${summary_line:-clean})"
fi

if (( quiet )); then
    echo "RUST_VERIFY: PASS | TESTS: core: ${core_count} | tray: ${tray_count} | gui: ${gui_count} | Total: ${total_tests}/${total_tests} | CLIPPY: CLEAN | FMT: CLEAN | LINT: CLEAN${mutants_tag} | TARGET: syspilot-workspace"
else
    echo "✔ All Rust checks passed: fmt, clippy, universalism lint, ${total_tests} tests (core: ${core_count}, tray: ${tray_count}, gui: ${gui_count}), and footprint benchmark${mutants_tag}."
fi
exit 0
