# WuwaID Launcher

Launcher mempermudah instalasi patch Bahasa Indonesia untuk Wuthering Waves.

## Cara Penggunaan

1. Buka aplikasi `WuwaIDLauncher.exe`.
2. Pilih direktori instalasi game (launcher akan mendeteksi otomatis jika berada di lokasi instalasi default).
3. Klik **Instal Patch ID** dan tunggu hingga proses unduhan selesai.
4. Klik **Mainkan** untuk masuk ke dalam game.

## Fitur Utama

- **Manajemen Patch:** Instal, perbarui, atau hapus patch Bahasa Indonesia dengan sekali klik.
- **Deteksi Otomatis:** Mencari dan mengenali jalur folder instalasi game secara otomatis.
- **Bermain Langsung:** Jalankan Wuthering Waves langsung melalui launcher.
- **Tray Hemat Resource:** Saat game berjalan, launcher bersembunyi ke tray dengan WebView2 suspended sambil mempertahankan heartbeat pemain aktif.
- **Hak Akses Admin:** Opsi memuat ulang aplikasi dengan hak Administrator jika diperlukan (melalui menu ⋯ → Jalankan sebagai Administrator).

## Mode Tray

Game yang dijalankan melalui launcher otomatis memindahkan launcher ke system tray. Klik dua kali ikon tray atau pilih **Buka Launcher** untuk menampilkan UI read-only mode `Mati`. Setelah game keluar, launcher tampil kembali dan memulihkan profil tersimpan. Pilihan **Keluar** menghentikan launcher dan heartbeat, tetapi diblokir selama pemulihan signature masih wajib diselesaikan.

Heartbeat anonim tetap dikirim setiap 5 menit selama launcher hidup, termasuk ketika visible, tray, atau mendeteksi game eksternal. Media, animasi, patch check, release notes, dan network latar belakang lain tetap dihentikan selama game aktif.

## Persyaratan Sistem

- **OS:** Windows 10 atau 11
- **Dependency:** [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (Umumnya sudah terinstal bawaan pada Windows 11)

## Benchmark Optimasi (Windows)

Build compressed dan uncompressed dengan output/intermediate terisolasi. Default produksi tetap compressed:

```powershell
.\tests\build_benchmark_variants.ps1
```

Ukur `CleanEveryRun`, `CleanFirst`, dan enam warm run. CSV mencatat `ui_interactive`, `patch_ready`, CPU, working set, dan private bytes:

```powershell
.\tests\measure_startup.ps1 -ExePath .\publish\benchmark\compressed\WuwaIDLauncher.exe -Runs 6 -ProfileMode CleanEveryRun -OutputCsv compressed-clean.csv
.\tests\measure_startup.ps1 -ExePath .\publish\benchmark\compressed\WuwaIDLauncher.exe -Runs 6 -ProfileMode CleanFirst -OutputCsv compressed-clean-first.csv
.\tests\measure_startup.ps1 -ExePath .\publish\benchmark\compressed\WuwaIDLauncher.exe -Runs 6 -ProfileMode Warm -OutputCsv compressed-warm.csv
.\tests\measure_startup.ps1 -ExePath .\publish\benchmark\compressed\WuwaIDLauncher.exe -Runs 6 -ProfileMode Warm -MinimizeAfterInteractive -OutputCsv compressed-minimized.csv
```

Jalankan matriks sama untuk `publish/benchmark/uncompressed`. Jadikan uncompressed default hanya bila median enam warm run minimal 10% lebih cepat dan median clean startup tidak regresi lebih dari 5%.

Untuk dampak ke game, rekam skenario launcher tertutup dan launcher `tray-suspended` dengan PresentMon pada rute, preset, resolusi, dan durasi identik. Simpan CSV proses `Client-Win64-Shipping.exe`, lalu bandingkan GPU busy, P99 frametime, 1% low, working set launcher, dan private bytes. Lulus bila CPU/GPU launcher masing-masing di bawah 1%, working set tray turun minimal 25% dari visible-full, dan P99/1% low game tidak memburuk lebih dari 2%.

## Kredit

- [AlteriaX/WuWa-Configs](https://github.com/AlteriaX/WuWa-Configs) — Menyediakan preset konfigurasi game yang digunakan untuk Mode Performa Tinggi (High Performance mode).
- **[CallMeDangDev](https://github.com/CallMeDangDev)** — Terima kasih untuk referensi WuwaVH dan launcher. Source code launcher WuwaID mengacu pada repo [WuwaVHLauncher](https://github.com/CallMeDangDev/WuwaVHLauncher).

## Lisensi

Proyek ini dilisensikan di bawah [GNU General Public License v3.0](LICENSE).
