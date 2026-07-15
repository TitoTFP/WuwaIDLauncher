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
- **Hak Akses Admin:** Opsi memuat ulang aplikasi dengan hak Administrator jika diperlukan (melalui menu ⋯ → Jalankan sebagai Administrator).

## Persyaratan Sistem

- **OS:** Windows 10 atau 11
- **Dependency:** [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (Umumnya sudah terinstal bawaan pada Windows 11)

## Benchmark Optimasi (Windows)

Build baseline dan varian eksperimen tanpa mengubah default produksi:

```powershell
dotnet publish -c Release -r win-x64 -p:EnableCompressionInSingleFile=true  -o publish/baseline
dotnet publish -c Release -r win-x64 -p:EnableCompressionInSingleFile=false -o publish/no-compression
dotnet publish -c Release -r win-x64 -p:UseOpaqueWindowBenchmark=true       -o publish/opaque
```

Ukur clean-profile, warm-start, visible idle, dan minimized idle menggunakan `tests/measure_startup.ps1`:

```powershell
.\tests\measure_startup.ps1 -ExePath .\publish\baseline\WuwaIDLauncher.exe -Runs 6 -CleanProfile -OutputCsv baseline-visible.csv
.\tests\measure_startup.ps1 -ExePath .\publish\baseline\WuwaIDLauncher.exe -Runs 5 -MinimizeAfterInteractive -OutputCsv baseline-minimized.csv
```

Varian opaque hanya diadopsi bila tampilan setara dan penggunaan idle/startup membaik minimal 10%. Compression hanya dinonaktifkan bila cold-start membaik minimal 10% dengan kenaikan ukuran distribusi maksimal 30%.

## Kredit

- [AlteriaX/WuWa-Configs](https://github.com/AlteriaX/WuWa-Configs) — Menyediakan preset konfigurasi game yang digunakan untuk Mode Performa Tinggi (High Performance mode).
- **[CallMeDangDev](https://github.com/CallMeDangDev)** — Terima kasih untuk referensi WuwaVH dan launcher. Source code launcher WuwaID mengacu pada repo [WuwaVHLauncher](https://github.com/CallMeDangDev/WuwaVHLauncher).

## Lisensi

Proyek ini dilisensikan di bawah [GNU General Public License v3.0](LICENSE).
