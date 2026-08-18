# Launcher Operations and Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove unavailable remote diagnostics, add tray/update/first-open UX, and ship a smaller binary-only professional Windows release pipeline.

**Architecture:** Keep Tauri commands as the backend boundary. Use the existing tray and notification plugin for OS-level tray feedback, emit a backend-owned twelve-second update countdown, and keep patch-note presentation in Svelte with the existing sanitized release-note payload. Separate PR CI from tag-based release publishing; both build only `WuwaIDLauncher.exe`, while release automation creates the updater ZIP and checksum manifest.

**Tech Stack:** Rust/Tauri 2, Svelte 5/TypeScript, Vite, PowerShell acceptance scripts, GitHub Actions, Cargo release profile.

---

### Task 1: Remove unavailable remote diagnostics and telemetry

**Files:**
- Modify: `src-tauri/src/engine/settings.rs`
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src/lib/bridge.ts`
- Modify: `src/lib/launcherState.svelte.ts`
- Modify: `src/lib/types.ts`
- Modify: `src/components/RightPanel.svelte`
- Modify: `src/components/SettingsPanel.svelte`
- Modify: `src/components/AboutPanel.svelte`
- Delete: `src/components/LogsPanel.svelte`
- Delete: `src-tauri/src/engine/log_collector.rs`
- Delete: `src-tauri/src/engine/telemetry.rs`
- Modify: `src-tauri/tests/milestone6_contract_tests.rs`

- [ ] **Step 1: Write the failing contract tests**

Add tests asserting that legacy `diagnosticsUploadEnabled` and
`telemetryEnabled` keys are ignored while the normalized settings no longer
contain those fields, and add a source contract assertion that the generated
command list has no `upload_logs` or `get_log_upload_enabled` registration.

- [ ] **Step 2: Run the focused tests and observe the expected failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test milestone6_contract_tests -- --test-threads=1
```

Expected: the new settings/command assertions fail because the old fields and
commands are still present.

- [ ] **Step 3: Remove the production upload/telemetry surface**

Delete the two unused engine modules, remove their command functions, event
callbacks, heartbeat task, settings fields, frontend state, bridge methods,
menu entry, privacy toggles, and unused `LogsPanel`. Keep legacy JSON keys
ignored by normalization so old installations load safely. Remove the
`multipart` reqwest feature and any now-unused dependency declarations.

- [ ] **Step 4: Run the focused tests and frontend check**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test milestone6_contract_tests -- --test-threads=1
npm run check
```

Expected: focused Rust tests pass and Svelte diagnostics report zero errors and
zero warnings.

### Task 2: Add a system-tray notification

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/tests/milestone5_contract_tests.rs`

- [ ] **Step 1: Write the failing helper test**

Add a pure test for the exact notification body:

```rust
assert_eq!(tray_notification_body(), "Launcher berjalan di system tray. Klik ikon tray untuk membukanya kembali.");
```

- [ ] **Step 2: Run the test to verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml tray_notification_body -- --exact
```

Expected: compile/test failure because the helper does not exist.

- [ ] **Step 3: Implement best-effort notification**

Initialize `tauri_plugin_notification`, import `NotificationExt`, add the pure
copy helper, and call the notification builder after hiding the window. Ignore
or log notification errors so window behavior remains reliable.

- [ ] **Step 4: Run the focused test**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml tray_notification_body -- --exact
```

Expected: PASS.

### Task 3: Make self-update restart visible and deterministic

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/bridge.ts`
- Modify: `src/lib/launcherState.svelte.ts`
- Modify: `src/lib/types.ts`
- Modify: `src/components/UpdateModal.svelte`
- Modify: `src-tauri/tests/milestone5_contract_tests.rs`

- [ ] **Step 1: Write countdown tests**

Add a Rust test asserting the restart delay constant is twelve seconds and a
frontend state contract that the restart countdown is cleared after the zero
event.

- [ ] **Step 2: Run tests to verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml launcher_update_restart -- --test-threads=1
npm run check
```

Expected: the new Rust symbol/state contract is missing.

- [ ] **Step 3: Implement backend-owned countdown**

Emit `onLauncherUpdateRestarting` with `{remainingSeconds}` every second for
12 seconds after the staged update is verified, then start the existing Windows
handoff and exit. Keep error cleanup unchanged.

- [ ] **Step 4: Implement frontend notice**

Parse the payload in `bridge.ts`, store `launcherUpdateRestartCountdown`, render
the Indonesian auto-close/reopen message and countdown in `UpdateModal`, and
disable dismissal while active.

