<div align="center">

# 🌊 WuwaID Launcher

### Wuthering Waves, sekarang dalam Bahasa Indonesia.

Launcher resmi **WuwaID** untuk memasang, memperbarui, dan memainkan **Wuthering Waves dengan patch Bahasa Indonesia** secara mudah.

[![Latest Release](https://img.shields.io/github/v/release/TitoTFP/WuwaIDLauncher?style=flat-square&label=Release)](https://github.com/TitoTFP/WuwaIDLauncher/releases/latest)
[![Platform](https://img.shields.io/badge/Platform-Windows%20x64-0078D6?style=flat-square&logo=windows)](https://github.com/TitoTFP/WuwaIDLauncher/releases/latest)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8D8?style=flat-square&logo=tauri)](https://tauri.app/)
[![License](https://img.shields.io/github/license/TitoTFP/WuwaIDLauncher?style=flat-square)](LICENSE)

**[⬇️ Download Latest Release](https://github.com/TitoTFP/WuwaIDLauncher/releases/latest)**

</div>

---

## ✨ Apa itu WuwaID Launcher?

**WuwaID Launcher** adalah companion launcher untuk proyek lokalisasi [WuwaID](https://github.com/TitoTFP/WuwaID).

Alih-alih memasang patch secara manual, launcher menangani proses instalasi, update, verifikasi, dan peluncuran game dari satu tempat.

Dibangun ulang menggunakan **Tauri v2**, **Rust**, dan **Svelte 5** agar tetap cepat, ringan, dan modern.

## 🚀 Fitur

- 🇮🇩 **Install & update patch sekali klik**
- 🔄 **Update otomatis** untuk patch dan launcher
- 🧩 **Dua metode instalasi** — Resource Mount dan Loader
- 🛡️ **Verifikasi SHA-256** dan rollback saat instalasi gagal
- 🎮 **Launch game langsung** dari launcher
- ⚙️ Pengaturan **Custom UID, DirectX 11, dan C# Environment**
- 🎬 Background, BGM, dan release notes dinamis
- 🎨 **Tema tampilan dinamis** — desain per versi game bisa diganti tanpa rilis launcher baru
- 💤 Otomatis masuk **system tray** saat game berjalan
- 🧰 Pesan error peluncuran ringkas untuk membantu troubleshooting

## 🎮 Mulai

1. Download **WuwaIDLauncher** dari [GitHub Releases](https://github.com/TitoTFP/WuwaIDLauncher/releases/latest).
2. Ekstrak `WuwaIDLauncher-vX.Y.Z.zip`.
3. Jalankan `WuwaIDLauncher.exe`.
4. Pilih folder instalasi **Wuthering Waves**.
5. Klik **Instal Patch ID**.
6. Selesai — klik **Mainkan** dan jelajahi Solaris-3 dalam Bahasa Indonesia.

> Distribusi resmi menyediakan `WuwaIDLauncher-vX.Y.Z.zip` beserta `SHA256sums.txt` untuk verifikasi integritas file.

## 🧩 Metode Instalasi

| Metode | Cara Kerja |
| --- | --- |
| **Resource Mount** | Memasang patch melalui resource mount game tanpa mengganti signature utama game. |
| **Loader** | Memuat patch menggunakan `winhttp.dll` pada direktori binary game. |

Keduanya dapat dipilih langsung dari pengaturan launcher.

## 💻 Persyaratan

- **Windows 10/11 64-bit**
- **Microsoft Edge WebView2**
- Wuthering Waves versi PC

## 🛠️ Development

**Requirements:** Node.js 20+, Rust 1.97.1+. Build native Windows memerlukan MSVC toolchain. Cross-build MSVC dari Linux memerlukan `cargo-xwin`, `clang-cl`, dan `lld-link`; install `cargo-xwin` dengan `cargo install cargo-xwin`. `cargo-xwin` mengunduh Windows SDK/CRT bila belum tersedia.

```bash
git clone https://github.com/TitoTFP/WuwaIDLauncher.git
cd WuwaIDLauncher

npm install
npm run tauri -- dev
```

Validasi frontend:

```bash
npm run check
npm run build
```

Build binary Windows dari Windows:

```bash
npm run tauri -- build --no-bundle
```

Build binary Windows MSVC dari Linux x64:

```bash
npm run launcher-build:msvc
```

### Tema tampilan dinamis

Tema bawaan (Umum) selalu jadi fallback. Tema per versi game dikirim lewat
`assets.json` dan hanya dipakai bila manifest-nya bertanda tangan Ed25519
dengan kunci yang tertanam di binary. Cara menulis dan menandatangani tema:
[`docs/theming.md`](docs/theming.md).

```bash
KEY="$HOME/.config/wuwaid-launcher/keys/web-manifest-2026-03.key.pem"
node scripts/sign-manifest.mjs --in Web/assets.json --key "$KEY" \
  --key-id wuwa-web-2026-03
```

### Tech Stack

`Tauri v2` · `Rust` · `Svelte 5` · `TypeScript` · `Vite`

## ✅ Acceptance Game Nyata (Manual Windows Kompatibel)

Acceptance yang benar-benar menjalankan Wuthering Waves dilakukan secara manual pada mesin Windows kompatibel melalui `scripts/acceptance/run-windows-real-acceptance.ps1` dan **tidak dijalankan oleh GitHub Actions**.

```powershell
pwsh -NoProfile -File scripts/acceptance/run-windows-real-acceptance.ps1 `
  -LauncherPath "release-artifacts/v2.11.0/WuwaIDLauncher.exe" `
  -GamePath "D:\Wuthering Waves" `
  -OutputRoot "real-acceptance-evidence/v2.11.0"
```

Ganti `GamePath` dengan instalasi game asli yang kompatibel dan sudah ter-patch. Periksa evidence hasil runner serta UAC, tray, WebView2, dan restart self-update secara manual; build silang Linux dan fixture CI tidak membuktikan acceptance game nyata.

### Persiapan dan publikasi v2.11.0

Catatan rilis terkurasi: [`.github/release-notes/v2.11.0.md`](.github/release-notes/v2.11.0.md).

Sebelum membuat tag:

1. Pastikan versi `package.json`, kedua lockfile, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, dan fallback frontend sama; jalankan `npm run test:version`.
2. Jalankan gate CI, build Windows, dan acceptance manual di atas. Jangan menganggap persiapan Linux sebagai kelulusan Windows release gate.
3. Simpan perubahan persiapan di commit release pada `main` dan pastikan CI commit tersebut lulus. Jangan sertakan private signing key atau artefak build dalam commit.
4. Setelah seluruh gate lulus, buat dan push tag `v2.11.0` pada commit tersebut. **Push tag memulai workflow Release dan dapat memublikasikan rilis** setelah job build dan approval environment `release-production` bila dikonfigurasi.

Workflow `.github/workflows/release.yml` memverifikasi tag terhadap versi checkout, membangun ulang binary Windows dari tag, memeriksa artefak dengan Windows release gate, dan memublikasikan hanya `WuwaIDLauncher-v2.11.0.zip` serta `SHA256sums.txt` menggunakan catatan rilis terkurasi. `workflow_dispatch` ditujukan untuk tag yang sudah ada, bukan dry run.

ZIP hasil cross-build lokal adalah kandidat untuk acceptance, bukan pengganti artefak yang dibangun workflow release. Direktori `release-artifacts/` dan evidence acceptance tidak dilacak Git.

## 🔒 Privasi

WuwaID Launcher tidak lagi mengirim heartbeat statistik ke `logs.titotfp.my.id`.

File diagnostik peluncuran dari versi sebelumnya dibersihkan dari folder data aplikasi saat launcher dimulai; launcher tidak membuat file diagnostik baru.

Launcher tetap menghubungi GitHub untuk memeriksa rilis dan mengunduh update/aset.

## 🤝 Kontribusi

Bug, ide fitur, maupun kontribusi kode sangat diterima.

- [Open an Issue](https://github.com/TitoTFP/WuwaIDLauncher/issues)
- [Pull Requests](https://github.com/TitoTFP/WuwaIDLauncher/pulls)
- [WuwaID Translation Project](https://github.com/TitoTFP/WuwaID)

## 📜 License

WuwaID Launcher tersedia di bawah **[GNU General Public License v3.0](LICENSE)**.

---

<div align="center">

**Dibuat untuk komunitas Wuthering Waves Indonesia 🇮🇩**

*See you in Solaris-3, Rover.*

</div>
