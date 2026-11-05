# WraithOS interactive bash - identity + lockdown.
if [ -z "$WRAITH_BASHRC_LOADED" ]; then
    WRAITH_BASHRC_LOADED=1
    BASH_ENV=
    ENV=
    PATH=/usr/sbin:/usr/bin:/sbin:/bin
    HISTFILE=/dev/null
    HISTSIZE=0
    HISTFILESIZE=0
    INPUTRC=/etc/inputrc
    TMOUT=600
    COLORTERM=truecolor
    export PATH BASH_ENV ENV HISTFILE HISTSIZE HISTFILESIZE INPUTRC TMOUT COLORTERM
    unset PROMPT_COMMAND
    umask 0077
    ulimit -c 0
    set -o noclobber
    readonly BASH_ENV ENV PATH HISTFILE HISTSIZE HISTFILESIZE \
        INPUTRC TMOUT COLORTERM 2>/dev/null || true
fi

SLOT=$(sed -n 's/.*slot=\([AB]\).*/\1/p' /proc/cmdline | head -1)
[ -n "$SLOT" ] || SLOT=?

# Exact identity palette. No gradient, no generic purple.
W_SURF=$'\033[48;2;24;20;40m'
W_VIO=$'\033[38;2;157;78;221m'
W_DEEP=$'\033[38;2;138;43;226m'
W_COBALT=$'\033[38;2;58;134;255m'
W_TEXT=$'\033[38;2;248;249;250m'
W_RESET=$'\033[0m'
PS1="\[${W_SURF}\]\[${W_VIO}\]\u@\h"
PS1="${PS1}\[${W_COBALT}\][$SLOT]\[${W_TEXT}\] \w"
PS1="${PS1}\[${W_DEEP}\] > \[${W_RESET}\] "

alias ls='ls --color=auto'
alias ll='ls -lah'
alias la='ls -A'
alias grep='grep --color=auto'

[ -f /etc/inputrc ] && bind -f /etc/inputrc 2>/dev/null || true
bind 'set enable-bracketed-paste on' 2>/dev/null || true

A1='\033[38;2;157;78;221m'
A2='\033[38;2;58;134;255m'
A3='\033[38;2;248;249;250m'
AR='\033[0m'
if [ -n "$PS1" ] && [ -z "$WRAITHOS_SHOWN" ]; then
    echo ""
    printf '%b\n' "  ${A1}WraithOS${AR}  |  slot ${A2}${SLOT}${AR}"
    printf '%b\n' "  ${A3}type wraith-help${AR}"
    echo ""
    export WRAITHOS_SHOWN=1
fi
