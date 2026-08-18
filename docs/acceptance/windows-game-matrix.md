# Windows & Wuthering Waves Acceptance Matrix

Dokumen ini adalah gate manual untuk rilis Windows. Fixture harus berada di folder
temporary yang dibuat khusus untuk pengujian; jangan menunjuk ke instalasi game
utama pengguna.

## Fixture yang dapat dibalik

1. Buat salinan fixture game dengan struktur minimal:
   Client/Binaries/Win64/Client-Win64-Shipping.exe,
   Client/Content/Paks, dan Client/Saved/Resources/<version>/ResManifest.
2. Simpan snapshot seluruh fixture sebelum setiap skenario.
3. Jalankan launcher dengan WUWAID_E2E_APPDATA ke folder temporary terpisah.
4. Setelah skenario selesai, tutup launcher, bandingkan snapshot, lalu hapus
   folder temporary. Runner hanya mengelola path kanonis yang memang menjadi
   target metode; artefak di luar path kanonis tidak disentuh.

## Runner contract (WUT-30)

Runner PowerShell membuat child folder ber-ID run di bawah `FixtureRoot`. Runner
hanya boleh menerima artifact release, output evidence, dan fixture yang berada
di lokasi terpisah; `FixtureRoot` yang sama/overlap dengan `GamePath` ditolak
sebelum fixture dibuat.

Contoh artifact-only automated run:

    $runRoot = Join-Path $env:TEMP "wuwaid-release-gate"
    $artifactRoot = "E:\Wuwa Mod\WuwaIDLauncher\src-tauri\target\release"
    $outputRoot = Join-Path $runRoot "evidence"
    $fixtureRoot = Join-Path $runRoot "fixtures"
    pwsh -NoProfile -File .\scripts\acceptance\windows-release-gate.ps1 `
      -Mode automated `
      -ArtifactRoot $artifactRoot `
      -OutputRoot $outputRoot `
      -FixtureRoot $fixtureRoot

