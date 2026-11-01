#!/usr/bin/env bash
# Install the Ember Day Discord theme for Vencord and Vesktop.
#
#   ./discord/install.sh
#
# Copies emberday.theme.css into the themes/ folder of every Vencord or
# Vesktop it finds: ~/.config/Vencord and ~/.config/vesktop (or under
# $XDG_CONFIG_HOME) and the Flatpak ones under ~/.var/app. Backs up a
# differing copy to <file>.bak. Then tick it under Settings -> Vencord ->
# Themes (Load missing themes when it is not listed). Safe to run again.
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
file=emberday.theme.css

# label | config folder that holds themes/ | folder whose presence means the client is installed
candidates=(
    "Vencord|$cfg/Vencord|$cfg/Vencord"
    "Vesktop|$cfg/vesktop|$cfg/vesktop"
    "Vencord in Flatpak Discord|$HOME/.var/app/com.discordapp.Discord/config/Vencord|$HOME/.var/app/com.discordapp.Discord"
    "Vencord in Flatpak Discord Canary|$HOME/.var/app/com.discordapp.DiscordCanary/config/Vencord|$HOME/.var/app/com.discordapp.DiscordCanary"
    "Flatpak Vesktop|$HOME/.var/app/dev.vencord.Vesktop/config/vesktop|$HOME/.var/app/dev.vencord.Vesktop"
)
found=0
for c in "${candidates[@]}"; do
    IFS='|' read -r label conf marker <<<"$c"
    [ -d "$marker" ] || continue
    found=1
    dest="$conf/themes"
    mkdir -p "$dest"
    if [ -f "$dest/$file" ] && ! cmp -s "$dir/$file" "$dest/$file"; then
        backup "$dest/$file"
    fi
    cp "$dir/$file" "$dest/$file"
    echo "  $label: $dest/$file"
done

if [ "$found" = 0 ]; then
    echo "  Vencord and Vesktop not installed, skipped"
    exit 0
fi
echo "Done. Settings -> Vencord -> Themes -> tick Ember Day (Load missing themes when it is not listed), then ctrl+r"
