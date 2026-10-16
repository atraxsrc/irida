# Ember zsh layer: colours, aliases, prompt.
#
# Source it at the END of your own ~/.zshrc, after oh-my-zsh.sh:
#   source /path/to/cosmic-ember-theme/zsh/ember.zsh
#
# Keep machine-specific or private lines (PATH, scaling, nvm, ...) in your
# own ~/.zshrc, not here.
#
#  pane:    #1B1E24   fg:      #D6D9E0   (terminal pane; window bg #15171C)
#  ember:   #E8713A   moss:    #8FB58A
#  amber:   #D9A441   brick:   #D85A5A
#  slate:   #7FA3C7   heather: #B08BC4   verdigris: #7FB7B3
#  muted:   #7C8290   comment: #5A6070

# ── File colours (ls, lsd file names, completion menu) ─────────────────────────

export LS_COLORS="di=1;38;2;127;163;199:ln=3;38;2;127;183;179:ex=1;38;2;143;181;138:*.zip=4;38;2;217;164;65:*.tar=4;38;2;217;164;65:*.gz=4;38;2;217;164;65:*.xz=4;38;2;217;164;65:*.7z=4;38;2;217;164;65:*.deb=4;38;2;217;164;65:*.png=38;2;176;139;196:*.jpg=38;2;176;139;196:*.jpeg=38;2;176;139;196:*.webp=38;2;176;139;196:*.gif=38;2;176;139;196:*.mp4=3;38;2;176;139;196:*.mkv=3;38;2;176;139;196:*.rs=38;2;143;181;138:*.py=38;2;143;181;138:*.js=38;2;143;181;138:*.ts=38;2;143;181;138:*.sh=38;2;143;181;138:*.md=38;2;214;217;224:*.txt=38;2;214;217;224:or=9;38;2;216;90;90"

# oh-my-zsh reads LS_COLORS before this file loads, so refresh the menu colours
zstyle ':completion:*' list-colors ${(s.:.)LS_COLORS}

# ── zsh-autosuggestions / zsh-syntax-highlighting ──────────────────────────────

ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE='fg=#474C59'

typeset -gA ZSH_HIGHLIGHT_STYLES
ZSH_HIGHLIGHT_STYLES[command]='fg=#E8713A'
ZSH_HIGHLIGHT_STYLES[builtin]='fg=#E8713A'
ZSH_HIGHLIGHT_STYLES[alias]='fg=#E8713A'
ZSH_HIGHLIGHT_STYLES[function]='fg=#E8713A'
ZSH_HIGHLIGHT_STYLES[hashed-command]='fg=#E8713A'
ZSH_HIGHLIGHT_STYLES[suffix-alias]='fg=#E8713A,underline'
ZSH_HIGHLIGHT_STYLES[precommand]='fg=#8FB58A,underline'
ZSH_HIGHLIGHT_STYLES[reserved-word]='fg=#D6D9E0,bold'
ZSH_HIGHLIGHT_STYLES[unknown-token]='fg=#D85A5A,bold,underline'
ZSH_HIGHLIGHT_STYLES[path]='fg=#D6D9E0,underline'
ZSH_HIGHLIGHT_STYLES[globbing]='fg=#D9A441'
ZSH_HIGHLIGHT_STYLES[history-expansion]='fg=#D9A441'
ZSH_HIGHLIGHT_STYLES[single-hyphen-option]='fg=#7C8290'
ZSH_HIGHLIGHT_STYLES[double-hyphen-option]='fg=#7C8290'
ZSH_HIGHLIGHT_STYLES[single-quoted-argument]='fg=#8FB58A'
ZSH_HIGHLIGHT_STYLES[double-quoted-argument]='fg=#8FB58A'
ZSH_HIGHLIGHT_STYLES[dollar-quoted-argument]='fg=#8FB58A'
ZSH_HIGHLIGHT_STYLES[commandseparator]='fg=#7C8290'
ZSH_HIGHLIGHT_STYLES[redirection]='fg=#7C8290'
ZSH_HIGHLIGHT_STYLES[comment]='fg=#5A6070'

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
