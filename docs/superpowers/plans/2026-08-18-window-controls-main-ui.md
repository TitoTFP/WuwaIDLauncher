# Window Controls and Main UI Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Tauri launcher shell match `main` and make minimize/close/update interactions work in a release binary.

**Architecture:** Keep `LauncherState` as the Svelte source of truth and keep all privileged work behind the existing Tauri bridge. Port the `main` DOM into focused Svelte components and add only the missing typed adapters and ACL entries.

**Tech Stack:** Svelte 5, TypeScript, Tauri 2, Rust, PowerShell contract tests, Cargo test, Vite.

---

### Task 1: Add a failing window-command ACL contract

**Files:**
- Create: `scripts/acceptance/wut-main-ui-acl.tests.ps1`
- Modify: none in production

- [ ] **Step 1: Write the failing test**

Read `src-tauri/permissions/app-commands.toml` and
`src-tauri/gen/schemas/acl-manifests.json`; require exactly
`minimize_window` and `close_window` in both command lists and fail with the
missing identifier.

- [ ] **Step 2: Run the test and verify RED**

Run `pwsh -NoProfile -File scripts/acceptance/wut-main-ui-acl.tests.ps1`.
Expected: non-zero exit with `minimize_window` or `close_window` missing.

### Task 2: Make the window command contract green

**Files:**
- Modify: `src-tauri/permissions/app-commands.toml`
- Modify: `src-tauri/src/lib.rs`
- Regenerate: `src-tauri/gen/schemas/acl-manifests.json`

- [ ] **Step 1: Add the two exact ACL identifiers**

Add both command names to `app-commands` and no other permission set.

- [ ] **Step 2: Align minimize behavior with `main`**

Change `minimize_window` to call `window.hide()` when
`engine::runtime::is_game_running()` is true and `window.minimize()` otherwise.
Keep `close_window` signature restoration and game-running hide behavior.

- [ ] **Step 3: Regenerate and run the contract**

Run `npm run tauri -- build` to regenerate the manifest, then run the PowerShell
contract. Expected: `PASS: main UI window ACL contract`.

### Task 3: Add typed page/toast state before UI changes

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/launcherState.svelte.ts`
- Create: `src/components/ToastHost.svelte`
- Modify: `src/App.svelte`

- [ ] **Step 1: Add a failing Svelte contract fixture**

Add a source contract script that asserts `PageId` contains `performance`,
`App.svelte` renders `ToastHost`, and `UpdateModal` uses the direct release
version binding. Run it before the changes and confirm it fails.

- [ ] **Step 2: Add the minimal state transitions**

Add a typed toast queue with `{message, kind}` entries, a dismiss action, and a
single update-check error path that clears update-modal state before enqueueing
the error. Keep backend diagnostics intact for toast text and logs.

- [ ] **Step 3: Implement `ToastHost` using existing `main` CSS**

Render `#toasts` and `.toast` elements with `ok`, `err`, and `info` classes;
remove each entry after its timeout and allow manual dismissal.

- [ ] **Step 4: Run `npm run check` and the source contract**

Expected: Svelte check has zero errors/warnings and the contract passes.

### Task 4: Port the `main` top navigation and update modal

**Files:**
- Modify: `src/components/TopBar.svelte`
- Modify: `src/components/UpdateModal.svelte`
- Modify: `src/components/RightPanel.svelte`
- Modify: `src/lib/launcherState.svelte.ts`

- [ ] **Step 1: Replace the top-nav items**

Render only `HOME`, `PERFORMA`, and `METODE`, with the same IDs/classes and
active behavior as `main`. Keep minimize and close IDs unchanged.

- [ ] **Step 2: Match update modal content/lifecycle**

Render `{version}` directly, keep `Nanti` and `Perbarui sekarang`, show the
progress/restart states from the existing event bridge, and route errors to
the toast host rather than a persistent right-panel card.

- [ ] **Step 3: Remove persistent update/error cards from the home view**

Keep progress data bindings available for operations, but use the `main`
visibility rules so a failed automatic launcher update does not occupy the
home layout indefinitely.

- [ ] **Step 4: Run `npm run check` and inspect the rendered DOM**

Expected: no extra Settings/Logs/About nav buttons, modal IDs match `main`, and
the update failure path produces `#toasts` output.

### Task 5: Port the Performa page and backend bridge contract

**Files:**
- Create: `src/components/PerformancePanel.svelte`
- Modify: `src/App.svelte`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/bridge.ts`
- Modify: `src-tauri/permissions/app-commands.toml`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/engine/performance.rs`
- Modify: `src-tauri/src/engine/mod.rs`

- [ ] **Step 1: Add a failing Rust INI test**

Test that a managed `r.Shadow.MaxResolution` assignment is replaced while an
unrelated `Custom.UserSetting=keep` line remains unchanged. Run the targeted
Cargo test and confirm failure because the performance module does not exist.

- [ ] **Step 2: Implement the bounded performance engine**

Add typed settings parsing, managed-key filtering, backup-once apply, clear,
active detection, valid-path checks, and game-running rejection. Use the exact
keys/values documented in `docs/superpowers/specs/2026-08-18-performance-config-design.md`.

- [ ] **Step 3: Add Tauri commands and ACL entries**

Expose `get_performance_config_active`, `apply_performance_config`, and
`clear_performance_config` through the existing bridge and app permission.

- [ ] **Step 4: Port the exact `main` panel markup**

Copy the headings, fourteen toggle IDs, footer note, and Apply/Clear controls
from `main:Resources/Web/index.html`; bind launcher visual mode to existing
settings and bind game toggles to the typed performance config.

- [ ] **Step 5: Run targeted Cargo tests and `npm run check`**

Expected: performance unit tests, Svelte check, and ACL contract pass.

### Task 6: Release verification and handoff

**Files:**
- Modify: `scripts/acceptance/windows-release-gate.ps1` only if the new contract
  needs to be included
- Create: `scripts/acceptance/wut-main-ui.tests.ps1`

- [ ] **Step 1: Run the complete Rust suite**

Run `cargo test --manifest-path src-tauri/Cargo.toml`.

- [ ] **Step 2: Run frontend and source contracts**

Run `npm run check`, `wut-main-ui-acl.tests.ps1`, and
`wut-main-ui.tests.ps1`.

- [ ] **Step 3: Build a release binary**

Run `npm run tauri -- build`; record the executable, MSI, and NSIS paths and
SHA-256 values without committing them.

- [ ] **Step 4: Perform the manual UI smoke check**

Start the release executable, click close and minimize, confirm no ACL error,
open Performa, open the update modal, dismiss it, and confirm an unavailable
launcher ZIP creates a toast rather than a persistent `Update gagal` card.

- [ ] **Step 5: Run `git diff --check` and report exact evidence**

Do not claim completion unless all tests/build/manual observations are recorded.
