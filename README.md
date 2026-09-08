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
- 💤 Otomatis masuk **system tray** saat game berjalan
- 🩺 Diagnostics lokal untuk membantu troubleshooting

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

**Requirements:** Node.js 20+, Rust 1.97.1+, dan Windows toolchain.

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

Build binary Windows:

```bash
npm run tauri -- build --no-bundle
```

### Tech Stack

`Tauri v2` · `Rust` · `Svelte 5` · `TypeScript` · `Vite`

## 🔒 Privasi

WuwaID Launcher **tidak mengunggah diagnostics atau log lokal**.

Statistik active player hanya menggunakan heartbeat minimal seperti ID acak launcher, versi launcher, metode instalasi, dan jenis event — tanpa mengirim path game, username Windows, akun game, atau isi log.

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
