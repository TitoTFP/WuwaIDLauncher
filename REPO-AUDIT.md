# WuwaID Launcher Repository Audit

Audit date: 2026-08-23
Baseline: `main` at `054587c` (`feat: coordinate tray pause state`)
Scope: frontend Svelte/Vite, Tauri/Rust backend, filesystem and update flows, CI/release, dependencies, tests, and documentation consistency.

The audit findings are being remediated in the current worktree. The pre-existing worktree change to `.gitignore` is recorded separately under `WRK-001`.

## Severity

Critical means a direct compromise or unrecoverable data loss is likely without a realistic prerequisite.

High means release integrity, code execution, launcher availability, or patch integrity can fail under a realistic failure or compromise path.

Medium means a meaningful correctness, safety, or operational risk exists, but scope or prerequisites limit impact.

Low means a bounded defect, stale state, documentation mismatch, or hardening gap.

Each finding has a completion criterion. A finding is complete only when the criterion is observable in code or in an automated test.

## Findings

### High

#### SEC-001: Frontend-controlled self-update can install an arbitrary executable

Evidence: `src-tauri/src/lib.rs:1083-1120` accepts `version`, `zip_url`, and `checksums_url` from IPC. `src-tauri/src/engine/updater.rs:36-42` only checks that each URL starts with HTTPS. `src-tauri/permissions/app-commands.toml:12-18` exposes the command to the webview.

Impact: A compromised or injected UI can select an attacker-controlled ZIP and checksum manifest. The checksum then authenticates the attacker's own file rather than an independently trusted release.

Completion criterion: The backend derives asset URLs from a fixed repository and release tag, rejects unexpected hosts and redirects, and verifies a backend-controlled signed or otherwise independently authenticated manifest. Add an IPC test proving arbitrary hosts are rejected.

#### UPD-001: Self-update replacement is non-atomic and has no rollback

Evidence: `src-tauri/src/engine/updater.rs:96-123` generates a handoff script using `copy /Y` directly over the running executable. `src-tauri/src/lib.rs:1223-1239` launches the handoff without post-copy verification or recovery. `src-tauri/tests/milestone5_contract_tests.rs:121-133` explicitly asserts that rollback steps are absent.

Impact: Disk-full, interruption, permission failure, or partial copy can leave the launcher missing or invalid. The documented recovery guarantee is not implemented.

Completion criterion: Stage the replacement, retain a verified backup, replace atomically where Windows permits, verify the new executable before cleanup, and restore the backup on failure. Add tests for copy failure and recovery.

#### RES-001: Downloads and ZIP extraction have no effective resource limits

Evidence: `src-tauri/src/engine/downloader.rs:113-236` trusts server `Content-Length` and streams without an absolute maximum. `src-tauri/src/lib.rs:1140-1153` reads the ZIP and checksum response without a bounded checksum read. `src-tauri/src/engine/updater.rs:270-320` extracts without compressed-size, expanded-size, entry-count, or per-file limits.

Impact: A faulty or malicious server can exhaust disk or memory. The risk applies to patch, media, and launcher update flows.

Completion criterion: Enforce maximum response, download, ZIP compressed, ZIP expanded, entry-count, and per-file limits during streaming and extraction. Add oversized-response and decompression-bomb tests.

#### OPS-001: Destructive asynchronous operations are not serialized

Evidence: `src-tauri/src/lib.rs:878-984,1095-1123,1359-1371,1403-1638` spawns media sync, update, cache reset, and installation tasks without an operation lock. `src-tauri/src/engine/downloader.rs:166-169` derives a fixed temporary path. `src-tauri/src/lib.rs:1830-1892` does not enforce a backend operation state for method switch or uninstall.

Impact: Concurrent calls can share `update.zip`, `.staging`, cache files, temporary downloads, and `versions.json`. Installation, switch, or uninstall can also race with a running game because the frontend is the only guard.

Completion criterion: Add a backend operation coordinator that rejects or queues conflicting operations, blocks patch mutation while the game is running, and uses operation-specific temporary paths. Add concurrent IPC tests.

#### UI-001: Installation can be double-submitted before the UI lock is set

