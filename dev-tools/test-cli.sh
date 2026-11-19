#!/usr/bin/env bash
# [SRE-TOOLING: Hermetic CLI & Packaging Test Suite for SysPilot]
# Sprint 6 Gate: Verifies CLI flags, exit semantics, engine resilience, and packaging metadata.
# Supports quiet 1-line mode (-q / --quiet).
set -Eeuo pipefail
IFS=$'\n\t'

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$(cd -- "$script_dir/.." && pwd -P)
CLI="$project_root/bin/syspilot"

quiet=0
if [[ "${1:-}" == "-q" || "${1:-}" == "--quiet" ]]; then
    quiet=1
fi

sandbox=$(mktemp -d /tmp/syspilot-cli-test.XXXXXX)
trap 'rm -rf "$sandbox"' EXIT

# Isolate environment to sandbox
export HOME="$sandbox/home"
export XDG_STATE_HOME="$sandbox/state"
export XDG_CONFIG_HOME="$sandbox/config"
export XDG_BIN_HOME="$sandbox/bin"
mkdir -p "$HOME" "$XDG_STATE_HOME/system-health" "$XDG_CONFIG_HOME/syspilot" "$XDG_BIN_HOME"

unset SYSPILOT_SYS_HEALTH SYSPILOT_MAX_STATUS_AGE SYSPILOT_TERMINAL SYSPILOT_COPILOT_CMD SYSPILOT_GOOSE_RECIPE

tests_passed=0
total_tests=16
current_test=""

fail() {
    local reason=$1
    if (( quiet )); then
        echo "CLI_VERIFY: FAIL | TESTS: ${tests_passed}/${total_tests} | TARGET: syspilot-cli"
        echo "DIAG_1: STEP=${current_test};EXIT=1;TARGET=syspilot-cli"
        echo "DIAG_2: SUMMARY=${reason}"
    else
        printf '  [FAIL] %s: %s\n' "$current_test" "$reason" >&2
    fi
    exit 1
}

pass_step() {
    (( ++tests_passed ))
    if (( ! quiet )); then
        printf '  [PASS] Test %2d/%d: %s\n' "$tests_passed" "$total_tests" "$current_test"
    fi
}

[[ -x "$CLI" ]] || fail "CLI executable not found or not executable at $CLI"

# -----------------------------------------------------------------------------
# Test 1: Help flags (-h, --help)
# -----------------------------------------------------------------------------
current_test="help_flags"
out_h=$("$CLI" -h 2>&1) || fail "-h returned non-zero exit code"
[[ "$out_h" == *"Usage: syspilot [OPTION]"* ]] || fail "-h missing usage header"
[[ "$out_h" =~ "-v, --version" ]] || fail "-h missing version flag"
[[ "$out_h" =~ "--copilot" ]] || fail "-h missing copilot description"
out_help=$("$CLI" --help 2>&1) || fail "--help returned non-zero exit code"
[[ "$out_help" == "$out_h" ]] || fail "-h and --help outputs differ"
pass_step

# -----------------------------------------------------------------------------
# Test 2: Version flags (-v, --version)
# -----------------------------------------------------------------------------
current_test="version_flags"
expected_ver=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$project_root/Cargo.toml" | head -n1)
out_v=$("$CLI" -v 2>&1) || fail "-v returned non-zero exit code"
[[ "$out_v" == "syspilot $expected_ver" ]] || fail "-v returned '$out_v', expected 'syspilot $expected_ver'"
out_ver=$("$CLI" --version 2>&1) || fail "--version returned non-zero exit code"
[[ "$out_ver" == "syspilot $expected_ver" ]] || fail "--version returned '$out_ver', expected 'syspilot $expected_ver'"
pass_step

# -----------------------------------------------------------------------------
# Test 3: Unknown option rejection
# -----------------------------------------------------------------------------
current_test="unknown_option_rejection"
set +e
out_unk=$("$CLI" --nonexistent-option-xyz 2>&1)
rc=$?
set -e
(( rc == 1 )) || fail "--nonexistent-option-xyz returned rc=$rc, expected 1"
[[ "$out_unk" == *"syspilot: unknown option: --nonexistent-option-xyz (use --help)"* ]] || fail "unexpected unknown option error output: $out_unk"
pass_step

