#!/usr/bin/env bash
# [SRE-TOOLING: Rust Verification Engine for SysPilot]
# Supports quiet 1-line NO_CHATTER mode for AI Hands / CI (-q)
set -Eeuo pipefail
IFS=$'\n\t'

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root="${SYSPILOT_PROJECT_ROOT:-$(cd -- "$script_dir/.." && pwd -P)}"

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
run_cov=0
gui_smoke=0
cov_fail_under="${SYSPILOT_COV_FAIL_UNDER:-}"
while (( $# > 0 )); do
    case "$1" in
        -q|--quiet)
            quiet=1
            shift
            ;;
        --gui-smoke)
            gui_smoke=1
            shift
            ;;
        --cov|--coverage)
            run_cov=1
            shift
            ;;
        --cov-fail-under)
            run_cov=1
            cov_fail_under="${2:?--cov-fail-under requires a percentage (0-100)}"
            shift 2
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
if ! clippy_out=$(cargo clippy --locked --all-targets -- -D warnings -D clippy::dbg_macro -D clippy::unimplemented -D clippy::await_holding_lock -D clippy::let_underscore_future 2>&1); then
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

# 4a. GUI Dual-Skin Smoke Test (--gui-smoke)
gui_smoke_tag=""
if (( gui_smoke )); then
    smoke_out=""
    smoke_rc=0
    smoke_out=$("$script_dir/smoke-gui.sh" -q 2>&1) || smoke_rc=$?
    if (( smoke_rc != 0 )); then
        echo "$smoke_out" > "$err_log"
        if (( quiet )); then
            echo "RUST_VERIFY: FAIL | TESTS: PASS | CLIPPY: PASS | FMT: PASS | GUI_SMOKE: FAIL | TARGET: syspilot-gui"
            echo "DIAG_1: STEP=gui-smoke;EXIT=${smoke_rc};TARGET=syspilot-gui"
            first_line=$(echo "$smoke_out" | head -n 1 | tr '|' '_')
            echo "DIAG_2: SUMMARY=${first_line:-smoke-gui failed}"
            echo "FULL_LOG: $err_log"
        else
            echo "smoke-gui failed:" >&2
            echo "$smoke_out" >&2
        fi
        exit 1
    fi
    gui_smoke_tag=" | GUI_SMOKE: PASS"
fi

