#!/usr/bin/env bash
# Install the Ember delta theme and select it in the global git config.
#
#   ./delta/install.sh
#
# Copies ember.gitconfig to ~/.config/delta/, adds it to include.path
# (once) and sets delta.features to ember, printing the previous value
# so it can be put back by hand. Safe to run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${XDG_CONFIG_HOME:-$HOME/.config}/delta"
theme="$dest/ember.gitconfig"

if ! command -v delta >/dev/null 2>&1; then
    echo "  delta not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
cp "$dir/ember.gitconfig" "$theme"
echo "  $theme"

if ! git config --global --get-all include.path 2>/dev/null | grep -qxF "$theme"; then
    git config --global --add include.path "$theme"
    echo "  include.path += $theme"
fi
before="$(git config --global --get-all delta.features 2>/dev/null | paste -sd ' ' - || true)"
if [ -n "$before" ] && [ "$before" != "ember" ]; then
    echo "  delta.features was: $before"
fi
git config --global --replace-all delta.features "ember"
echo "  delta.features = ember"

echo "Done. Takes effect on the next git command"
