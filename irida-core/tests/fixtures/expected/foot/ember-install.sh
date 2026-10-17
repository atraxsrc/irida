#!/usr/bin/env bash
# Install the Ember foot colours and include them from foot.ini.
#
#   ./foot/install.sh
#
# Copies ember.ini to ~/.config/foot/ and puts
# "include=~/.config/foot/ember.ini" at the top of foot.ini (include
# is a [main] option, so it must come before any section) unless it is there
# already. Needs foot 1.26 or newer. Backs up a differing theme file to
# ember.ini.bak and foot.ini to foot.ini.bak when it changes. Safe to
# run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${XDG_CONFIG_HOME:-$HOME/.config}/foot"
conf="$dest/foot.ini"
line="include=~/.config/foot/ember.ini"

if ! command -v foot >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  foot not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$dest/ember.ini" ] && ! cmp -s "$dir/ember.ini" "$dest/ember.ini"; then
    cp "$dest/ember.ini" "$dest/ember.ini.bak"
    echo "  backup: ember.ini.bak"
fi
cp "$dir/ember.ini" "$dest/ember.ini"
echo "  $dest/ember.ini"

if [ -f "$conf" ] && grep -qxF "$line" "$conf"; then
    echo "  foot.ini already includes it"
else
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    echo "$line" > "$tmp"
    if [ -f "$conf" ]; then
        /bin/cat "$conf" >> "$tmp"
        cp "$conf" "$conf.bak"
        echo "  backup: foot.ini.bak"
    fi
    /bin/cat "$tmp" > "$conf"
    echo "  foot.ini: $line"
fi

echo "Done. Open a new foot window"