# 4b. Optional Fast LLVM Coverage Pre-Gate (--cov / --coverage)
cov_tag=""
if (( run_cov )); then
    export PATH="$HOME/.local/bin:$PATH"
    if [[ -z "${LLVM_COV:-}" ]] && command -v llvm-cov >/dev/null 2>&1; then
        LLVM_COV="$(command -v llvm-cov)"
        export LLVM_COV
    fi
    if [[ -z "${LLVM_PROFDATA:-}" ]] && command -v llvm-profdata >/dev/null 2>&1; then
        LLVM_PROFDATA="$(command -v llvm-profdata)"
        export LLVM_PROFDATA
    fi
    cov_json=$(mktemp "/tmp/syspilot-cov.XXXXXX")
    cov_log="${SYSPILOT_COV_LOG:-/tmp/syspilot-cov-report.log}"
    cov_out=""
    cov_rc=0
    cov_out=$(cargo llvm-cov --workspace --locked --no-cfg-coverage --json --summary-only --output-path "$cov_json" 2>&1) || cov_rc=$?
    if (( cov_rc != 0 )); then
        rm -f "$cov_json"
        echo "$cov_out" > "$err_log"
        cov_hint=""
        if [[ "$cov_out" == *"no such command"* ]]; then
            cov_hint=" (Hint: sudo pacman -S cargo-llvm-cov)"
        elif [[ "$cov_out" == *"llvm-tools-preview"* ]]; then
            cov_hint=" (Hint: install llvm or set LLVM_COV/LLVM_PROFDATA)"
        fi
        if (( quiet )); then
            echo "RUST_VERIFY: FAIL | TESTS: PASS | COV: FAIL | TARGET: syspilot-workspace"
            echo "DIAG_1: STEP=cov;EXIT=${cov_rc};TARGET=syspilot-workspace"
            first_line=$(echo "$cov_out" | { grep -m1 "error:" || true; } | tr '|' '_')
            if [[ -z "$first_line" ]]; then
                first_line=$(echo "$cov_out" | head -n 1 | tr '|' '_')
            fi
            echo "DIAG_2: SUMMARY=${first_line:-cargo llvm-cov failed}${cov_hint}"
            echo "FULL_LOG: $err_log"
        else
            echo "cargo llvm-cov failed${cov_hint}:" >&2
            echo "$cov_out" >&2
        fi
        exit 1
    fi

    missing_out=$(cargo llvm-cov report --show-missing-lines 2>&1 || true)
    mapfile -t cov_changed_rs < <(git diff --name-only HEAD -- '*.rs')
    mapfile -t cov_untracked_rs < <(git ls-files --others --exclude-standard -- 'crates/*.rs')

    cov_parse_rc=0
    cov_summary=$(python3 - "$cov_json" "$target_dir" "$cov_log" "${cov_fail_under:-}" "${cov_changed_rs[@]}" "${cov_untracked_rs[@]}" <<'PY'
import json
import os
import sys

cov_json_path = sys.argv[1]
target_dir = os.path.realpath(sys.argv[2])
cov_log_path = sys.argv[3]
fail_under_str = sys.argv[4].strip()
diff_files = []
for p in sys.argv[5:]:
    if p and p not in diff_files:
        diff_files.append(p)

try:
    fail_under = float(fail_under_str) if fail_under_str else None
    if fail_under is not None and (fail_under < 0.0 or fail_under > 100.0):
        print("INVALID_THRESHOLD: must be between 0 and 100")
        sys.exit(2)
except ValueError:
    print(f"INVALID_THRESHOLD: {fail_under_str}")
    sys.exit(2)

with open(cov_json_path, "r", encoding="utf-8") as fh:
    payload = json.load(fh)

data0 = (payload.get("data") or [{}])[0]
totals = data0.get("totals") or {}
lines_pct = float((totals.get("lines") or {}).get("percent", 0.0))
regions_pct = float((totals.get("regions") or {}).get("percent", 0.0))
funcs_pct = float((totals.get("functions") or {}).get("percent", 0.0))

files_entries = data0.get("files") or []
diff_reg_cov = 0
diff_reg_cnt = 0
diff_line_cov = 0
diff_line_cnt = 0
diff_rows = []

for entry in files_entries:
    raw_fn = entry.get("filename", "")
    real_fn = os.path.realpath(raw_fn) if os.path.isabs(raw_fn) else os.path.realpath(os.path.join(target_dir, raw_fn))
    try:
        rel_fn = os.path.relpath(real_fn, target_dir)
    except ValueError:
        rel_fn = raw_fn
    matched = False
    for df in diff_files:
        if rel_fn == df or raw_fn == df or raw_fn.endswith("/" + df):
            matched = True
            break
    if matched:
        s = entry.get("summary") or {}
        l_obj = s.get("lines") or {}
        r_obj = s.get("regions") or {}
        f_obj = s.get("functions") or {}
        l_c, l_n, l_p = int(l_obj.get("covered", 0)), int(l_obj.get("count", 0)), float(l_obj.get("percent", 0.0))
        r_c, r_n, r_p = int(r_obj.get("covered", 0)), int(r_obj.get("count", 0)), float(r_obj.get("percent", 0.0))
        f_p = float(f_obj.get("percent", 0.0))
        diff_reg_cov += r_c
        diff_reg_cnt += r_n
        diff_line_cov += l_c
        diff_line_cnt += l_n
        diff_rows.append(f"  {rel_fn}: L:{l_p:.1f}% ({l_c}/{l_n}) R:{r_p:.1f}% ({r_c}/{r_n}) F:{f_p:.1f}%")

diff_suffix = ""
gate_metric = regions_pct
gate_label = f"workspace R:{regions_pct:.1f}%"
if diff_rows:
    diff_r_pct = (100.0 * diff_reg_cov / diff_reg_cnt) if diff_reg_cnt > 0 else 100.0
    diff_l_pct = (100.0 * diff_line_cov / diff_line_cnt) if diff_line_cnt > 0 else 100.0
    diff_suffix = f" DIFF_L:{diff_l_pct:.1f}% DIFF_R:{diff_r_pct:.1f}%"
    gate_metric = diff_r_pct
    gate_label = f"diff R:{diff_r_pct:.1f}%"

with open(cov_log_path, "w", encoding="utf-8") as logf:
    logf.write(f"=== WORKSPACE COVERAGE: L:{lines_pct:.1f}% R:{regions_pct:.1f}% F:{funcs_pct:.1f}% ===\n")
    if diff_rows:
        logf.write("=== DIFF FILE COVERAGE ===\n")
        for r in diff_rows:
            logf.write(r + "\n")
    logf.write("\n")

summary_str = f"L:{lines_pct:.1f}% R:{regions_pct:.1f}% F:{funcs_pct:.1f}%{diff_suffix}"
if fail_under is not None and gate_metric < fail_under:
    print(f"{summary_str} | {gate_label} < {fail_under:.1f}%")
    sys.exit(3)

print(summary_str)
PY
    ) || cov_parse_rc=$?
    rm -f "$cov_json"
    printf '%s\n' "$missing_out" >> "$cov_log"

    if (( cov_parse_rc != 0 )); then
        cp -f "$cov_log" "$err_log" 2>/dev/null || echo "$cov_summary" > "$err_log"
        if (( quiet )); then
            echo "RUST_VERIFY: FAIL | TESTS: PASS | COV: FAIL (${cov_summary}) | TARGET: syspilot-workspace"
            echo "DIAG_1: STEP=cov;EXIT=${cov_parse_rc};TARGET=syspilot-workspace"
            echo "DIAG_2: SUMMARY=coverage threshold check failed (${cov_summary})"
            echo "FULL_LOG: $cov_log"
        else
            echo "Coverage gate failed (${cov_summary}). Full report: $cov_log" >&2
        fi
        exit 1
    fi
    cov_tag=" | COV: PASS (${cov_summary})"