# -----------------------------------------------------------------------------
# Test 4: Missing engine defense & actionable guidance
# -----------------------------------------------------------------------------
current_test="missing_engine_defense"
# 4a. Forwarding command with nonexistent engine override
set +e
out_missing=$(env SYSPILOT_SYS_HEALTH="$sandbox/no-such-engine" "$CLI" --audit 2>&1)
rc=$?
set -e
(( rc == 1 )) || fail "missing engine --audit returned rc=$rc, expected 1"
[[ "$out_missing" =~ "sys-health is not installed" ]] || fail "expected 'sys-health is not installed' message"

# 4b. Status check when no summary exists and sys-health is missing
rm -f "$XDG_STATE_HOME/system-health/summary.json"
mkdir -p "$sandbox/empty-path"
ln -s "$(command -v bash)" "$sandbox/empty-path/bash"
ln -s "$(command -v readlink)" "$sandbox/empty-path/readlink"
ln -s "$(command -v dirname)" "$sandbox/empty-path/dirname"
set +e
out_no_summary=$(env PATH="$sandbox/empty-path" "$CLI" -s 2>&1)
rc=$?
set -e
(( rc == 1 )) || fail "status check with no summary/engine returned rc=$rc, expected 1"
[[ "$out_no_summary" =~ "no readable canonical summary; sys-health is not installed" ]] || fail "expected actionable install message when summary and engine missing, got: $out_no_summary"
pass_step

# -----------------------------------------------------------------------------
# Test 5: Forwarding options to engine
# -----------------------------------------------------------------------------
current_test="forwarding_options"
mock_engine="$sandbox/bin/mock-sys-health"
cat <<'EOF' > "$mock_engine"
#!/usr/bin/env bash
printf 'MOCK_ENGINE_CALLED:%s\n' "$*"
EOF
chmod +x "$mock_engine"

for flag in "--audit" "--upgrade" "--software" "-o" "--mirrors" "-m" "-d" "-g"; do
    out_fwd=$(env SYSPILOT_SYS_HEALTH="$mock_engine" "$CLI" "$flag" 2>&1)
    [[ "$out_fwd" =~ "MOCK_ENGINE_CALLED:" ]] || fail "flag $flag failed to forward to engine: $out_fwd"
done
pass_step

# -----------------------------------------------------------------------------
# Test 6: Refusal of circular embedded engine copies
# -----------------------------------------------------------------------------
current_test="refusal_circular_embedded_engine"
embedded_engine="$sandbox/sysPilot/bin/sys-health.sh"
mkdir -p "$(dirname -- "$embedded_engine")"
cp "$mock_engine" "$embedded_engine"
set +e
out_embed=$(env SYSPILOT_SYS_HEALTH="$embedded_engine" "$CLI" --audit 2>&1)
rc=$?
set -e
(( rc == 1 )) || fail "embedded engine execution returned rc=$rc, expected 1"
[[ "$out_embed" =~ "refusing embedded sys-health copy:" ]] || fail "expected refusal message for embedded engine, got: $out_embed"
pass_step

# -----------------------------------------------------------------------------
# Test 7: Valid summary.json parsing (-j exit 0, -s exit 0)
# -----------------------------------------------------------------------------
current_test="valid_summary_json"
summary_file="$XDG_STATE_HOME/system-health/summary.json"
now_iso=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
cat <<EOF > "$summary_file"
{
  "schema_version": "2.0.0",
  "status": "FLIGHT_READY",
  "timestamp": "$now_iso",
  "counts": { "errors": 0, "warnings": 0 },
  "environment": { "kernel": "6.12.1-arch1-1" },
  "gaming": { "detected": false }
}
EOF
touch "$summary_file"

out_j=$("$CLI" -j) || fail "-j failed on valid summary"
[[ "$out_j" =~ "FLIGHT_READY" ]] || fail "-j output missing FLIGHT_READY"

