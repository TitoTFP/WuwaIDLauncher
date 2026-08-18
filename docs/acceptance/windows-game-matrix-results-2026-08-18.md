# Windows & Wuthering Waves Acceptance Evidence — 2026-08-18

## Scope

Evidence terbaru mencakup perubahan operasi launcher: logging lokal tanpa
upload remote, notifikasi tray, countdown restart self-update, patch notes pada
first open, serta release pipeline binary-only. Tidak ada instalasi game utama
pengguna yang dipakai atau diubah.

## Automated gate

| Gate | Result | Evidence |
| --- | --- | --- |
| Frontend type/check | PASS | `npm run check`: 0 errors, 0 warnings |
| Frontend production build | PASS | `npm run build` completed |
| Rust unit/integration/milestone suites | PASS | 58 unit tests plus all integration/contract suites passed |
| Rust lint | PASS | `cargo clippy --all-targets -- -D warnings` |
| Local diagnostics contract | PASS | Legacy upload/telemetry settings are discarded; no upload command or worker remains |
| Tray/update/patch-note contracts | PASS | Notification body, twelve-second restart sequence, and first-open state contracts passed |
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
- Size: `4,981,760` bytes
- Baseline before this change: `5,022,208` bytes

The local updater archive was generated as:

- `src-tauri/target/release/WuwaIDLauncher-v2.6.1.zip`
- `src-tauri/target/release/SHA256sums.txt`

The ZIP contains exactly `WuwaIDLauncher.exe`. Its SHA-256 is:

```text
44dbc71a3e3de886a9b69420ae3def3b6e2e9878c899d074aec868a03e024438
```

The automated artifact gate completed with `PASS`; its JSON evidence is
written under `src-tauri/target/gate-evidence-final2/`. MSI and NSIS artifacts
are intentionally not produced or published.

## Remaining release-machine gate

These rows still require a visible Windows release-machine smoke test:

- real Wuthering Waves launch, UAC approval, and external-process detection;
- taskkill/force-quit against the real game;
- read-only ACL and Administrator elevation behavior;
- tray hide/show plus the OS notification appearing to the user;
- actual background video and BGM playback through WebView2;
- end-to-end self-update replacement/restart against a published release;
- first-open patch-notes modal and once-per-tag persistence;
- screenshot evidence for the final user-visible flows.

The fixture runner and cleanup steps are reversible and are defined in
`docs/acceptance/windows-game-matrix.md`.