fi

# 5. Optional Surgical Mutation Testing (--mutants <file> or --mutants-diff)
mutants_tag=""
if [[ -n "$mutants_target" ]] || (( mutants_diff )); then
    export PATH="$HOME/.local/bin:$PATH"
    mut_out=""
    mut_rc=0
    summary_line=""
    resolve_mutants_pkg() {
        local path="$1"
        mut_pkg=""
        mut_test_args=()
        case "$path" in
            crates/syspilot-core/*|*syspilot-core*)
                mut_pkg="syspilot-core"
                mut_test_args=(--lib)
                ;;
            crates/syspilot-tray/*|*syspilot-tray*)
                mut_pkg="syspilot-tray"
                mut_test_args=(--bin syspilot-tray)
                ;;
            crates/syspilot-gui/*|*syspilot-gui*)
                mut_pkg="syspilot-gui"
                mut_test_args=(--bin syspilot-gui)
                ;;
        esac
    }

    if [[ -n "$mutants_target" ]]; then
        resolve_mutants_pkg "$mutants_target"
        if [[ -z "$mut_pkg" ]]; then
            mut_pkg="syspilot-core"
            mut_test_args=(--lib)
        fi
        mut_out=$(cargo mutants --in-place -p "$mut_pkg" -f "$mutants_target" -- "${mut_test_args[@]}" 2>&1) || mut_rc=$?
    else
        diff_tmp=$(mktemp "/tmp/syspilot-mutants-diff.XXXXXX")
        git diff HEAD > "$diff_tmp"
        mapfile -t changed_rs < <(git diff --name-only HEAD -- '*.rs')
        mapfile -t untracked_rs < <(git ls-files --others --exclude-standard -- 'crates/*.rs')
        diff_pkgs=()
        for f in "${changed_rs[@]}"; do
            resolve_mutants_pkg "$f"
            pkg="$mut_pkg"
            if [[ -n "$pkg" ]]; then
                already_in=0
                for p in "${diff_pkgs[@]}"; do
                    if [[ "$p" == "$pkg" ]]; then
                        already_in=1
                        break
                    fi
                done
                if (( !already_in )); then
                    diff_pkgs+=("$pkg")
                fi
            fi
        done

        if (( ${#diff_pkgs[@]} == 0 && ${#untracked_rs[@]} == 0 )); then
            summary_line="Found 0 mutants in diff"
            mut_rc=0
        else
            for pkg in "${diff_pkgs[@]}"; do
                resolve_mutants_pkg "$pkg"
                pkg_rc=0
                pkg_out=$(cargo mutants --in-place --in-diff "$diff_tmp" -p "$pkg" -- "${mut_test_args[@]}" 2>&1) || pkg_rc=$?
                mut_out="${mut_out}${pkg_out}"$'\n'
                if (( pkg_rc != 0 )); then
                    mut_rc=1
                fi
                rm -rf "$target_dir/mutants.out" "$target_dir/mutants.out.old"
            done
            for ufile in "${untracked_rs[@]}"; do
                resolve_mutants_pkg "$ufile"
                if [[ -n "$mut_pkg" ]]; then
                    u_rc=0
                    u_out=$(cargo mutants --in-place -p "$mut_pkg" -f "$ufile" -- "${mut_test_args[@]}" 2>&1) || u_rc=$?
                    mut_out="${mut_out}${u_out}"$'\n'
                    if (( u_rc != 0 )); then
                        mut_rc=1
                    fi
                    rm -rf "$target_dir/mutants.out" "$target_dir/mutants.out.old"
                fi
            done
        fi
        rm -f "$diff_tmp"
    fi
    rm -rf "$target_dir/mutants.out" "$target_dir/mutants.out.old"
    if [[ -z "$summary_line" ]]; then
        summary_line=$(echo "$mut_out" | grep -E "mutants tested|Found 0 mutants" | awk '{printf "%s%s", (NR>1?"; ":""), $0}' | tr '|' '_' || true)
    fi
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
    echo "RUST_VERIFY: PASS | TESTS: core: ${core_count} | tray: ${tray_count} | gui: ${gui_count} | Total: ${total_tests}/${total_tests} | CLIPPY: CLEAN | FMT: CLEAN | LINT: CLEAN${gui_smoke_tag}${cov_tag}${mutants_tag} | TARGET: syspilot-workspace"
else
    echo "✔ All Rust checks passed: fmt, clippy, universalism lint, ${total_tests} tests (core: ${core_count}, tray: ${tray_count}, gui: ${gui_count}), and footprint benchmark${gui_smoke_tag}${cov_tag}${mutants_tag}."
fi
exit 0
