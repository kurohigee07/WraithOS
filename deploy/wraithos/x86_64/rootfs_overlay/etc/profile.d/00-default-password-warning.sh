#!/bin/sh
# One-line nudge at every interactive login while the shipped default
# root password is still in /etc/shadow. Runs after profile sets PATH.
# The hash below is the default $6$2aAqow7... shipped at build time.
# When the operator runs `passwd`, the hash changes and this stops.
DEFAULT_HASH_PREFIX='$6$2aAqow7'
if [ -r /etc/shadow ]; then
    CURRENT=$(awk -F: '$1=="root"{print $2}' /etc/shadow 2>/dev/null)
    case "$CURRENT" in
        "$DEFAULT_HASH_PREFIX"*)
            printf '\n\033[1;31m[wraith] root password is the shipped default — run: passwd\033[0m\n\n' >&2
            ;;
    esac
fi
