#!/usr/bin/env bash
# Install the Ember Day WezTerm colour scheme.
#
#   ./wezterm/install.sh
#
# Copies colors/emberday.toml to ~/.config/wezterm/colors/ (or under
# $XDG_CONFIG_HOME). wezterm.lua is Lua, so it is not edited: add
#   config.color_scheme = "emberday"
# to it once. Backs up a differing scheme file to emberday.toml.bak. Safe
# to run again.
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
dest="$cfg/wezterm"
scheme="$dest/colors/emberday.toml"

if ! command -v wezterm >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  wezterm not installed, skipped"
    exit 0
fi

mkdir -p "$dest/colors"
if [ -f "$scheme" ] && ! cmp -s "$dir/colors/emberday.toml" "$scheme"; then
    backup "$scheme"
fi
cp "$dir/colors/emberday.toml" "$scheme"
echo "  $scheme"

if [ -f "$dest/wezterm.lua" ] && grep -qE "color_scheme[[:space:]]*=[[:space:]]*[\"']emberday[\"']" "$dest/wezterm.lua"; then
    echo "  wezterm.lua already picks it"
else
    echo "  add to wezterm.lua once:  config.color_scheme = \"emberday\""
fi

echo "Done. WezTerm reloads its config on its own (ctrl+shift+r by hand)"
