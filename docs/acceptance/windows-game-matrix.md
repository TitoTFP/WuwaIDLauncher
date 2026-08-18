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
   folder temporary. Artefak yang boleh tertinggal hanya artefak ber-owner marker
   yang sengaja sedang diuji.

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
`npm run build`, Cargo all-target tests, dan `npm run tauri -- build`. Flag
dibuat opt-in karena Tauri build memutasi output build dan dapat memerlukan
beberapa menit; artifact gate tetap berjalan pada setiap `automated` run.

Untuk menjalankan preflight manual pada dedicated game copy, tambahkan
`-Mode manual -GamePath "D:\Wuthering Waves-Test"`. `-Mode all` menjalankan
keduanya. Runner mengembalikan exit code `0` untuk PASS, `1` untuk FAIL, dan
`2` untuk BLOCKED; path JSON report selalu dicetak untuk run yang sudah mulai.

Report mencatat `runId`, timestamps, scenario status, owner marker, baseline
snapshot SHA-256, evidence path, dan cleanup result. Run yang seluruhnya PASS
menghapus hanya child fixture miliknya. Run FAIL/BLOCKED mempertahankan child
fixture dan evidence untuk diagnosis; file asing di parent fixture tanpa owner
marker tidak pernah dihapus.

Artifact gate mencari executable standalone dan bundle release pada root atau
`bundle/release`, lalu memverifikasi versi konsisten dari `package.json`,
`src-tauri/Cargo.toml`, dan `src-tauri/tauri.conf.json`. Lima configured icon
files wajib tersedia. `SHA256sums.txt` wajib cocok untuk ZIP, MSI, dan NSIS;
hash executable standalone juga dihitung dan dicatat. ZIP wajib berisi
`WuwaIDLauncher.exe` dan tidak boleh memiliki path traversal.

Contract test runner:

    pwsh -NoProfile -File .\scripts\acceptance\windows-release-gate.tests.ps1

## Matrix

| Skenario | Fixture / kondisi | Hasil yang harus terlihat | Evidence |
| --- | --- | --- | --- |
| Install resource_mount | Fixture valid + ResManifest | PAK, SIG, mount file, dan owner marker dibuat; status ready | onProgressUpdate, onInstallComplete, filesystem diff |
| Install loader | Fixture valid + target Win64 writable | winhttp.dll, PAK, dan loader marker dibuat atomik | marker hash + status ready |
| Install signature_bypass | Fixture valid + PAK target kosong | PAK dan marker dibuat; signature asli tetap ada sebelum launch | signature snapshot + marker hash |
| Patch update | Fixture sudah memiliki patch lama | Download memakai cache temp; kegagalan mengembalikan snapshot lama | log terminal + filesystem diff |
| Switch method | Patch owned pada method A | Artefak A dibersihkan; metadata berubah hanya jika cleanup selesai | CleanupReport, versions.json |
| Foreign artifact | Target berisi file tanpa owner marker | File dipertahankan dan UI menampilkan partial cleanup; metadata tidak berubah | preserved report + metadata diff |
| Uninstall | Patch owned, lalu ulangi uninstall | Cleanup idempotent; metadata dihapus hanya setelah cleanup sukses | dua hasil command + filesystem diff |
| Invalid / nested path | Pilih subfolder Client/Binaries dan folder acak | Path dinormalisasi hanya ke root game valid; path invalid ditolak sebelum mutasi | visible error + event payload |
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
| Diagnostics disabled | Default settings / toggle off | Command ditolak; tidak ada request jaringan dan tidak ada client ID baru | settings + network trace |
| Diagnostics enabled | Toggle on + fixture logs | Payload settings/versions disensor, retry maksimal 2, bundle lokal tersedia saat gagal | ZIP content + status UI |
| Tray / window | Game running, minimize/close, tray show | Launcher hide saat game aktif; tray dapat show/focus; close aman | screen recording |
| Packaging | Windows MSVC + WebView2 tersedia | EXE, MSI, dan NSIS installer dibuat; version/icon/manifest konsisten | src-tauri/target/release/bundle/** |

## Automated gate yang sudah dijalankan

    npm run check
    npm run build
    & 'C:\Users\Gipar\.cargo\bin\cargo.exe' test --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
    $env:Path = "C:\Users\Gipar\.cargo\bin;$env:Path"
    npm run tauri -- build

Packaging menghasilkan:

- src-tauri/target/release/WuwaIDLauncher.exe
- src-tauri/target/release/bundle/msi/WuwaIDLauncher_2.6.1_x64_en-US.msi
- src-tauri/target/release/bundle/nsis/WuwaIDLauncher_2.6.1_x64-setup.exe

Skenario yang bergantung pada executable game asli, taskkill Windows, tray,
hak Administrator, dan restart self-update tetap harus dicentang manual pada
mesin Windows release sebelum publish.
