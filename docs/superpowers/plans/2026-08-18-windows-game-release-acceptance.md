# Milestone 7 — Windows/Game Release Acceptance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking. Do not create a commit; the repository owner will commit after review.

**Goal:** Membuat release-gate Windows yang repeatable untuk pemeriksaan deterministik dan terhubung ke acceptance manual game nyata tanpa menyentuh instalasi game utama.

**Architecture:** PowerShell runner mengelola input artifact, fixture temporary, snapshot, automated checks, evidence, dan cleanup. Skenario yang membutuhkan executable game, UAC, tray, WebView2, atau published HTTPS release tetap berjalan sebagai manual gate, tetapi memakai fixture dan format evidence yang sama.

**Tech Stack:** PowerShell bawaan Windows, Rust/Cargo tests, Svelte/Vite checks, Tauri v2 Windows build, ZIP/SHA-256, Markdown evidence, Linear.

---

## File map

- Create: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.ps1 — runner automated/manual.
- Create: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.tests.ps1 — self-contained PowerShell assertions for runner contracts.
- Modify: E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\windows-game-matrix.md — add runner commands, input contract, and evidence rules.
- Create per run: E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\milestone-7-results-2026-08-18.md — first release-gate report; later runs use the same ISO date naming pattern.
- No launcher Rust/Svelte code is changed unless a gate produces a separately scoped defect.

## Task 1: Define and test the runner contract

**Files:**
- Create: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.tests.ps1
- Create: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.ps1

- [ ] **Step 1: Write the failing contract test**

The test script must invoke the runner with an artifact folder and temporary
output folder, then assert that the runner returns a report containing:

    $report = & $runner -Mode automated -ArtifactRoot $artifactRoot -OutputRoot $outputRoot -FixtureRoot $fixtureRoot
    if ($LASTEXITCODE -ne 0) {
        throw "Runner exited with code $LASTEXITCODE"
    }
    $result = Get-Content -Raw -LiteralPath $report | ConvertFrom-Json
    if (-not $result.runId) { throw "runId is required" }
    if (-not $result.startedAt) { throw "startedAt is required" }
    if (-not $result.scenarios) { throw "scenarios are required" }
    if (@($result.scenarios | Where-Object status -notin @("PASS", "FAIL", "BLOCKED")).Count -gt 0) {
        throw "Unknown scenario status"
    }

Run:

    pwsh -NoProfile -File E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.tests.ps1

Expected first result: FAIL because the runner and report contract do not
exist yet.

- [ ] **Step 2: Implement the minimal runner input validation**

Implement parameters Mode, ArtifactRoot, OutputRoot, FixtureRoot, and optional
GamePath. Reject missing artifact paths, a missing required executable, a
fixture path equal to the supplied game path, and unsupported mode values before
creating or mutating a fixture.

- [ ] **Step 3: Implement report creation**

Create a run ID from the current UTC timestamp and process ID. Store each
scenario as an object with name, status, startedAt, finishedAt, evidence, and
message. Write a JSON report to OutputRoot and return its absolute path on
success.

- [ ] **Step 4: Run the contract test again**

Run the same PowerShell test. Expected result: PASS with only the initial
input/report scenarios present.

## Task 2: Add the automated release artifact gate

**Files:**
- Modify: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.ps1
- Modify: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.tests.ps1
- Modify: E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\windows-game-matrix.md

- [ ] **Step 1: Add failing artifact assertions**

The test fixture must create a small checksum manifest for known temporary
files and assert that the runner reports:

    if (($result.scenarios | Where-Object name -eq "artifact-version").status -ne "PASS") {
        throw "Version gate did not pass"
    }
    if (($result.scenarios | Where-Object name -eq "artifact-checksum").status -ne "PASS") {
        throw "Checksum gate did not pass"
    }

The test must also assert that a deliberately changed checksum is reported as
FAIL and never as PASS.

- [ ] **Step 2: Implement artifact checks**

Validate that the release directory contains:

    wuwaid-launcher.exe
    WuwaIDLauncher_2.6.1_x64.zip
    WuwaIDLauncher_2.6.1_x64_en-US.msi
    WuwaIDLauncher_2.6.1_x64-setup.exe
    SHA256sums.txt

Read version 2.6.1 from package/Cargo/Tauri metadata, verify configured icon
files exist, calculate SHA-256 for each release asset, and compare it to
SHA256sums.txt. A mismatch produces FAIL and stops downstream artifact
scenarios.

- [ ] **Step 3: Add the local automated command gate**

Run these commands from the workspace root and record exit codes and output
paths:

    npm run check
    npm run build
    & "C:\Users\Gipar\.cargo\bin\cargo.exe" test --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
    $env:Path = "C:\Users\Gipar\.cargo\bin;$env:Path"
    npm run tauri -- build

The runner marks the scenario FAIL on any non-zero exit code and includes the
command output path in evidence.

- [ ] **Step 4: Verify artifact gate**

Run the PowerShell contract test against the current release folder. Expected
result: all artifact/version/checksum scenarios PASS and a tampered checksum
scenario FAIL.

