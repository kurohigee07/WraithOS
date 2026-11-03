BASH_ENV=
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH BASH_ENV
umask 0077

# Always source .bashrc (safe: /root is 700, only root can write)
if [ -f /root/.bashrc ]; then
    . /root/.bashrc
fi

if [ -z "$PS1" ] || [ "$PS1" = '\s-\v\$ ' ]; then
    PS1='\[\e[0;37m\]\u@\h\[\e[0m\]:\[\e[1;37m\]\w\[\e[0m\]\$ '
fi
bind 'set enable-bracketed-paste on' 2>/dev/null || true
