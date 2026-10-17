#!/usr/bin/env bash
# Install the Ember eza theme as ~/.config/eza/theme.yml.
#
#   ./eza/install.sh
#
# Backs up a differing theme.yml to theme.yml.bak. File names keep following
# LS_COLORS when that is set (zsh/install.sh sets it). Safe to run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${EZA_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/eza}"

if ! command -v eza >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  eza not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$dest/theme.yml" ] && ! cmp -s "$dir/theme.yml" "$dest/theme.yml"; then
    cp "$dest/theme.yml" "$dest/theme.yml.bak"
    echo "  backup: theme.yml.bak"
fi
cp "$dir/theme.yml" "$dest/theme.yml"
echo "  $dest/theme.yml"

echo "Done. Run: eza -l"
