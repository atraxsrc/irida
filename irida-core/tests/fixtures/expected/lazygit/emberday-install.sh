#!/usr/bin/env bash
# Install the Ember Day lazygit colours beside your own config.yml.
#
#   ./lazygit/install.sh
#
# Copies emberday.yml to ~/.config/lazygit/ (or under $XDG_CONFIG_HOME).
# lazygit reads it through LG_CONFIG_FILE, which zsh/emberday.zsh sets to
# config.yml plus this file; another shell needs that export itself. Backs
# up a differing theme file to emberday.yml.bak. Safe to run again.
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
cfg="${XDG_CONFIG_HOME:-}"
case "$cfg" in /*) ;; *) cfg="$HOME/.config" ;; esac
dest="$cfg/lazygit"
theme="$dest/emberday.yml"

if ! command -v lazygit >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  lazygit not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$theme" ] && ! cmp -s "$dir/emberday.yml" "$theme"; then
    backup "$theme"
fi
cp "$dir/emberday.yml" "$theme"
echo "  $theme"

echo "Done. zsh/emberday.zsh points LG_CONFIG_FILE at it; elsewhere export"
echo "  LG_CONFIG_FILE=\"$dest/config.yml,$theme\""
