# Canonical Artifact Ownership Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove ownership-marker gating from all installer methods so legacy and partial canonical artifacts can be updated, switched, or uninstalled safely within the explicitly accepted path-based ownership model.

**Architecture:** Keep the existing canonical path definitions, game-path preconditions, PAK structure validation, release SHA-256 validation, staging, snapshots, and rollback. Stop writing and requiring ownership markers; cleanup targets canonical artifacts across every semver resource version, rejects reparse-point paths, and only reports `preserved` for non-file targets or real filesystem failures. Keep marker path helpers only long enough to remove markers left by older Tauri builds. Switch/uninstall commit metadata only after cleanup and restore the filesystem snapshot if that metadata commit fails.

**Tech Stack:** Rust/Tauri backend, Cargo unit and integration tests, Svelte frontend contract checks, PowerShell Windows release gate.

---

### Task 1: Add a failing regression test for marker-less legacy installation

**Files:**
- Modify: `src-tauri/tests/milestone2_contract_tests.rs`
- Test target: `install_patch_transaction` and `validate_installed_signature_bypass`

- [x] **Step 1: Write the failing test**

Add a test beside the existing transaction tests. It creates a valid source PAK, places a valid legacy PAK at `signature::get_signature_bypass_pak_path(&game)` without creating `.wuwaid-managed-signature-bypass`, then installs `SignatureBypass` and asserts the canonical target is replaced, the installation validates, and no marker exists:

```rust
#[test]
fn signature_bypass_migrates_legacy_pak_without_marker() {
    let (_temp, game) = setup_game();
    let source = game.join("new.pak");
    let legacy = signature::get_signature_bypass_pak_path(&game);
    release_like_pak(&source);
    release_like_pak(&legacy);

    installer::install_patch_transaction(
        &game,
        InstallMethod::SignatureBypass,
        &source,
        None,
    )
    .expect("canonical legacy target must be replaceable");

    assert!(installer::validate_installed_signature_bypass(&game).unwrap());
    assert!(!signature::get_signature_bypass_marker_path(&game).exists());
    assert_eq!(fs::read(&legacy).unwrap(), fs::read(&source).unwrap());
}
```

- [x] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test milestone2_contract_tests signature_bypass_migrates_legacy_pak_without_marker -- --exact --nocapture
```

Expected: FAIL with `target_conflict` because `reject_foreign_targets` currently rejects the existing PAK without a marker.

### Task 2: Remove marker requirements and marker creation from installer validation/deployment

**Files:**
- Modify: `src-tauri/src/engine/installer.rs:142-304, 367-478, 521-583, 888-991`
- Modify: `src-tauri/src/engine/patch_status.rs:34-88`
- Test: `src-tauri/tests/milestone1_contract_tests.rs`, `src-tauri/tests/app_command_integration_tests.rs`

- [x] **Step 1: Make validators use canonical files only**

Change the three validators to require only their canonical payloads:

```rust
pub fn validate_installed_signature_bypass(game_path: &Path) -> Result<bool, String> {
    let pak_path = signature_bypass_pak_path(game_path);
    Ok(pak_path.is_file() && validate_pak_file(&pak_path)?)
}
```

The loader validator must require a valid PAK and non-empty `winhttp.dll`, without reading `.wuwaid-managed-loader`. The Resource Mount validator must require PAK, SIG, and mount file, validate their hashes/content and official signature match, but must not require or parse `.wuwaid-resource-mount`.

- [x] **Step 2: Stop writing markers while preserving rollback cleanup of old markers**

Remove marker entries from new deployment payloads and remove any existing legacy marker only after the new canonical files validate. Keep marker paths in snapshots/rollback and cleanup lists so an old Tauri marker is restored on rollback and removed after a successful deployment.

- [x] **Step 3: Remove the foreign-target rejection gate**

Delete `reject_foreign_targets` and its call from `install_patch_transaction`. Existing canonical files must reach the existing transactional replacement code instead of returning `target_conflict`.

- [x] **Step 4: Update patch status artifact detection**

In `classify_installation`, determine `any_artifact` from canonical payload files only:

```rust
let any_artifact = [&plan.pak_path, &plan.sig_path, &plan.mount_path]
    .iter()
    .any(|path| path.exists());