out_s=$("$CLI" -s) || fail "-s failed on valid summary"
[[ "$out_s" =~ "Status:       FLIGHT_READY" ]] || fail "-s output missing status"
[[ "$out_s" =~ "SysPilot — cached canonical health summary" ]] || fail "-s missing header"
pass_step

# -----------------------------------------------------------------------------
# Test 8: Stale cache detection (-j exit 3, -s exit 3)
# -----------------------------------------------------------------------------
current_test="stale_cache_detection"
# Set mtime back 25000s (> default MAX_STATUS_AGE 21600s)
touch -d "25000 seconds ago" "$summary_file"

set +e
out_stale_j=$("$CLI" -j 2>&1)
rc_j=$?
set -e
(( rc_j == 3 )) || fail "-j on stale cache returned rc=$rc_j, expected 3"
[[ "$out_stale_j" == *"FLIGHT_READY"* ]] || fail "-j on stale cache missing FLIGHT_READY output"

set +e
out_stale_s=$("$CLI" -s 2>&1)
rc_s=$?
set -e
(( rc_s == 3 )) || fail "-s on stale cache returned rc=$rc_s, expected 3"
[[ "$out_stale_s" == *"(STALE)"* ]] || fail "-s missing (STALE) marker"
[[ "$out_stale_s" =~ "cached data is stale" ]] || fail "-s missing stale warning message"
pass_step

# -----------------------------------------------------------------------------
# Test 9: Degraded state handling (ACTION_REQUIRED exit 1)
# -----------------------------------------------------------------------------
current_test="degraded_state_handling"
cat <<EOF > "$summary_file"
{
  "schema_version": "2.0.0",
  "status": "ACTION_REQUIRED",
  "timestamp": "$now_iso",
  "counts": { "errors": 2, "warnings": 1 },
  "environment": { "kernel": "6.12.1-arch1-1" },
  "gaming": { "detected": true }
}
EOF
touch "$summary_file"

set +e
out_action=$("$CLI" -s 2>&1)
rc_action=$?
set -e
(( rc_action == 1 )) || fail "ACTION_REQUIRED returned rc=$rc_action, expected 1"
[[ "$out_action" =~ "Status:       ACTION_REQUIRED" ]] || fail "missing ACTION_REQUIRED in status output"
[[ "$out_action" =~ "Game profile: detected" ]] || fail "missing Game profile detection in status output"
pass_step

# -----------------------------------------------------------------------------
# Test 10: Corrupted JSON resilience
# -----------------------------------------------------------------------------
current_test="corrupted_json_resilience"
# 10a. Malformed JSON syntax
echo '{"schema_version": "2.0.0", "broken_json":' > "$summary_file"
set +e
out_broken=$("$CLI" -s 2>&1)
rc_broken=$?
set -e
(( rc_broken == 1 )) || fail "broken JSON returned rc=$rc_broken, expected 1"
[[ "$out_broken" =~ "syspilot: invalid or incompatible summary schema" ]] || fail "expected invalid schema error, got: $out_broken"

# 10b. Incompatible schema version
echo '{"schema_version": "1.0.0", "status": "FLIGHT_READY", "timestamp": "2026-01-01"}' > "$summary_file"
set +e
out_v1=$("$CLI" -s 2>&1)
rc_v1=$?
set -e
(( rc_v1 == 1 )) || fail "incompatible schema v1 returned rc=$rc_v1, expected 1"
[[ "$out_v1" =~ "syspilot: invalid or incompatible summary schema" ]] || fail "expected invalid schema error for v1, got: $out_v1"
pass_step

# -----------------------------------------------------------------------------
# Test 11: Autostart & packaging metadata audit
# -----------------------------------------------------------------------------
current_test="packaging_and_autostart_metadata"
# 11a. Check autostart desktop entry for duplicate prevention
autostart_file="$project_root/autostart/syspilot.desktop"
[[ -f "$autostart_file" ]] || fail "autostart/syspilot.desktop not found"
grep -q "X-systemd-skip=true" "$autostart_file" || fail "autostart desktop missing X-systemd-skip=true"
grep -q "Exec=syspilot --tray" "$autostart_file" || fail "autostart desktop missing Exec=syspilot --tray"

