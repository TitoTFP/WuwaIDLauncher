# WuwaID Launcher optimization audit

Date: 2026-09-07  
Profile: balanced production hardening, backward-compatible where possible.

## Scope

Reviewed the frontend, Tauri/Rust backend, downloader and patch/media integrity, lifecycle/state handling, accessibility, dependency hygiene, CI/release workflows, packaging constraints, and manual acceptance boundaries.

Real-game acceptance remains a manual Windows-compatible check. It is intentionally not faked or moved into GitHub-hosted Actions.

## Baseline

Captured before changes:

- Frontend check/build, JavaScript contracts, Rust tests, formatting, and Clippy were already green.
- `npm audit` reported 4 development vulnerabilities.
- Real-game acceptance and Windows resource profiling were not reproducible in this Linux worktree and had no GitHub-hosted real-game workflow.
- Release verification lacked explicit npm security and frontend-control gates; the release Node setup also enabled package-manager caching.

## Thresholds

The post-change gates use these explicit thresholds:

- `npm audit --audit-level=high`: zero reported vulnerabilities (actual result: zero vulnerabilities at all severities).
- `npm run check`: zero Svelte/type/lint errors and warnings; `npm run build`: successful frontend artifact.
- Rust formatting and `cargo clippy -- -D warnings`: zero failures/warnings; all Rust tests must pass.
- Manifest bodies are capped at 1 MiB; downloads are capped at 512 MiB and must match expected size plus SHA-256.
- Generic downloads must remain HTTPS; official GitHub redirects are host-allowlisted; no self-hosted workflow may remain.

## Findings closed

| Severity | Finding | Durable closure |
| --- | --- | --- |
| High | Development dependency vulnerabilities | Vite/plugin refresh and lockfile update; npm audit now reports zero vulnerabilities. |
| High | Unrestricted download redirects and shell-backed wrappers | Shared GitHub/HTTPS redirect policies and direct Node entrypoint execution. |
| Medium | Media manifest replacement could delete the last good cache first | Atomic replacement through the existing platform-aware helper. |
| Medium | Icon-only controls lacked assistive names/state | `aria-label`, `aria-expanded`, and `aria-pressed` plus regression tests. |
| Medium | CI/release omitted security/control regressions | Gates are now required in CI, release, and the Windows workflow contract. |
| Medium | Shell command substitution was unsafe for the deterministic auditor | Added `node scripts/tests/workflow-contract.test.mjs`, a literal-argument check for hosted runners and manual real-game acceptance. |

## Delivered

- Upgraded Vite and the Svelte Vite plugin; refreshed `package-lock.json`.
- Added `npm run test:security` and ran it in CI and release verification. Local result: `found 0 vulnerabilities`.
- Disabled implicit package-manager caching in the release setup-node step.
- Added static regression coverage for icon-only controls and wired it into lifecycle, CI, and release checks.
- Added accessible names/state to titlebar, audio, panel, and menu controls; decorative SVGs are hidden from assistive technology.
- Made the cached media manifest replacement atomic.
- Added media manifest URL validation: production assets must be HTTPS files under the official raw GitHub repository; loopback HTTP is retained only for local/integration fixtures.
- Hardened downloader redirect handling:
  - official GitHub/API/raw/release-asset hosts are allowlisted;
  - generic downloads stay HTTPS and reject downgrade redirects;
  - loopback HTTP remains available for tests;
  - content-length and final-response host checks remain bounded.
- Reused the hardened GitHub client for release metadata, patch-version notes, media synchronization, launcher release notes, and checksum retrieval.
- Removed shell-backed process spawning from the frontend/Tauri wrapper scripts; Node now launches the package entrypoints directly (with a fixed Windows npm fallback).
- Preserved SHA-256 verification, expected-size checks, resumable-download validators, bounded response bodies, archive validation, and local-only diagnostics behavior.
- Extended the Windows workflow contract to require dependency auditing and frontend-control regressions.

## Verification

Passing locally:

```text
npm run test:security
npm run check                 # svelte-check: 0 errors, 0 warnings
npm run build
npm run test:lifecycle
node scripts/tests/workflow-contract.test.mjs
npm run test:patch-status
npm run test:version
npm run test:tray
(cd src-tauri && cargo fmt --all -- --check)
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git diff --check
node scripts/tests/workflow-contract.test.mjs
test -s docs/launcher-optimization-audit.md
grep -q Baseline docs/launcher-optimization-audit.md
grep -q Threshold docs/launcher-optimization-audit.md
grep -q Findings docs/launcher-optimization-audit.md
grep -q Residual docs/launcher-optimization-audit.md
grep -q GitHub-hosted docs/launcher-optimization-audit.md
```

The Rust test matrix passed, including 126 library tests and the 11-test downloader integration suite. Fresh primary LSP diagnostics for `src/` and the changed Rust files are clean.

## Residual review items

- The project-wide auxiliary scan still reports pre-existing Rust `unwrap`/`unsafe` usage, small duplicate blocks, and complexity hotspots. Rust formatting, tests, Clippy with `-D warnings`, and fresh primary LSP checks pass; these findings are a separate refactor queue rather than silently claimed fixes.
- Knip cannot infer the package entrypoints launched by the Node wrappers and reports `@tauri-apps/cli` and `svelte-check` as unused. Both are required by `npm run tauri`/the frontend gates and were smoke-tested after the spawn hardening.

## Intentional boundaries

- No installer bundle was added; the existing portable release contract is preserved.
- No speculative cache, telemetry, or lifecycle abstraction was introduced.
- Windows real-game/UAC/WebView2/resource acceptance still requires the manual runner and a real compatible game installation. `pwsh` was not installed in this Linux worktree, so that Windows-only script was not run locally.
- Resource benchmarks on a Windows release build remain a release-operator task; the README continues to avoid unmeasured performance claims.

## Follow-up gate

Before publishing a release, run `scripts/acceptance/run-windows-real-acceptance.ps1` and the resource gate on a Windows machine with the real game and WebView2, then inspect the generated local evidence.
