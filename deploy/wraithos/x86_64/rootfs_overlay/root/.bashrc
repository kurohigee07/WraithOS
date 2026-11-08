# WraithOS — shell lockdown + Spectral prompt

INPUTRC=/etc/inputrc
TMOUT=600
export INPUTRC TMOUT
readonly INPUTRC TMOUT

export HISTFILE=/dev/null
export HISTSIZE=0
export HISTFILESIZE=0
set +H 2>/dev/null

set -o noclobber

export PATH=/usr/sbin:/usr/bin:/sbin:/bin
readonly PATH

umask 022
ulimit -c 0

# Terminal palette (belt+braces with /etc/issue)
printf '\033]11;#1C1C1C\033\\'
printf '\033]10;#D0D0D0\033\\'

__slot() { sed -n 's/.*slot=\([AB]\).*/\1/p' /proc/cmdline 2>/dev/null | head -1; }
__s=$(__slot)
PS1='\[\e[38;5;179m\]\u@\h\[\e[0m\][\[\e[38;5;66m\]${__s:-?}\[\e[0m\]]:\[\e[38;5;252m\]\w\[\e[0m\] \[\e[38;5;179m\]\$ \[\e[0m\]'

alias rm='rm -i'
alias cp='cp -i'
alias mv='mv -i'
alias ls='ls --color=auto'
alias ll='ls -lah'
alias la='ls -A'
alias grep='grep --color=auto'

[ -f /etc/inputrc ] && bind -f /etc/inputrc 2>/dev/null || true
bind 'set enable-bracketed-paste on' 2>/dev/null || true