# 11b. Check packaging desktop entry
app_desktop="$project_root/packaging/syspilot.desktop"
[[ -f "$app_desktop" ]] || fail "packaging/syspilot.desktop not found"
grep -q "Exec=syspilot --gui" "$app_desktop" || fail "packaging desktop missing Exec=syspilot --gui"
grep -q "Keywords=.*system.*health.*copilot.*" "$app_desktop" || fail "packaging desktop missing Keywords metadata"

# 11c. Check systemd service unit
service_file="$project_root/systemd/syspilot-tray.service"
[[ -f "$service_file" ]] || fail "systemd/syspilot-tray.service not found"
grep -q "Slice=app.slice" "$service_file" || fail "systemd service missing Slice=app.slice"

# 11d. Check PKGBUILD syntax
pkgbuild="$project_root/packaging/PKGBUILD"
[[ -f "$pkgbuild" ]] || fail "packaging/PKGBUILD not found"
bash -n "$pkgbuild" 2>&1 || fail "bash -n packaging/PKGBUILD failed"
grep -q "pkgver=$expected_ver" "$pkgbuild" || fail "PKGBUILD pkgver mismatch (expected $expected_ver)"

# 11e. Shellcheck validation if available
shellcheck_status="CLEAN"
if command -v shellcheck >/dev/null 2>&1; then
    shellcheck -s bash "$CLI" >/dev/null 2>&1 || fail "shellcheck failed on bin/syspilot"
    shellcheck -s bash "$project_root/dev-tools/build-and-install.sh" >/dev/null 2>&1 || fail "shellcheck failed on build-and-install.sh"
    shellcheck -s bash "$project_root/dev-tools/verify-rust.sh" >/dev/null 2>&1 || fail "shellcheck failed on verify-rust.sh"
    shellcheck -s bash "$project_root/dev-tools/smoke-gui.sh" >/dev/null 2>&1 || fail "shellcheck failed on smoke-gui.sh"
    shellcheck -s bash "$project_root/dev-tools/test-cli.sh" >/dev/null 2>&1 || fail "shellcheck failed on test-cli.sh"
else
    shellcheck_status="SKIP(not_installed)"
fi
pass_step

# -----------------------------------------------------------------------------
# Test 12: Upstream update check flag (--check-update) in help
# -----------------------------------------------------------------------------
current_test="check_update_flag_help"
[[ "$out_h" =~ "--check-update" ]] || fail "-h missing --check-update flag"
[[ "$out_h" =~ "Check upstream GitHub release for updates" ]] || fail "-h missing --check-update description"
pass_step

# -----------------------------------------------------------------------------
# Test 13: Self-update flag (--self-update) in help
# -----------------------------------------------------------------------------
current_test="self_update_flag_help"
[[ "$out_h" =~ "--self-update" ]] || fail "-h missing --self-update flag"
[[ "$out_h" =~ "Perform safe self-update and atomic build" ]] || fail "-h missing --self-update description"
pass_step

# -----------------------------------------------------------------------------
# Test 14: verify-rust.sh --mutants-diff untracked crate file coverage (PATCH-036)
# -----------------------------------------------------------------------------
current_test="verify_rust_mutants_diff_untracked"
mut_td=$(mktemp -d "/tmp/syspilot-test-mutants.XXXXXX")
mkdir -p "$mut_td/bin" "$mut_td/dev-tools" "$mut_td/crates/syspilot-core/src"
printf '[workspace]\n' > "$mut_td/Cargo.toml"
printf 'pub fn ok() {}\n' > "$mut_td/crates/syspilot-core/src/lib.rs"
printf '#!/usr/bin/env bash\necho "LINT_UNIVERSALISM: PASS"\n' > "$mut_td/dev-tools/lint-universalism.sh"
printf '#!/usr/bin/env bash\necho "FOOTPRINT: PASS"\n' > "$mut_td/dev-tools/bench-footprint.sh"
cp "$project_root/dev-tools/verify-rust.sh" "$mut_td/dev-tools/verify-rust.sh"
chmod +x "$mut_td/dev-tools/lint-universalism.sh" "$mut_td/dev-tools/bench-footprint.sh" "$mut_td/dev-tools/verify-rust.sh"
cat > "$mut_td/bin/cargo" <<EOF
#!/usr/bin/env bash
if [[ "\$1" == "test" ]]; then
    echo "Running unittests (syspilot_core)"
    echo "test result: ok. 1 passed; 0 failed;"
    echo "Running unittests (syspilot_tray)"
    echo "test result: ok. 1 passed; 0 failed;"
    echo "Running unittests (syspilot_gui)"
    echo "test result: ok. 1 passed; 0 failed;"
    exit 0
