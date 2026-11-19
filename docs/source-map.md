# Complete source map

All **files tracked at baseline commit `20cb378`**, indexed individually. Links below point at that immutable baseline for stability; current development continues on `main`. Documentation added after that baseline is listed separately.

Source, configuration and scripts were inspected for behavior. Lockfiles, minified vendor code and executables were identified as generated/artifact files rather than misrepresented as newly authored source. Credential contents are not reproduced.

## Reading routes

- Request path: `main.rs` → `telemetry.rs` → `http_api.rs` → `wasm/module.rs`.
- Events: `events.rs` → `rhai_worker.rs` / `ws.rs`.
- Appliance: choose the architecture-specific tree first; similar filenames do not imply identical behavior.
- Historical notes, `.bak` scripts and RAUC configs are not automatic statements of current status.

## Workspace and project entry points

| File | Purpose / caution |
|---|---|
| [.github/workflows/rust.yml](https://github.com/kurohigee07/WraithOS/blob/main/.github/workflows/rust.yml) | CI: formatting/Clippy, guest build/tests, x86 musl static-linkage check. |
| [.gitignore](https://github.com/kurohigee07/WraithOS/blob/main/.gitignore) | Ignores build/runtime output, databases, sidecars/signatures and distribution artifacts; already tracked files remain tracked. |
| [Cargo.lock](https://github.com/kurohigee07/WraithOS/blob/main/Cargo.lock) | Resolved dependency graph; Wasmtime 48.0.8 and workspace crate versions. |
| [Cargo.toml](https://github.com/kurohigee07/WraithOS/blob/main/Cargo.toml) | Workspace membership, shared dependencies, Rust 1.85 minimum and release profile. |
| [LICENSE](LICENSE) | Full text of the GNU Affero General Public License v3.0 (AGPLv3), the open-source half of the dual license. |
| [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) | Terms for the separate commercial license, when it is required, and how to contact the copyright holder. |
| [README.md](https://github.com/kurohigee07/WraithOS/blob/main/README.md) | Project entry point, rewritten as an English source-guided overview in this update. |
| [orchestrator.toml](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator.toml) | Development config: loopback listener, two Wasm permits, 256 MiB memory, build/auth disabled. |

## Host runtime

| File | Purpose / caution |
|---|---|
| [orchestrator/.gitignore](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/.gitignore) | Ignores nested target output. |
| [orchestrator/Cargo.toml](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/Cargo.toml) | Host crate manifest, version 0.1.0. |
| [orchestrator/src/build_api.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/build_api.rs) | Opt-in tar source ingestion and Bubblewrap Cargo pipeline; one global build slot. |
| [orchestrator/src/capabilities.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/capabilities.rs) | Host/port/event/log/table matchers; DB checker has no wired guest import. |
| [orchestrator/src/cli.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/cli.rs) | Clap config, module-directory, logging and dry-run flags. |
| [orchestrator/src/config.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/config.rs) | TOML schema/defaults, tenant config and script selection. |
| [orchestrator/src/context.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/context.rs) | Shared HTTP/SQLite state, global pool, owner-checked socket registry and tests. |
| [orchestrator/src/dashboard.html](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/dashboard.html) | HTMX UI and live event feed; auth/HTTPS/event-escaping limitations documented. |
| [orchestrator/src/error.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/error.rs) | Library error enum and result alias. |
| [orchestrator/src/events.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/events.rs) | In-memory Tokio broadcast EventBus. |
| [orchestrator/src/fs_util.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/fs_util.rs) | Individual-file temp-write/fsync/rename helper. |
| [orchestrator/src/http_api.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/http_api.rs) | Module CRUD/execution/quota/audit routes, headers and error mapping. |
| [orchestrator/src/lib.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/lib.rs) | Public library modules and re-exports. |
| [orchestrator/src/main.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/main.rs) | Startup ordering, migration, registry/listener/Rhai setup, smoke event and signal shutdown. |
| [orchestrator/src/module.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/module.rs) | OrchestratorModule lifecycle/execution trait. |
| [orchestrator/src/module_registry.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/module_registry.rs) | Tenant-keyed registry, disk loading, sidecars and Ed25519 verification. |
| [orchestrator/src/quota.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/quota.rs) | Continuous token bucket and optional module semaphore. |
| [orchestrator/src/rhai_worker.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/rhai_worker.rs) | Compiled AST dispatched through spawn_blocking; event sink and self-loop prevention. |
| [orchestrator/src/static/htmx.min.js](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/static/htmx.min.js) | Vendored minified HTMX 2.0.4; third-party artifact. |
| [orchestrator/src/telemetry.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/telemetry.rs) | ApiState/router, optional auth, fixed HTTP timeout and health/metrics/debug routes. |
| [orchestrator/src/tenant.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/tenant.rs) | Token verification/resource caps and migration into literal default directory. |
| [orchestrator/src/ui.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/ui.rs) | Embedded dashboard and escaped server-rendered polling fragments. |
| [orchestrator/src/wasm/host_impls.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/wasm/host_impls.rs) | Capability-gated HTTP/TCP/event bridges. |
| [orchestrator/src/wasm/mod.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/wasm/mod.rs) | Host bindgen, Engine and 100 ms epoch ticker; 600-tick deadline. |
| [orchestrator/src/wasm/module.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/wasm/module.rs) | Fresh-Store component execution, quota gates, JSON conversion, metrics/audit. |
| [orchestrator/src/wasm/state.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/wasm/state.rs) | Per-call WASI/resource state and linear-memory StoreLimits. |
| [orchestrator/src/wizard.html](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/wizard.html) | Three-step setup UI; no encryption input or actual browser-triggered reboot. |
| [orchestrator/src/wizard.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/wizard.rs) | Setup state/endpoints and optional destructive hard-coded /dev/vda4 LUKS path. |
| [orchestrator/src/ws.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/src/ws.rs) | WebSocket bus subscription without replay or tenant filtering. |
| [orchestrator/tests/integration.rs](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/tests/integration.rs) | Ten Axum tower::oneshot tests; some depend on example.com. |
| [orchestrator/wit/host-capabilities.wit](https://github.com/kurohigee07/WraithOS/blob/main/orchestrator/wit/host-capabilities.wit) | Authoritative HTTP/TCP/event imports and string-based run export. |

## Shared types and sample guest

| File | Purpose / caution |
|---|---|
| [sample-plugin/Cargo.toml](https://github.com/kurohigee07/WraithOS/blob/main/sample-plugin/Cargo.toml) | Guest cdylib using wit-bindgen 0.37; excluded from default workspace builds. |
| [sample-plugin/src/lib.rs](https://github.com/kurohigee07/WraithOS/blob/main/sample-plugin/src/lib.rs) | Guest performing HTTPS GET, raw TCP HTTP and event emission; catches host errors. |
| [shared-types/.gitignore](https://github.com/kurohigee07/WraithOS/blob/main/shared-types/.gitignore) | Ignores nested target output. |
| [shared-types/Cargo.toml](https://github.com/kurohigee07/WraithOS/blob/main/shared-types/Cargo.toml) | Shared DTO crate manifest. |
| [shared-types/src/lib.rs](https://github.com/kurohigee07/WraithOS/blob/main/shared-types/src/lib.rs) | Serializable manifest, tagged capabilities and quota types. |

## SDK and CLI

| File | Purpose / caution |
|---|---|
| [wraith-cli/Cargo.toml](https://github.com/kurohigee07/WraithOS/blob/main/wraith-cli/Cargo.toml) | CLI manifest; output binary named wraith. |
| [wraith-cli/src/main.rs](https://github.com/kurohigee07/WraithOS/blob/main/wraith-cli/src/main.rs) | Clap CLI: list/deploy/exec/delete/logs and global server/token/tenant flags. |
| [wraith-sdk/Cargo.toml](https://github.com/kurohigee07/WraithOS/blob/main/wraith-sdk/Cargo.toml) | SDK crate manifest. |
| [wraith-sdk/src/lib.rs](https://github.com/kurohigee07/WraithOS/blob/main/wraith-sdk/src/lib.rs) | Async HTTP SDK; schema fallback and missing advanced upload headers documented. |

## Checked-in module artifacts

| File | Purpose / caution |
|---|---|
| [modules/.gitkeep](https://github.com/kurohigee07/WraithOS/blob/main/modules/.gitkeep) | Placeholder retaining module storage directory. |
| [modules/default/sample.wasm](https://github.com/kurohigee07/WraithOS/blob/main/modules/default/sample.wasm) | Sample Wasm component, 54,639 bytes at baseline; not benchmark empty3. |
| [modules/default/sample.wasm.quota.json](https://github.com/kurohigee07/WraithOS/blob/main/modules/default/sample.wasm.quota.json) | All-zero sample quota: inherited memory and no additional module gates. |

## Deployment helpers

| File | Purpose / caution |
|---|---|
| [deploy/package.sh](https://github.com/kurohigee07/WraithOS/blob/main/deploy/package.sh) | Two-architecture tarball packager; hardcoded v0.18.0 and cleans dist output. |
| [deploy/run-wraithos.sh](https://github.com/kurohigee07/WraithOS/blob/main/deploy/run-wraithos.sh) | Boots an existing x86 image using KVM or TCG; does not build an image. |
| [deploy/sign.sh](https://github.com/kurohigee07/WraithOS/blob/main/deploy/sign.sh) | GPG-signs checksum manifest and release tarballs using an existing private key. |
| [deploy/verify-signature.sh](https://github.com/kurohigee07/WraithOS/blob/main/deploy/verify-signature.sh) | Temporary-keyring public-key verification and checksum checks. |
| [deploy/wraith-release.pub](https://github.com/kurohigee07/WraithOS/blob/main/deploy/wraith-release.pub) | Release public key only; fingerprint trust must be established out of band. |

## WraithOS x86 source tree

| File | Purpose / caution |
|---|---|
| [wraithos/br2-ext/Config.in](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/Config.in) | Empty Buildroot external-package menu placeholder. |
| [wraithos/br2-ext/board/wraithos/genimage.cfg](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/genimage.cfg) | BIOS/DOS: 64 MiB boot, two 128 MiB roots, 256 MiB ext4 data. |
| [wraithos/br2-ext/board/wraithos/grub.cfg](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/grub.cfg) | BIOS boot_b marker selection; absence selects A. |
| [wraithos/br2-ext/board/wraithos/kernel.fragment](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/kernel.fragment) | BPF/cgroups/watchdog/SquashFS fragment; ARM adds EFI/boot-contract/DM-CRYPT options. |
| [wraithos/br2-ext/board/wraithos/post-image.sh](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/post-image.sh) | BIOS GRUB/genimage assembly reference with historical fallback paths. |
| [wraithos/br2-ext/board/wraithos/post-image.sh.bak](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/post-image.sh.bak) | Legacy BIOS image-script backup; not the active ARM update path. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/fstab](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/fstab) | Mount declarations; critical mounts also occur in inittab/init scripts. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/hostname](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/hostname) | Default hostname wraithos. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S01hostname](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S01hostname) | Sets hostname from /etc/hostname. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S30data](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S30data) | Mounts persistent paths; ARM handles LUKS and unmounted fallback. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S41network-optional](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S41network-optional) | Best-effort en* DHCP; does not apply wizard static network TOML. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S99wraith-init](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S99wraith-init) | Starts/stops host; ARM avoids duplicate process. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/inittab](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/inittab) | BusyBox sysinit/tmpfs/console setup; ARM adds watchdog. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/issue](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/issue) | Pre-login console identity banner. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/motd](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/motd) | Post-login operational hints; ARM references boot contract/watchdog. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/network/interfaces](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/network/interfaces) | Loopback declaration; external interfaces handled elsewhere. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/os-release](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/os-release) | WraithOS identity metadata. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/passwd](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/passwd) | Root account entry using /bin/bash; password material separate. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/profile](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/profile) | Shell environment; ARM restricts history/core dumps and PATH. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/rauc/system.conf](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/rauc/system.conf) | Historical RAUC config; active updater is custom, not proof RAUC is installed. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/shells](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/shells) | Lists /bin/sh and /bin/bash. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/sysctl.conf](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/sysctl.conf) | Kernel printk verbosity setting. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/systemd/system/multi-user.target.wants/orchestrator.service](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/systemd/system/multi-user.target.wants/orchestrator.service) | Absolute target-root systemd symlink; appears broken outside that root. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/wraith.default/orchestrator.toml](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/wraith.default/orchestrator.toml) | First-boot config seed copied into persistent storage if absent. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/wraith/orchestrator.toml](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/etc/wraith/orchestrator.toml) | Overlay runtime config; persistent bind can override it. ARM absolute vs x86 relative paths. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/root/.bash_profile](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/root/.bash_profile) | Root login-shell loading/identity setup. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/root/.bashrc](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/root/.bashrc) | Interactive prompt/aliases; ARM adds slot identity and shell restrictions. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-help](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-help) | Architecture-specific console command reference. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-update](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-update) | Writes inactive root; ARM arms trial, x86 toggles markers. No image signature check. |
| [wraithos/br2-ext/board/wraithos/rootfs_overlay/usr/lib/systemd/system/orchestrator.service](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/board/wraithos/rootfs_overlay/usr/lib/systemd/system/orchestrator.service) | Target systemd unit for /sbin/wraith-init; ARM selects BusyBox instead. |
| [wraithos/br2-ext/external.desc](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/external.desc) | External-tree identity: WRAITHOS. |
| [wraithos/br2-ext/external.mk](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/br2-ext/external.mk) | Empty Buildroot external make hook. |

