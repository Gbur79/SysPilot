#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$(cd -- "$script_dir/.." && pwd -P)
dest_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
dest="$dest_dir/syspilot-tray"
tmp=''
trap '[[ -n ${tmp:-} ]] && rm -f -- "$tmp"' EXIT

cd -- "$project_root"
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked -p syspilot-tray

install -d -m 0755 -- "$dest_dir"
tmp=$(mktemp --tmpdir="$dest_dir" .syspilot-tray.XXXXXX)
install -m 0755 -- target/release/syspilot-tray "$tmp"
mv -fT -- "$tmp" "$dest"
tmp=''

if systemctl --user is-active --quiet syspilot-tray.service; then
    systemctl --user restart syspilot-tray.service
fi
printf 'Installed %s\n' "$dest"
