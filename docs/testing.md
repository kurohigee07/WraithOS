# Testing, verification, and contributing

## What the Rust suite contains

At the documented baseline there are **23 unit tests** in the orchestrator and **10 HTTP integration tests**:

| Area | Count | Coverage shape |
|---|---:|---|
| Capability matching | 2 | Wildcard and exact HTTP host matching |
| Context/resource registries | 3 | Socket cap, owner checking, global pool timeout/release |
| Module signatures | 4 | Valid, tampered, wrong key, bad signature length |
| Module quota | 3 | Token refill, concurrency, zero/default quota |
| Tenant registry | 6 | Token comparison/auth, rate, concurrency, module capacity |
| HTML escaping | 3 | Server-rendered dashboard field escaping |
| Wizard markers | 2 | Fresh state and persisted markers |
| HTTP integration | 10 | Health, upload/execute/audit, missing module, auth, build gate, capability behavior |

The integration tests call the Axum router through `tower::oneshot`; they do not launch the production binary or bind port 9090. Several use the checked-in sample and can require real access to `example.com`. Some assertions are permissive, and the suite is not a complete hostile-tenant/security test.

## Run the source checks

```bash
cargo fmt --all -- --check
cargo clippy --locked -p orchestrator --lib -- -D warnings
cargo build --locked -p sample-plugin --target wasm32-wasip2 --release
cargo test --locked -p orchestrator -p wraith-sdk -p wraith-cli --all-targets
```

The CI workflow runs on pushes/PRs targeting `main`. It has formatting/Clippy, guest build/tests, and an x86 musl static-linkage job. A session working branch does not receive the push CI trigger unless the workflow is changed or a qualifying PR is opened. CI configuration is not proof that the current checkout passed CI.

**Verification boundary for this update:** Rust/Cargo was not installed in the documentation workspace, so these Rust commands were not rerun. The work changed documentation and documentation tooling, not runtime source.

## Documentation site

The site is plain HTML/CSS/JavaScript. Markdown guides are compiled into `docs/book/` by a small Node build script using marked. Fonts and scripts are local; no CDN or orchestrator backend is required.

```bash
cd docs
npm ci
npm run build
npm start
```

Open `http://localhost:4173/`. The server binds `0.0.0.0` for container/live-preview compatibility and accepts arbitrary preview hostnames. It serves only the documentation directory, not repository secrets or the runtime API. For a laptop-only bind, use the Python alternative from the repository root:

```bash
python3 -m http.server 4173 --bind 127.0.0.1
```

Then open `/docs/`. The generated `docs/index.html` also works as a local file in a modern browser.

### Browser checks

With a Playwright-compatible Chromium available:

```bash
cd docs
npx playwright install chromium
npm test
```

Or point to an existing browser:

```bash
CHROMIUM_PATH=/path/to/chromium npm test
```

`npm test` first checks local links/assets/anchors, then runs the browser smoke suite against the running preview server. The browser checks cover guide rendering/search, request stepping and scenarios, concurrency simulation, ARM boot transitions, benchmark scale switching, keyboard/accessibility basics, mobile overflow, and browser console errors. Run `npm run test:links` for the server-free link check alone. These checks do not test the Rust runtime.

## Source/documentation consistency checklist

When changing behavior, update the corresponding guide and interactive explanation:

- WIT change → `plugins.md`, sample guests, architecture host bridges.
- Config field/default → `configuration.md` and quick start/appliance examples.
- Route/status/header → `api.md`, SDK/CLI docs if affected.
- Timer/counter semantics → `benchmarks.md` and benchmark lab labels.
- Auth/capability/socket/event scope → `security.md` and architecture.
- Boot script/partition/update change → `wraithos.md` and A/B lab.
- New/removed tracked file → regenerate/update `source-map.md`.


## Contributing a behavior change

1. Describe the user-visible problem and the trust boundary affected.
2. Add a focused failing test where practical.
3. Keep host/guest contract changes coordinated.
4. Run formatting, Clippy, tests, and a relevant manual smoke test.
5. Update English documentation and examples in the same change.
6. Report what was actually run; do not convert a planned test into a claimed pass.
7. For performance claims, preserve raw data and methodology alongside derived figures.

## Release verification

Use the [operations checklist](operations.md#pre-release-operational-checklist). Security-sensitive changes additionally need negative tests for authorization, tenant data visibility, signature persistence, source extraction, resource exhaustion, and shutdown/update failure paths. The current test count alone is not a security certification.
