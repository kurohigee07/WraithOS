#!/bin/bash
set -e
TARGET_DIR="${TARGET_DIR:-${BASE_DIR}/target}"

echo "[post-build] Drop kernel modules (all builtin =y)"
rm -rf "${TARGET_DIR}/lib/modules"

echo "[post-build] Drop GRUB (kept in ESP partition)"
rm -rf "${TARGET_DIR}/lib/grub"
rm -rf "${TARGET_DIR}/usr/lib/grub"
rm -rf "${TARGET_DIR}/usr/share/grub"
rm -f  "${TARGET_DIR}/usr/bin/grub-"* 2>/dev/null || true
rm -f  "${TARGET_DIR}/usr/sbin/grub-"* 2>/dev/null || true
rm -f  "${TARGET_DIR}/sbin/grub-"* 2>/dev/null || true

echo "[post-build] Drop locale/docs (appliance: English + no docs)"
rm -rf "${TARGET_DIR}/usr/share/locale"
rm -rf "${TARGET_DIR}/usr/share/info"
rm -rf "${TARGET_DIR}/usr/share/man"
rm -rf "${TARGET_DIR}/usr/share/doc"

# GLib libs KEPT — RAUC needs libglib/libgio/libgobject/libgmodule at runtime.
# Removed in a previous revision on the assumption "config removed, files
# stayed"; that was wrong: BR2_PACKAGE_RAUC depends on libglib2.

echo "[post-build] Drop OpenSSL libs (orchestrator static musl + rustls-ring)"
# KEPT (cryptsetup needs libcrypto.so.3): rm -f  "${TARGET_DIR}/usr/lib/libcrypto"* 2>/dev/null || true
# KEPT: rm -f  "${TARGET_DIR}/usr/lib/libssl"*    2>/dev/null || true
# openssl CLI KEPT — useful for RAUC CA/cert inspection (~1MB)

echo "[post-build] Drop SQLite shared (bundled in orchestrator)"
rm -f  "${TARGET_DIR}/usr/lib/libsqlite3"* 2>/dev/null || true

echo "[post-build] Drop unused CLI tools"
rm -f  "${TARGET_DIR}/usr/bin/pcre2test"    2>/dev/null || true
rm -f  "${TARGET_DIR}/usr/bin/pcre2grep"    2>/dev/null || true
rm -f  "${TARGET_DIR}/usr/bin/sqlite3"      2>/dev/null || true
rm -f  "${TARGET_DIR}/usr/bin/gio"          2>/dev/null || true
rm -f  "${TARGET_DIR}/usr/bin/gdbus"        2>/dev/null || true
rm -f  "${TARGET_DIR}/usr/bin/gsettings"    2>/dev/null || true
rm -f  "${TARGET_DIR}/usr/bin/gresource"    2>/dev/null || true
rm -f  "${TARGET_DIR}/usr/bin/gapplication" 2>/dev/null || true

echo "[post-build] Done"
