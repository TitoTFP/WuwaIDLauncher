# Desain: Ownership Artefak Berbasis Path Canonical

Tanggal: 2026-08-19  
Status: Menunggu review sebelum implementasi

## Keputusan

Launcher tidak lagi menggunakan ownership marker untuk menentukan apakah
artefak patch boleh diganti atau dihapus. Untuk semua metode, path canonical
yang sudah ditentukan launcher menjadi identitas artefak yang dikelola.

Marker lama tetap dianggap sebagai artefak sementara yang boleh dibersihkan,
tetapi tidak lagi dibuat atau dibutuhkan untuk validasi status instalasi.

## Path yang dikelola

- Resource Mount: file PAK, SIG, dan mount file di resource version aktif,
  termasuk folder patch `wuwaindonesia`.
- Loader: `Client/Binaries/Win64/wuwaIndonesia/WuWaID_99_P.pak`, folder
  loader, dan `Client/Binaries/Win64/winhttp.dll`.
- Signature Bypass: `Client/Content/Paks/pakchunk0-ID-WindowsNoEditor_1000_P.pak`.
- Signature backup tetap dikelola melalui siklus backup dan restore yang sudah
  ada.

## Perilaku instalasi dan migrasi

1. File lama dari launcher WPF maupun file yang dibuat Tauri dapat diadopsi
   berdasarkan path canonical tanpa marker.
2. Instalasi atau update boleh mengganti target canonical yang sudah ada.
3. PAK baru tetap harus lolos verifikasi struktur Unreal PAK dan SHA-256
   release sebelum masuk ke folder game.
4. Penggantian file tetap menggunakan staging, transaksi, snapshot, dan
   rollback apabila deployment atau validasi akhir gagal.
5. Metadata instalasi hanya ditulis setelah deployment dan cleanup berhasil.

## Perilaku cleanup, switch, dan uninstall

- Cleanup menghapus semua artefak pada path canonical, termasuk instalasi
  parsial dan marker lama.
- Switch metode membersihkan seluruh path canonical metode sebelumnya sebelum
  metadata metode baru disimpan.
- Uninstall membersihkan seluruh path canonical dan memulihkan signature asli.
- `preserved` hanya digunakan untuk target yang bukan file atau kegagalan
  filesystem yang nyata, bukan karena marker tidak ditemukan.

## Trade-off keamanan yang disetujui

File pihak ketiga yang kebetulan memakai nama dan path canonical yang sama akan
diperlakukan sebagai artefak launcher dan dapat ditimpa atau dihapus. Risiko
ini diterima untuk mengutamakan migrasi tanpa friksi dari launcher lama.

Perlindungan yang tetap dipertahankan adalah validasi path game, validasi
permission, validasi struktur PAK, checksum asset release, transaksi atomic,
dan rollback.

## Verifikasi

Tambahkan contract tests untuk memastikan:

- setiap metode dapat mengenali instalasi tanpa marker;
- instalasi baru tidak membuat marker ownership;
- target canonical lama dapat diganti oleh asset release baru;
- switch dan uninstall menghapus instalasi lama maupun parsial;
- rollback tetap mengembalikan snapshot ketika validasi deployment gagal;
- test konflik lama diperbarui agar mencerminkan keputusan path canonical.

Release tag `v2.8.0` tetap ditahan sampai test Rust, Svelte check, build
launcher, dan acceptance gate kembali lulus.
