# WraithOS — interactive shell config
# Sourced dari .bash_profile (login shell) atau langsung (non-login)

PS1='\[\e[0;37m\]\u@\h\[\e[0m\]:\[\e[1;37m\]\w\[\e[0m\]\$ '

alias ls='ls --color=auto'
alias ll='ls -lah'
alias la='ls -A'
alias grep='grep --color=auto'

# Bracketed paste (Termius/Termux wrap paste, bash buffer sampai Enter)
bind 'set enable-bracketed-paste on' 2>/dev/null || true