Evidence: `src/components/RightPanel.svelte:179-231` awaits `checkGameFolderWriteAccess` before setting `appState.installing`. `src/components/TopBar.svelte:9-34` and the uninstall flow have no independent in-flight operation guard.

Impact: Rapid clicks or repeated IPC calls can start multiple transactions against the same patch artifacts.

Completion criterion: Set an operation token before the first await, disable every conflicting action, and make the backend coordinator authoritative. Add a browser or bridge test for rapid repeated clicks.

#### UI-002: Close remains enabled during mutation

Evidence: `src/components/TopBar.svelte:110-114` always exposes the close button. `src/styles/styles-base.css:110-119` explicitly excludes `#btnClose` from readonly pointer blocking. `src-tauri/src/lib.rs:317-320` exits immediately.

Impact: Closing during installation or update can terminate an async operation between file writes, leaving partial temporary or patch state.

Completion criterion: Disable close during mutation or show a confirmation that waits for cancellation/drain. Add a test covering close during download and transaction commit.

#### REL-001: Manual release dispatch can build the wrong revision

Evidence: `.github/workflows/release.yml:7-12` accepts a tag input, but `.github/workflows/release.yml:25-26` uses default checkout behavior. The input is only validated and copied to release metadata at `47-63`.

Impact: A manual run can build the selected branch or default branch while publishing under a different tag.

Completion criterion: Checkout the exact validated tag, verify `HEAD` matches the tag commit, and fail before build if it does not.

#### REL-002: Release artifact contract omits the documented standalone EXE

Evidence: `.github/workflows/release.yml:97-105` creates the EXE and ZIP, but `117-119` uploads only the ZIP and `SHA256sums.txt`. `README.md:91-92` tells users to download `WuwaIDLauncher.exe` directly.

Impact: The documented portable download is absent from the GitHub release assets.

Completion criterion: Upload the standalone EXE and include it in the checksum manifest, or change all user-facing documentation to require the ZIP.

#### REL-003: Release acceptance gates are not wired into CI/CD

Evidence: `scripts/acceptance/windows-release-gate.ps1:227-383,396-400` checks versions, icons, checksums, ZIP contents, cleanup, and command gates. Neither `.github/workflows/ci.yml:43-76` nor `.github/workflows/release.yml:76-119` invokes it.

Impact: The claimed release gates do not block publication. Version drift, missing icons, unsafe ZIP contents, and checksum errors can reach release handling.

Completion criterion: Run the gate in the release workflow after packaging and fail publication on a non-PASS result. Keep the artifact evidence as a workflow artifact.

#### SEC-006: Release job combines repository write permission with mutable actions

Evidence: `.github/workflows/release.yml:14-15` grants `contents: write` to the whole build job. The same job runs checkout, Node setup, Rust setup, dependency installation, and `softprops/action-gh-release@v2` at `25-45,108-109`; CI also uses mutable action tags at `.github/workflows/ci.yml:24-40,70-72`.

Impact: A compromised action or dependency step runs in a job that can publish releases or modify repository contents.

Completion criterion: Pin third-party actions and toolchains to reviewed commit SHAs, separate read-only build from write-only publication, and grant write permission only to the final publishing step or job.

### Medium

#### UI-003: Patch status events can apply to the wrong selection

Evidence: Backend status payloads include `gamePath` and `installMethod` at `src-tauri/src/lib.rs:1328-1335`, but `src/lib/launcherState.svelte.ts:167-184` ignores both fields.

Impact: A slow response for path or method A can overwrite state after the user selects B, causing an incorrect status, launch decision, or install prompt.

Completion criterion: Associate every request with a monotonically increasing token or compare payload identity with the current selection before applying it. Add an out-of-order response test.

#### UI-004: Failed settings saves can roll back newer input

Evidence: `src/lib/launcherState.svelte.ts:341-344` writes without serialization. `src/components/AudioPlayer.svelte:151-167` and `src/components/SettingsPanel.svelte:51-70` capture a local previous value and restore it after an asynchronous save failure.

Impact: A failed older save can overwrite a newer volume or boolean choice in memory and on the next save.

