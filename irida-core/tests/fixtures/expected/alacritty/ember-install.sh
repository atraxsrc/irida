#!/usr/bin/env bash
# Install the Ember Alacritty colours and import them from alacritty.toml.
#
#   ./alacritty/install.sh
#
# Copies ember.toml to ~/.config/alacritty/themes/ (or under
# $XDG_CONFIG_HOME) and, when
# alacritty.toml has no import line naming it yet, appends a [general]
# section with the import (Alacritty 0.14 or newer). An existing import
# array, or an existing [general] section, is left for you to edit by hand,
# since TOML allows only one of each. Backs up a differing theme file to
# ember.toml.bak. Safe to run again.
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
# The config folder as the include line names it: ~/ inside the home folder.
case "$cfg" in "$HOME"/?*) shown="~/${cfg#"$HOME"/}" ;; *) shown="$cfg" ;; esac
dest="$cfg/alacritty"
conf="$dest/alacritty.toml"
theme="$dest/themes/ember.toml"
import="$shown/alacritty/themes/ember.toml"

if ! command -v alacritty >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  alacritty not installed, skipped"
    exit 0
fi

mkdir -p "$dest/themes"
if [ -f "$theme" ] && ! cmp -s "$dir/ember.toml" "$theme"; then
    backup "$theme"
fi
cp "$dir/ember.toml" "$theme"
echo "  $theme"

if [ -f "$conf" ] && grep -qF "$import" "$conf"; then
    echo "  alacritty.toml already imports it"
elif [ -f "$conf" ] && grep -qE '^[[:space:]]*import[[:space:]]*=' "$conf"; then
    echo "  alacritty.toml has an import array; add \"$import\" to it by hand"
elif [ -f "$conf" ] && grep -qE '^[[:space:]]*\[general\]' "$conf"; then
    echo "  alacritty.toml has a [general] section; add import = [\"$import\"] to it by hand"
else
    if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
        echo >> "$conf"
    fi
    printf '\n[general]\nimport = ["%s"]\n' "$import" >> "$conf"
    echo "  alacritty.toml: import = [\"$import\"]"
fi

echo "Done. Alacritty reloads its config on its own"