## WraithOS ARM source tree

| File | Purpose / caution |
|---|---|
| [wraithos/aarch64-br2-ext/Config.in](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/Config.in) | Empty Buildroot external-package menu placeholder. |
| [wraithos/aarch64-br2-ext/board/wraithos/etc/issue](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/etc/issue) | Pre-login console identity banner. |
| [wraithos/aarch64-br2-ext/board/wraithos/etc/motd](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/etc/motd) | Post-login operational hints; ARM references boot contract/watchdog. |
| [wraithos/aarch64-br2-ext/board/wraithos/etc/shadow](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/etc/shadow) | Tracked build-time credential material outside overlay; contents not reproduced. Review/rotate before distribution. |
| [wraithos/aarch64-br2-ext/board/wraithos/genimage-arm.cfg](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/genimage-arm.cfg) | ARM GPT: 200 MiB ESP, two 64 MiB roots, 128 MiB unformatted data region. |
| [wraithos/aarch64-br2-ext/board/wraithos/genimage.cfg](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/genimage.cfg) | BIOS/DOS: 64 MiB boot, two 128 MiB roots, 256 MiB ext4 data. |
| [wraithos/aarch64-br2-ext/board/wraithos/grub-arm.cfg](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/grub-arm.cfg) | ARM trial/tries/good/lkg selection and separate ESP kernels. |
| [wraithos/aarch64-br2-ext/board/wraithos/grub.cfg](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/grub.cfg) | BIOS boot_b marker selection; absence selects A. |
| [wraithos/aarch64-br2-ext/board/wraithos/kernel.fragment](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/kernel.fragment) | BPF/cgroups/watchdog/SquashFS fragment; ARM adds EFI/boot-contract/DM-CRYPT options. |
| [wraithos/aarch64-br2-ext/board/wraithos/post-build-arm.sh](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/post-build-arm.sh) | Prunes selected docs/tools/libraries while retaining cryptsetup dependencies. |
| [wraithos/aarch64-br2-ext/board/wraithos/post-image-arm.sh](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/post-image-arm.sh) | Builds ARM ESP/GRUB EFI/env and GPT disk; generated env format needs target validation. |
| [wraithos/aarch64-br2-ext/board/wraithos/post-image.sh](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/post-image.sh) | BIOS GRUB/genimage assembly reference with historical fallback paths. |
| [wraithos/aarch64-br2-ext/board/wraithos/post-image.sh.bak](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/post-image.sh.bak) | Legacy BIOS image-script backup; not the active ARM update path. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/fstab](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/fstab) | Mount declarations; critical mounts also occur in inittab/init scripts. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/hostname](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/hostname) | Default hostname wraithos. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S01hostname](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S01hostname) | Sets hostname from /etc/hostname. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S05bootstate](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S05bootstate) | ARM publishes active/other slot and boot state under /run/wraith. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S30data](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S30data) | Mounts persistent paths; ARM handles LUKS and unmounted fallback. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S41network-optional](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S41network-optional) | Best-effort en* DHCP; does not apply wizard static network TOML. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S99wraith-init](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/init.d/S99wraith-init) | Starts/stops host; ARM avoids duplicate process. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/inittab](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/inittab) | BusyBox sysinit/tmpfs/console setup; ARM adds watchdog. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/inputrc](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/inputrc) | ARM readline paste/bell/color settings. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/issue](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/issue) | Pre-login console identity banner. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/motd](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/motd) | Post-login operational hints; ARM references boot contract/watchdog. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/network/interfaces](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/network/interfaces) | Loopback declaration; external interfaces handled elsewhere. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/os-release](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/os-release) | WraithOS identity metadata. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/passwd](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/passwd) | Root account entry using /bin/bash; password material separate. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/profile](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/profile) | Shell environment; ARM restricts history/core dumps and PATH. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/rauc/system.conf](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/rauc/system.conf) | Historical RAUC config; active updater is custom, not proof RAUC is installed. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/securetty](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/securetty) | Console restriction list; x86 empty, ARM ttyAMA0/console. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/shells](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/shells) | Lists /bin/sh and /bin/bash. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/sysctl.conf](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/sysctl.conf) | Kernel printk verbosity setting. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/systemd/system/multi-user.target.wants/orchestrator.service](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/systemd/system/multi-user.target.wants/orchestrator.service) | Absolute target-root systemd symlink; appears broken outside that root. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/wraith.default/orchestrator.toml](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/wraith.default/orchestrator.toml) | First-boot config seed copied into persistent storage if absent. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/wraith/orchestrator.toml](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/etc/wraith/orchestrator.toml) | Overlay runtime config; persistent bind can override it. ARM absolute vs x86 relative paths. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/root/.bash_profile](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/root/.bash_profile) | Root login-shell loading/identity setup. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/root/.bashrc](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/root/.bashrc) | Interactive prompt/aliases; ARM adds slot identity and shell restrictions. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init) | Prebuilt AArch64 ELF, 19,796,464 bytes; no INTERP observed. Source edits do not regenerate it. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-watchdog](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-watchdog) | ARM five-second pet, 180-second grace and proof/commit/demotion worker. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-bootcheck](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-bootcheck) | ARM slot/process/health/env/mount/data-write proof. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-bootenv](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-bootenv) | GRUB state read/commit/trial/demote helper; overwrite plus sync, not proven power-loss atomic. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-help](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-help) | Architecture-specific console command reference. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-update](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/bin/wraith-update) | Writes inactive root; ARM arms trial, x86 toggles markers. No image signature check. |
| [wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/lib/systemd/system/orchestrator.service](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/usr/lib/systemd/system/orchestrator.service) | Target systemd unit for /sbin/wraith-init; ARM selects BusyBox instead. |
| [wraithos/aarch64-br2-ext/external.desc](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/external.desc) | External-tree identity: WRAITHOS. |
| [wraithos/aarch64-br2-ext/external.mk](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/aarch64-br2-ext/external.mk) | Empty Buildroot external make hook. |

