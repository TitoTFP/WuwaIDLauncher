# WUT-38 Launch Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make release game launch elevation-aware and produce actionable evidence for spawn failures, immediate exits, and later process exits.

**Architecture:** Keep `launch_game` as the Tauri boundary. Move process-mode, UAC fallback, output capture, exit status, and bounded log-tail helpers into `src-tauri/src/engine/runtime.rs`; keep lifecycle cleanup and UI events in `src-tauri/src/lib.rs`. Persist one JSON evidence record per attempt under launcher `Diagnostics`.

**Tech Stack:** Rust 2021, Tauri 2, Tokio, Windows ShellExecuteExW, serde JSON, existing Rust unit/integration tests, Svelte status UI.

---

### Task 1: Lock the launch evidence contract with failing tests

**Files:**
- Modify: `src-tauri/src/engine/runtime.rs`
- Test: `src-tauri/src/engine/runtime.rs`

- [ ] **Step 1: Add tests for the required pure behaviors**

Add tests for:

```rust
assert_eq!(classify_spawn_error(Some(740)), SpawnFailureKind::ElevationRequired);
assert_eq!(classify_spawn_error(Some(1223)), SpawnFailureKind::ElevationCancelled);
assert_eq!(classify_spawn_error(Some(2)), SpawnFailureKind::SpawnFailed);

let command = LaunchCommand::new(exe, work_dir, true);
assert_eq!(command.arguments, vec!["-dx11"]);

let evidence = LaunchEvidence::for_failure(command, SpawnFailureKind::ElevationRequired, None);
let text = evidence.user_message();
assert!(text.contains("elevation_required"));
assert!(text.contains("pid=none"));
```

- [ ] **Step 2: Run the focused Rust test and verify the expected RED failure**

Run:

```powershell
$env:Path = 'C:\Users\Gipar\.cargo\bin;' + $env:Path
cargo test --manifest-path src-tauri/Cargo.toml runtime::tests -- --nocapture
```

Expected: compilation/test failure because the launch command, failure kind,
and evidence types do not yet exist.

### Task 2: Implement runtime process and evidence primitives

**Files:**
- Modify: `src-tauri/src/engine/runtime.rs`

- [ ] **Step 1: Add command, failure, and evidence types**

Implement `LaunchCommand`, `SpawnFailureKind`, `LaunchMode`, `LaunchEvidence`,
and `LaunchFailure` with deterministic formatting. Keep user-facing detail
bounded and include `pid`, `exit_code`, `stderr`, `stdout`, `game_log_tail`, and
`evidence_path` fields even when values are `none`.

- [ ] **Step 2: Add bounded output and game-log tail helpers**

Read only the final 8 KiB of captured output and the newest relevant files from
`Client/Saved/Logs`, `game.log`, and `wuwaid_loader_log.txt`. Return explicit
`none` text when no data exists.

- [ ] **Step 3: Add the direct child process wrapper**

Spawn with piped stdout/stderr, drain both streams on reader threads, expose the
child PID, wait for exit, and return the exit code plus bounded output tails.

- [ ] **Step 4: Add the Windows `runas` fallback**

When direct spawn returns OS error 740, call `ShellExecuteExW` with the `runas`
verb, `SEE_MASK_NOCLOSEPROCESS`, the game executable, working directory, and
optional `-dx11` arguments. Map a cancelled UAC prompt to
`SpawnFailureKind::ElevationCancelled`; close the process handle after waiting.
Keep non-Windows behavior on the direct path.

- [ ] **Step 5: Run the focused tests and verify GREEN**

Run:

```powershell
$env:Path = 'C:\Users\Gipar\.cargo\bin;' + $env:Path
cargo test --manifest-path src-tauri/Cargo.toml runtime::tests -- --nocapture
```

Expected: all runtime tests pass.

### Task 3: Persist launch evidence and reconcile lifecycle in the Tauri command

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add an evidence writer in the launch command module**

Create `Diagnostics` under `get_appdata_dir()`, serialize one JSON file per
attempt, and return its path in every failure message.

- [ ] **Step 2: Add a single cleanup helper**

Centralize signature restoration, coordinator reset, `onGameLaunchFinished`,
runtime inactive emission, and window show/focus so every early-return branch
uses the same cleanup behavior.

- [ ] **Step 3: Replace immediate active-state emission with detection polling**

After spawn, poll for the launched PID for a bounded interval. If the process
exits before detection, write evidence with the exit code and emit
`onLaunchError` with non-empty details. Only then hide the launcher and emit the
active runtime state.

- [ ] **Step 4: Record the monitor result**

Wait for the managed process, collect exit code/output/log tail, write the
completed evidence record, restore signature state, and restore the launcher
window. Preserve normal completion behavior while reporting non-zero early
termination with its evidence path.

- [ ] **Step 5: Run the existing core command tests**

Run:

```powershell
$env:Path = 'C:\Users\Gipar\.cargo\bin;' + $env:Path
cargo test --manifest-path src-tauri/Cargo.toml core_commands_run_through_mock_ipc_with_deterministic_results -- --nocapture
```

Expected: PASS; invalid preconditions must still return synchronously.

### Task 4: Add acceptance contract coverage for WUT-38 evidence

**Files:**
- Create: `scripts/acceptance/wut-38-launch-evidence.tests.ps1`

- [ ] **Step 1: Add source contract assertions**

Assert that the release source contains the UAC fallback, non-empty evidence
fields, exit-code handling, and cleanup events. Keep the script deterministic
and exit 0 only when all required markers are present.

- [ ] **Step 2: Run the contract script**

Run:

```powershell
pwsh -NoProfile -File scripts/acceptance/wut-38-launch-evidence.tests.ps1
```

Expected: `PASS: WUT-38 launch evidence contract`.

### Task 5: Verify release behavior and build artifacts

**Files:**
- Modify: `docs/superpowers/specs/2026-08-18-f8-03-launch-evidence-design.md`
- Modify: `docs/superpowers/plans/2026-08-18-f8-03-launch-evidence.md`

- [ ] **Step 1: Run the full Rust suite**

Run:

```powershell
$env:Path = 'C:\Users\Gipar\.cargo\bin;' + $env:Path
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: zero failures.

- [ ] **Step 2: Run frontend checks and release build**

Run:

```powershell
npm run check
npm run tauri -- build
```

Expected: no Svelte errors and release EXE/MSI/NSIS bundles produced.

- [ ] **Step 3: Run the WUT-38 contract and diff checks**

Run:

```powershell
pwsh -NoProfile -File scripts/acceptance/wut-38-launch-evidence.tests.ps1
git diff --check
```

Expected: contract PASS and no whitespace errors.

- [ ] **Step 4: Perform the manual release smoke test**

Launch the new release EXE against `F:\\Wuthering Waves`, click Play, approve
UAC manually if prompted, verify the game PID is detected, then close the game.
For a forced immediate-exit/spawn-failure case, verify the launcher shows a
non-empty detail and a `Diagnostics/launch-*.json` path.

- [ ] **Step 5: Inspect final status without committing**

Run `git status --short --branch` and preserve all existing user changes. Do not
commit; the repository owner will commit the result.
