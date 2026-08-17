# WuwaID Launcher Stabilization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menyelesaikan seluruh milestone dan issue pada project Linear `WuwaID Launcher — Audit & Stabilization` dengan kontrak runtime yang aman, UI yang observable, dan release gate Windows yang dapat diverifikasi.

**Architecture:** Pertahankan boundary Tauri command sebagai sumber validasi dan normalisasi, engine Rust sebagai sumber kebenaran filesystem/process/network, dan `launcherState.svelte.ts` sebagai sumber kebenaran UI. Setiap operasi berbahaya memakai precondition, staging, ownership marker, rollback, dan satu terminal event; setiap event backend memiliki consumer state/UI atau dihapus dari kontrak.

**Tech Stack:** Rust 2021, Tauri v2, Tokio, Reqwest, Zip, SHA-256/SHA-1, Svelte 5, TypeScript, Vite, Cargo tests, Svelte check/build, Windows release smoke tests.

---

## Current baseline and safety rules

- [x] Preserve user commit `cd07a1f`; do not reset, checkout, or commit over it.
- [x] Keep the existing Milestone 1 changes in the current worktree until the user reviews them.
- [ ] Run `npm run check`, `npm run build`, and `C:\Users\Gipar\.cargo\bin\cargo.exe test --manifest-path src-tauri\Cargo.toml --all-targets -- --test-threads=1` after every milestone gate.
- [ ] Keep all tests deterministic: no production endpoints or real user game folders in automated tests.
- [ ] Update the matching Linear issue with evidence only after its acceptance criteria and fresh verification pass.

## Milestone 2 — Installer, switching, and uninstall safety

Files: `src-tauri/src/engine/path.rs`, `src-tauri/src/engine/installer.rs`, `src-tauri/src/engine/downloader.rs`, `src-tauri/src/engine/signature.rs`, `src-tauri/src/lib.rs`, and Rust integration tests.

- [ ] WUT-9: centralize canonical game-path validation and method-specific writable-target checks before backup, cleanup, download, or deployment; test invalid, nested, read-only, and per-method paths.
- [ ] WUT-10: add atomic download staging, deployment transactions, snapshot restoration, cleanup of temporary files, one terminal error, and fault-injection tests for copy/write/missing signature/loader.
- [ ] WUT-11: make ownership-aware cleanup return structured results; validate switch method and path, preserve foreign/partial artifacts, report partial cleanup, and remove metadata only after successful cleanup; test repeated uninstall and foreign artifacts.

## Milestone 3 — Launch, process, and recovery

Files: `src-tauri/src/engine/runtime.rs`, `src-tauri/src/engine/signature.rs`, `src-tauri/src/lib.rs`, `src/lib/bridge.ts`, `src/lib/launcherState.svelte.ts`, and lifecycle tests.

- [ ] WUT-12: add one runtime reconciliation seam for launcher-started and externally-started processes, with cancellable polling and deterministic process-state tests.
- [ ] WUT-13: make signature bypass/restore lifecycle idempotent and process-driven; cancel safety timers on exit, restore on spawn failure/crash/force quit, and test early exit and long-running process behavior.
- [ ] WUT-14: make force quit return a result, emit runtime false/launch finished, restore temporary state, and show/focus the window after termination; cover already-exited and taskkill failure paths.
- [ ] WUT-15: validate path, method, patch readiness, and executable before launch; introduce a launch-domain error event and ensure frontend busy state always returns to idle/error.

## Milestone 4 — Navigation, errors, and preferences

Files: `src/App.svelte`, `src/components/TopBar.svelte`, `src/components/RightPanel.svelte`, new settings/log/about views as needed, `src/components/AudioPlayer.svelte`, `src/components/BackgroundFx.svelte`, `src/lib/types.ts`, `src/lib/bridge.ts`, and frontend tests/checks.

- [ ] WUT-16: render `home`, `settings`, `logs`, and `about` as real views with active navigation and documented actions.
- [ ] WUT-17: expose operation progress, log upload, media, update, success, error, dismissal, and reset states without conflating patch installation with other operations.
- [ ] WUT-18: guard every async action with deterministic busy/pending/reject/finally handling and rollback config mutations when bridge commands fail.
- [ ] WUT-19: make config/state the single source for visual mode, audio, DX11, auto-update, and method; apply it on init, reload, change, and game-running transitions; test failed media loading and preference persistence.

## Milestone 5 — Media, release notes, and self-update

Files: `src-tauri/src/engine/media.rs`, `src-tauri/src/engine/updater.rs`, `src-tauri/src/engine/atom_feed.rs`, `src-tauri/src/lib.rs`, `src/components/SidePanel.svelte`, `src/components/UpdateModal.svelte`, `src/App.svelte`, and Rust/TypeScript tests.

- [ ] WUT-20: connect launcher-update available/progress/staged/restarting/failure events to `UpdateModal`; support dismissal and retry without stale state.
- [ ] WUT-21: verify update URL, checksum, ZIP contents, expected executable/version, atomic Windows handoff, rollback, cleanup, and restart; test invalid ZIP, mismatch, missing executable, valid staging, and handoff failure.
- [ ] WUT-22: verify cached media hashes before `MediaReady`, stage replacements before publishing, preserve valid old media on revalidation failure, and test valid/corrupt/partial/changed/offline cache.
- [ ] WUT-23: separate media status/progress from patch progress; stop/reset media references on cache reset and resync or retain valid offline cache; test reset/recovery event flow.
- [ ] WUT-24: sanitize release-note HTML/Markdown with an allowlist, reject unsafe URL schemes/event handlers, validate cached notes, and test safe markup plus XSS payloads.

## Milestone 6 — Diagnostics, packaging, and release gate

Files: `src-tauri/src/engine/log_collector.rs`, `src-tauri/src/engine/telemetry.rs`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/build.rs`, `README.md`, new release-gate/acceptance documentation, and integration tests.

- [ ] WUT-25: exercise core async commands through Tauri mock IPC with fixtures and event assertions for success/error/cleanup/failure injection/duplicate action.
- [ ] WUT-26: create a Windows/game acceptance matrix with reversible fixtures, expected evidence, and release-artifact smoke instructions covering install/update/uninstall/switch/launch/force quit/offline/read-only/admin/tray/media/self-update.
- [ ] WUT-27: run and repair Windows Tauri packaging, capability/resource/media protocol/version/icon/updater/checksum gates; record artifact and smoke-test evidence.
- [ ] WUT-28: add explicit diagnostics consent, payload redaction, timeout/retry/status handling, local bundle fallback, and tests for enabled/disabled/redacted/network-failure paths.
- [ ] WUT-29: align README feature status, commands, method mapping, artifacts, recovery, privacy, and release checklist with verified behavior and known Windows-only limits.

## Completion audit

- [ ] Every issue WUT-5 through WUT-29 has acceptance-criteria evidence in Linear.
- [ ] Every milestone reports 100% only after its issue statuses and gate evidence are complete.
- [ ] Fresh frontend check/build, all Cargo targets, Windows packaging, and acceptance documentation agree with the implementation.
- [ ] Worktree diff is reviewed; no user commit is rewritten and no assistant commit is created unless explicitly requested.
