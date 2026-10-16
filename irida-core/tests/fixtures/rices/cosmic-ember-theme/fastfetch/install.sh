#!/usr/bin/env bash
# Installs the Ember COSMIC fastfetch config
set -e
dir="$(cd "$(dirname "$0")" && pwd)"
mkdir -p ~/.config/fastfetch
[ -f ~/.config/fastfetch/config.jsonc ] && cp ~/.config/fastfetch/config.jsonc ~/.config/fastfetch/config.jsonc.bak
cp "$dir/config.jsonc" "$dir/cosmic.txt" ~/.config/fastfetch/
echo "Done. Run: fastfetch"