## Buildroot configurations

| File | Purpose / caution |
|---|---|
| [wraithos/buildroot-configs/wraithos_aarch64_defconfig](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/buildroot-configs/wraithos_aarch64_defconfig) | ARM Cortex-A72/musl/BusyBox/GRUB EFI config with post scripts, cryptsetup and hardcoded paths. |
| [wraithos/buildroot-configs/wraithos_x86_64_defconfig](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/buildroot-configs/wraithos_x86_64_defconfig) | x86 musl/kernel 6.6 config; systemd/BusyBox and post-image wiring mismatches. |
| [wraithos/buildroot-configs/wraithos_x86_64_final.config](https://github.com/kurohigee07/WraithOS/blob/main/wraithos/buildroot-configs/wraithos_x86_64_final.config) | Only one mirror setting despite filename; not a complete final config. |

## Termux and OS notes

| File | Purpose / caution |
|---|---|
| [deploy/wraithos/README.md](https://github.com/kurohigee07/WraithOS/blob/main/deploy/wraithos/README.md) | English x86 rebuild outline; expanded and qualified by the WraithOS guide. |
| [deploy/wraithos/termux-package/README.md](https://github.com/kurohigee07/WraithOS/blob/main/deploy/wraithos/termux-package/README.md) | Termux usage; binary supplied separately. |
| [deploy/wraithos/termux-package/orchestrator.toml](https://github.com/kurohigee07/WraithOS/blob/main/deploy/wraithos/termux-package/orchestrator.toml) | Phone-oriented 128 MiB/64-socket config with permissive HTTP wildcard. |
| [deploy/wraithos/termux-package/run.sh](https://github.com/kurohigee07/WraithOS/blob/main/deploy/wraithos/termux-package/run.sh) | Termux process launcher and health check. |
| [deploy/wraithos/termux-package/status.sh](https://github.com/kurohigee07/WraithOS/blob/main/deploy/wraithos/termux-package/status.sh) | Termux process/health/readiness check. |
| [deploy/wraithos/termux-package/stop.sh](https://github.com/kurohigee07/WraithOS/blob/main/deploy/wraithos/termux-package/stop.sh) | Termux process-pattern stop helper. |

## Documentation added after the baseline

| Path | Purpose |
|---|---|
| `README.md` | Rewritten English project overview. |
| `docs/*.md` | Detailed English guides and this map. |
| `docs/benchmark-data.json` | Structured supplied evidence and provenance. |
| `docs/examples/empty-plugin/` | Complete offline guest and copied WIT contract. |

## Baseline artifact identity

```text
modules/default/sample.wasm
  bytes: 54639
  sha256: 21d8a86762c639f66ddfeea88c97627fc8a92ad7df8fdf10333ba29ca5a1073a

wraithos/aarch64-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init
  bytes: 19796464
  format: ELF64 AArch64 EXEC; no INTERP segment observed
  sha256: 93b68d9e2e4d4536185fcf6298f9b5a31b8b9c0216a6d9c8785208e738c9f1bb
```

These identify checked-in artifacts, not the supplied benchmark binary or a signed-release claim.

## v0.53.1 additions

| File | Purpose |
|------|---------|
| `deploy/wraithos/{arch}/rootfs_overlay/usr/bin/wraith-verity-rehash` | Recompute verity tree + rewrite grub.cfg menuentry for target slot |
| `deploy/wraithos/{arch}/rootfs_overlay/usr/lib/rauc/post-install` | RAUC post-install handler wrapper (calls rehash) |
| `deploy/wraithos/{arch}/rootfs_overlay/etc/rauc/system.conf` | `[handlers] post-install=` entry |
| `deploy/wraithos/{arch}/rootfs_overlay/usr/bin/wraith-install-bundle` | Added `--override-boot-slot`; guard removed |
| `deploy/wraithos/{arch}/rootfs_overlay/usr/bin/wraith-rauc-healthcheck` | Added `--override-boot-slot` on mark-good/mark-bad |
| `deploy/wraithos/{arch}/rootfs_overlay/usr/bin/wraith-update` | Guard removed; rehash before trial arm |
