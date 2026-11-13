#!/bin/sh
# tpm-lib.sh — shared helpers for wraith-tpm-* scripts
# Sourced by wraith-tpm-seal, -unseal, -finalize, -reseal, -prepare-update.
# Do not execute directly.

# PCR 0-7 verified stable across cold boots on ARM (edk2 QEMU_EFI.fd).
# x86 policy subject to Block 9 measurement.
export WRAITH_TPM_PCRS="${WRAITH_TPM_PCRS:-sha256:0,1,2,3,4,5,6,7}"

# Kernel resource manager (avoids /dev/tpm0 exclusive-access contention).
export TPM2TOOLS_TCTI="${TPM2TOOLS_TCTI:-device:/dev/tpmrm0}"

# Diagnostics log. /var/log is tmpfs on WraithOS; survives until reboot.
export WRAITH_TPM_LOG="${WRAITH_TPM_LOG:-/var/log/wraith-tpm.log}"

tpm_log() {
    _msg="$(date -u +%Y-%m-%dT%H:%M:%SZ) $*"
    echo "$_msg" >&2
    echo "$_msg" >> "$WRAITH_TPM_LOG" 2>/dev/null || true
}

# 0=present+functional, 1=present+broken, 2=absent
tpm_check() {
    [ -c /dev/tpm0 ]   || return 2
    [ -c /dev/tpmrm0 ] || return 2
    tpm2_getcap properties-fixed >/dev/null 2>&1 || return 1
    return 0
}

# Ensure /boot writable. Mounts rw if unmounted. Loud failure on ro.
boot_rw_check() {
    _b="${1:-/boot}"
    if ! grep -q " ${_b} " /proc/mounts; then
        mount -t vfat -o rw /dev/vda1 "$_b" 2>/dev/null || {
            tpm_log "FATAL: $_b not mounted, cannot mount rw"
            return 1
        }
    fi
    if ! touch "${_b}/.wraith-probe" 2>/dev/null; then
        tpm_log "FATAL: $_b rw probe failed"
        return 1
    fi
    rm -f "${_b}/.wraith-probe"
    return 0
}