Untuk menjalankan full WUT-31 command gate, tambahkan `-RunCommandGate`:

    pwsh -NoProfile -File .\scripts\acceptance\windows-release-gate.ps1 `
      -Mode automated `
      -ArtifactRoot $artifactRoot `
      -OutputRoot $outputRoot `
      -FixtureRoot $fixtureRoot `
      -RunCommandGate

Flag tersebut menjalankan dan menyimpan log terpisah untuk `npm run check`,
`npm run build`, Cargo all-target tests, dan `npm run tauri -- build --no-bundle`. Flag
dibuat opt-in karena Tauri build memutasi output build dan dapat memerlukan
beberapa menit; artifact gate tetap berjalan pada setiap `automated` run.

Untuk menjalankan preflight manual pada dedicated game copy, tambahkan
`-Mode manual -GamePath "D:\Wuthering Waves-Test"`. `-Mode all` menjalankan
keduanya. Runner mengembalikan exit code `0` untuk PASS, `1` untuk FAIL, dan
`2` untuk BLOCKED; path JSON report selalu dicetak untuk run yang sudah mulai.

Report mencatat `runId`, timestamps, scenario status, baseline snapshot SHA-256,
evidence path, dan cleanup result. Run yang seluruhnya PASS menghapus hanya child
fixture miliknya. Run FAIL/BLOCKED mempertahankan child fixture dan evidence untuk
diagnosis; file asing di parent fixture tidak pernah dihapus. Di dalam game copy,
file pada path kanonis metode diperlakukan sebagai artefak launcher tanpa
memeriksa ownership marker; ini memungkinkan migrasi instalasi dari launcher lama,
dengan trade-off bahwa file pihak ketiga pada path yang sama dapat diganti atau
dihapus.

Artifact gate mencari executable standalone, updater ZIP, dan manifest checksum
di root artifact, lalu memverifikasi versi konsisten dari `package.json`,
`src-tauri/Cargo.toml`, dan `src-tauri/tauri.conf.json`. Lima configured icon
files wajib tersedia. `SHA256sums.txt` wajib cocok untuk ZIP; hash executable
standalone juga dihitung dan dicatat. ZIP wajib berisi `WuwaIDLauncher.exe` dan
tidak boleh memiliki path traversal. MSI/NSIS sengaja tidak dibuat.

Contract test runner:

    pwsh -NoProfile -File .\scripts\acceptance\windows-release-gate.tests.ps1

## Matrix

| Skenario | Fixture / kondisi | Hasil yang harus terlihat | Evidence |
| --- | --- | --- | --- |
| Install resource_mount | Fixture valid + ResManifest | PAK, SIG, dan mount file dibuat; status ready tanpa marker | onProgressUpdate, onInstallComplete, filesystem diff |
| Install loader | Fixture valid + target Win64 writable | winhttp.dll dan PAK dibuat atomik; status ready tanpa marker | status ready + filesystem diff |
| Install signature_bypass | Fixture valid + PAK target kosong | PAK dibuat; signature asli tetap ada sebelum launch | signature snapshot + status ready |
| Patch update | Fixture sudah memiliki patch lama pada satu atau beberapa resource version | Download memakai cache temp; kegagalan mengembalikan snapshot seluruh target kanonis | log terminal + filesystem diff |
| Switch method | Patch owned pada method A | Artefak A dibersihkan; metadata berubah hanya jika cleanup selesai | CleanupReport, versions.json |
| Canonical legacy artifact | Target kanonis berisi file dari launcher lama tanpa marker | File diganti/dihapus sesuai operasi; metadata berubah setelah cleanup berhasil | removed report + metadata diff |
| Uninstall | Patch owned, lalu ulangi uninstall | Cleanup idempotent; metadata dihapus hanya setelah cleanup sukses | dua hasil command + filesystem diff |
| Invalid / nested path | Pilih subfolder Client/Binaries dan folder acak | Path dinormalisasi hanya ke root game valid; path invalid ditolak sebelum mutasi | visible error + event payload |
| Junction / reparse path | Target kanonis atau parent-nya adalah junction/symlink/reparse point | Operasi ditolak sebelum write/cleanup agar tidak menyentuh lokasi di luar game | error code + filesystem diff |
| Read-only / admin | Target fixture dibuat read-only | Status needs_admin; tidak ada download atau cleanup parsial | error code + filesystem diff |
| Launch valid | Patch ready, executable ada | Runtime state launcher=true; launcher hide; selesai kembali ke idle | runtime events + process evidence |
| Launch invalid / not ready | Patch missing/corrupt atau executable hilang | onLaunchError; busy state kembali idle; signature tidak tertinggal bypass | event payload + filesystem diff |
| External process | Jalankan game fixture di luar launcher | Runtime state external=true; launcher tidak mengklaim sebagai child | runtime event |
| Force quit | Game sedang berjalan / sudah berhenti | Window kembali tampil; signature dipulihkan; error taskkill terlihat bila gagal | runtime + launch-finished events |
| Offline media | Cache valid lalu putuskan jaringan | MediaReady hanya setelah hash cocok; status offline tetap memakai cache valid | media events + hashes |
| Corrupt / partial media | Cache file dipotong atau manifest hilang | File candidate gagal diverifikasi; media lama valid tetap dipertahankan | cache diff + media status |
| Release notes XSS | Body berisi script, event handler, javascript: link | Hanya allowlist HTML dan link aman yang tampil | screenshot + sanitized HTML |
| Self-update valid | ZIP berisi WuwaIDLauncher.exe + checksum benar | ZIP terverifikasi, staging dan handoff dibuat, installer restart | SHA-256 + handoff + artifact |
| Self-update invalid | Checksum mismatch, exe hilang, traversal, atau URL HTTP | Update ditolak sebelum handoff; staging/temp dibersihkan | onLauncherUpdateError + filesystem diff |
| Local diagnostics | Fixture logs atau launch failure | Detail operasi tersedia lokal; tidak ada upload atau telemetry request | local state/log + network trace |
| Tray / window | Game running, minimize/close, tray show | Launcher hide saat game aktif; tray dapat show/focus; close aman | screen recording |
| Packaging | Windows MSVC + WebView2 tersedia | EXE portable dibangun; updater ZIP dan checksum konsisten; MSI/NSIS tidak dibuat | release root + checksum |

## Automated gate yang sudah dijalankan

    npm run check
    npm run build
    & 'C:\Users\Gipar\.cargo\bin\cargo.exe' test --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
    $env:Path = "C:\Users\Gipar\.cargo\bin;$env:Path"
    npm run tauri -- build --no-bundle

Packaging menghasilkan binary standalone:

- src-tauri/target/release/WuwaIDLauncher.exe

Skenario yang bergantung pada executable game asli, taskkill Windows, tray,
hak Administrator, dan restart self-update tetap harus dicentang manual pada
mesin Windows release sebelum publish.
