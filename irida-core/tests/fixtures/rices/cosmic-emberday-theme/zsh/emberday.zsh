# Ember Day zsh layer: colours, aliases, prompt.
#
# Source it at the END of your own ~/.zshrc, after oh-my-zsh.sh:
#   source /path/to/cosmic-emberday-theme/zsh/emberday.zsh
#
# Keep machine-specific or private lines (PATH, scaling, nvm, ...) in your
# own ~/.zshrc, not here.
#
#  pane:    #DDD9D1   fg:      #2B2E36   (terminal pane; window bg #E9E6DF)
#  ember:   #A4441A   moss:    #4C6B45
#  amber:   #8A5E12   brick:   #9E3A3A
#  slate:   #2F5C8A   heather: #6E4E8C   verdigris: #2E6E6A
#  muted:   #6B6F78   comment: #8C9099

# ── File colours (ls, lsd file names, completion menu) ─────────────────────────

export LS_COLORS="di=1;38;2;47;92;138:ln=3;38;2;46;110;106:ex=1;38;2;76;107;69:*.zip=4;38;2;138;94;18:*.tar=4;38;2;138;94;18:*.gz=4;38;2;138;94;18:*.xz=4;38;2;138;94;18:*.7z=4;38;2;138;94;18:*.deb=4;38;2;138;94;18:*.png=38;2;110;78;140:*.jpg=38;2;110;78;140:*.jpeg=38;2;110;78;140:*.webp=38;2;110;78;140:*.gif=38;2;110;78;140:*.mp4=3;38;2;110;78;140:*.mkv=3;38;2;110;78;140:*.rs=38;2;76;107;69:*.py=38;2;76;107;69:*.js=38;2;76;107;69:*.ts=38;2;76;107;69:*.sh=38;2;76;107;69:*.md=38;2;43;46;54:*.txt=38;2;43;46;54:or=9;38;2;158;58;58"

# oh-my-zsh reads LS_COLORS before this file loads, so refresh the menu colours
zstyle ':completion:*' list-colors ${(s.:.)LS_COLORS}

# ── zsh-autosuggestions / zsh-syntax-highlighting ──────────────────────────────

ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE='fg=#A4A6AA'

typeset -gA ZSH_HIGHLIGHT_STYLES
ZSH_HIGHLIGHT_STYLES[command]='fg=#A4441A'
ZSH_HIGHLIGHT_STYLES[builtin]='fg=#A4441A'
ZSH_HIGHLIGHT_STYLES[alias]='fg=#A4441A'
ZSH_HIGHLIGHT_STYLES[function]='fg=#A4441A'
ZSH_HIGHLIGHT_STYLES[hashed-command]='fg=#A4441A'
ZSH_HIGHLIGHT_STYLES[suffix-alias]='fg=#A4441A,underline'
ZSH_HIGHLIGHT_STYLES[precommand]='fg=#4C6B45,underline'
ZSH_HIGHLIGHT_STYLES[reserved-word]='fg=#2B2E36,bold'
ZSH_HIGHLIGHT_STYLES[unknown-token]='fg=#9E3A3A,bold,underline'
ZSH_HIGHLIGHT_STYLES[path]='fg=#2B2E36,underline'
ZSH_HIGHLIGHT_STYLES[globbing]='fg=#8A5E12'
ZSH_HIGHLIGHT_STYLES[history-expansion]='fg=#8A5E12'
ZSH_HIGHLIGHT_STYLES[single-hyphen-option]='fg=#6B6F78'
ZSH_HIGHLIGHT_STYLES[double-hyphen-option]='fg=#6B6F78'
ZSH_HIGHLIGHT_STYLES[single-quoted-argument]='fg=#4C6B45'
ZSH_HIGHLIGHT_STYLES[double-quoted-argument]='fg=#4C6B45'
ZSH_HIGHLIGHT_STYLES[dollar-quoted-argument]='fg=#4C6B45'
ZSH_HIGHLIGHT_STYLES[commandseparator]='fg=#6B6F78'
ZSH_HIGHLIGHT_STYLES[redirection]='fg=#6B6F78'
ZSH_HIGHLIGHT_STYLES[comment]='fg=#8C9099'

# ── Aliases (only when the tool is installed) ──────────────────────────────────

if (( $+commands[lsd] )); then
    alias ls='lsd'
    alias la='lsd -a'
    alias ll='lsd -l'
    alias lla='lsd -la'
fi

# Debian/Ubuntu ship bat as batcat
if (( $+commands[batcat] )); then
    alias cat='batcat'
elif (( $+commands[bat] )); then
    alias cat='bat'
fi

(( $+commands[cosmic-edit] )) && alias edit='cosmic-edit'

if (( $+commands[nvim] )); then
    alias vi='nvim'
    alias vim='nvim'
    alias v='nvim'
fi

# ── Prompt ─────────────────────────────────────────────────────────────────────

# needs ZSH_THEME="" in ~/.zshrc
(( $+commands[starship] )) && eval "$(starship init zsh)"
