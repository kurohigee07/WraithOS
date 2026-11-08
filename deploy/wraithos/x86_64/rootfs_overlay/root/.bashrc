# WraithOS — shell lockdown (defense-in-depth)
# Rootfs read-only, no persistent history, restricted environment.

# ── No history persistence (leak protection)
export HISTFILE=/dev/null
export HISTSIZE=0
export HISTFILESIZE=0
set +H 2>/dev/null   # disable history expansion (!cmd)

# ── No overwrite via > redirect; use >| to force
set -o noclobber

# ── Restricted PATH (readonly to prevent override)
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
readonly PATH

# ── Umask: group/other have no write
umask 022

# ── Prompt with slot indicator
__slot() { sed -n 's/.*slot=\([AB]\).*/\1/p' /proc/cmdline 2>/dev/null | head -1; }
__s=$(__slot)
PS1='\[\e[0;37m\]\u@\h\[\e[0m\][\[\e[1;33m\]${__s:-?}\[\e[0m\]]:\[\e[1;37m\]\w\[\e[0m\]\$ '

# ── Safe aliases (confirmation on destructive ops)
alias rm='rm -i'
alias cp='cp -i'
alias mv='mv -i'
alias ls='ls --color=auto'
alias ll='ls -lah'
alias la='ls -A'
alias grep='grep --color=auto'

# ── Bracketed paste (Termius/Termux wrap)
bind 'set enable-bracketed-paste on' 2>/dev/null || true
