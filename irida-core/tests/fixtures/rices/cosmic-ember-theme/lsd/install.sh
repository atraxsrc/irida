#!/usr/bin/env bash
# Installs the Ember lsd config (column colours; file names come from LS_COLORS in zsh/ember.zsh)
set -e
dir="$(cd "$(dirname "$0")" && pwd)"
mkdir -p ~/.config/lsd
for f in config.yaml colors.yaml; do
    [ -f ~/.config/lsd/$f ] && cp ~/.config/lsd/$f ~/.config/lsd/$f.bak
done
cp "$dir/config.yaml" "$dir/colors.yaml" ~/.config/lsd/
echo "Done. Run: lsd -l"
