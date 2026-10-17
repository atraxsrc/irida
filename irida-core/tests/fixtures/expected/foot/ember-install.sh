#!/usr/bin/env bash
# Install the Ember foot colours and include them from foot.ini.
#
#   ./foot/install.sh
#
# Copies ember.ini to ~/.config/foot/ (or $XDG_CONFIG_HOME/foot/) and puts
# "include=~/.config/foot/ember.ini" at the top of foot.ini (include
# is a [main] option, so it must come before any section) unless it is there
# already. Needs foot 1.26 or newer. Backs up a differing theme file to
# ember.ini.bak and foot.ini to foot.ini.bak when it changes. Safe to
# run again.
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
dest="$cfg/foot"
conf="$dest/foot.ini"
line="include=$shown/foot/ember.ini"

if ! command -v foot >/dev/null 2>&1 && [ ! -d "$dest" ]; then
    echo "  foot not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$dest/ember.ini" ] && ! cmp -s "$dir/ember.ini" "$dest/ember.ini"; then
    backup "$dest/ember.ini"
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
        cat "$conf" >> "$tmp"
        backup "$conf"
    fi
    cat "$tmp" > "$conf"
    echo "  foot.ini: $line"
fi

echo "Done. Open a new foot window"
