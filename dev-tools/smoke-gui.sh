#!/usr/bin/env bash
# [SRE-TOOLING: Deterministic Dual-Skin Runtime Smoke Test for SysPilot GUI]
# Validates LCARS and Classic Slint skin initialization, single-frame rendering,
# and clean exit (0) using SLINT_BACKEND=winit-software + LIBGL_ALWAYS_SOFTWARE=1.
set -Eeuo pipefail
IFS=$'\n\t'

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root="${SYSPILOT_PROJECT_ROOT:-$(cd -- "$script_dir/.." && pwd -P)}"

quiet=0
gui_bin="${SYSPILOT_GUI_BIN:-}"

while (( $# > 0 )); do
    case "$1" in
        -q|--quiet)
            quiet=1
            shift
            ;;
        --bin)
            gui_bin="${2:?--bin requires a path to syspilot-gui}"
            shift 2
            ;;
        *)
            echo "Usage: $0 [-q|--quiet] [--bin <path>]" >&2
            exit 1
            ;;
    esac
done

fail_smoke() {
    local step="$1"
    local reason="$2"
    if (( quiet )); then
        echo "GUI_SMOKE: FAIL | STEP=${step} | SUMMARY=${reason} | TARGET=syspilot-gui"
    else
        echo "Error (${step}): ${reason}" >&2
    fi
    exit 1
}

if [[ -z "$gui_bin" ]]; then
    if ! (cd -- "$project_root" && cargo build --locked -p syspilot-gui >/tmp/syspilot-gui-smoke-build.log 2>&1); then
        fail_smoke "build" "cargo build -p syspilot-gui failed (see /tmp/syspilot-gui-smoke-build.log)"
    fi
    gui_bin="$project_root/target/debug/syspilot-gui"
fi

[[ -x "$gui_bin" ]] || fail_smoke "binary" "syspilot-gui binary not executable at $gui_bin"

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/syspilot-gui-smoke.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT

mkdir -p "$tmp_dir/home" "$tmp_dir/state/system-health" "$tmp_dir/config/syspilot"
now_iso=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
cat > "$tmp_dir/state/system-health/summary.json" <<EOF
{
  "schema_version": "2.0.0",
  "status": "FLIGHT_READY",
  "timestamp": "$now_iso",
  "counts": { "errors": 0, "warnings": 0 },
  "environment": { "kernel": "6.12.1-arch1-1" },
  "gaming": { "detected": false }
}
EOF

has_dri_render_node() {
    local dev
    for dev in /dev/dri/renderD*; do
        if [[ -r "$dev" && -w "$dev" ]]; then
            return 0
        fi
    done
    return 1
}

run_skin_smoke() {
    local skin="$1"
    local backend="$2"
    local gl_env=()
    if [[ "$backend" == "winit-software" ]]; then
        gl_env=(LIBGL_ALWAYS_SOFTWARE=1)
    fi
    if [[ -n "${WAYLAND_DISPLAY:-}" || -n "${DISPLAY:-}" ]]; then
        env HOME="$tmp_dir/home" XDG_STATE_HOME="$tmp_dir/state" XDG_CONFIG_HOME="$tmp_dir/config" \
            SLINT_BACKEND="$backend" "${gl_env[@]}" \
            timeout 10s "$gui_bin" --smoke --skin "$skin"
    elif [[ "$backend" == "winit-software" ]] && command -v cage >/dev/null 2>&1; then
        env HOME="$tmp_dir/home" XDG_STATE_HOME="$tmp_dir/state" XDG_CONFIG_HOME="$tmp_dir/config" \
            WLR_BACKENDS=headless LIBGL_ALWAYS_SOFTWARE=1 SLINT_BACKEND=winit-software \
            timeout 10s cage -s -- "$gui_bin" --smoke --skin "$skin"
    elif [[ "$backend" == "winit-software" ]] && command -v xvfb-run >/dev/null 2>&1; then
        env HOME="$tmp_dir/home" XDG_STATE_HOME="$tmp_dir/state" XDG_CONFIG_HOME="$tmp_dir/config" \
            SLINT_BACKEND=winit-software LIBGL_ALWAYS_SOFTWARE=1 \
            timeout 10s xvfb-run -a "$gui_bin" --smoke --skin "$skin"
    else
        return 42
    fi
}

backends=("winit-software")
if [[ -n "${WAYLAND_DISPLAY:-}" || -n "${DISPLAY:-}" ]] && has_dri_render_node; then
    backends+=("winit-femtovg")
fi

for backend in "${backends[@]}"; do
    for skin in lcars classic; do
        smoke_log="$tmp_dir/smoke-${backend}-${skin}.log"
        rc=0
        run_skin_smoke "$skin" "$backend" >"$smoke_log" 2>&1 || rc=$?
        if (( rc == 42 )); then
            if (( quiet )); then
                echo "GUI_SMOKE: SKIP | no display server (WAYLAND_DISPLAY/DISPLAY), cage, or xvfb-run"
            else
                echo "GUI_SMOKE: SKIP (no display server, cage, or xvfb-run available)"
            fi
            exit 0
        elif (( rc != 0 )); then
            first_err=$(head -n 1 "$smoke_log" | tr '|' '_' || true)
            fail_smoke "render-${backend}-${skin}" "syspilot-gui --smoke --skin ${skin} (${backend}) failed (rc=${rc}: ${first_err:-non-zero exit})"
        fi
    done
done

backends_csv=$(IFS=,; echo "${backends[*]}")
if (( quiet )); then
    echo "GUI_SMOKE: PASS | SKINS: lcars,classic | BACKENDS: ${backends_csv} | TARGET: syspilot-gui"
else
    echo "✔ Dual-Skin Runtime Smoke Test passed (skins: lcars, classic; backends: ${backends_csv})."
fi
exit 0
