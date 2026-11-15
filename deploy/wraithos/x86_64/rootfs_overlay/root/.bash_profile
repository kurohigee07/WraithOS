# WraithOS — login shell startup
# Login shell baca .bash_profile, BUKAN .bashrc
# Standard pattern: source .bashrc dari .bash_profile

if [ -f ~/.bashrc ]; then
    . ~/.bashrc
fi

# Fallback PS1 kalau .bashrc gagal load
if [ -z "$PS1" ] || [ "$PS1" = '\s-\v\$ ' ]; then
    PS1='\[\e[0;37m\]\u@\h\[\e[0m\]:\[\e[1;37m\]\w\[\e[0m\]\$ '
fi

# Fallback bracketed paste (kalau .bashrc gagal)
bind 'set enable-bracketed-paste on' 2>/dev/null || true

if [ -n "$PS1" ] && [ -z "$WRAITHOS_SHOWN" ]; then
    echo ""
    echo "  WraithOS 0.53.0  |  type 'wraith-help' for commands"
    echo ""
    export WRAITHOS_SHOWN=1
fi
