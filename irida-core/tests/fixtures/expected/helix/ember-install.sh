#!/usr/bin/env bash
# Install the Ember Helix theme and select it in config.toml.
#
#   ./helix/install.sh
#
# Copies themes/ember.toml to ~/.config/helix/themes/ and sets
# theme = "ember" at the top of config.toml (replacing an existing
# top-level theme line, else inserting one before the first section). Backs
# up config.toml to config.toml.bak when it changes. Safe to run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${XDG_CONFIG_HOME:-$HOME/.config}/helix"
conf="$dest/config.toml"
line='theme = "ember"'

if ! command -v hx >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  helix not installed, skipped"
    exit 0
fi

mkdir -p "$dest/themes"
cp "$dir/themes/ember.toml" "$dest/themes/ember.toml"
echo "  themes/ember.toml"

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
        cp "$conf" "$conf.bak"
        echo "  backup: config.toml.bak"
        /bin/cat "$tmp" > "$conf"
        echo "  config.toml: $line"
    fi
else
    echo "$line" > "$conf"
    echo "  config.toml: $line"
fi

echo "Done. Run :config-reload in Helix"