Completion criterion: Serialize settings writes or use revision numbers; only roll back when the failed write still owns the current revision. Add overlapping save tests.

#### UI-005: Method-switch failure can desynchronize disk and memory

Evidence: `src/components/TopBar.svelte:16-33` and `src/components/SettingsPanel.svelte:27-48` perform backend cleanup and then persist settings. Failure restores only the in-memory method. Backend metadata and artifacts were already changed.

Impact: The UI can show the old method while disk metadata and patch artifacts represent the new method or an uninstalled state.

Completion criterion: Treat cleanup, metadata, and settings as one transaction, or restore both backend state and persisted settings on every failure. Add failure-injection tests.

#### UI-006: Canceling folder selection can persist autodetected path

Evidence: `src-tauri/src/lib.rs:510-522` returns autodetected path when no folder is selected. `src/components/RightPanel.svelte:47-66` and `src/components/SettingsPanel.svelte:72-95` treat any non-empty result as an explicit selection.

Impact: Cancel can unexpectedly replace a configured game path.

Completion criterion: Return an explicit cancelled result from the backend and leave state unchanged on cancel. Add a cancel-path test.

#### UI-007: Settings and About pages are unreachable

Evidence: `src/components/SettingsPanel.svelte` and `src/components/AboutPanel.svelte` exist, but `src/App.svelte:3-12,30-57` does not import or render them. `src/lib/types.ts:197-199` restricts `page` to `"home"`.

Impact: Users cannot reach settings such as auto-update, BGM, or the About screen despite the components and README describing them.

Completion criterion: Add explicit navigation and conditional rendering for the supported pages, or remove the unreachable components and documentation.

#### SEC-002: Privileged WebView has no CSP and relies on regex HTML sanitization

Evidence: `src-tauri/tauri.conf.json:25-27` sets `csp` to `null`. `src/components/SidePanel.svelte:39-46` and `src/components/PatchNotesModal.svelte:36-38` render `{@html}`. Sanitization is implemented by `src/lib/sanitize.ts:1-36`.

Impact: Current sinks pass through a sanitizer, but any parser or sanitizer bypass would execute in a privileged webview with destructive IPC commands available. This is a defense-in-depth finding, not a demonstrated exploit in the current payload.

Completion criterion: Configure a restrictive CSP, use a parser-based allowlist sanitizer, validate every remote release-note path, and add XSS regression tests for tags, attributes, URLs, and encoded payloads.

#### SEC-003: Loader readiness does not prove loader provenance after installation

Evidence: The download is hash checked before deployment at `src-tauri/src/lib.rs:1519-1575`, but `src-tauri/src/engine/installer.rs:219-234` later accepts any non-empty `winhttp.dll`.

Impact: A replaced DLL is reported as ready and may be loaded by the game.

Completion criterion: Persist the expected hash or signature and require it during status validation and launch preflight. Add a loader-tampering test.

#### SEC-004: Process detection and force quit are name-based and overly broad

Evidence: `src-tauri/src/engine/runtime.rs:672-710` selects the first matching basename. `725-753` invokes `taskkill /F /IM` for every matching name.

Impact: Multiple game instances, another user's process, or a spoofed executable can be misidentified or terminated.

Completion criterion: Track the PID returned by launch, verify its executable path, and terminate only that PID. Use explicit path checks for external detection where possible.

#### SEC-005: Privileged utilities are resolved through PATH

Evidence: `src-tauri/src/lib.rs:848-850,1227-1229` launches bare `cmd`; `src-tauri/src/engine/runtime.rs:736-739` launches bare `taskkill`.

Impact: A user-controlled executable in the Windows search path can be run, especially when the launcher itself is elevated.

Completion criterion: Use Windows APIs or fully qualified system paths and a sanitized environment. Add a Windows PATH-hijack regression test.

#### DATA-001: Installation metadata is global instead of keyed by game path

Evidence: `src-tauri/src/lib.rs:690-713` reads `_vhVersion` from one global `versions.json`; installation writes it at `1598-1627`; uninstall removes the whole file at `1873-1892`.

