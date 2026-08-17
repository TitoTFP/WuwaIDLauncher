# Milestone 7 — Windows/Game Release Acceptance Design

**Status:** Design approved by user

**Goal:** Membuktikan WuwaID Launcher siap dirilis pada environment Windows
nyata melalui kombinasi automated gate yang repeatable dan acceptance test
manual yang aman terhadap instalasi game pengguna.

## Context

Milestone 1–6 sudah menyelesaikan kontrak state, installer transaction,
runtime/recovery, UI/settings, media/self-update security, diagnostics,
packaging, serta release documentation. WUT-26 dan WUT-27 membuktikan bahwa
matrix dan artifact lokal tersedia, tetapi beberapa hal tetap memerlukan
environment release: executable game nyata, WebView2, UAC/admin, tray,
taskkill, playback media, dan self-update dari release HTTPS yang dipublikasi.

## Scope

### In scope

- Runner PowerShell untuk fixture, automated gate, evidence collection, dan
  cleanup.
- Validasi EXE/MSI/NSIS/updater ZIP/checksum/version/icon.
- Validasi install, method switch, update, uninstall, foreign-artifact
  preservation, recovery, dan media/cache pada fixture.
- Acceptance manual untuk game nyata, WebView2/media, UAC/admin, tray,
  external-process detection, force quit, dan published self-update.
- Evidence report dengan status PASS, FAIL, atau BLOCKED.
- Milestone/issues F7 dan release sign-off di Linear.

### Out of scope

- Mengubah atau membersihkan instalasi game utama pengguna.
- Automasi penuh untuk interaksi tray, UAC, dan game nyata.
- Pembuatan CI cloud atau release hosting baru.
- Fitur launcher baru yang tidak diperlukan untuk release gate.

## Architecture

Runner utama akan dibuat di:

E:\Wuwa Mod\WuwaIDLauncher\scripts\acceptance\windows-release-gate.ps1

Runner menerima lokasi artifact release, mode automated, manual, atau all,
serta folder fixture sementara. Fixture berada di bawah folder temporary
ber-ID run dan selalu memiliki snapshot baseline sebelum mutasi.

Alur runner:

1. Validasi input dan pastikan target bukan instalasi game utama.
2. Siapkan fixture dan snapshot baseline.
3. Jalankan automated checks dan simpan output command.
4. Jalankan atau pandu manual release scenarios.
5. Tulis evidence report.
6. Bersihkan fixture jika sukses; pertahankan fixture dan log jika gagal agar
   diagnosis tidak hilang.

Runner tidak menghapus target tanpa owner marker dan tidak melakukan operasi
destruktif pada game path yang tidak secara eksplisit diberikan untuk testing.

## Evidence model

Matrix utama tetap berada di:

E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\windows-game-matrix.md

Runner dan operator menulis hasil run ke:

 E:\Wuwa Mod\WuwaIDLauncher\docs\acceptance\milestone-7-results-2026-08-18.md

Setiap skenario wajib mencatat:

- nama skenario dan status PASS, FAIL, atau BLOCKED;
- timestamp, Windows/build environment, dan versi launcher;
- command atau langkah manual yang dijalankan;
- artifact/hash/log/screenshot yang menjadi evidence;
- perubahan filesystem sebelum dan sesudah;
- alasan dan next action untuk FAIL atau BLOCKED.

BLOCKED tidak dihitung sebagai PASS dan menahan final release sign-off untuk
skenario critical.

## Linear issue decomposition

### F7-01 — Fixture Windows, snapshot, cleanup, dan runner

Membuat runner, folder fixture, baseline snapshot, safe cleanup, result schema,
dan command documentation. Menjadi blocker untuk automated dan manual gates.

### F7-02 — Automated artifact, installer, media, dan recovery gate

Menjalankan check/build/test, memverifikasi version/icon/hash, menguji EXE/MSI/
NSIS/updater ZIP, install/switch/update/uninstall, foreign artifact,
media/cache, dan rollback pada fixture.

### F7-03 — Real game runtime, process, privilege, dan tray gate

Pada dedicated game fixture, memvalidasi launch untuk tiga method, external
process detection, signature lifecycle, force quit, taskkill, read-only
folder, restart as Administrator, minimize/close, dan tray show/focus.

### F7-04 — WebView2, media, offline, dan release notes gate

Memvalidasi frontend pada release artifact, custom media:// protocol,
audio/video playback, offline valid cache, corrupt cache recovery, dan
sanitized release notes pada WebView2 target.

### F7-05 — Published self-update gate

Menggunakan dua release artifact HTTPS untuk memvalidasi update discovery,
checksum, ZIP extraction, staging, handoff, restart, old-binary backup,
cleanup, rollback, dan rejection untuk checksum/ZIP/URL yang invalid.

### F7-06 — Evidence, defect triage, dan release sign-off

Menggabungkan seluruh evidence, membuka defect issue untuk setiap failure,
memastikan tidak ada critical blocker, memperbarui README/release checklist,
dan hanya menandai Milestone 7 selesai setelah semua acceptance evidence
lengkap.

Dependency order: F7-01 memblokir F7-02/F7-03/F7-04; F7-02 memblokir F7-05;
F7-06 menunggu F7-02 sampai F7-05.

## Acceptance criteria

Milestone 7 selesai hanya jika:

1. Semua automated gate lulus pada clean release artifact.
2. Semua critical manual scenarios berstatus PASS.
3. Tidak ada perubahan permanen di instalasi game utama.
4. Hash dan version metadata seluruh artifact cocok dengan manifest.
5. Self-update valid dan invalid paths sama-sama memiliki evidence.
6. Defect yang ditemukan terhubung ke issue Linear dan tidak ada critical
   blocker yang terbuka.
7. Evidence report dapat dipakai operator lain untuk mengulang run tanpa
   pengetahuan implisit dari implementer.

## Safety and failure handling

- Operator menggunakan dedicated copy atau fixture, bukan game installation
  utama.
- Snapshot dibuat sebelum setiap kelompok mutasi.
- Jika cleanup gagal, runner mempertahankan folder dan menunjuk path evidence
  daripada menghapus secara paksa.
- Force quit hanya boleh menargetkan PID game yang terdeteksi oleh runner.
- Published self-update diuji pada installation disposable dengan versi
  rollback yang dapat dipulihkan.

No launcher code is changed by this design alone. Any defect discovered by
Milestone 7 becomes a separately scoped implementation issue before it is
fixed.