- [ ] **Step 5: Run focused tests and check**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml launcher_update_restart -- --test-threads=1
npm run check
```

Expected: PASS with zero Svelte diagnostics.

### Task 4: Show patch notes on first open

**Files:**
- Create: `src/components/PatchNotesModal.svelte`
- Modify: `src/lib/launcherState.svelte.ts`
- Modify: `src/lib/types.ts`
- Modify: `src/App.svelte`

- [ ] **Step 1: Add the state contract**

Define `firstLaunchPatchNotes: ReleaseNotePayload | null` and
`dismissFirstLaunchPatchNotes()` in `ILauncherState`; add an exported tag-key
helper in `types.ts` so the persistence rule is deterministic.

- [ ] **Step 2: Verify the missing contract**

```powershell
npm run check
```

Expected: the new component/state references fail until the implementation is
added.

- [ ] **Step 3: Implement the modal**

Use the existing `marked` plus `sanitizeReleaseNotesHtml` path, show the first
unseen tag when `onVHReleaseNotes` arrives, store the tag in localStorage on
dismiss/continue, and leave `SidePanel` rendering unchanged.

- [ ] **Step 4: Verify frontend behavior statically**

```powershell
npm run check
npm run build
```

Expected: zero diagnostics and a successful production bundle.

### Task 5: Switch to binary-only packaging and update release gates

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `scripts/acceptance/windows-release-gate.ps1`
- Modify: `scripts/acceptance/windows-release-gate.tests.ps1`
- Modify: `docs/acceptance/windows-game-matrix.md`
- Modify: `README.md`

- [ ] **Step 1: Extend fixture tests for the binary-only contract**

Change the fixture artifact to contain `WuwaIDLauncher.exe`, a release ZIP,
and `SHA256sums.txt`; assert the gate does not require MSI or NSIS.

- [ ] **Step 2: Run the gate tests to observe RED**

```powershell
pwsh -NoProfile -File scripts/acceptance/windows-release-gate.tests.ps1
```

Expected: the old fixture/gate contract fails because it still requires MSI and
NSIS.

- [ ] **Step 3: Disable Tauri installer bundling**

Set `bundle.active` to `false` and remove Windows installer-only configuration.
Keep the canonical executable name and version fields.

- [ ] **Step 4: Update gate and docs**

Require only the executable, updater ZIP, checksum manifest, version consistency,
and safe ZIP contents. Change the command gate to run `npm run tauri -- build
--no-bundle` and document that MSI/NSIS are intentionally absent.

- [ ] **Step 5: Run acceptance tests**

```powershell
pwsh -NoProfile -File scripts/acceptance/windows-release-gate.tests.ps1
```

Expected: PASS with binary-only artifact evidence.

### Task 6: Add professional CI and CD workflows

**Files:**
- Delete: `.github/workflows/build-tauri.yml`
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Add workflow contract tests**

Extend the PowerShell acceptance test to parse both workflow files and assert
that CI has read-only permissions, release has contents-write permissions, the
release workflow is tag/manual triggered, no workflow invokes `tauri-action`,
and no workflow references MSI/NSIS.

- [ ] **Step 2: Run workflow tests to verify RED**

```powershell
pwsh -NoProfile -File scripts/acceptance/windows-release-gate.tests.ps1
```

Expected: the old workflow contract fails because the current workflow invokes
the bundling release action.

- [ ] **Step 3: Implement CI**

Use `npm ci`, Rust stable/MSVC, cached npm/Cargo dependencies, Svelte check,
frontend build, Rust all-target tests, clippy, `npm run tauri -- build
--no-bundle`, and upload only the release EXE as a short-retention artifact.
Use concurrency with cancellation for superseded branch/PR runs.

- [ ] **Step 4: Implement CD**

On `v*.*.*` tags or manual dispatch, run the same checks, build the EXE,
compress it as `WuwaIDLauncher-vX.Y.Z.zip`, generate lowercase SHA-256, and
publish a non-draft release with only ZIP/checksum assets. Use release
concurrency without cancellation.

- [ ] **Step 5: Run workflow contract tests**

```powershell
pwsh -NoProfile -File scripts/acceptance/windows-release-gate.tests.ps1
```

Expected: PASS.

### Task 7: Remove remaining dead code, fix clippy, and measure size

**Files:**
- Modify: `src-tauri/src/engine/installer.rs`
- Modify: `src-tauri/src/engine/media.rs`
- Modify: `src-tauri/src/engine/runtime.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `README.md`

- [ ] **Step 1: Capture baseline**

```powershell
Get-Item src-tauri/target/release/WuwaIDLauncher.exe | Select-Object Length
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Record the current size and each warning in evidence; do not use a warning
allow-list for code that can be fixed without changing behavior.

- [ ] **Step 2: Fix the reported clippy findings**

Use `is_empty`, `sort_by_key(Reverse(...))`, collapsed conditions, a boxed
launch error where required, an initializer for `SHELLEXECUTEINFOW`, remove the
needless return/borrows, and delete any newly unreachable helper.

- [ ] **Step 3: Rebuild and compare**

```powershell
npm run tauri -- build --no-bundle
Get-Item src-tauri/target/release/WuwaIDLauncher.exe | Select-Object Length
```

Expected: successful binary-only build; report whether the new size is smaller
than the captured baseline.

### Task 8: Full verification and handoff

**Files:**
- Modify: `docs/acceptance/windows-game-matrix.md` only if final command/evidence paths changed.

- [ ] **Step 1: Run the full automated suite**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check
npm run build
npm run tauri -- build --no-bundle
pwsh -NoProfile -File scripts/acceptance/windows-release-gate.tests.ps1
git diff --check
```

- [ ] **Step 2: Inspect artifacts and workflow diff**

Confirm `src-tauri/target/release/WuwaIDLauncher.exe` exists, no MSI/NSIS is
produced or required, the ZIP contains exactly the canonical executable, and
the working tree has no accidental generated files.

- [ ] **Step 3: Report evidence**

Report the commit/diff, test counts, binary size comparison, binary path, and
the manual Windows checks still requiring a real desktop session (toast/tray,
first-open modal, and actual self-update restart).