Impact: Switching between game folders can report a version belonging to another folder. Uninstalling one folder removes unrelated cached metadata.

Completion criterion: Store metadata under a normalized game-path key or per-install file and add a two-game test.

#### DATA-002: Metadata writes are not part of the filesystem transaction

Evidence: `src-tauri/src/lib.rs:1588-1627` deploys patch files before writing metadata. `991-1055` and `1842-1889` also read/write/remove the same file for unrelated concerns.

Impact: Metadata failure after deployment reports an error while files remain installed; release-note cache can also be discarded by a later installation.

Completion criterion: Merge unknown metadata keys, write atomically, and commit metadata together with an explicit install-state transaction. Add failure-injection tests.

#### PATH-001: Game validation accepts a directory named as the executable

Evidence: `src-tauri/src/engine/path.rs:23-36` checks `direct_exe.exists()` rather than `is_file()`. Install preflight then accepts the normalized path at `src-tauri/src/engine/installer.rs:369-383`.

Impact: Installation can write patch artifacts into a fake game tree and only fail later during launch.

Completion criterion: Require a regular executable file and canonicalize the selected game root. Add a directory-as-executable test.

#### DATA-003: Unreadable files are treated as absent during rollback

Evidence: `src-tauri/src/engine/installer.rs:423-430` converts `fs::read` errors to `None`; `447-453` then removes the target without restoring its original bytes.

Impact: Permission or sharing errors can turn a failed cleanup into data loss.

Completion criterion: Distinguish absent, readable, and unreadable states; abort destructive work when an existing target cannot be snapshotted. Add an unreadable-file rollback test.

#### SAFE-001: Canonical cleanup has no ownership proof

Evidence: `src-tauri/src/engine/installer.rs:555-588` deletes canonical PAK, DLL, signature, and mount filenames regardless of marker or hash. Tests deliberately cover foreign canonical files at `src-tauri/tests/installer_safety_tests.rs:113-118`.

Impact: A third-party file sharing a canonical name can be deleted. The README explicitly documents canonical paths as launcher-owned, so this is an intentional policy risk rather than an unconfirmed implementation mismatch.

Completion criterion: Either retain and prominently document the canonical-ownership policy with an explicit confirmation, or require ownership marker/hash matching before deletion and update the tests.

#### CFG-001: Test environment overrides remain active in production

Evidence: `src-tauri/src/lib.rs:41-71` reads `WUWAID_ASSETS_URL` and `WUWAID_E2E_APPDATA` in normal builds and falls back to a relative `WuwaIDLauncher` path when app-data discovery fails.

Impact: Environment variables can redirect settings, cache, media, and update inputs to unexpected locations.

Completion criterion: Compile test overrides only in test/dev configurations and require a stable absolute production app-data path. Add release-build tests proving overrides are ignored.

#### REL-004: Cargo reproducibility is not enforced

Evidence: `Cargo.lock` is committed, but `.github/workflows/ci.yml:52-59` and `.github/workflows/release.yml:82-89` omit `--locked`. Both workflows use an unpinned `stable` toolchain.

Impact: A stale lockfile or toolchain update can silently change the dependency graph or release binary.

Completion criterion: Use `--locked`, pin the supported Rust toolchain, and fail CI when the lockfile would change.

#### REL-005: Documented Node.js minimum conflicts with dependencies

Evidence: `README.md:146` says Node.js 18+, while `package-lock.json:1430-1440` records `marked@18.0.9` requiring Node >=20.

Impact: Node 18 contributors can receive unsupported-engine failures despite following the README.

Completion criterion: Set `engines.node` in `package.json`, update README, and ensure CI uses that same declared version.

#### REL-006: Windows ZIP traversal acceptance check is incomplete

Evidence: `scripts/acceptance/windows-release-gate.ps1:346-365` splits only on `/` and checks two leading backslashes. The test generator at `scripts/acceptance/windows-release-gate.tests.ps1:88-106,210-223` does not cover `..\\escape.txt` or `\\escape.txt`.

Impact: The acceptance gate can miss Windows-style traversal entries.