## Task 3: Implement reversible fixture and cleanup checks

**Files:**
- Modify: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.ps1
- Modify: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.tests.ps1
- Modify: E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\windows-game-matrix.md

- [ ] **Step 1: Define the fixture layout**

Create only under FixtureRoot:

    Client\Binaries\Win64\Client-Win64-Shipping.exe
    Client\Content\Paks
    Client\Saved\Resources\3.0.0\ResManifest

Copy any required source fixture data into that directory, record a recursive
baseline hash, and write an owner marker identifying the current run.

- [ ] **Step 2: Add mutation and cleanup assertions**

The test must verify that a successful run removes the fixture and that a
forced failure leaves the fixture path and evidence available. It must also
verify that a file without the runner owner marker is preserved.

- [ ] **Step 3: Implement safe cleanup**

Use an explicit run-owned root and owner marker. On success, remove only the
run-owned temporary root. On failure, do not perform recursive cleanup; report
the exact path for diagnosis. Never pass a workspace root, drive root, or user
game root to cleanup.

- [ ] **Step 4: Run fixture contract tests**

Run:

    pwsh -NoProfile -File E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.tests.ps1

Expected result: fixture creation, baseline comparison, foreign-file
preservation, success cleanup, and failure preservation all PASS.

## Task 4: Connect automated scenarios to the existing matrix

**Files:**
- Modify: E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.ps1
- Modify: E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\windows-game-matrix.md

- [ ] **Step 1: Map each existing matrix row to a runner scenario**

Use stable names for the rows:

    install-resource-mount
    install-loader
    install-signature-bypass
    patch-update
    switch-method
    foreign-artifact
    uninstall-idempotence
    invalid-nested-path
    offline-media
    corrupt-media
    release-notes-safety
    diagnostics-consent

Each automated row must include the exact command/test name or filesystem
evidence that supports its status.

- [ ] **Step 2: Implement automated scenario execution**

Invoke the existing Cargo integration/milestone tests where they cover the
row, run artifact/media checks where a real file is required, and mark rows
that require a real game or GUI as BLOCKED with an explicit reason instead of
pretending they passed.

- [ ] **Step 3: Verify matrix/report consistency**

Run the runner and confirm every matrix row appears once in the report, with no
unknown scenario names and no status other than PASS, FAIL, or BLOCKED.

## Task 5: Define the manual release-machine runbook

**Files:**
- Modify: E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\windows-game-matrix.md
- Create: E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\milestone-7-results-2026-08-18.md

- [ ] **Step 1: Prepare disposable release environment**

Use a dedicated Windows machine or disposable user profile with WebView2,
Administrator access, and a dedicated game copy. Record Windows version,
WebView2 version, launcher version, game executable path, and artifact hashes.

- [ ] **Step 2: Run real game runtime scenarios**

Execute all three install methods, launch the real executable, switch methods,
detect an externally started process, force quit only the detected game PID,
close/minimize/tray while the game runs, and verify signature restoration after
the process exits.

- [ ] **Step 3: Run privilege and filesystem scenarios**

Run a read-only fixture, verify needs-admin behavior, perform the approved
Administrator restart, and compare the filesystem against the pre-run
snapshot. Any unexpected mutation is a FAIL and blocks sign-off.

- [ ] **Step 4: Run WebView2 and media scenarios**

Verify frontend startup, custom media protocol, audio/video playback, release
note sanitization, valid offline cache, corrupt cache recovery, and visible
error states. Save screenshots and relevant launcher logs.

- [ ] **Step 5: Run published self-update scenarios**

Run version N against a published version N+1 ZIP and SHA256sums.txt over
HTTPS. Verify discovery, checksum, staging, handoff, shutdown, replacement,
restart, backup cleanup, and rollback for invalid checksum, invalid ZIP,
missing executable, and unsafe URL.

## Task 6: Final report, defect triage, and Linear sign-off

**Files:**
- Modify: E:\Wuwa Mod\WuwaIDLauncher\README.md
- Modify: E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\milestone-7-results-2026-08-18.md

- [ ] **Step 1: Generate the final evidence summary**

Include automated command output, artifact hashes, manual scenario results,
screenshots/log locations, environment details, cleanup result, and a list of
remaining BLOCKED rows.

- [ ] **Step 2: Triage failures**

For every FAIL, create a linked Linear defect under Milestone 7 with the
reproduction command, expected result, actual result, evidence path, and
whether the failure risks data loss, release security, or user experience.

- [ ] **Step 3: Update release documentation**

Update README release checklist and the acceptance matrix with the exact runner
command, report path, artifact manifest, and manual prerequisites.

- [ ] **Step 4: Apply the release decision**

Mark F7-06 and the milestone Done only when automated gates pass, critical
manual rows pass, no critical defect remains open, and evidence is reproducible.
If a required environment is unavailable, leave the relevant issue BLOCKED and
do not mark the release sign-off complete.

- [ ] **Step 5: Do not commit**

Leave all plan, runner, evidence, and implementation changes in the working
tree for the repository owner to review and commit.
