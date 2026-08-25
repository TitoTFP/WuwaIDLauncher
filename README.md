<div align="center">

# 🌊 WuwaID Launcher

**Launcher Resmi & Patch Installer Bahasa Indonesia untuk Wuthering Waves**

[![License: GPL v3](https://img.shields.io/badge/License-GPL_v3-blue.svg)](LICENSE)
[![Tauri Version](https://img.shields.io/badge/Tauri-v2-24C8D8?logo=tauri)](https://v2.tauri.app/)
[![Rust](https://img.shields.io/badge/Backend-Rust_1.97%2B-DEA584?logo=rust)](https://www.rust-lang.org/)
[![Frontend](https://img.shields.io/badge/Frontend-Svelte_5_%2B_TS-FF3E00?logo=svelte)](https://svelte.dev/)
[![Platform](https://img.shields.io/badge/Platform-Windows_x64-0078D6?logo=windows)](https://microsoft.com)
[![Launcher Version](https://img.shields.io/badge/Version-2.9.0-brightgreen)](#)

_Nikmati petualangan di Sol3 dengan teks Bahasa Indonesia yang presisi, launcher ultra-ringan berbasis Tauri v2 & Rust, serta konsumsi resource minimal tanpa mengganggu performa bermain game._

---

[Fitur Utama](#-fitur-utama) • [Cara Penggunaan](#-cara-penggunaan) • [Persyaratan Sistem](#-persyaratan-sistem) • [Pengembangan & Build](#-pengembangan--build) • [Performa](#-performa) • [Struktur Proyek](#-struktur-direktori-proyek) • [Kredit](#-kredit--apresiasi) • [Lisensi](#-lisensi)

</div>

---

## 📌 Tentang Proyek

**WuwaID Launcher** adalah aplikasi launcher generasi baru yang dibangun menggunakan **Tauri v2**, **Rust backend**, dan **Svelte 5 frontend**. Dirancang khusus untuk mempermudah komunitas Indonesia dalam menginstal, memperbarui, dan mengelola patch lokalisasi Bahasa Indonesia untuk game **Wuthering Waves**.

Rebuild dari arsitektur terdahulu (.NET 8 WPF) ke Tauri v2 memberikan fondasi untuk **startup cepat, konsumsi resource rendah saat game berjalan, verifikasi integritas data yang ketat (SHA-256), serta antarmuka modern yang responsif**. Angka resource final tetap perlu diukur pada mesin Windows release.

---

## ✅ Status Implementasi & Release Gate

Kontrak utama launcher sudah diimplementasikan dan diverifikasi melalui test suite Rust, Svelte check, frontend production build, serta cross-target check Windows. Status yang masih membutuhkan mesin game Windows release ditandai sebagai **partial/manual**; ini bukan asumsi bahwa smoke test game nyata sudah lulus.

| Area | Status | Bukti / batasan |
| :--- | :--- | :--- |
| Path game, dua metode, artefak kanonis, transaksi, rollback, switch, uninstall | **Implemented** | src-tauri/tests/milestone1_contract_tests.rs, milestone2_contract_tests.rs, installer safety |
| Launch, external process, force quit, error event | **Implemented + manual game smoke** | Contract tests lulus; perlu executable game nyata untuk taskkill dan lifecycle Windows |
| Launcher Workspace, konfigurasi, navigation lock, progress/error/reset state | **Implemented** | Svelte check 0 error/0 warning |
| Media cache hash, offline fallback, staged replacement, release-note sanitization | **Implemented** | milestone5_contract_tests.rs, media event tests |
| Self-update checksum, ZIP validation, staging, rollback handoff, cleanup | **Implemented + manual restart smoke** | Checksum/ZIP/handoff tests; valid release asset diperlukan untuk restart end-to-end |
| Local runtime diagnostics tanpa upload log | **Implemented** | Isi diagnostics tetap lokal; heartbeat active-player hanya mengirim payload minimal |
| Distribusi ZIP updater dan SHA256 manifest | **Implemented** | Artifact gate dan workflow release; executable tersedia di dalam ZIP; MSI/NSIS sengaja tidak dibuat |
| Real game/admin/read-only/tray/offline/restart acceptance | **Partial / manual** | Jalankan matrix acceptance pada Windows release machine |
| Future features di luar WUT-5 sampai WUT-29 | **Planned** | Tidak menjadi bagian release gate ini |

Acceptance manual Windows tetap diperlukan untuk memvalidasi game nyata, mode admin/read-only, tray, offline, dan restart self-update pada mesin release.

---

## ✨ Fitur Utama

### 🛠️ Manajemen Patch & Engine Mod Terpadu

- **Instalasi & Perbaruan Sekali Klik:** Mengunduh, memverifikasi integritas hash SHA-256, dan menerapkan patch Bahasa Indonesia secara otomatis.
- **Dua Metode Instalasi:** mapping internal memakai identifier semantik berikut; `method2/3` hanya alias legacy yang dimigrasikan saat membaca konfigurasi lama.
  - **Metode 1 — `resource_mount` (Resource Mount):** Deploy file PAK + signature + berkas mount ke folder resource game aktif (`Client/Saved/Resources/<ver>/Mount/`) tanpa menyentuh signature utama game. Dilengkapi proteksi rollback transaksional dan verifikasi integritas struktur Unreal PAK.
  - **Metode 2 — `loader` (Loader):** Menempatkan loader `winhttp.dll` dan folder `wuwaIndonesia/` pada direktori binaries game (`Client/Binaries/Win64/`).
- **Dynamic Method Switcher:** Berpindah metode instalasi secara instan dengan pembersihan artefak pada path kanonis metode sebelumnya. Path kanonis diperlakukan sebagai target launcher, termasuk saat artefak berasal dari launcher lama.
- **Settings Overlay:** Ikon gear accessible membuka pengaturan ringkas untuk metode instalasi, Sembunyikan UID, Optimasi C#, dan DirectX 11. Overlay dapat ditutup melalui tombol tutup, **SELESAI**, klik luar, atau `Esc`.
- **Pemilihan Folder Game:** Membuka dialog folder interaktif untuk memilih lokasi instalasi Wuthering Waves dan memvalidasi folder yang dipilih.
- **Engine PAK Packer & FNV64:** Modul Rust murni untuk pembuatan paket PAK Unreal Engine kompatibel dengan hashing FNV64 & index SHA-1.
- **Opsi Peluncuran:** Toggle opt-in DirectX 11 dan optimisasi environment C# (`-ForceEnableCSharpEnvironment`); keduanya diteruskan sebagai argumen proses tanpa memodifikasi shortcut Steam maupun executable game.
- **Hide UID:** Toggle opt-in memilih PAK terjemahan dengan tiga surface UID database diganti U+3164 Hangul Filler (`ㅤ`). Perubahan varian ditandai sebagai patch yang perlu dipasang ulang, bukan dianggap instalasi siap.

### 🎬 Live Media Ingestion & Dynamic Release Notes

- **Streaming Video Background & BGM:** Mengambil manifest live `assets.json`, mengunduh aset video latar dan musik dengan verifikasi SHA-256 ke cache lokal, dan men-stream melalui protokol `media://` dengan dukungan HTTP Range/206.
- **Dynamic Release Notes (Atom Feed):** Mengambil catatan rilis terbaru langsung dari `releases.atom` GitHub repo WuwaID dan merendernya sebagai Markdown di drawer pengumuman `SidePanel`.
- **Launcher Release Notes:** Saat pertama kali membuka tag rilis launcher baru, menampilkan body kurasi dan automated patch notes dari GitHub Release repo WuwaIDLauncher. Catatan launcher dicache terpisah dan hanya ditampilkan sekali per tag.
- **Countdown Tanggal Update:** Mem-parsing jadwal pembaruan game dari manifest untuk menampilkan hitung mundur waktu rilis patch berikutnya.

### ⚡ Mode Tray & Penghematan Resource Ekstrem

- **Window Minimization ke System Tray:** Launcher otomatis menyembunyikan jendela ke system tray saat game berjalan dan menangguhkan WebView2 untuk menekan penggunaan resource.
- **Operasi Lokal & Active Player:** Launcher menyimpan diagnostics lokal dan tidak mengunggah log. Statistik active player memakai heartbeat minimal tanpa path game, username Windows, akun, atau isi log.

### 🛡️ Keamanan, Diagnostik & Menu Cepat (7 Hamburger Actions)

- **Folder Game:** Dialog pemilih direktori instalasi game interaktif (`rfd`).
- **Perbarui Patch ID & Perbarui Launcher:** Validasi ulang integritas file mod lokal dan pengecekan rilis versi terbaru launcher.
- **Paksa Tutup Game:** Terminasi proses `Client-Win64-Shipping.exe` secara aman jika terjadi crash/hang.
- **Jalankan sebagai Admin:** Alur restart aplikasi dengan elevasi hak akses Administrator Windows (`runas`).
- **Reset Cache Tampilan:** Pembersihan data cache webview dan media cache lokal.
- **Hapus Patch ID:** Penghapusan bersih seluruh artefak mod yang dikelola launcher.

---

## 🚀 Cara Penggunaan

### 1️⃣ Jalankan Launcher

1. Unduh `WuwaIDLauncher-vX.Y.Z.zip` dari halaman GitHub Releases dan verifikasi dengan `SHA256sums.txt`.
2. Ekstrak ZIP, lalu jalankan `WuwaIDLauncher.exe` dari folder hasil ekstraksi.

### 2️⃣ Tentukan Folder Game

1. Klik ikon hamburger di kanan bawah ➔ **Folder Game**.
2. Pilih folder utama tempat `Client-Win64-Shipping.exe` berada; launcher akan memvalidasi folder tersebut.

### 3️⃣ Pilih Metode & Instal Patch

1. Klik ikon gear **Pengaturan**, lalu pilih kartu metode instalasi yang diinginkan.
2. Aktifkan **Sembunyikan UID**, **Optimisasi C#**, atau **DirectX 11** sesuai kebutuhan. Perubahan pengaturan tersimpan otomatis.
3. Klik tombol **Instal Patch ID** (atau **Perbarui Patch**).
4. Setelah selesai, klik **Mainkan** untuk langsung masuk ke Sol3 dalam Bahasa Indonesia!

### Kontrak Asset Patch

Rilis WuwaID menyediakan PAK normal mentah, `winhttp.dll`, dan `SHA256sums.txt`.
Launcher memverifikasi SHA-256 serta struktur PAK normal sebelum menggunakannya.

Jika **Sembunyikan UID** aktif, launcher membuat salinan lokal melalui crate repak V12,
mengubah tepat tiga ID berikut di database terjemahan menjadi U+3164 Hangul Filler yang tidak terlihat,
lalu mengemasnya kembali:

- `Text_FriendMyUid_Text`
- `Text_UserId_Text`
- `PrefabTextItem_1341587207_Text`

PAK hasil Hide UID hanya disimpan di cache launcher dan dipasang dengan nama PAK
normal. Launcher gagal dengan pesan jelas bila database atau salah satu ID target tidak
ada/ambigu; launcher tidak pernah diam-diam mengganti permintaan Hide UID dengan PAK
normal. Release WuwaID tidak perlu menyediakan `WuwaID.zip` atau PAK Hide UID kedua.

### Mapping Metode dan Recovery

Gunakan identifier canonical berikut pada konfigurasi atau command bridge:

- resource_mount — resource mount terisolasi.
- loader — winhttp.dll loader.

method2 dan method3 hanya alias legacy saat migrasi konfigurasi lama. Pemeriksaan update launcher otomatis selalu aktif. Nilai metode yang sudah dihapus dari versi lama dipulihkan ke `resource_mount`. Launcher menolak path yang tidak mengandung executable game, mengelola artefak pada path kanonis metode yang dipilih, dan menulis metadata hanya setelah cleanup berhasil. File pada path kanonis dianggap milik workflow launcher meskipun dibuat oleh versi launcher lama; file di luar path kanonis tidak disentuh. Jika instalasi atau update gagal, jangan hapus file game manual: simpan diagnostics lokal, jalankan pemeriksaan status, dan ulangi setelah penyebab permission/network diperbaiki.

Lokasi artefak runtime:

- Konfigurasi dan versi: %LOCALAPPDATA%/WuwaIDLauncher/.
- Media cache: %LOCALAPPDATA%/WuwaIDLauncher/Cache/.
- Diagnostics lokal dan log runtime: %LOCALAPPDATA%/WuwaIDLauncher/.
- Update staging/handoff sementara: .staging/, update.zip, dan update-handoff.cmd di appdata; artifact gagal dibersihkan otomatis.

### Privacy dan Consent

Launcher tidak mengirim log atau isi diagnostics ke server. Untuk statistik active
player, launcher hanya mengirim `client_id` acak, versi launcher, metode instalasi,
dan jenis event. Tidak ada path game, username Windows, akun, atau isi log yang
dikirim; isi log game tetap lokal dan dapat memuat data yang ditulis oleh game,
jadi tinjau sebelum membagikannya secara manual.

---

## 💻 Persyaratan Sistem

| Komponen           | Persyaratan Minimum                   | Rekomendasi                         |
| :----------------- | :------------------------------------ | :---------------------------------- |
| **Sistem Operasi** | Windows 10 (64-bit)                   | Windows 11 (64-bit)                 |
| **Arsitektur**     | x86_64 / x64                          | x86_64 / x64                        |
| **Web Runtime**    | Microsoft Edge WebView2 (terbawa OS)  | Microsoft Edge WebView2 versi baru  |
| **RAM**            | 50 MB kosong                          | 100 MB kosong                       |

---

## 🏗️ Pengembangan & Build

### Prasyarat

- **Node.js** v20+ dan **npm** / **pnpm**
- **Rust** 1.97.1 melalui `rust-toolchain.toml`
- **Windows SDK** / `x86_64-pc-windows-msvc` target (atau `cargo-xwin` untuk cross-compilation di Linux)

### Langkah Pengembangan Lokal

1. Clone repositori:

   ```bash
   git clone https://github.com/TitoTFP/WuwaIDLauncher.git
   cd WuwaIDLauncher
   ```

2. Instal dependensi frontend:

   ```bash
   npm install
   ```

3. Jalankan aplikasi mode pengembangan (Live Reload):

   ```bash
   npm run tauri -- dev
   ```

### Pengujian & Validasi Kualitas

```bash
# Validasi tipe & komponen Svelte
npm run check

# Build frontend
npm run build

# Test frontend async state contracts
npm run test:patch-status

# Menjalankan seluruh unit test, mock HTTP, command integration, dan installer safety tests
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
```

### Kompilasi Rilis Distribusi (Windows MSVC)

Build native pada Windows:

```bash
# Build binary rilis produksi via Tauri
npm run tauri -- build --no-bundle
```

Cross-build dari Linux menggunakan `cargo-xwin`:

```bash
npm run build

# Rebuild binary MSVC langsung tanpa membuat bundle installer
CARGOFLAGS=--locked npm run tauri -- build \
  --runner cargo-xwin \
  --target x86_64-pc-windows-msvc \
  --no-bundle \
  --ci
```

Output build berada di `src-tauri/target/x86_64-pc-windows-msvc/release/WuwaIDLauncher.exe`.

Artifact release yang diharapkan:

- WuwaIDLauncher-vX.Y.Z.zip (berisi WuwaIDLauncher.exe)
- SHA256sums.txt

Checklist sebelum publish:

- [ ] `npm run check` lulus.
- [ ] `npm run build` lulus.
- [ ] cargo test --locked --all-targets lulus dengan fixture deterministic.
- [ ] ZIP dan SHA256sums.txt ada serta menggunakan versi yang sama.
- [ ] Jalankan acceptance manual pada Windows dengan fixture reversible.
- [ ] Uji read-only/admin, tray, force quit, offline media, dan self-update restart pada mesin release.
- [ ] Tinjau diagnostics lokal sebelum membagikannya secara manual.

Gate otomatis mencakup test suite Rust dengan lockfile, format/lint, `npm run check`,
`npm run build`, dan Windows release gate. Acceptance game nyata tetap memerlukan
mesin Windows release.

---

## 📊 Performa

Perilaku runtime yang diterapkan mencakup target memori rendah WebView2, launcher yang disembunyikan ke system tray saat game berjalan tanpa men-suspend WebView agar event lifecycle tetap diterima, pembacaan media berbasis HTTP Range, capture output proses yang dibatasi, dan response metadata HTTP yang dibaca dengan hard limit. Angka benchmark tidak dicantumkan sampai profiling dilakukan pada mesin Windows release.

---

## 📁 Struktur Direktori Proyek

```text
WuwaIDLauncher/
├── 📁 src/                       # Frontend Svelte 5 + TypeScript
│   ├── 📁 components/            # Komponen UI (TopBar, SettingsOverlay, SidePanel, RightPanel, AudioPlayer, dll.)
│   ├── 📁 lib/                   # Bridge RPC Tauri, State Management (Svelte 5 runes), Types
│   ├── 📁 styles/                # CSS Modular (base, panel, effects)
│   ├── 📄 App.svelte             # Root UI Layout
│   └── 📄 main.ts               # Frontend Entrypoint
├── 📁 src-tauri/                 # Backend Rust & Engine Mod
│   ├── 📁 capabilities/          # Definisi permission & security capability Tauri v2
│   ├── 📁 src/
│   │   ├── 📁 engine/            # Modul inti (downloader, installer, media, pak, path, runtime, updater, dll.)
│   │   ├── 📄 lib.rs             # Registrasi RPC commands, event listeners, dan media protocol
│   │   └── 📄 main.rs            # Application Runner
│   ├── 📁 tests/                 # Integration tests (app command, download, installer safety, media events)
│   └── 📄 Cargo.toml             # Konfigurasi dependensi Rust
├── 📄 package.json               # Konfigurasi npm & dependensi frontend
├── 📄 src-tauri/tauri.conf.json  # Konfigurasi utama Tauri v2 (window size 1280x720, CSP, identifier)
└── 📄 README.md                  # Dokumentasi Proyek
```

---

## 🤝 Kredit & Apresiasi

Terima kasih kepada proyek-proyek berikut atas inspirasi dan ekosistem open-source:

- **[Tauri Apps](https://tauri.app/)** — Framework desktop multi-platform yang cepat dan aman.
- **[Svelte Team](https://svelte.dev/)** — Framework reaktif modern Svelte 5.
- **[CallMeDangDev](https://github.com/CallMeDangDev)** — Referensi arsitektur launcher mod Wuthering Waves.
- **Komunitas & Penerjemah Wuthering Waves Indonesia** — Dedikasi dalam menghadirkan terjemahan Bahasa Indonesia berkualitas bagi pemain Sol3.

---

## 📜 Lisensi

Proyek ini dilisensikan di bawah lisensi terbuka **[GNU General Public License v3.0 (GPL-3.0)](LICENSE)**.

---

<div align="center">
  Dibuat dengan ❤️ untuk Komunitas Wuthering Waves Indonesia
</div>
