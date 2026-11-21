# Contributing


> Note: this file covers policy for external contributions.
> The maintainer's own AI usage is documented in [`AI-DISCLOSURE.md`](AI-DISCLOSURE.md).

Thank you for considering a contribution to WraithOS. This document
covers the mechanics — how to build, how to test, how to submit — and
the AI-assistance policy.

## Licensing and DCO

WraithOS is licensed AGPL-3.0-or-later. **There is no CLA.** By
submitting a pull request you agree that your contribution is
licensed under the same terms as the project (inbound = outbound).

Every commit must carry a `Signed-off-by:` line with your name and
email, matching the Developer Certificate of Origin 1.1
(https://developercertificate.org/). Use `git commit -s` — this is
the only formality required.

## Building

Prerequisites:

- Rust 1.85+ (workspace `rust-version` is 1.85 in `Cargo.toml`)
- Buildroot 2024.02.9 checked out next to the repo (or `BR2_ROOT` set)
- For cross-compile: the musl toolchains from each Buildroot output
  tree (`x86_64-buildroot-linux-musl-gcc`,
  `aarch64-buildroot-linux-musl-gcc`)

Orchestrator build (native, for tests):

    cargo build --release -p orchestrator

Orchestrator build (cross, target image):

    export CC_x86_64_unknown_linux_musl=<buildroot>/output/host/bin/x86_64-buildroot-linux-musl-gcc
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=$CC_x86_64_unknown_linux_musl
    cargo build --release --target x86_64-unknown-linux-musl -p orchestrator

Sync the binary into the external tree, then rebuild the image:

    deploy/wraithos/sync-wraith-init.sh
    cd <buildroot> && make O=output-arm-m31 -j4   # or default output for x86

## Testing

Before opening a pull request, all of these must pass:

    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test --all-targets

If your change touches the boot chain, TPM path, RAUC, or the Wasm
sandbox, also run the relevant QEMU smoke test described in
`docs/testing.md`.

## Code style

- Keep functions small and single-purpose. The existing pattern is a
  long-running worker that reads events and dispatches to focused
  helpers; follow it.
- Error paths must return a clear message. Do not swallow errors with
  `let _ =` unless the reason is documented inline.
- Every subprocess invocation must be bounded: either a `timeout`
  wrapper, `tokio::time::timeout`, or `kill_on_drop(true)` with an
  outer timeout. Unbounded subprocesses have caused brick-level bugs
  in this project (see `CHANGELOG.md` v0.53.1).
- Security-relevant code paths (anything under
  `orchestrator/src/wizard.rs`, `orchestrator/src/wasm/`,
  `orchestrator/src/rhai_worker.rs`, and the shell scripts under
  `deploy/wraithos/*/rootfs_overlay/`) require a PR description
  covering threat-model impact. See `GOVERNANCE.md` "Review policy".

## AI-assistance policy

This policy applies to **contributions to WraithOS as a project**. It
follows NLnet Foundation's Generative AI Policy 2.0 (October 2026),
under which WraithOS has applied for grant funding.

- **You may use AI as a read-only reviewer.** Ask it to point out
  bugs, missing cases, or unclear code.
- **You must write the resulting patch yourself.** Do not paste
  AI-generated code, tests, docs, or design text into a pull request
  without reading, understanding, and being able to defend every
  line.
- **Disclose significant AI use in the PR description.** A sentence
  is enough: "Wrote the fix myself; used <tool> to review the error
  path." Omission is worse than use.
- **Translation.** If English is not your first language, regular
  machine translation (DeepL, Google Translate web UI) is fine.
  Chat-based LLM translation/rewriting is not, for prose that
  represents your own reasoning.
- **Do not submit AI-generated design documents, threat models, or
  security rationale.** These represent the contributor's
  understanding; an LLM does not have one.

The maintainer reserves the right to reject any contribution whose
authorship cannot be reasonably established, and to ask for
clarification. This is a policy about understanding, not about tools.

## Reporting bugs

For security bugs, **do not open a public issue** — see
[SECURITY.md](docs/security.md). For non-security bugs, use the GitHub
issue tracker and include:

- Release tag (e.g. `v0.53.1-update-hook`)
- Architecture (x86_64 or aarch64)
- Boot media (QEMU, USB, SD)
- Exact steps to reproduce
- Serial log if the bug is at boot, `wraith.log` otherwise

## Proposing larger changes

Open an issue with a short design note before implementing. Anything
touching the boot chain, TPM seal policy, module manifest format, or
`docs/security.md` requires maintainer sign-off before code lands.

## Code of conduct

Be civil. Technical disagreement is fine and expected; personal
attacks are not. The maintainer may lock threads or block accounts
that cross that line. There is no formal CoC document yet — this
paragraph is the whole policy.
