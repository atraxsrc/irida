#!/usr/bin/env bash
# Install the Ember Day Ghostty theme and select it in ~/.config/ghostty/config.
#
#   ./ghostty/install.sh
#
# Copies themes/emberday to ~/.config/ghostty/themes/ and sets
# "theme = emberday" in the config (replacing an existing theme line, else
# appending one). Backs up the config to config.bak when it changes. Safe to
# run again.
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
dest="${XDG_CONFIG_HOME:-$HOME/.config}/ghostty"
conf="$dest/config"

if ! command -v ghostty >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  ghostty not installed, skipped"
    exit 0
fi

mkdir -p "$dest/themes"
cp "$dir/themes/emberday" "$dest/themes/emberday"
echo "  themes/emberday"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
if [ -f "$conf" ]; then
    if grep -qE '^[[:space:]]*theme[[:space:]]*=' "$conf"; then
        sed -E 's/^[[:space:]]*theme[[:space:]]*=.*$/theme = emberday/' "$conf" > "$tmp"
    else
        cat "$conf" > "$tmp"
        if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
            echo >> "$tmp"
        fi
        echo "theme = emberday" >> "$tmp"
    fi
    if cmp -s "$tmp" "$conf"; then
        echo "  config already up to date"
    else
        backup "$conf"
        cat "$tmp" > "$conf"
        echo "  config: theme = emberday"
    fi
else
    echo "theme = emberday" > "$conf"
    echo "  config: theme = emberday"
fi

echo "Done. Reload Ghostty with ctrl+shift+, (1.2.0 or newer)"
