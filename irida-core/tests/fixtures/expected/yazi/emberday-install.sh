#!/usr/bin/env bash
# Install the Ember Day yazi flavor and select it in theme.toml.
#
#   ./yazi/install.sh
#
# Copies emberday.yazi/ to ~/.config/yazi/flavors/ (or under
# $XDG_CONFIG_HOME) and, when theme.toml does not set flavor yet, adds a
# [flavor] section with dark and light both set to emberday (yazi 25.12.29
# or newer). An existing flavor setting is left for you to edit by hand.
# Backs up differing flavor files to <file>.bak and theme.toml to
# theme.toml.bak when it changes. Restart yazi to see it. Safe to run again.
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
dest="$cfg/yazi"
conf="$dest/theme.toml"
flavor="$dest/flavors/emberday.yazi"

if ! command -v yazi >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  yazi not installed, skipped"
    exit 0
fi

mkdir -p "$flavor"
for f in flavor.toml tmtheme.xml; do
    if [ -f "$flavor/$f" ] && ! cmp -s "$dir/emberday.yazi/$f" "$flavor/$f"; then
        backup "$flavor/$f"
    fi
    cp "$dir/emberday.yazi/$f" "$flavor/$f"
done
echo "  $flavor"

if [ -f "$conf" ] && grep -qxF 'dark = "emberday"' "$conf" && grep -qxF 'light = "emberday"' "$conf"; then
    echo "  theme.toml already selects it"
elif [ -f "$conf" ] && grep -qE '^[[:space:]]*(\[flavor\]|flavor[[:space:]]*[=.])' "$conf"; then
    echo "  theme.toml already sets flavor; set dark and light to \"emberday\" by hand"
else
    if [ -f "$conf" ]; then
        backup "$conf"
    fi
    if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
        echo >> "$conf"
    fi
    printf '[flavor]\ndark = "%s"\nlight = "%s"\n' "emberday" "emberday" >> "$conf"
    echo "  theme.toml: [flavor] dark and light = \"emberday\""
fi

echo "Done. Restart yazi to see it"
