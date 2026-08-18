# Windows & Wuthering Waves Acceptance Evidence — 2026-08-18

## Scope

Evidence terbaru mencakup perubahan operasi launcher: logging lokal tanpa
upload remote, notifikasi tray, countdown restart self-update, serta release
pipeline binary-only. Pengumuman patch game tetap berasal dari Atom feed repo
WuwaID; patch notes launcher pada first open berasal dari GitHub Release repo
WuwaIDLauncher dan body yang dibuat otomatis oleh workflow release. Tidak ada
instalasi game utama pengguna yang dipakai atau diubah.

## Automated gate

| Gate | Result | Evidence |
| --- | --- | --- |
| Frontend type/check | PASS | `npm run check`: 0 errors, 0 warnings |
| Frontend production build | PASS | `npm run build` completed |
| Rust unit/integration/milestone suites | PASS | 60 unit tests plus all integration/contract suites passed |
| Rust lint | PASS | `cargo clippy --all-targets -- -D warnings` |
| Local diagnostics contract | PASS | Legacy upload/telemetry settings are discarded; no upload command or worker remains |
| Tray/update/patch-note contracts | PASS | Notification body, twelve-second restart sequence, GitHub launcher-release parser/payload, and first-open state contracts passed |
| Launcher release-note ACL/native smoke | PASS | `get_launcher_release_notes` is present in source/generated ACL; final binary cached live release `v2.7.0` with body length 297 |
| Release workflow contract | PASS | CI/release workflows use `npm ci`, `--no-bundle`, semantic tags, and no MSI/NSIS publishing |
| Runner contract | PASS | `windows-release-gate.tests.ps1`: fixture safety, snapshot, cleanup, and evidence |

## Release artifact gate

Command:

```powershell
$env:Path = "C:\Users\Gipar\.cargo\bin;$env:Path"
npm run tauri -- build --no-bundle
```

Result: PASS. The produced standalone binary is:

- `src-tauri/target/release/WuwaIDLauncher.exe`
- Size: `4,989,952` bytes
- Baseline before this change: `5,022,208` bytes

The final executable SHA-256 is:

```text
99F4672AD24B9E9EC437E7D7061CB0516F513DCD56309E196B764FD8D051C25D
```

The local updater archive was generated as:

- `src-tauri/target/release/WuwaIDLauncher-v2.6.1.zip`
- `src-tauri/target/release/SHA256sums.txt`

The ZIP contains exactly `WuwaIDLauncher.exe`. Its SHA-256 is:

```text
7b027e96e06e178325efd9bef9e2af4efe06259f988f7b2332cece9ce6ce200d
```

The automated artifact gate completed with `PASS`; its JSON evidence is
written under `src-tauri/target/gate-evidence-launcher-notes-final/`
(`windows-release-gate-20260818T153034215Z-p7064.json`). MSI and NSIS artifacts
are intentionally not produced or published.

## Remaining release-machine gate

These rows still require a visible Windows release-machine smoke test:

- real Wuthering Waves launch, UAC approval, and external-process detection;
- taskkill/force-quit against the real game;
- read-only ACL and Administrator elevation behavior;
- tray hide/show plus the OS notification appearing to the user;
- actual background video and BGM playback through WebView2;
- end-to-end self-update replacement/restart against a published release;
- visible first-open launcher release-notes modal and once-per-tag persistence
  (the backend cache fetch is covered by the native smoke test);
- screenshot evidence for the final user-visible flows.

The fixture runner and cleanup steps are reversible and are defined in
`docs/acceptance/windows-game-matrix.md`.
