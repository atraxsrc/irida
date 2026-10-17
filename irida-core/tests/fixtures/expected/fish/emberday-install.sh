#!/usr/bin/env bash
# Install the Ember Day fish colours as ~/.config/fish/conf.d/emberday.fish.
#
#   ./fish/install.sh
#
# Backs up a differing file to emberday.fish.bak. Safe to run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d"

if ! command -v fish >/dev/null 2>&1 && [ ! -d "${XDG_CONFIG_HOME:-$HOME/.config}/fish" ]; then
    echo "  fish not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$dest/emberday.fish" ] && ! cmp -s "$dir/emberday.fish" "$dest/emberday.fish"; then
    cp "$dest/emberday.fish" "$dest/emberday.fish.bak"
    echo "  backup: emberday.fish.bak"
fi
cp "$dir/emberday.fish" "$dest/emberday.fish"
echo "  $dest/emberday.fish"

echo "Done. Open a new fish shell"
