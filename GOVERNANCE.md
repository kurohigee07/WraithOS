# Governance

## Maintainer

WraithOS is currently a single-maintainer project:

- **Jace** — design, implementation, releases, security response
- Contact: 231419737+kurohigee07@users.noreply.github.com (or open an issue)

## Decision-making

Technical decisions are made by the maintainer, in public, on GitHub
issues and pull requests. Significant changes (new features, breaking
changes to the module manifest, boot chain, or security invariants)
are documented in a proposal issue before implementation. Anyone may
comment; the maintainer makes the final call and documents the
reasoning in the commit message or the issue thread.

## Review policy

- Small fixes (docs, typos, isolated bug fixes): maintainer review
  before merge.
- Changes to `orchestrator/src/wasm/`, `orchestrator/src/wizard.rs`,
  `orchestrator/src/rhai_worker.rs`, RAUC/boot chain, or
  `docs/security.md`: one maintainer review plus a written rationale
  in the PR description covering threat-model impact.
- Security issues: see [SECURITY.md](docs/security.md) — do **not** open a
  public issue.

## Bus factor

**WraithOS currently has a bus factor of 1.** This is disclosed
openly because reviewers and downstream users need to plan for it.

Concrete mitigations in place today:

- Every design decision is documented in-repo (see `docs/` and
  `CHANGELOG.md`; the CHANGELOG contains the *why* for each release
  and links to the relevant commit).
- Build and release are reproducible from public state; no private
  CI secrets are required to produce a working image (the Secure Boot
  key-loss post-mortems in `docs/security.md` document what happens
  when a private key is lost, and what the recovery path is).
- All external dependencies are pinned in `Cargo.lock` and the
  Buildroot `*_defconfig` files, and every release commit records the
  build command used.
- The release signing process is documented in
  `docs/wraithos.md` so a new maintainer can pick it up.

Concrete mitigations **planned** (depend on grant funding, tracked in
[ROADMAP.md](ROADMAP.md)):

- Recruit one co-maintainer or paid contractor for a fixed engagement
  (e.g. one full-time equivalent over three months) to share
  maintenance load and reduce single-person operational risk.
- Publish a written incident-response runbook separate from
  `SECURITY.md` (who to contact, what SLA is intended, how a CVE is
  handled).
- Set up a second-release reviewer for tagged releases, so no tag
  ships from a single pair of eyes.

## Becoming a maintainer

If you contribute regularly over a sustained period and want to take
on maintainer responsibilities, open a discussion issue. There is no
formal ladder; the criterion is "shipped code, good judgement, follow
through on review and release". The bus-factor mitigation above
depends on this path succeeding.

## Licensing of contributions

There is **no CLA**. Contributions are accepted under the project's
own license, AGPL-3.0-or-later, via Developer Certificate of Origin
sign-off (see CONTRIBUTING.md). Commercial re-licensing of
contributions is **not** performed; the project does not dual-license.
