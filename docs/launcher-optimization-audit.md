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

- `npm audit --audit-level=high`: zero high or critical vulnerabilities. One moderate advisory remains: `devalue <5.9.1` (GHSA-9rgm-9g3h-6x36, CVSS 5.3), a transitive dependency of `svelte@5.56.9` resolved to `devalue@5.9.0`; a fix is available via `npm audit fix`.
- `npm run check`: zero Svelte/type/lint errors and warnings; `npm run build`: successful frontend artifact.
- Rust formatting and `cargo clippy -- -D warnings`: zero failures/warnings; all Rust tests must pass.
- Manifest bodies are capped at 1 MiB; downloads are capped at 512 MiB and must match expected size plus SHA-256.
- Generic downloads must remain HTTPS; official GitHub redirects are host-allowlisted; no self-hosted workflow may remain.

## Findings closed

| Severity | Finding | Durable closure |
| --- | --- | --- |
| High | Development dependency vulnerabilities | Vite/plugin refresh and lockfile update; npm audit reports zero high or critical vulnerabilities. |
| High | Unrestricted download redirects and shell-backed wrappers | Shared GitHub/HTTPS redirect policies and direct Node entrypoint execution. |
| Medium | Media manifest replacement could delete the last good cache first | Atomic replacement through the existing platform-aware helper. |
| Medium | Icon-only controls lacked assistive names/state | `aria-label`, `aria-expanded`, and `aria-pressed` plus regression tests. |
| Medium | CI/release omitted security/control regressions | Gates are now required in CI, release, and the Windows workflow contract. |
| Medium | Shell command substitution was unsafe for the deterministic auditor | Added `node scripts/tests/workflow-contract.test.mjs`, a literal-argument check for hosted runners and manual real-game acceptance. |
| Medium | Media sync waited for the manifest signature before downloading anything | The `.sig` fetch runs in the background and is collected after the media sync, so a stalled or rejected signature can no longer delay the media download or the media status events. |
| Medium | Startup hashed the cached media twice and copied whole response bodies | The cached-media check now carries its digests forward, so the media sync reuses the verdict instead of re-hashing; theme and metadata reads navigate the document they already parsed; four full-buffer `to_vec` copies became moves. A `#[cfg(test)]` allocation probe with per-scenario budgets guards all of it. |
| Medium | The idle tick re-derived the game path from disk, and the update and install paths did their I/O twice | The monitor tick now reads a resolved game path from `RuntimeCoordinator`, retired by a settings-write counter that travels with the only two writers of `settings.json`; the launcher update carries its SHA-256 out of the download write loop and validates the archive once, inside extraction; the install transaction runs on a blocking thread. The two-second cadence, the size caps, the redirect policy, the rollback semantics and every error string are unchanged, and the allocation probe holds each path to a budget. |
| Medium | The shipped payload carried a 270,398-byte favicon the Windows build already embeds, and the derived repack cache was unbounded | `public/images/app.ico` is re-emitted as a 16/32/48 PNG-in-ICO of 10,018 bytes at the same path, taking the `dist/` tree from 554,141 to 293,761 bytes. `retain_derived_pak_cache` keeps only the PAK being installed and its `.sha256` marker, and `remove_orphaned_repack_artifacts` reclaims repack work directories and interrupted atomic-write temporaries whose owning process is provably gone or that are older than 6 hours, never one this process owns. `scripts/tests/resource-lifecycle.test.mjs` caps the `public/` tree and per-file size and fails if any file duplicates `src-tauri/icons/icon.ico`. |
| Medium | `tauri-plugin-process` was registered, granted `process:default` and linked into the release binary, but never invoked | The registration, the capability entry, the `Cargo.toml` dependency and the `gen/schemas` regenerated from it are removed; the launcher closes through `app.exit(0)` and spawns children with `std::process::Command`. |

## Delivered

