#!/usr/bin/env bash
# Install the Ember Day Helix theme and select it in config.toml.
#
#   ./helix/install.sh
#
# Copies themes/emberday.toml to ~/.config/helix/themes/ and sets
# theme = "emberday" at the top of config.toml (replacing an existing
# top-level theme line, else inserting one before the first section). Backs
# up config.toml to config.toml.bak when it changes. Safe to run again.
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
dest="${XDG_CONFIG_HOME:-$HOME/.config}/helix"
conf="$dest/config.toml"
line='theme = "emberday"'

if ! command -v hx >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  helix not installed, skipped"
    exit 0
fi

mkdir -p "$dest/themes"
cp "$dir/themes/emberday.toml" "$dest/themes/emberday.toml"
echo "  themes/emberday.toml"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
if [ -f "$conf" ]; then
    # Only the top-level theme line, before the first [section], is Helix's.
    awk -v line="$line" '
        /^[[:space:]]*\[/ && !insection { insection = 1; if (!done) { print line; done = 1 } }
        !insection && /^[[:space:]]*theme[[:space:]]*=/ { if (!done) { print line; done = 1 }; next }
        { print }
        END { if (!done) print line }
    ' "$conf" > "$tmp"
    if cmp -s "$tmp" "$conf"; then
        echo "  config.toml already up to date"
    else
        backup "$conf"
        cat "$tmp" > "$conf"
        echo "  config.toml: $line"
    fi
else
    echo "$line" > "$conf"
    echo "  config.toml: $line"
fi

echo "Done. Run :config-reload in Helix"
