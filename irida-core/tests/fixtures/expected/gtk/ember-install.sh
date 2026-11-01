#!/usr/bin/env bash
# Install the Ember GTK stylesheet for GTK 3 and GTK 4 apps.
#
#   ./gtk/install.sh
#
# Copies ember.css into ~/.config/gtk-3.0/ and ~/.config/gtk-4.0/ (or
# under $XDG_CONFIG_HOME) and imports it from gtk.css there, after COSMIC's
# own import so these colours win. Another rice's import line is commented
# out, never deleted. Backs up differing files to <file>.bak. Restart GTK
# apps to see it. Safe to run again.
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
line='@import url("ember.css");'

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
for v in gtk-3.0 gtk-4.0; do
    dest="$cfg/$v"
    conf="$dest/gtk.css"
    mkdir -p "$dest"
    if [ -f "$dest/ember.css" ] && ! cmp -s "$dir/ember.css" "$dest/ember.css"; then
        backup "$dest/ember.css"
    fi
    cp "$dir/ember.css" "$dest/ember.css"
    if [ ! -f "$conf" ]; then
        echo "$line" > "$conf"
        echo "  $v: gtk.css created, imports ember.css"
        continue
    fi
    # Other rices' <slug>.css imports off, ours back on when a rice turned it off,
    # and added after the last @import (COSMIC puts its own first) when missing.
    awk -v line="$line" '
        $0 == "/* " line " */" { print line; next }
        /^@import url\("[a-z]+\.css"\);$/ && $0 !~ /"(cosmic|ember)\.css"/ { print "/* " $0 " */"; next }
        { print }
    ' "$conf" > "$tmp"
    if ! grep -qxF "$line" "$tmp"; then
        awk -v line="$line" '
            { buf[NR] = $0; if ($0 ~ /^@import/) last = NR }
            END {
                if (!last) print line
                for (i = 1; i <= NR; i++) { print buf[i]; if (i == last) print line }
            }
        ' "$tmp" > "$tmp.2" && mv "$tmp.2" "$tmp"
    fi
    if cmp -s "$tmp" "$conf"; then
        echo "  $v: gtk.css already imports ember.css"
    else
        backup "$conf"
        cat "$tmp" > "$conf"
        echo "  $v: gtk.css imports ember.css"
    fi
done

echo "Done. Restart GTK apps to see it"