- Upgraded Vite and the Svelte Vite plugin; refreshed `package-lock.json`.
- Added `npm run test:security` and ran it in CI and release verification. Local result: 0 high and 0 critical, 1 moderate (`devalue <5.9.1`); gate exits 0.
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
- Media sync no longer waits on the manifest signature: the `.sig` fetch runs in the background, the media sync and its status events finish first, and the theme is resolved from the collected signature afterwards — which also means `onThemeReady` can now arrive after `onMediaReady`.
- Removed duplicate work rather than changing cadence: the 2-second runtime monitor still ticks every 2 seconds, and every media, theme, metadata, and update-response byte is read once instead of two or three times, with an in-crate allocation probe (`perf_probe`/`perf_scenarios`) holding each path to a budget.
- Kept the cadence and cut the work per tick: `settings.json` is read and parsed once instead of thirty times a minute, a downloaded update is hashed in the write loop that produced its bytes and its archive is walked once instead of twice, and the install transaction's snapshot, copy and hash passes no longer occupy a runtime worker. The probe's re-cut budgets fail against the pre-fix shapes (42 allocations per tick against a budget of 8, 38 per archive against 21) and run in a new `ubuntu-latest` CI job beside the unchanged Windows matrix.
- Cut the shipped payload: `public/images/app.ico` was a 270,398-byte 256x256 32-bit BMP, byte-identical to `src-tauri/icons/icon.ico` — which `tauri-build` already embeds as a Win32 resource and as the default window/tray icon — and 48.8% of the whole `dist/` tree. It is now a 10,018-byte 16/32/48 PNG-in-ICO at the same path, with the `<link rel="icon" type="image/x-icon" href="/images/app.ico">` in `index.html` and every other reference unchanged. Measured `dist/` tree: 554,141 → 293,761 bytes. Measured Linux release binary, same machine and same release profile: 7,208,672 → 7,077,728 bytes, and a rebuild of the baseline commit `cc4f929` does not reproduce that 7,208,672 — it produces 7,205,088, in two separate worktrees at two different path lengths, so the −127,360 this document stands behind is 7,205,088 → 7,077,728.
- Removed `tauri-plugin-process` from all four sites it occupied — the `.plugin(...)` registration in `lib.rs`, the `process:default` entry in `src-tauri/capabilities/default.json`, the `Cargo.toml` dependency, and the `gen/schemas` files `tauri-build` regenerates from the capability. Before removing it: no `plugin:process|*` invoke anywhere in the frontend, no `@tauri-apps/plugin-process` in `package.json`, and no other `tauri_plugin_process` use outside the registration. Measured Linux release binary: 7,077,728 → 7,074,656 bytes.
- Bounded the derived repack cache. `PatchVariant::cache_version()` returns `custom_uid_<sha256 of the UID>` for a custom UID, so every distinct UID a user typed named another derived PAK of up to 128 MiB plus its `.sha256` marker, and nothing ever removed the old ones; a launcher killed mid-repack left the whole unpacked PAK tree behind. `retain_derived_pak_cache` now keeps only the PAK being installed and its marker, and `remove_orphaned_repack_artifacts` reclaims crash-orphaned repack work directories and interrupted atomic-write temporaries — but only when the owning process is provably gone or the artifact is older than 6 hours, and never for the process doing the cleanup. Neither is allowed to fail an install. Measured over five repacks with four distinct custom UIDs plus hide-uid: 77,730,237 → 35,747,265 bytes (-41,982,972), with the media, theme, loader, manifest, source PAK, and the interrupted download all byte-for-byte intact.

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
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib perf_scenarios -- --test-threads=1 --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git diff --check
test -s docs/launcher-optimization-audit.md
grep -q Baseline docs/launcher-optimization-audit.md
grep -q Threshold docs/launcher-optimization-audit.md
grep -q Findings docs/launcher-optimization-audit.md
grep -q Residual docs/launcher-optimization-audit.md
grep -q GitHub-hosted docs/launcher-optimization-audit.md
```

The Rust test matrix passed 237 tests: 183 in the library target, which includes the 12 allocation-probe scenarios, and 54 across the eleven integration binaries, the largest being the 12-test downloader integration suite. Fresh primary LSP diagnostics for `src/` and the changed Rust files are clean.

## Residual review items

- The project-wide auxiliary scan still reports pre-existing Rust `unwrap`/`unsafe` usage, small duplicate blocks, and complexity hotspots. Rust formatting, tests, Clippy with `-D warnings`, and fresh primary LSP checks pass; these findings are a separate refactor queue rather than silently claimed fixes.
- Knip cannot infer the package entrypoints launched by the Node wrappers and reports `@tauri-apps/cli` and `svelte-check` as unused. Both are required by `npm run tauri`/the frontend gates and were smoke-tested after the spawn hardening.
- No logger is installed anywhere in the project: no `set_logger`, no `env_logger`, no `tauri-plugin-log`, and `main.rs` is nine lines that only call `run()`. The 43 `log::` call sites across four files — 40 in `lib.rs`, and one each in `engine/elevation.rs`, `engine/runtime.rs` and `engine/theme.rs` — therefore have their Indonesian format strings compiled into the release binary and never emitted. This was measured, not assumed: replacing all 43 with `()` in a scratch copy of `13cfca1` and rebuilding the same release profile took the binary from 7,074,656 to 7,074,272 bytes, a saving of 384 bytes, because the strings are largely literals the linker already deduplicates; and dropping the direct `log` dependency would save nothing either, since `log` is already a normal dependency of 22 runtime crates in the graph. Installing a logger is a product decision that would contradict the README's documented policy that the launcher creates no new diagnostics file, so it was deliberately not done here. The one genuinely silent failure found while checking this — `signal_launcher_update_ready` in `lib.rs` calls `std::process::exit(1)` mid-update-handoff when the readiness marker cannot be written, and the user sees no error at all — wants its own work item, not a logging subsystem.

## Intentional boundaries

- No installer bundle was added; the existing portable release contract is preserved.
- No speculative cache, telemetry, or lifecycle abstraction was introduced.
- Windows real-game/UAC/WebView2/resource acceptance still requires the manual runner and a real compatible game installation. `pwsh` was not installed in this Linux worktree, so that Windows-only script was not run locally.
- Resource benchmarks on a Windows release build remain a release-operator task; the README continues to avoid unmeasured performance claims.
- The shipped Windows `WuwaIDLauncher.exe` and `WuwaIDLauncher-vX.Y.Z.zip` were never measured. `release.yml`'s "Package ZIP distribution" step zips only the executable (`Compress-Archive -LiteralPath $exe.FullName`), so ZIP size tracks binary size, but no MSVC toolchain exists in this Linux worktree and every binary number in this document is a Linux ELF measured on this machine with the release profile. The A/B recipe, from baseline commit `cc4f929`: on a Windows machine with the MSVC toolchain, build `cc4f929` and then the WUL-2 commits, and for each compare `(Get-Item src-tauri/target/release/WuwaIDLauncher.exe).Length` and the size of the `Compress-Archive` output.
- Process liveness is answered through `/proc/<pid>`, which only Linux can do portably: `owning_process_is_gone` returns false on every other platform, and its regression test `repack_retention_reclaims_debris_of_a_finished_process` is `#[cfg(target_os = "linux")]` for that reason. So on Windows a crash-orphaned repack work directory is not reclaimed immediately — the 6-hour age rule takes over, and the directory is removed on the *next* repack, once it is older than 6 hours. That is the deliberate trade: an immediate answer on Windows would need a platform-specific handle query to protect a directory of at most 128 MiB, and guessing would risk deleting a live repack's own output.

## Follow-up gate

Before publishing a release, run `scripts/acceptance/run-windows-real-acceptance.ps1` and the resource gate on a Windows machine with the real game and WebView2, then inspect the generated local evidence.
