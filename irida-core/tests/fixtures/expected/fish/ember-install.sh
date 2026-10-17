#!/usr/bin/env bash
# Install the Ember fish colours as ~/.config/fish/conf.d/ember.fish.
#
#   ./fish/install.sh
#
# Backs up a differing file to ember.fish.bak. Safe to run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d"

if ! command -v fish >/dev/null 2>&1 && [ ! -d "${XDG_CONFIG_HOME:-$HOME/.config}/fish" ]; then
    echo "  fish not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$dest/ember.fish" ] && ! cmp -s "$dir/ember.fish" "$dest/ember.fish"; then
    cp "$dest/ember.fish" "$dest/ember.fish.bak"
    echo "  backup: ember.fish.bak"
fi
cp "$dir/ember.fish" "$dest/ember.fish"
echo "  $dest/ember.fish"

echo "Done. Open a new fish shell"