Completion criterion: Normalize both separators, reject absolute and parent segments, and add backslash traversal fixtures.

#### REL-007: Acceptance tests hardcode version 2.9.0

Evidence: `scripts/acceptance/windows-release-gate.tests.ps1:66-85,197-219` creates and mutates `WuwaIDLauncher-v2.9.0.zip`, while the runner reads the package version dynamically at `scripts/acceptance/windows-release-gate.ps1:188-207`.

Impact: The acceptance suite becomes stale on the next release.

Completion criterion: Derive generated artifact names from `package.json` or pass the version into the test fixture.

### Low

#### UI-008: Final update restart event has an inconsistent payload

Evidence: Normal countdown events send an object at `src-tauri/src/lib.rs:1216-1220`, but the final event sends `()` at `1233`. The frontend always reads `p.remainingSeconds` at `src/lib/bridge.ts:167-169`.

Impact: The final event can cause a frontend `TypeError` immediately before process exit.

Completion criterion: Use one event schema for every emission or split the final event name. Add a bridge payload test.

#### UI-009: Patch version display remains stale after uninstall

Evidence: `src/lib/launcherState.svelte.ts:184` only updates `vhVersion` for truthy versions. `src/components/RightPanel.svelte:158-166,428-430` does not clear it during uninstall.

Impact: The footer can display a removed Patch ID version.

Completion criterion: Clear `vhVersion` on uninstall and when a status payload has no current version.

#### UI-010: Event listeners are not cleaned up

Evidence: `src/lib/bridge.ts:111-123,191` returns unlisteners, but `src/lib/launcherState.svelte.ts:159` discards them and `src/App.svelte:14-16` provides no cleanup.

Impact: Remounts or hot reload can accumulate duplicate event handlers and toasts.

Completion criterion: Store unlisteners and return a cleanup function from `onMount`. Add a remount test.

#### DATA-004: Offline install records version unknown

Evidence: `src-tauri/src/lib.rs:1428-1430` stores `unknown` when release lookup fails. `src-tauri/src/engine/patch_status.rs:73-82` treats `unknown` as newer than every release.

Impact: The next online check reports `needs_update` and can redownload a valid patch.

Completion criterion: Preserve the last known version when lookup fails; only mark metadata unknown when no prior version exists.

#### SEC-008: Elevation errors are discarded at the IPC boundary

Evidence: `src-tauri/src/lib.rs:1894-1897` ignores the result of `engine::elevation::restart_as_admin()` and exposes a void command.

Impact: UAC cancellation and restart failure are indistinguishable from success.

Completion criterion: Return `Result<(), String>` through IPC and show a user-visible failure state.

#### DOC-001: README contains stale or non-portable build references

Evidence: `README.md:8` advertises Tauri 2.1 while the lockfile resolves newer Tauri packages; `README.md:181` hardcodes a user-specific Cargo path; `README.md:236` places `tauri.conf.json` at the wrong tree level.

Impact: Contributors and release operators can follow commands or paths that do not match the repository.

Completion criterion: Replace environment-specific commands with portable commands and derive version/tree references from the repository layout.

#### DOC-002: Release notes overstate verification

Evidence: `.github/release-notes/v2.9.0.md:20-26` claims format check and manual game testing completed, while README states real Windows game, UAC, tray, offline, and restart acceptance remains partial/manual.

Impact: Release consumers cannot distinguish automated evidence from manual evidence.

Completion criterion: Separate automated, Windows-manual, and game-real acceptance results and link each claim to an artifact or test command.

## Remediation Status

The findings above preserve the baseline evidence. The current worktree now contains the following remediation:

- **Completed High code paths:** `SEC-001`, `UPD-001`, `RES-001`, `OPS-001`, `UI-001`, `UI-002`, `REL-001`, `REL-002`, `REL-003`, and `SEC-006`. Windows handoff execution remains a Windows-only acceptance gap.
- **Completed Medium code paths:** `UI-003` through `UI-007`, `SEC-003` through `SEC-005`, `DATA-001` through `DATA-003`, `PATH-001`, `CFG-001`, and `REL-004` through `REL-007`.
- **Completed Low/documentation:** `UI-008` through `UI-010`, `DATA-004`, `SEC-008`, `DOC-001`, and `DOC-002`.
- **Implemented with a remaining browser-test gap:** `SEC-002` now has a restrictive CSP and DOM-based release-note allowlist sanitizer; browser XSS/CSP regression tests are still absent.
- **Intentional policy:** `SAFE-001` retains canonical-path ownership as the documented legacy cleanup policy. Files outside canonical launcher paths remain untouched.

