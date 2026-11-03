# WraithOS bash lockdown — no history, no BASH_ENV, fixed PATH
HISTFILE=/dev/null
HISTSIZE=0
HISTFILESIZE=0
BASH_ENV=
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH HISTFILE HISTSIZE HISTFILESIZE BASH_ENV
umask 0077
set -o noclobber

# PS1 (gray)
PS1='\[\e[0;37m\]\u@\h\[\e[0m\]:\[\e[1;37m\]\w\[\e[0m\]\$ '

alias ls='ls --color=auto'
alias ll='ls -lah'
alias la='ls -A'
alias grep='grep --color=auto'

# Bracketed paste (Termius/Termux multi-line safe)
bind 'set enable-bracketed-paste on' 2>/dev/null || true

if [ -n "$PS1" ] && [ -z "$WRAITHOS_SHOWN" ]; then
    echo ""
    echo "  WraithOS 0.23.1  |  type 'wraith-help'"
    echo ""
    export WRAITHOS_SHOWN=1
fi
