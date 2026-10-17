#!/usr/bin/env bash
# Install the Ember Day delta theme and select it in the global git config.
#
#   ./delta/install.sh
#
# Copies emberday.gitconfig to ~/.config/delta/, adds it to include.path
# (once) and sets delta.features to emberday. Safe to run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${XDG_CONFIG_HOME:-$HOME/.config}/delta"
theme="$dest/emberday.gitconfig"

if ! command -v delta >/dev/null 2>&1; then
    echo "  delta not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
cp "$dir/emberday.gitconfig" "$theme"
echo "  $theme"

if ! git config --global --get-all include.path 2>/dev/null | grep -qxF "$theme"; then
    git config --global --add include.path "$theme"
    echo "  include.path += $theme"
fi
git config --global delta.features "emberday"
echo "  delta.features = emberday"

echo "Done. Takes effect on the next git command"
