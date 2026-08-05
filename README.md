<div align="center">

# 🌊 WuwaID Launcher

**Launcher Resmi & Patch Installer Bahasa Indonesia untuk Wuthering Waves**

[![License: GPL v3](https://img.shields.io/badge/License-GPL_v3-blue.svg)](LICENSE)
[![.NET Version](https://img.shields.io/badge/.NET-8.0--windows-512BD4?logo=dotnet)](https://dotnet.microsoft.com/)
[![Platform](https://img.shields.io/badge/Platform-Windows_x64-0078D6?logo=windows)](https://microsoft.com)
[![Launcher Version](https://img.shields.io/badge/Version-2.6.1-brightgreen)](#)
[![UI Engine](https://img.shields.io/badge/UI-WPF_%2B_WebView2-00589C)](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)

*Nikmati petualangan di Sol3 dengan teks Bahasa Indonesia yang presisi, launcher yang responsif, serta optimasi performa tinggi tanpa mengganggu pengalaman bermain game Anda.*

---

[Fitur Utama](#-fitur-utama) • [Cara Penggunaan](#-cara-penggunaan) • [Persyaratan Sistem](#-persyaratan-sistem) • [Pengembangan & Build](#-pengembangan--build) • [Benchmark & Optimasi](#-benchmark--optimasi) • [Struktur Proyek](#-struktur-direktori-proyek) • [Kredit](#-kredit) • [Lisensi](#-lisensi)

</div>

---

## 📌 Tentang Proyek

**WuwaID Launcher** adalah aplikasi launcher kustom berbasis WPF dan Microsoft Edge WebView2 yang dirancang khusus untuk mempermudah komunitas Indonesia dalam menginstal, memperbarui, dan mengelola patch terjemahan Bahasa Indonesia untuk game **Wuthering Waves**.

Launcher ini dibuat dengan fokus utama pada **kecepatan, verifikasi integritas data, efisiensi konsumsi resource (RAM & CPU), serta tampilan antarmuka modern yang interaktif**.

---

## ✨ Fitur Utama

### 🛠️ Manajemen Patch & Engine Mod
- **Instalasi & Perbaruan Sekali Klik:** Mengunduh, memverifikasi integritas file, dan menerapkan patch Bahasa Indonesia secara cepat dan otomatis.
- **Tiga Mode Instalasi (`method1/2/3`):**
  - **Method 1** (default) — PAK kanonik diunduh ke folder game + bypass signature.
  - **Method 2** — manual loader `winhttp.dll` untuk game yang tidak bisa diganti signature-nya.
  - **Method 3 (eksperimental)** — *Resource Mount*: pak + sig + mount-file ditulis ke folder resource versi game aktif, tanpa bypass signature, dengan owner-marker & verifikasi SHA-1 mount.
- **Deteksi Folder Otomatis:** Otomatis mencari dan mengenali jalur instalasi Wuthering Waves melalui registry sistem dan lokasi default Windows.
- **Verifikasi SHA256 Checksum:** Setiap file patch diverifikasi terhadap manifest checksum (`SHA256sums.txt`) sebelum diterapkan; file yang tidak cocok langsung ditolak dan dihapus.
- **Verifikasi Self-Update:** ZIP update launcher juga diverifikasi SHA-256 terhadap manifest sebelum diekstrak & dieksekusi.
- **Engine PAK Packer Kustom:** Dilengkapi dengan modul internal (`WuwaPakPacker`) berbasis algoritma hash FNV64 & SHA-1 untuk pengelolaan paket patch secara efisien.

### ⚡ Mode Tray & Penghematan Resource Game
- **WebView2 Suspension:** Saat game diluncurkan, proses WebView2 langsung ditangguhkan (*suspended*) untuk membebaskan konsumsi RAM dan CPU.
- **Sistem Tray Cerdas:** Launcher otomatis meminimalkan diri ke System Tray dengan footprint resource yang sangat minim (penurunan Working Set RAM > 25%).
- **Heartbeat Pemain Aktif Anonim:** Tetap mengirimkan pingsan anonim setiap 5 menit untuk menghitung jumlah pemain aktif secara real-time tanpa mengganggu performa jaringan game.

### 🛡️ Keamanan, Diagnostik & Hak Akses
- **Enkripsi Aset Frontend Build Time:** Seluruh aset HTML, CSS, dan JavaScript UI dienkripsi menggunakan *MSBuild Custom Task* (`XorEncryptFiles`) sebelum di-embed ke dalam binary aplikasi.
- **Elevasi Administrator:** Menyediakan menu internal untuk memuat ulang launcher dengan hak akses Administrator (*Run as Administrator*) apabila diperlukan untuk akses folder game.
- **Pengumpul & Pengunggah Log Diagnostik:** Fitur pengumpul log terpadu (`LogUploadService` & `GameLogCollector`) untuk mengompresi log aplikasi dan game ke file ZIP guna mempermudah diagnostik dan bantuan kendala.

---

## 🚀 Cara Penggunaan

### 1️⃣ Jalankan Launcher
1. Unduh file `WuwaIDLauncher.exe` dari halaman perilisan resmi.
2. Jalankan file `.exe` (aplikasi bersifat *portable / self-contained* dan tidak memerlukan instalasi rumit).

### 2️⃣ Pilih Direktori Game
1. Launcher akan mencoba mendeteksi direktori instalasi Wuthering Waves secara otomatis.
2. Jika direktori tidak terdeteksi otomatis, klik **Pilih Folder Game** dan arahkan ke lokasi folder utama Wuthering Waves (folder tempat `Client-Win64-Shipping.exe` berada).

### 3️⃣ Instal Patch & Bermain
1. Klik tombol **Instal Patch ID** (atau **Perbarui Patch** jika tersedia versi baru).
2. Tunggu proses pengunduhan dan verifikasi file hingga selesai.
3. Klik **Mainkan** untuk meluncurkan game secara langsung melalui launcher!

---

## 💻 Persyaratan Sistem

| Komponen | Persyaratan Minimum | Rekomendasi |
| :--- | :--- | :--- |
| **Sistem Operasi** | Windows 10 (64-bit) | Windows 11 (64-bit) |
| **Arsitektur** | x86_64 / x64 | x86_64 / x64 |
| **Runtime** | [.NET 8 Desktop Runtime](https://dotnet.microsoft.com/download/dotnet/8.0) *(Sudah include di Self-Contained)* | [.NET 8 Desktop Runtime](https://dotnet.microsoft.com/download/dotnet/8.0) |
| **WebView2** | [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) | WebView2 Runtime versi terbaru |

> ℹ️ **Catatan:** Executable rilis produksi didistribusikan dalam bentuk *Self-Contained Single File*, sehingga pengguna tidak perlu menginstal .NET 8 Runtime secara manual.

---

## 🏗️ Pengembangan & Build

Bagi Anda yang ingin berkontribusi atau melakukan kompilasi mandiri dari source code:

### Prasyarat Build
- **Visual Studio 2022** (dengan beban kerja *.NET Desktop Development*) atau **.NET 8.0 SDK**.
- **Windows 10/11 SDK**.

### Langkah Kompilasi
1. Clone repositori ini:
   ```bash
   git clone https://github.com/TitoTFP/WuwaIDLauncher.git
   cd WuwaIDLauncher
   ```

2. Restore dependency dan build proyek:
   ```powershell
   dotnet build -c Release
   ```

3. Untuk mempublikasikan executable *Single-File* terkompresi (versi rilis produksi):
   ```powershell
   dotnet publish -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true
   ```

> 🔐 **Enkripsi Aset MSBuild:**
> Sebelum kompilasi C# berjalan, task `XorEncryptFiles` secara otomatis mengamankan seluruh file di `Resources/Web/` menggunakan kunci XOR dan menyimpannya sebagai `EmbeddedResource`.

### Pengujian Otomatis & CI

CI (GitHub Actions) menjalankan pengujian pada setiap *build* dan sebagai **gate** sebelum rilis.

1. **Konsistensi & kebijakan statis:**
   ```powershell
   ./tests/verify_launcher_consistency.sh
   ```
2. **Unit test:**
   ```powershell
   dotnet test .\WuwaIDLauncher.Tests\ -c Release
   ```
3. **End-to-end (pipeline asli, tanpa UI):**
   ```powershell
   $env:WUWAID_E2E_APPDATA = "$env:TEMP\wuwaid-e2e"
   .\publish\WuwaIDLauncher.exe --e2e
   Write-Host "exit=$LASTEXITCODE"   # 0 = lulus
   ```
   Mode `--e2e` menjalankan seluruh jalur nyata (unduh → verifikasi SHA-256 → pasang → tulis version cache → bersihkan metode lain) untuk **method1/2/3** plus verifikasi **self-update ZIP**, terhadap *stub server* lokal, lalu keluar dengan kode 0/1. Hasil per-skenario ditulis ke `e2e-results.txt` di AppData yang diarahkan env di atas.

---

## 📊 Benchmark & Optimasi

Repositori ini menyertakan skrip otomasi pengujian untuk mengukur durasi startup serta efisiensi memori/CPU:

### Uji Varian Startup
Jalankan pengujian varian *Compressed* dan *Uncompressed*:
```powershell
.\tests\build_benchmark_variants.ps1
```

Jalankan pengukuran startup menggunakan skrip PowerShell:
```powershell
# Pengujian versi compressed
.\tests\measure_startup.ps1 -ExePath .\publish\benchmark\compressed\WuwaIDLauncher.exe -Runs 6 -ProfileMode CleanEveryRun -OutputCsv compressed-clean.csv
.\tests\measure_startup.ps1 -ExePath .\publish\benchmark\compressed\WuwaIDLauncher.exe -Runs 6 -ProfileMode CleanFirst -OutputCsv compressed-clean-first.csv
.\tests\measure_startup.ps1 -ExePath .\publish\benchmark\compressed\WuwaIDLauncher.exe -Runs 6 -ProfileMode Warm -OutputCsv compressed-warm.csv
.\tests\measure_startup.ps1 -ExePath .\publish\benchmark\compressed\WuwaIDLauncher.exe -Runs 6 -ProfileMode Warm -MinimizeAfterInteractive -OutputCsv compressed-minimized.csv
```

### Kriteria Kinerja Saat Game Berjalan
Pengujian dampak ke game dilakukan menggunakan **PresentMon** pada proses `Client-Win64-Shipping.exe`:
- **Beban CPU/GPU Launcher:** Wajib berada di bawah **1%** saat game berjalan di background.
- **Efisiensi RAM:** *Working Set* memori launcher berkurang sekurang-kurangnya **25%** saat berada di tray mode.
- **Stabilitas FPS Game:** Frametime P99 dan 1% Low game tidak boleh mengalami regresi lebih dari **2%**.

---

## 📁 Struktur Direktori Proyek

```text
WuwaIDLauncher/
├── 📄 MainWindow.xaml / .cs       # UI Utama WPF & Host WebView2, jalur patch/update
├── 📄 InstallMethods.cs           # Pemetaan method1/method2/method3
├── 📄 ResourceMountInstaller.cs   # Instalasi Resource Mount (method3)
├── 📄 Helpers.cs                  # Path game/patch & utilitas SHA-256
├── 📄 OptimizationServices.cs     # Status patch, version cache, manifest checksum
├── 📄 ActivePlayerService.cs      # Layanan Heartbeat Anonim Pemain Aktif
├── 📄 LogUploadService.cs         # Service Kompresi & Unggah Log Diagnostik
├── 📄 GameLogCollector.cs         # Pengumpul Log Otomatis Game & Launcher
├── 📄 WuwaPakPacker.cs            # Engine Internal Pembentuk PAK Patch File
├── 📄 AppLogger.cs                # Sistem Logging Internal
├── 📄 E2eConfig.cs / E2eStubServer.cs / E2eRunner.cs   # Mode uji --e2e
├── 📁 WuwaIDLauncher.Tests/       # Unit test (xunit)
├── 📁 Resources/
│   ├── 📁 Images/                 # Asset Gambar & Ikon Utama WPF
│   └── 📁 Web/                    # Source Code UI (HTML, CSS, JS) WebView2
├── 📁 tests/                      # Skrip Benchmark, Konsistensi & Verifikasi
├── 📁 .github/workflows/          # CI: release, testing, consistency
├── 📄 CONTEXT.md + 📁 docs/adr/   # Glosarium konteks & keputusan (ADR)
└── 📄 WuwaIDLauncher.csproj       # Definisi Proyek .NET 8 & Enkripsi Build MSBuild
```
```

---

## 🤝 Kredit & Apresiasi

Terima kasih kepada proyek-proyek hebat berikut yang memberikan inspirasi dan dukungan teknologi:

- **[AlteriaX/WuWa-Configs](https://github.com/AlteriaX/WuWa-Configs)** — Menyediakan preset konfigurasi game yang digunakan pada fitur *High Performance Mode*.
- **[CallMeDangDev](https://github.com/CallMeDangDev)** — Referensi utama arsitektur launcher melalui repositori [WuwaVHLauncher](https://github.com/CallMeDangDev/WuwaVHLauncher).
- **Komunitas & Penerjemah Wuthering Waves Indonesia** — Atas dedikasi dan kerja keras dalam menghadirkan terjemahan Bahasa Indonesia yang berkualitas.

---

## 📜 Lisensi

Proyek ini dilisensikan di bawah lisensi open-source **[GNU General Public License v3.0 (GPL-3.0)](LICENSE)**.

Anda diperbolehkan untuk menggunakan, mempelajari, memodifikasi, serta membagikan ulang perangkat lunak ini selama menyertakan hak lisensi terbuka yang sama dan mencantumkan kredit ke pembuat asli.

---

<div align="center">
  Dibuat dengan ❤️ untuk Komunitas Wuthering Waves Indonesia
</div>
