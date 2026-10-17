#!/usr/bin/env bash
# Install the Ember Day zellij theme and select it in config.kdl.
#
#   ./zellij/install.sh
#
# Copies emberday.kdl to ~/.config/zellij/themes/ (or under
# $XDG_CONFIG_HOME) and, when config.kdl has no theme line yet, adds
# theme "emberday". An existing theme line is left for you to change by
# hand. zellij 0.45 or newer picks it up live; restart older sessions.
# Backs up a differing theme file to emberday.kdl.bak. Safe to run again.
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
dest="$cfg/zellij"
conf="$dest/config.kdl"
theme="$dest/themes/emberday.kdl"
line='theme "emberday"'

if ! command -v zellij >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  zellij not installed, skipped"
    exit 0
fi

mkdir -p "$dest/themes"
if [ -f "$theme" ] && ! cmp -s "$dir/emberday.kdl" "$theme"; then
    backup "$theme"
fi
cp "$dir/emberday.kdl" "$theme"
echo "  $theme"

if [ -f "$conf" ] && grep -qxF "$line" "$conf"; then
    echo "  config.kdl already selects it"
elif [ -f "$conf" ] && grep -qE '^[[:space:]]*theme[[:space:]]' "$conf"; then
    echo "  config.kdl has a theme line; change it to $line by hand"
else
    if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
        echo >> "$conf"
    fi
    echo "$line" >> "$conf"
    echo "  config.kdl: $line"
fi

echo "Done. zellij 0.45 or newer picks it up live; restart older sessions"
