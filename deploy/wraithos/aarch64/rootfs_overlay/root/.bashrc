# WraithOS — shell lockdown (defense-in-depth)

# Session timeout + input config
INPUTRC=/etc/inputrc
TMOUT=600
export INPUTRC TMOUT
readonly INPUTRC TMOUT

# No history persistence (leak protection)
export HISTFILE=/dev/null
export HISTSIZE=0
export HISTFILESIZE=0
set +H 2>/dev/null

# No overwrite via > redirect; use >| to force
set -o noclobber

# Restricted PATH, readonly
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
readonly PATH

# Umask + no core dumps
umask 022
ulimit -c 0

# Prompt with slot indicator (matches x86)
__slot() { sed -n 's/.*slot=\([AB]\).*/\1/p' /proc/cmdline 2>/dev/null | head -1; }
__s=$(__slot)
PS1='\[\e[0;37m\]\u@\h\[\e[0m\][\[\e[1;33m\]${__s:-?}\[\e[0m\]]:\[\e[1;37m\]\w\[\e[0m\]\$ '

# Safe aliases
alias rm='rm -i'
alias cp='cp -i'
alias mv='mv -i'
alias ls='ls --color=auto'
alias ll='ls -lah'
alias la='ls -A'
alias grep='grep --color=auto'

# Input config + bracketed paste
[ -f /etc/inputrc ] && bind -f /etc/inputrc 2>/dev/null || true
bind 'set enable-bracketed-paste on' 2>/dev/null || true
