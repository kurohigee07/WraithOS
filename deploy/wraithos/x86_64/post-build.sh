#!/bin/bash
# WraithOS x86_64 post-build: root password hash override.
# Called by Buildroot after target/ is populated, before image packing.
# TARGET_DIR points to output/target/.
set -e
TARGET_DIR="${TARGET_DIR:-${BASE_DIR}/target}"

if [ -n "${WRAITH_ROOT_PASSWORD_HASH:-}" ]; then
    SHADOW_FILE="${TARGET_DIR}/etc/shadow"
    if [ ! -f "$SHADOW_FILE" ]; then
        echo "[post-build] ERROR: WRAITH_ROOT_PASSWORD_HASH set but $SHADOW_FILE missing" >&2
        exit 1
    fi
    case "$WRAITH_ROOT_PASSWORD_HASH" in
        \$*) : ;;
        *) echo "[post-build] ERROR: WRAITH_ROOT_PASSWORD_HASH must start with \$ (crypt format)" >&2; exit 1 ;;
    esac
    awk -v h="$WRAITH_ROOT_PASSWORD_HASH" -F: 'BEGIN{OFS=":"} $1=="root"{$2=h} {print}' \
        "$SHADOW_FILE" > "${SHADOW_FILE}.tmp"
    mv "${SHADOW_FILE}.tmp" "$SHADOW_FILE"
    chmod 600 "$SHADOW_FILE"
    echo "[post-build] root hash overridden from WRAITH_ROOT_PASSWORD_HASH"
else
    echo "[post-build] WRAITH_ROOT_PASSWORD_HASH unset; using shipped default"
    echo "[post-build]   (production images MUST set this; see README.md)"
fi
