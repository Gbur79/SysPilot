#!/usr/bin/env bash
# [SRE-TOOLING: Footprint Benchmark Engine for SysPilot Tray]
# Reads /proc/$pid telemetry (smaps_rollup, status, fd, task status) and validates resource budgets.
set -Eeuo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$(cd -- "$script_dir/.." && pwd -P)

quiet=0
smoke=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        -q|--quiet)
            quiet=1
            shift
            ;;
        --smoke)
            smoke=1
            shift
            ;;
        *)
            echo "Usage: $0 [--smoke] [-q|--quiet]" >&2
            exit 1
            ;;
    esac
done

spawned_pid=""
tmp_state=""
cleanup() {
    if [[ -n "$spawned_pid" ]]; then
        pkill -TERM -P "$spawned_pid" 2>/dev/null || true
        kill -TERM "$spawned_pid" 2>/dev/null || true
        wait "$spawned_pid" 2>/dev/null || true
    fi
    if [[ -n "$tmp_state" && -d "$tmp_state" ]]; then
        rm -rf "$tmp_state"
    fi
}
trap cleanup EXIT

pid=$(pgrep -x syspilot-tray | head -n 1 || true)

if [[ -z "$pid" ]]; then
    # Locate candidate binary
    tray_bin=""
    for candidate in \
        "$project_root/target/release/syspilot-tray" \
        "$HOME/.local/bin/syspilot-tray" \
        "$project_root/target/debug/syspilot-tray"; do
        if [[ -x "$candidate" ]]; then
            tray_bin="$candidate"
            break
        fi
    done

    if [[ -z "$tray_bin" ]]; then
        if (( quiet )); then
            echo "FOOTPRINT: FAIL | syspilot-tray binary not found and no running process"
            echo "DIAG_1: STEP=bench;EXIT=1;TARGET=syspilot-tray"
            echo "DIAG_2: SUMMARY=binary not found"
        else
            echo "Error: syspilot-tray binary not found and no active instance running." >&2
        fi
        exit 1
    fi

    tmp_state=$(mktemp -d "${TMPDIR:-/tmp}/syspilot-bench-state.XXXXXX")
    for fd in $(ls /proc/$$/fd 2>/dev/null || true); do
        if [[ "$fd" =~ ^[0-9]+$ ]] && (( fd > 2 )); then
            eval "exec $fd>&-" 2>/dev/null || true
        fi
    done
    if command -v dbus-run-session >/dev/null 2>&1; then
        XDG_STATE_HOME="$tmp_state" dbus-run-session "$tray_bin" >/dev/null 2>&1 &
        spawned_pid=$!
    else
        XDG_STATE_HOME="$tmp_state" "$tray_bin" >/dev/null 2>&1 &
        spawned_pid=$!
    fi

    # Allow daemon to settle
    sleep 1.2
    child_pid=$(pgrep -P "$spawned_pid" -x syspilot-tray | head -n 1 || true)
    if [[ -n "$child_pid" ]]; then
        pid="$child_pid"
    else
        pid="$spawned_pid"
    fi
fi

if [[ ! -d "/proc/$pid" ]]; then
    if (( quiet )); then
        echo "FOOTPRINT: FAIL | PID $pid disappeared"
        echo "DIAG_1: STEP=bench;EXIT=1;TARGET=syspilot-tray"
        echo "DIAG_2: SUMMARY=process exited prematurely"
    else
        echo "Error: Process $pid terminated prematurely." >&2
    fi
    exit 1
fi

# Telemetry extraction
if [[ ! -f "/proc/$pid/smaps_rollup" ]]; then
    if (( quiet )); then
        echo "FOOTPRINT: FAIL | /proc/$pid/smaps_rollup not accessible"
        echo "DIAG_1: STEP=bench;EXIT=1;TARGET=syspilot-tray"
        echo "DIAG_2: SUMMARY=smaps_rollup missing"
    else
        echo "Error: /proc/$pid/smaps_rollup unreadable." >&2
    fi
    exit 1
fi

private_dirty=$(grep -m1 '^Private_Dirty:' "/proc/$pid/smaps_rollup" | awk '{print $2}')
pss=$(grep -m1 '^Pss:' "/proc/$pid/smaps_rollup" | awk '{print $2}')
threads=$(grep -m1 '^Threads:' "/proc/$pid/status" | awk '{print $2}')
fds=$(ls -1 "/proc/$pid/fd" 2>/dev/null | wc -l)
voluntary_switches=$(awk '/voluntary_ctxt_switches:/ { sum += $2 } END { print sum+0 }' /proc/"$pid"/task/*/status 2>/dev/null || echo 0)

private_dirty=${private_dirty:-0}
pss=${pss:-0}
threads=${threads:-0}
fds=${fds:-0}

# Budget enforcement:
# Private_Dirty <= 2048 kB
# Pss <= 5000 kB
# Threads <= 8
# FDs <= 22
budget_pass=1
failures=()

if (( private_dirty > 2048 )); then
    budget_pass=0
    failures+=("Private_Dirty: ${private_dirty}kB > 2048kB")
fi

if (( pss > 5000 )); then
    budget_pass=0
    failures+=("Pss: ${pss}kB > 5000kB")
fi

if (( threads > 8 )); then
    budget_pass=0
    failures+=("Threads: ${threads} > 8")
fi

if (( fds > 22 )); then
    budget_pass=0
    failures+=("FDs: ${fds} > 22")
fi

if (( budget_pass )); then
    if (( quiet )); then
        echo "FOOTPRINT: PASS | PRIVATE_DIRTY: ${private_dirty}KB | PSS: ${pss}KB | THREADS: ${threads} | FDS: ${fds}"
    else
        echo "✔ SysPilot Tray Footprint Benchmark Passed:"
        echo "  - Private_Dirty: ${private_dirty} kB (budget: <= 2048 kB)"
        echo "  - Pss:           ${pss} kB (budget: <= 5000 kB)"
        echo "  - Threads:       ${threads} (budget: <= 8)"
        echo "  - Open FDs:      ${fds} (budget: <= 22)"
        echo "  - Context Sw.:   ${voluntary_switches}"
    fi
    exit 0
else
    if (( quiet )); then
        echo "FOOTPRINT: FAIL | PRIVATE_DIRTY: ${private_dirty}KB | PSS: ${pss}KB | THREADS: ${threads} | FDS: ${fds}"
        echo "DIAG_1: STEP=bench;EXIT=1;TARGET=syspilot-tray"
        echo "DIAG_2: SUMMARY=${failures[*]}"
    else
        echo "Error: SysPilot Tray exceeded memory/thread footprint budget:" >&2
        for fail in "${failures[@]}"; do
            echo "  ✖ $fail" >&2
        done
    fi
    exit 1
fi
