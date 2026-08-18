# F8-01 ACL Command Release Design

## Goal

Memulihkan capability release untuk command yang sudah dipakai UI/bridge, sehingga pengguna dapat memilih folder game, memuat/menyimpan settings, dan membaca runtime state tanpa error `Command ... not allowed by ACL`.

## Root cause

Handler Rust dan registrasi `invoke_handler` untuk `is_game_running`, `browse_game_folder`, `save_settings`, dan `load_settings` sudah ada. `src/lib/bridge.ts` juga memanggil keempat command tersebut. Namun `src-tauri/permissions/app-commands.toml` hanya mengizinkan command lain, sehingga capability `default` yang memakai permission set `app-commands` menolak invoke pada release binary. Generated `src-tauri/gen/schemas/acl-manifests.json` mengikuti allowlist yang sama.

## Chosen approach

Tambahkan tepat empat identifier command yang hilang ke `app-commands.toml`, lalu regenerate artifact ACL melalui Tauri build. Tambahkan contract check tanpa dependency baru yang memeriksa source permission dan generated manifest memuat keempat command. Tidak ada perubahan handler Rust, bridge/UI, atau permission global.

## Verification

1. Contract check harus gagal sebelum patch karena command belum ada di allowlist.
2. Setelah patch dan regeneration, contract check harus lulus.
3. Jalankan `npm run check`, `npm run build`, dan targeted Rust tests; seluruh Rust suite juga dijalankan dan kegagalan baseline dicatat apa adanya.
4. Jalankan `npm run tauri -- build` untuk menghasilkan installer/bundle release baru.
5. Jalankan binary release dengan disposable app-data dan verifikasi `is_game_running`, `load_settings`, `save_settings`, serta alur GUI folder picker/settings tidak lagi ditolak ACL.

## Security boundary

Permission yang ditambahkan dibatasi pada command yang memang sudah menjadi bagian dari bridge/UI. Tidak membuka filesystem, shell, process, atau command lain di luar scope WUT-36.
