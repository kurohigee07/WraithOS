#!/bin/bash
# gen-sb-keys.sh — generate per-device UEFI Secure Boot keys (PK/KEK/db).
#
# These keys are DEVICE-SPECIFIC. Do not share across devices. Do not
# commit them. Losing db.key = no signed updates for the deployed fleet.
# Leaking db.key = anyone can sign an image that passes Secure Boot.
#
# Backup procedure (MUST be completed before running this in production):
#   1. Encrypted USB x 2, stored in different physical locations.
#   2. Password manager entry for the key password (WRAITH_SB_KEYPASS).
#   3. Rotation policy documented.
#
# Usage:
#   export WRAITH_SB_KEYPASS='...'    # long, random, backed up
#   ./gen-sb-keys.sh [/output/dir]
#
# Default output: /tmp/sb-keys (test only, wiped on reboot).
# Non-/tmp output: warns and sleeps 5 seconds before proceeding.

set -euo pipefail

OUT="${1:-$HOME/sb-keys}"
FORCE_DEV=0
if [ "${1:-}" = "--force-dev" ]; then
    OUT="/tmp/sb-keys"
    FORCE_DEV=1
fi

if [ -z "${WRAITH_SB_KEYPASS:-}" ]; then
    echo "FATAL: WRAITH_SB_KEYPASS is not set." >&2
    echo "       Export it with the device key password before running." >&2
    exit 1
fi
if [ "${#WRAITH_SB_KEYPASS}" -lt 16 ]; then
    echo "FATAL: WRAITH_SB_KEYPASS must be at least 16 characters." >&2
    exit 1
fi

# Warn if destination is volatile (under /tmp). Persistent paths
# ($HOME/sb-keys or similar) do NOT warn — that is the intended target.
case "$OUT" in
    /tmp/*|/var/tmp/*)
        if [ "$FORCE_DEV" != "1" ]; then
            echo ""
            echo "*** FATAL: refusing to write keys under /tmp/ ***"
            echo "*** Output dir: $OUT"
            echo "*** /tmp is wiped on reboot. Keys generated here WILL be lost."
            echo "*** Use the default $HOME/sb-keys, or a persistent path."
            echo "*** For throwaway test keys, pass --force-dev as the first arg."
            echo ""
            exit 1
        fi
        echo ""
        echo "*** DEV MODE: keys will be written to $OUT and lost on reboot ***"
        echo "*** Do NOT use these keys in production ***"
        echo ""
        sleep 3
        ;;
esac

# Persistent path reminder
echo ""
echo "=== Key directory: $OUT ==="
echo "Before proceeding, confirm backup plan:"
echo "  [ ] encrypted USB x 2, different physical locations"
echo "  [ ] password-manager entry for WRAITH_SB_KEYPASS"
echo "  [ ] copy to offline cloud storage (OCI / Drive)"
echo "  [ ] rotation policy documented"
echo ""
echo "Sleeping 5 seconds. Ctrl-C to abort."
sleep 5

if [ -d "$OUT" ] && [ -n "$(ls -A "$OUT" 2>/dev/null)" ]; then
    echo "FATAL: $OUT is not empty. Refusing to overwrite." >&2
    echo "       Move it aside or pick a different directory." >&2
    exit 1
fi
mkdir -p "$OUT"
chmod 0700 "$OUT"

PASS_FILE="$OUT/.pass"
umask 077
printf '%s\n' "$WRAITH_SB_KEYPASS" > "$PASS_FILE"
trap 'rm -f "$PASS_FILE"' EXIT

gen_ca() {
    local name=$1
    openssl req -new -x509 -newkey rsa:2048 -sha256 -days 3650 \
        -passout "file:$PASS_FILE" \
        -keyout "$OUT/$name.key" -out "$OUT/$name.crt" \
        -subj "/CN=WraithOS $name/ O=WraithOS/ C=ID" \
        -addext "basicConstraints=critical,CA:TRUE" \
        -addext "keyUsage=critical,digitalSignature,keyCertSign,cRLSign" \
        2>/dev/null
    chmod 0600 "$OUT/$name.key"
    chmod 0644 "$OUT/$name.crt"
    echo "  generated: $name.key $name.crt"
}

gen_db() {
    openssl req -new -x509 -newkey rsa:2048 -sha256 -days 3650 \
        -passout "file:$PASS_FILE" \
        -keyout "$OUT/db.key" -out "$OUT/db.crt" \
        -subj "/CN=WraithOS db/O=WraithOS/C=ID" \
        -addext "basicConstraints=critical,CA:FALSE" \
        -addext "keyUsage=critical,digitalSignature" \
        -addext "extendedKeyUsage=codeSigning" \
        2>/dev/null
    chmod 0600 "$OUT/db.key"
    chmod 0644 "$OUT/db.crt"
    chmod 0600 "$OUT/db.key"
echo "  generated: db.key db.crt"
}

echo "=== Generating WraithOS Secure Boot keys ==="
echo "  output: $OUT"
echo ""
gen_ca PK
gen_ca KEK
gen_db
echo ""

echo "=== Key fingerprints ==="
for f in "$OUT"/*.crt; do
    fp=$(openssl x509 -in "$f" -noout -fingerprint -sha256 | cut -d= -f2)
    echo "  $(basename "$f"): $fp"
done
echo ""

echo "=== Backup manifest (write this down / copy to password manager) ==="
for f in "$OUT"/*.crt; do
    fp=$(openssl x509 -in "$f" -noout -fingerprint -sha256 | cut -d= -f2)
    echo "  $(basename "$f"): $fp"
done
echo ""

echo "=== Next steps ==="
echo "  1. Verify backups (encrypted USB x2, different locations)."
echo "  2. Enroll PK/KEK/db into OVMF vars for QEMU test (see deploy/wraithos/uefi/README.md)."
echo "  3. Sign: sbsign --key $OUT/db.key --cert $OUT/db.crt --output out.efi in.efi"
echo "  4. Verify: sbverify --cert $OUT/db.crt out.efi"
echo ""
echo "=== Password will NOT be cached; unset WRAITH_SB_KEYPASS when done ==="
