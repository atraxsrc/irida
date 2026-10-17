#!/usr/bin/env bash
# Install the Ember Day Starship palette and select it in starship.toml.
#
#   ./starship/install.sh
#
# Edits ~/.config/starship.toml (or $STARSHIP_CONFIG): when it has no
# palette line and no [palettes.emberday] table yet, puts
# palette = "emberday" on the first line and appends the table from
# emberday.toml. An existing palette line is left for you to change by
# hand. Backs up starship.toml to starship.toml.bak when it changes. Takes
# effect at the next prompt. Safe to run again.
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
conf="${STARSHIP_CONFIG:-$cfg/starship.toml}"

if ! command -v starship >/dev/null 2>&1; then
    echo "  starship not installed, skipped"
    exit 0
fi

if [ -f "$conf" ] && grep -qxF '[palettes.emberday]' "$conf"; then
    echo "  starship.toml already has the emberday palette"
elif [ -f "$conf" ] && grep -qE '^[[:space:]]*palette[[:space:]]*=' "$conf"; then
    echo "  starship.toml has a palette line; set it to \"emberday\" and add the table from emberday.toml by hand"
else
    mkdir -p "$(dirname "$conf")"
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    echo 'palette = "emberday"' > "$tmp"
    if [ -f "$conf" ]; then
        cat "$conf" >> "$tmp"
        backup "$conf"
        if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
            echo >> "$tmp"
        fi
    fi
    cat "$dir/emberday.toml" >> "$tmp"
    cat "$tmp" > "$conf"
    echo "  $conf: palette = \"emberday\""
fi

echo "Done. Takes effect at the next prompt"
