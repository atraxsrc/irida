#!/usr/bin/env bash
# Install the Ember Day eza theme as ~/.config/eza/theme.yml.
#
#   ./eza/install.sh
#
# Backs up a differing theme.yml to theme.yml.bak. File names keep following
# LS_COLORS when that is set (zsh/install.sh sets it). Safe to run again.
set -euo pipefail

# Copy a file to <file>.bak, or to <file>.bak-1, -2 and so on when that is
# taken, so an earlier backup is never replaced.
backup() {
    local bak="$1.bak" n=1
    while [ -e "$bak" ]; do
        bak="$1.bak-$n"
        n=$((n + 1))
    done
    cp "$1" "$bak"
    echo "  backup: ${bak##*/}"
}

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${EZA_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/eza}"

if ! command -v eza >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  eza not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$dest/theme.yml" ] && ! cmp -s "$dir/theme.yml" "$dest/theme.yml"; then
    backup "$dest/theme.yml"
fi
cp "$dir/theme.yml" "$dest/theme.yml"
echo "  $dest/theme.yml"

echo "Done. Run: eza -l"