elif [[ "\$1" == "mutants" ]]; then
    echo "MUTANTS:\$*" >> "$mut_td/calls.log"
    if [[ "\${STUB_MUTANTS_FAIL:-0}" == "1" ]]; then
        echo "1 mutants tested in 1s: 1 missed"
        exit 2
    fi
    echo "1 mutants tested in 1s: 1 caught"
    exit 0
fi
exit 0
EOF
chmod +x "$mut_td/bin/cargo"
(
    cd "$mut_td"
    git init -q
    git config user.email "test@localhost"
    git config user.name "Test"
    git add .
    git commit -q -m "init"
)
out_clean=$(PATH="$mut_td/bin:$PATH" SYSPILOT_PROJECT_ROOT="$mut_td" "$mut_td/dev-tools/verify-rust.sh" --mutants-diff -q 2>&1) || { rm -rf "$mut_td"; fail "verify-rust.sh clean --mutants-diff failed"; }
[[ "$out_clean" == *"Found 0 mutants in diff"* ]] || { rm -rf "$mut_td"; fail "expected Found 0 mutants in diff on clean repo"; }

printf 'pub fn probe() -> bool { true }\n' > "$mut_td/crates/syspilot-core/src/untracked_probe.rs"
out_untracked=$(PATH="$mut_td/bin:$PATH" SYSPILOT_PROJECT_ROOT="$mut_td" "$mut_td/dev-tools/verify-rust.sh" --mutants-diff -q 2>&1) || { rm -rf "$mut_td"; fail "verify-rust.sh untracked --mutants-diff failed"; }
[[ "$out_untracked" == *"MUTANTS: PASS"* ]] || { rm -rf "$mut_td"; fail "expected MUTANTS: PASS on untracked file, got: $out_untracked"; }
grep -q -- "-p syspilot-core -f crates/syspilot-core/src/untracked_probe.rs -- --lib" "$mut_td/calls.log" || { rm -rf "$mut_td"; fail "verify-rust.sh did not pass untracked_probe.rs to cargo mutants"; }

if PATH="$mut_td/bin:$PATH" SYSPILOT_PROJECT_ROOT="$mut_td" STUB_MUTANTS_FAIL=1 "$mut_td/dev-tools/verify-rust.sh" --mutants-diff -q >"$mut_td/fail.out" 2>&1; then
    rm -rf "$mut_td"
    fail "verify-rust.sh should fail when untracked file has surviving mutants"
fi
grep -q "MUTANTS: FAIL" "$mut_td/fail.out" || { rm -rf "$mut_td"; fail "expected MUTANTS: FAIL on surviving mutant"; }
rm -rf "$mut_td"
pass_step

# -----------------------------------------------------------------------------
# Test 15: verify-rust.sh --cov fast LLVM coverage gate (PATCH-038)
# -----------------------------------------------------------------------------
current_test="verify_rust_cov_gate"
cov_td=$(mktemp -d "/tmp/syspilot-test-cov.XXXXXX")
mkdir -p "$cov_td/bin" "$cov_td/dev-tools" "$cov_td/crates/syspilot-core/src"
printf '[workspace]\n' > "$cov_td/Cargo.toml"
printf 'pub fn ok() {}\n' > "$cov_td/crates/syspilot-core/src/lib.rs"
printf '#!/usr/bin/env bash\necho "LINT_UNIVERSALISM: PASS"\n' > "$cov_td/dev-tools/lint-universalism.sh"
printf '#!/usr/bin/env bash\necho "FOOTPRINT: PASS"\n' > "$cov_td/dev-tools/bench-footprint.sh"
cp "$project_root/dev-tools/verify-rust.sh" "$cov_td/dev-tools/verify-rust.sh"
chmod +x "$cov_td/dev-tools/lint-universalism.sh" "$cov_td/dev-tools/bench-footprint.sh" "$cov_td/dev-tools/verify-rust.sh"
cat > "$cov_td/bin/cargo" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "test" ]]; then
    echo "Running unittests (syspilot_core)"
    echo "test result: ok. 1 passed; 0 failed;"
    echo "Running unittests (syspilot_tray)"
    echo "test result: ok. 1 passed; 0 failed;"
    echo "Running unittests (syspilot_gui)"
    echo "test result: ok. 1 passed; 0 failed;"
    exit 0