```

Use the equivalent canonical sets for Loader and Signature Bypass. A marker by itself must not report an installed patch.

### Task 3: Make cleanup, switch, and uninstall remove canonical partial artifacts

**Files:**
- Modify: `src-tauri/src/engine/installer.rs:586-735`
- Modify: `src-tauri/src/lib.rs:1744-1798` only if the returned report contract needs wording changes
- Test: `src-tauri/tests/installer_safety_tests.rs`, `src-tauri/src/engine/installer.rs` tests

- [x] **Step 1: Add the failing partial-cleanup test**

Add a test that creates invalid/partial canonical PAK, SIG, mount, loader DLL, and old marker files, calls `remove_all_owned_artifacts`, and asserts all canonical files and old markers are gone with an empty `preserved` list. Run it before implementation and confirm it fails because current cleanup preserves unvalidated artifacts.

- [x] **Step 2: Remove canonical files unconditionally during cleanup**

For each method not selected by `keep`, call `remove_if_file` for every canonical payload and legacy marker path without first calling a marker-based validator:

```rust
for target in [
    plan.owner_marker_path,
    plan.pak_path,
    plan.sig_path,
    plan.mount_path,
] {
    remove_if_file(&target, &mut report);
}
```

Use the same rule for signature bypass (`pak` plus old marker) and loader (`pak`, DLL, plus old marker); remove stale markers even when the corresponding method is kept. Continue restoring the signature backup and reporting actual removal errors.

- [x] **Step 3: Update old foreign-artifact tests to the approved path model**

Replace assertions that a same-path artifact is preserved with assertions that it is removed/replaced. Keep tests for non-file targets and permission failures so `preserved` and `failures` still represent real filesystem conditions.

- [x] **Step 4: Run focused cleanup and transaction tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test installer_safety_tests -- --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --test milestone2_contract_tests -- --test-threads=1
```

Expected: all focused tests PASS, including legacy PAK migration and partial cleanup.

### Task 4: Align contract tests and documentation with path-based ownership

**Files:**
- Modify: `src-tauri/src/engine/installer.rs` unit tests
- Modify: `src-tauri/tests/app_command_integration_tests.rs`
- Modify: `src-tauri/tests/milestone1_contract_tests.rs`
- Modify: `src-tauri/tests/milestone2_contract_tests.rs`
- Modify: `README.md:38,61,114`

- [x] **Step 1: Assert new installs do not create ownership markers**

After every Resource Mount, Loader, and Signature Bypass deployment test, assert the corresponding old marker path does not exist. Remove test setup that writes a marker merely to make a valid install; retain one cleanup fixture containing stale markers to prove backward cleanup.

- [x] **Step 2: Replace the old conflict contract**

Change `transaction_switches_owned_artifacts_and_preserves_foreign_targets` into a canonical-target replacement test. The same-path PAK must be replaced and validated, and cleanup must remove it. Preserve tests for unrelated files outside canonical paths.

- [x] **Step 3: Update README claims**

Replace “ownership marker” with “canonical artifact paths” in the implementation matrix, describe the accepted path-based ownership trade-off, and remove the claim that foreign artifacts without owner markers are preserved. Keep the documented integrity, transaction, rollback, and permission guarantees.

- [x] **Step 4: Run all Rust tests and frontend checks**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check
```

Expected: all Rust tests pass, Clippy reports no warnings, and Svelte check reports zero errors and zero warnings.

### Task 5: Build and verify release artifacts before any tag push

**Files:**
- Verify only: `src-tauri/target/release/WuwaIDLauncher.exe`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- Evidence: local command output and artifact hash

- [x] **Step 1: Build the frontend and standalone launcher**

Run:

```powershell
npm run build
npm run tauri -- build --no-bundle
```

Expected: build succeeds and creates `src-tauri/target/release/WuwaIDLauncher.exe`.

- [x] **Step 2: Run the release gate tests**

Run the contract suite:

```powershell
pwsh -NoProfile -File scripts/acceptance/windows-release-gate.tests.ps1
```

Then run the automated artifact gate against the local release output:

```powershell
$runRoot = Join-Path $env:TEMP ("wuwaid-release-gate-" + [guid]::NewGuid().ToString("N"))
$artifactRoot = (Resolve-Path "src-tauri/target/release").Path
$outputRoot = Join-Path $runRoot "evidence"
$fixtureRoot = Join-Path $runRoot "fixtures"
pwsh -NoProfile -File scripts/acceptance/windows-release-gate.ps1 `
  -Mode automated `
  -ArtifactRoot $artifactRoot `
  -OutputRoot $outputRoot `
  -FixtureRoot $fixtureRoot
```

Expected: both commands exit 0; the gate reports version `2.8.0`, a non-empty executable, and no MSI/NSIS artifact.

- [x] **Step 3: Inspect the final diff and commit implementation**

Run `git diff --check`, inspect all changed installer/tests/docs, then commit only the implementation changes with:

```powershell
git add src-tauri/src/engine/installer.rs src-tauri/src/engine/patch_status.rs src-tauri/tests README.md
git commit -m "fix: manage patch artifacts by canonical path"
```

- [x] **Step 4: Keep release tag paused**

Verify `git ls-remote origin refs/tags/v2.8.0` returns no tag. Do not push `v2.8.0` until the user reviews the destructive path-based ownership behavior and the Windows game acceptance gate is complete.
