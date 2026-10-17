#!/usr/bin/env bash
# Install the Ember kitty colours and include them from kitty.conf.
#
#   ./kitty/install.sh
#
# Copies ember.conf to ~/.config/kitty/ and appends
# "include ember.conf" to kitty.conf when it is not there yet (a later
# line wins in kitty, so the theme overrides earlier colours). Backs up a
# differing theme file to ember.conf.bak. Safe to run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${KITTY_CONFIG_DIRECTORY:-${XDG_CONFIG_HOME:-$HOME/.config}/kitty}"
conf="$dest/kitty.conf"
line="include ember.conf"

if ! command -v kitty >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  kitty not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$dest/ember.conf" ] && ! cmp -s "$dir/ember.conf" "$dest/ember.conf"; then
    cp "$dest/ember.conf" "$dest/ember.conf.bak"
    echo "  backup: ember.conf.bak"
fi
cp "$dir/ember.conf" "$dest/ember.conf"
echo "  $dest/ember.conf"

if [ -f "$conf" ] && grep -qxF "$line" "$conf"; then
    echo "  kitty.conf already includes it"
else
    if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
        echo >> "$conf"
    fi
    echo "$line" >> "$conf"
    echo "  kitty.conf: $line"
fi

echo "Done. Press ctrl+shift+f5 in kitty to reload its config"
