#!/usr/bin/env bash
# Install the Ember Starship palette and select it in starship.toml.
#
#   ./starship/install.sh
#
# Edits ~/.config/starship.toml (or $STARSHIP_CONFIG): when it has no
# palette line and no [palettes.ember] table yet, puts
# palette = "ember" on the first line and appends the table from
# ember.toml. An existing palette line is left for you to change by
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

if [ -f "$conf" ] && grep -qxF '[palettes.ember]' "$conf"; then
    echo "  starship.toml already has the ember palette"
elif [ -f "$conf" ] && grep -qE '^[[:space:]]*palette[[:space:]]*=' "$conf"; then
    echo "  starship.toml has a palette line; set it to \"ember\" and add the table from ember.toml by hand"
else
    mkdir -p "$(dirname "$conf")"
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    echo 'palette = "ember"' > "$tmp"
    if [ -f "$conf" ]; then
        cat "$conf" >> "$tmp"
        backup "$conf"
        if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
            echo >> "$tmp"
        fi
    fi
    cat "$dir/ember.toml" >> "$tmp"
    cat "$tmp" > "$conf"
    echo "  $conf: palette = \"ember\""
fi

echo "Done. Takes effect at the next prompt"
