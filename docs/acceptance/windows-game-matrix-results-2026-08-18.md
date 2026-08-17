# Windows & Wuthering Waves Acceptance Evidence — 2026-08-18

## Scope

Evidence untuk WUT-26 dan WUT-27 dijalankan pada fixture sementara dan artefak
release lokal. Tidak ada folder game utama pengguna yang dipakai atau diubah.

## Automated gate

| Gate | Result | Evidence |
| --- | --- | --- |
| Frontend type/check | PASS | `npm run check`: 0 errors, 0 warnings |
| Frontend production build | PASS | `npm run build` completed |
| Rust unit/integration/milestone suites | PASS | `cargo test --all-targets -- --test-threads=1`: 44 unit tests plus all integration suites passed |
| Installer methods and recovery | PASS | Resource mount, loader, signature bypass, switch, foreign-artifact preservation, invalid/nested path, and idempotent uninstall tests passed |
| Runtime and launch contracts | PASS | Launcher/external/idle reconciliation, launch preconditions, signature restore, force-quit safety, and mock IPC tests passed |
| Media/cache/release-note security | PASS | Hash validation, corrupt-cache recovery, safe HTML/link validation, checksum, ZIP traversal, and update archive tests passed |
| Diagnostics/privacy | PASS | Opt-in defaults, redaction, bounded retry, local fallback, and telemetry gating tests passed |

## Release artifact gate

Command:

```powershell
$env:Path = "C:\Users\Gipar\.cargo\bin;$env:Path"
npm run tauri -- build
```

Result: PASS. Versions in `package.json`, `src-tauri/Cargo.toml`, and
`src-tauri/tauri.conf.json` are all `2.6.1`. All configured icon files exist.
Capability manifest exposes only `core`, `dialog`, `process`, and the
launcher command allowlist; the unused notification permission was removed.

Release staging:

- `src-tauri/target/release/bundle/release/wuwaid-launcher.exe`
- `src-tauri/target/release/bundle/release/WuwaIDLauncher_2.6.1_x64.zip`
- `src-tauri/target/release/bundle/release/WuwaIDLauncher_2.6.1_x64_en-US.msi`
- `src-tauri/target/release/bundle/release/WuwaIDLauncher_2.6.1_x64-setup.exe`
- `src-tauri/target/release/bundle/release/SHA256sums.txt`

The updater ZIP contains `wuwaid-launcher.exe`. The updater extraction path
accepts the legacy `WuwaIDLauncher.exe` name and normalizes both forms to the
actual packaged name, so the handoff path matches the running binary.

SHA-256:

```text
a2f7c1c960f9f15d4aeb103d7581081b215f5ceb65f99ac02e26bad8e0518257  wuwaid-launcher.exe
ec153a8ebbb58e993944776d55d93bafc90f4e4cfbf9cf1e39a9aaf4b1c4daa3  WuwaIDLauncher_2.6.1_x64.zip
cdeac360431c102b6211000d4fb0d7e4a577fec19441fa7c68edb3294324cd19  WuwaIDLauncher_2.6.1_x64_en-US.msi
250938193670a3a3a133d14bd945deb2c166d9f14e9c1938cb198fba9db6f1ef  WuwaIDLauncher_2.6.1_x64-setup.exe
```

## Windows smoke evidence

| Scenario | Result | Evidence |
| --- | --- | --- |
| Portable binary start/stop | PASS | Process remained alive; window title `WuwaID Launcher`; window handle present; process stopped by its own smoke PID |
| NSIS current-user install | PASS | Installer exit code `0`; installed `wuwaid-launcher.exe` and `uninstall.exe` found |
| NSIS installed binary start | PASS | Window title `WuwaID Launcher`; window handle present |
| NSIS uninstall | PASS | Uninstaller exit code `0`; temporary smoke directory removed |
| Release ZIP/checksum consistency | PASS | ZIP entry and all three manifest hashes matched actual bytes |

## Remaining release-machine gate

These rows remain manual because this workspace does not contain a real
Wuthering Waves installation or a release WebView2/admin/game environment:

- real game executable launch and external-process detection;
- taskkill/force-quit against the real game;
- read-only ACL and elevation restart behavior;
- tray hide/show while the game is running;
- real media protocol playback and offline endpoint behavior;
- end-to-end self-update replacement/restart against a published release;
- screenshot/network-trace evidence for release-note rendering and diagnostics.

The temporary fixture and smoke steps are reversible and are defined in
`docs/acceptance/windows-game-matrix.md`.
