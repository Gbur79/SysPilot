#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$(cd -- "$script_dir/.." && pwd -P)
dest_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
dest="$dest_dir/syspilot-tray"
tmp=''
tmp_gui=''
tmp_cli=''
tmp_fallback=''
trap '[[ -n ${tmp:-} ]] && rm -f -- "$tmp"; [[ -n ${tmp_gui:-} ]] && rm -f -- "$tmp_gui"; [[ -n ${tmp_cli:-} ]] && rm -f -- "$tmp_cli"; [[ -n ${tmp_fallback:-} ]] && rm -f -- "$tmp_fallback"' EXIT

cd -- "$project_root"
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked -p syspilot-cli -p syspilot-tray -p syspilot-gui

install -d -m 0755 -- "$dest_dir"
lib_bin_dir="$(dirname -- "$(cd -- "$dest_dir" && pwd -P)")/lib/syspilot/bin"
install -d -m 0755 -- "$lib_bin_dir"

# 1. Install syspilot-tray
tmp=$(mktemp --tmpdir="$dest_dir" .syspilot-tray.XXXXXX)
install -m 0755 -- target/release/syspilot-tray "$tmp"
mv -fT -- "$tmp" "$dest"
tmp=''

# 2. Install syspilot-gui
tmp_gui=$(mktemp --tmpdir="$dest_dir" .syspilot-gui.XXXXXX)
install -m 0755 -- target/release/syspilot-gui "$tmp_gui"
mv -fT -- "$tmp_gui" "$dest_dir/syspilot-gui"
tmp_gui=''

# 3. Install native Rust syspilot CLI
tmp_cli=$(mktemp --tmpdir="$dest_dir" .syspilot.XXXXXX)
install -m 0755 -- target/release/syspilot "$tmp_cli"
mv -fT -- "$tmp_cli" "$dest_dir/syspilot"
tmp_cli=''

# 4. Install bin/syspilot shell fallback router
tmp_fallback=$(mktemp --tmpdir="$lib_bin_dir" .syspilot-fallback.XXXXXX)
install -m 0755 -- bin/syspilot "$tmp_fallback"
mv -fT -- "$tmp_fallback" "$lib_bin_dir/syspilot"
tmp_fallback=''

if systemctl --user is-active --quiet syspilot-tray.service; then
    systemctl --user restart syspilot-tray.service
fi
printf 'Installed %s, %s/syspilot-gui, %s/syspilot (Rust CLI), and %s/syspilot (fallback)\n' "$dest" "$dest_dir" "$dest_dir" "$lib_bin_dir"
