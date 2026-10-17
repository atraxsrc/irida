#!/usr/bin/env bash
# Install the Ember Ghostty theme and select it in ~/.config/ghostty/config.
#
#   ./ghostty/install.sh
#
# Copies themes/ember to ~/.config/ghostty/themes/ and sets
# "theme = ember" in the config (replacing an existing theme line, else
# appending one). Backs up the config to config.bak when it changes. Safe to
# run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${XDG_CONFIG_HOME:-$HOME/.config}/ghostty"
conf="$dest/config"

if ! command -v ghostty >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  ghostty not installed, skipped"
    exit 0
fi

mkdir -p "$dest/themes"
cp "$dir/themes/ember" "$dest/themes/ember"
echo "  themes/ember"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
if [ -f "$conf" ]; then
    if grep -qE '^[[:space:]]*theme[[:space:]]*=' "$conf"; then
        sed -E 's/^[[:space:]]*theme[[:space:]]*=.*$/theme = ember/' "$conf" > "$tmp"
    else
        cat "$conf" > "$tmp"
        if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
            echo >> "$tmp"
        fi
        echo "theme = ember" >> "$tmp"
    fi
    if cmp -s "$tmp" "$conf"; then
        echo "  config already up to date"
    else
        cp "$conf" "$conf.bak"
        echo "  backup: config.bak"
        cat "$tmp" > "$conf"
        echo "  config: theme = ember"
    fi
else
    echo "theme = ember" > "$conf"
    echo "  config: theme = ember"
fi

echo "Done. Reload Ghostty with ctrl+shift+, (1.2.0 or newer)"
