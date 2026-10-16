#!/usr/bin/env bash
# Hook zsh/ember.zsh into ~/.zshrc and fetch the two oh-my-zsh plugins it colours.
#
#   ./zsh/install.sh
#
# Needs zsh and oh-my-zsh already installed. Backs up ~/.zshrc to ~/.zshrc.bak
# before changing it. Safe to run again.
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
zsh_dir="${ZSH:-$HOME/.oh-my-zsh}"
custom="${ZSH_CUSTOM:-$zsh_dir/custom}"
line="source \"$dir/ember.zsh\""

if [ ! -d "$zsh_dir" ]; then
    echo "oh-my-zsh not found in $zsh_dir. Install it first: https://ohmyz.sh/#install"
    exit 1
fi

for plugin in zsh-autosuggestions zsh-syntax-highlighting; do
    if [ ! -d "$custom/plugins/$plugin" ]; then
        git clone --depth 1 "https://github.com/zsh-users/$plugin" "$custom/plugins/$plugin"
    fi
done

touch ~/.zshrc
if grep -qF "$line" ~/.zshrc; then
    echo "~/.zshrc already sources ember.zsh"
else
    cp ~/.zshrc ~/.zshrc.bak
    printf '\n# Ember rice\n%s\n' "$line" >> ~/.zshrc
    echo "Added to ~/.zshrc (backup: ~/.zshrc.bak)"
fi

cat <<'EOF'

Check your ~/.zshrc has:
  ZSH_THEME=""
  plugins=(git sudo zsh-autosuggestions zsh-syntax-highlighting)
and drop any old LS_COLORS, lsd/bat aliases or `starship init` lines it still has.

Then: exec zsh
EOF