elif [[ "$1" == "llvm-cov" ]]; then
    if [[ "${STUB_COV_MISSING:-0}" == "1" ]]; then
        echo "error: no such command: \`llvm-cov\`" >&2
        exit 101
    fi
    if [[ "$2" == "report" ]]; then
        echo "Filename   Regions   Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover"
        exit 0
    fi
    out_path=""
    while (( $# > 0 )); do
        if [[ "$1" == "--output-path" ]]; then
            out_path="$2"
            shift 2
        else
            shift
        fi
    done
    cat > "$out_path" <<'JSON'
{
  "data": [
    {
      "files": [
        {
          "filename": "crates/syspilot-core/src/lib.rs",
          "summary": {
            "lines": { "count": 20, "covered": 19, "percent": 95.0 },
            "functions": { "count": 2, "covered": 2, "percent": 100.0 },
            "regions": { "count": 10, "covered": 9, "percent": 90.0 }
          }
        },
        {
          "filename": "crates/syspilot-core/src/new_cov.rs",
          "summary": {
            "lines": { "count": 10, "covered": 5, "percent": 50.0 },
            "functions": { "count": 1, "covered": 1, "percent": 100.0 },
            "regions": { "count": 8, "covered": 4, "percent": 50.0 }
          }
        }
      ],
      "totals": {
        "lines": { "count": 30, "covered": 24, "percent": 80.0 },
        "functions": { "count": 3, "covered": 3, "percent": 100.0 },
        "regions": { "count": 18, "covered": 13, "percent": 72.2 }
      }
    }
  ],
  "version": "2.0.1"
}
JSON
    exit 0
fi
exit 0
EOF
chmod +x "$cov_td/bin/cargo"
(
    cd "$cov_td"
    git init -q
    git config user.email "test@localhost"
    git config user.name "Test"
    git add .
    git commit -q -m "init"
)
out_cov_clean=$(PATH="$cov_td/bin:$PATH" SYSPILOT_PROJECT_ROOT="$cov_td" SYSPILOT_COV_LOG="$cov_td/cov.log" "$cov_td/dev-tools/verify-rust.sh" --cov -q 2>&1) || { rm -rf "$cov_td"; fail "verify-rust.sh clean --cov failed: $out_cov_clean"; }
[[ "$out_cov_clean" == *"COV: PASS (L:80.0% R:72.2% F:100.0%)"* ]] || { rm -rf "$cov_td"; fail "unexpected clean --cov output: $out_cov_clean"; }
[[ -f "$cov_td/cov.log" ]] || { rm -rf "$cov_td"; fail "expected coverage report log at $cov_td/cov.log"; }

printf 'pub fn uncovered() -> bool { false }\n' > "$cov_td/crates/syspilot-core/src/new_cov.rs"
out_cov_diff=$(PATH="$cov_td/bin:$PATH" SYSPILOT_PROJECT_ROOT="$cov_td" SYSPILOT_COV_LOG="$cov_td/cov.log" "$cov_td/dev-tools/verify-rust.sh" --cov -q 2>&1) || { rm -rf "$cov_td"; fail "verify-rust.sh diff --cov failed: $out_cov_diff"; }
[[ "$out_cov_diff" == *"DIFF_L:50.0% DIFF_R:50.0%"* ]] || { rm -rf "$cov_td"; fail "expected DIFF_L:50.0% DIFF_R:50.0% in --cov output, got: $out_cov_diff"; }

if PATH="$cov_td/bin:$PATH" SYSPILOT_PROJECT_ROOT="$cov_td" SYSPILOT_COV_LOG="$cov_td/cov.log" "$cov_td/dev-tools/verify-rust.sh" --cov-fail-under 80 -q >"$cov_td/fail_thresh.out" 2>&1; then
    rm -rf "$cov_td"
    fail "verify-rust.sh should fail when diff region coverage (50%) is below --cov-fail-under 80"
fi
grep -q "COV: FAIL" "$cov_td/fail_thresh.out" || { rm -rf "$cov_td"; fail "expected COV: FAIL on threshold breach"; }

if PATH="$cov_td/bin:$PATH" SYSPILOT_PROJECT_ROOT="$cov_td" SYSPILOT_COV_LOG="$cov_td/cov.log" STUB_COV_MISSING=1 "$cov_td/dev-tools/verify-rust.sh" --cov -q >"$cov_td/fail_missing.out" 2>&1; then
    rm -rf "$cov_td"
    fail "verify-rust.sh should fail when cargo-llvm-cov is missing"
fi
grep -q "cargo-llvm-cov" "$cov_td/fail_missing.out" || { rm -rf "$cov_td"; fail "expected cargo-llvm-cov install hint on missing subcommand"; }
rm -rf "$cov_td"
pass_step

# -----------------------------------------------------------------------------
# Test 16: lint-universalism.sh /usr/bin & /usr/local/bin guard + allow override
# -----------------------------------------------------------------------------
current_test="lint_universalism_usr_bin_guard"
lint_td=$(mktemp -d "/tmp/syspilot-test-lint.XXXXXX")
cat > "$lint_td/sample.rs" <<'EOF'
use std::path::{Path, PathBuf};
pub fn bad_bins() -> (PathBuf, &'static Path) {
    (PathBuf::from("/usr/bin/bash"), Path::new("/usr/local/bin/tool"))
}
EOF
if "$project_root/dev-tools/lint-universalism.sh" --target-dir "$lint_td" -q >"$lint_td/fail.out" 2>&1; then
    rm -rf "$lint_td"
    fail "lint-universalism.sh should reject unparameterized /usr/bin and /usr/local/bin paths"
fi
grep -q "LINT_UNIVERSALISM: FAIL" "$lint_td/fail.out" || { rm -rf "$lint_td"; fail "expected LINT_UNIVERSALISM: FAIL on /usr/bin fixture"; }
"$project_root/dev-tools/lint-universalism.sh" --target-dir "$lint_td" >"$lint_td/detail.out" 2>&1 || true
grep -q "sample.rs:3:" "$lint_td/detail.out" || { rm -rf "$lint_td"; fail "expected sample.rs:3 violation line"; }

cat > "$lint_td/sample.rs" <<'EOF'
use std::path::{Path, PathBuf};
pub fn allowed_bins() -> (PathBuf, &'static Path) {
    (
        PathBuf::from("/usr/bin/bash"), // syspilot-lint:allow
        Path::new("/usr/local/bin/tool"), // syspilot-lint:allow
    )
}
EOF
"$project_root/dev-tools/lint-universalism.sh" --target-dir "$lint_td" -q >"$lint_td/pass.out" 2>&1 || { rm -rf "$lint_td"; fail "lint-universalism.sh should allow lines marked with syspilot-lint:allow"; }
grep -q "LINT_UNIVERSALISM: PASS" "$lint_td/pass.out" || { rm -rf "$lint_td"; fail "expected LINT_UNIVERSALISM: PASS on allowed /usr/bin fixture"; }
rm -rf "$lint_td"
pass_step

# -----------------------------------------------------------------------------
# Final Reporting
# -----------------------------------------------------------------------------
if (( quiet )); then
    echo "CLI_VERIFY: PASS | TESTS: ${tests_passed}/${total_tests} | SHELLCHECK: ${shellcheck_status} | METADATA: AUDITED | TARGET: syspilot-cli"
else
    printf '\n✔ All %d CLI & packaging tests passed successfully (Shellcheck: %s).\n' "$tests_passed" "$shellcheck_status"
fi
exit 0
