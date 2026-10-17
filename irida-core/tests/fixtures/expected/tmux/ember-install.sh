#!/usr/bin/env bash
# Install the Ember tmux colours and source them from tmux.conf.
#
#   ./tmux/install.sh
#
# Copies ember.conf to ~/.config/tmux/ (or $XDG_CONFIG_HOME/tmux/) and
# adds "source-file ~/.config/tmux/ember.conf" to the end of
# ~/.config/tmux/tmux.conf unless it is there already. tmux 3.1 or newer
# reads that file after ~/.tmux.conf, so these colours win. Backs up a
# differing ember.conf to ember.conf.bak and tmux.conf to
# tmux.conf.bak when it changes. Safe to run again.
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
# The config folder as the source-file line names it: ~/ inside the home folder.
case "$cfg" in "$HOME"/?*) shown="~/${cfg#"$HOME"/}" ;; *) shown="$cfg" ;; esac
dest="$cfg/tmux"
conf="$dest/tmux.conf"
line="source-file $shown/tmux/ember.conf"

if ! command -v tmux >/dev/null 2>&1; then
    echo "  tmux not installed, skipped"
    exit 0
fi

mkdir -p "$dest"
if [ -f "$dest/ember.conf" ] && ! cmp -s "$dir/ember.conf" "$dest/ember.conf"; then
    backup "$dest/ember.conf"
fi
cp "$dir/ember.conf" "$dest/ember.conf"
echo "  $dest/ember.conf"

if [ -f "$conf" ] && grep -qxF "$line" "$conf"; then
    echo "  tmux.conf already sources it"
else
    if [ -f "$conf" ]; then
        backup "$conf"
        if [ -s "$conf" ] && [ -n "$(tail -c 1 "$conf")" ]; then
            echo >> "$conf"
        fi
    fi
    echo "$line" >> "$conf"
    echo "  tmux.conf: $line"
fi

if tmux has-session >/dev/null 2>&1; then
    tmux source-file "$conf"
    echo "Done. Reloaded the running tmux server"
else
    echo "Done. The next tmux server picks it up"
fi
