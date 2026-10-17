#!/usr/bin/env bash
# Install the Ember bat theme, rebuild bat's cache and select it.
#
#   ./bat/install.sh
#
# Copies ember.tmTheme to "$(bat --config-dir)/themes/", runs
# `bat cache --build`, and sets --theme=ember in "$(bat --config-file)"
# (replacing an existing --theme line, else appending one). Backs up the
# config to config.bak when it changes. BAT_THEME in your environment wins
# over the config file. Safe to run again.
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
bat="$(command -v bat || command -v batcat || true)"
if [ -z "$bat" ]; then
    echo "  bat not installed, skipped"
    exit 0
fi
themes="$("$bat" --config-dir)/themes"
conf="$("$bat" --config-file)"

mkdir -p "$themes"
cp "$dir/ember.tmTheme" "$themes/ember.tmTheme"
echo "  $themes/ember.tmTheme"
(cd "$HOME" && "$bat" cache --build >/dev/null)
echo "  bat cache rebuilt"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
line='--theme=ember'
if [ -f "$conf" ]; then
    if grep -qE '^[[:space:]]*--theme[[:space:]]*=' "$conf"; then
        sed -E "s/^[[:space:]]*--theme[[:space:]]*=.*$/$line/" "$conf" > "$tmp"
    else
        cat "$conf" > "$tmp"
        if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
            echo >> "$tmp"
        fi
        echo "$line" >> "$tmp"
    fi
    if cmp -s "$tmp" "$conf"; then
        echo "  config already up to date"
    else
        backup "$conf"
        cat "$tmp" > "$conf"
        echo "  $conf: $line"
    fi
else
    mkdir -p "$(dirname "$conf")"
    echo "$line" > "$conf"
    echo "  $conf: $line"
fi

echo "Done. Run: bat --list-themes | grep ember"