The official release contract is now a versioned ZIP plus `SHA256sums.txt`; the executable remains inside the ZIP and is not published as a standalone release asset.

## Verification

Passed locally during remediation:

`npm run check` completed with 0 errors and 0 warnings.

`npm run build` completed successfully.

`cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1` passed 117 tests: 75 library tests and 42 integration/contract tests.

`cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets` passed.

`cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` passed.

`cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` passed.

`cargo check --locked --target x86_64-pc-windows-gnu --manifest-path src-tauri/Cargo.toml` passed.

`npm audit --omit=dev` reported 0 vulnerabilities.

`npm run tauri -- build --no-bundle` produced a Linux release binary successfully.

`npm ci --ignore-scripts --dry-run` completed successfully against the committed lockfile.

`git diff --check` passed.

## Verification Gaps

`cargo audit` was unavailable because the subcommand is not installed. Dependency advisories are therefore not fully assessed.

PowerShell acceptance tests were not run because `pwsh` is unavailable in the Linux environment.

No frontend test script, browser test, or bridge contract test exists in `package.json`; the DOM sanitizer and CSP therefore still lack browser-level regression coverage.

No automated frontend/browser test covers double-submit, stale status events, close during mutation, folder-dialog cancellation, settings write races, sanitizer bypasses, or CSP behavior.

No automated test covers update redirects, oversized responses, ZIP bombs, Windows copy failure/rollback, loader tampering, full executable-path PID targeting, UAC, PATH hijacking, or Windows ACL behavior. Linux tests cover canonical URL rejection, directory-as-executable, unreadable rollback snapshots, and multi-game metadata isolation.

CI and release workflows now run the locked Cargo checks and `cargo fmt --check`; workflow syntax/security linting was not run locally.

Workflow security linting with `actionlint` or `zizmor` was not available.

## Original Remediation Order

1. Secure self-update: fixed asset provenance, redirect policy, resource limits, atomic replacement, rollback, and update tests. Completion criterion: a malformed, oversized, redirected, or failed update cannot replace or corrupt the current launcher.
2. Serialize mutations: backend operation coordinator, game-running guard, frontend operation token, and close behavior. Completion criterion: concurrent IPC calls either queue or fail safely, and no mutation runs after close begins.
3. Harden release automation: checkout exact tag, upload documented artifacts, run release gate, pin actions/toolchain, and enforce lockfile/version consistency. Completion criterion: a release job proves source revision, metadata versions, ZIP contents, checksums, and cleanup before publication.
4. Repair runtime/data invariants: per-path metadata, loader hash validation, PID targeting, regular-file path checks, and safe rollback snapshots. Completion criterion: tampered, missing, unreadable, or multi-install fixtures produce deterministic safe failures.
5. Repair UI/documentation and add frontend tests: make Settings/About reachable, fix event identity and cleanup, configure CSP, align Node/version documentation, and separate automated from manual claims. Completion criterion: each remaining UI and documentation contract has an automated check or an explicit manual acceptance record.

## Worktree Note

#### WRK-001: `.gitignore` currently ignores project documentation

Evidence: The worktree has a pre-existing uncommitted change at `.gitignore:26-29` adding `AGENTS.md` and `docs/` to ignore rules. Remediation files are also currently uncommitted, so ignored ADR files remain hidden from normal staging.

Impact: ADRs, specifications, and agent instructions under those paths are hidden from normal staging. This may be intentional local-agent hygiene, but it prevents those documents from being committed if it is not intentional.

Completion criterion: Keep these rules only if the files are explicitly local; otherwise move agent-only ignores to `.git/info/exclude` and restore project documentation tracking.
