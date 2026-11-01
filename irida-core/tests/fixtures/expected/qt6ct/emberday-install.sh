#!/usr/bin/env bash
# Install the Ember Day Qt colour scheme for qt5ct and qt6ct.
#
#   ./qt/install.sh
#
# Copies emberday.conf to ~/.config/qt5ct/colors/ and ~/.config/qt6ct/colors/
# (or under $XDG_CONFIG_HOME), for each tool that is installed, and selects it
# in qt5ct.conf / qt6ct.conf under [Appearance] with custom_palette=true and
# color_scheme_path. Backs up differing files to <file>.bak. Qt apps read it
# with QT_QPA_PLATFORMTHEME=qt5ct (or qt6ct) in the environment; restart them
# to see it. Safe to run again.
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

# Set key=value under [section] in an INI file (printed to stdout): replaces
# the key in that section, else adds it there, else adds the section at the end.
set_key() {
    awk -v s="[$2]" -v k="$3" -v v="$4" '
        function flush() { while (pend > 0) { print ""; pend-- } }
        $0 == s { flush(); insec = 1; print; next }
        /^\[/ { if (insec && !done) { print k "=" v; done = 1 }; flush(); insec = 0 }
        insec && $0 == "" { pend++; next }
        insec && index($0, k "=") == 1 { if (!done) { flush(); print k "=" v; done = 1 }; next }
        { flush(); print }
        END {
            if (!done) {
                if (!insec) print s
                print k "=" v
            }
            flush()
        }
    ' "$1"
}

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cfg="${XDG_CONFIG_HOME:-}"
case "$cfg" in /*) ;; *) cfg="$HOME/.config" ;; esac
# The config folder as the path is written: ~/ inside the home folder.
case "$cfg" in "$HOME"/?*) shown="~/${cfg#"$HOME"/}" ;; *) shown="$cfg" ;; esac

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
found=0
for tool in qt5ct qt6ct; do
    dest="$cfg/$tool"
    if ! command -v "$tool" >/dev/null 2>&1 && [ ! -d "$dest" ]; then
        continue
    fi
    found=1
    scheme="$dest/colors/emberday.conf"
    conf="$dest/$tool.conf"
    mkdir -p "$dest/colors"
    if [ -f "$scheme" ] && ! cmp -s "$dir/emberday.conf" "$scheme"; then
        backup "$scheme"
    fi
    cp "$dir/emberday.conf" "$scheme"
    echo "  $scheme"
    [ -f "$conf" ] || : > "$conf"
    set_key "$conf" Appearance color_scheme_path "$shown/$tool/colors/emberday.conf" > "$tmp"
    set_key "$tmp" Appearance custom_palette true > "$tmp.2" && mv "$tmp.2" "$tmp"
    if cmp -s "$tmp" "$conf"; then
        echo "  $tool.conf already selects it"
    else
        [ -s "$conf" ] && backup "$conf"
        cat "$tmp" > "$conf"
        echo "  $tool.conf: color_scheme_path, custom_palette=true"
    fi
done

if [ "$found" = 0 ]; then
    echo "  qt5ct and qt6ct not installed, skipped"
    exit 0
fi
echo "Done. Qt apps need QT_QPA_PLATFORMTHEME=qt6ct (or qt5ct); restart them to see it"
