# Dead-code audit

Status: selesai dengan cleanup konservatif.

## Batas

- Seluruh file tracked dan artifact lokal di bawah root repo diaudit.
- `.git`, path di luar root, serta data game/pengguna eksternal tidak disentuh.
- Item dihapus hanya jika tidak memiliki referensi kode/config/test/packaging yang terverifikasi.

## Validation bootstrap

`package.json` menjalankan `scripts/run-frontend-gate.mjs` untuk `check` dan `build`. Jika `node_modules/` sudah ada, script meneruskan langsung ke tool asli. Jika dependency lokal sudah dibersihkan, script melakukan `npm ci` sementara di direktori root yang dihapus dalam `finally`, menjalankan `svelte-check`/Vite asli, lalu menghapus symlink dependency dan `dist/` hasil build. Ini mempertahankan gate nyata sekaligus memenuhi keadaan akhir tanpa `node_modules/` dan `dist/`; tidak ada fallback atau assertion palsu.

Binary GUI Rust diberi feature `app` dan tidak menjadi target `cargo test --all-targets`; test memakai mock context/assets sehingga test tidak bergantung pada `dist/`. `scripts/run-tauri.mjs` menambahkan feature tersebut untuk `npm run tauri build/dev`, menjaga build produksi tetap memakai `tauri::generate_context!` dan asset frontend nyata.

## Dihapus

| Item | Bukti |
| --- | --- |
| `public/images/header.jpg` | Tidak ada referensi `header.jpg`, `/images/header`, atau `images/header` pada file tracked selain aset itu sendiri. |
| `src-tauri/src/engine/path.rs`: `detect_game_path` dan `scan_windows_registry` | LSP dan pencarian kata utuh menemukan zero caller. Fitur path yang dipakai (`normalize_game_path`) tetap dipertahankan. |
| `src-tauri/src/engine/runtime.rs`: wrapper `find_game_process_id`, `is_game_running`, dan smoke test-nya | Tidak ada caller eksternal; runtime memakai API path-specific `find_game_process_id_for_path` dan `is_game_running_for_path`. |
| `src-tauri/src/lib.rs`: wrapper `media_response` | Tidak ada caller pada source, tests, scripts, atau config; callback Tauri memakai `media_response_from_path` langsung. |
| `src-tauri/src/engine/downloader.rs`: wrapper `download_file_with_expected_size_limited` | Tidak ada caller; semua download path memakai `download_file_with_expected_size_limited_policy` dengan policy eksplisit. |
| `src-tauri/src/engine/installer.rs`: wrapper `loader_marker_path` | Tidak ada caller; installer dan tests memakai `signature::get_loader_marker_path` langsung. |
| Dependency JS `@tauri-apps/plugin-dialog` dan `@tauri-apps/plugin-process` | Tidak ada import/source reference tracked. Plugin Rust `tauri-plugin-dialog` dan `tauri-plugin-process` tetap dipertahankan karena didaftarkan di backend. |
| Dependency Rust langsung `winreg` dan registry autodetection | Tidak ada caller untuk registry autodetection; `winreg` langsung dan implementasi deteksi registry dihapus. Feature Windows `Win32_System_Registry` tetap dipertahankan karena binding `ShellExecuteExW`/`SHELLEXECUTEINFOW` pada crate `windows` mengharuskannya, meskipun launcher tidak membaca registry. |
| CSS stale untuk permukaan UI yang sudah dihapus | Selector legacy tanpa referensi frontend dihapus dari `styles-base.css` (`body.sidebar-open .bg-vignette`, `.ap--playing`, `.ap--muted`, `.ap__dot`, `.ap__ico-*`, `.ap__mute`), `styles-font.css` (`.fc-*`), `styles-panel.css` (`.pm-*` dan warning/badge lama), `styles-effects.css` (ripple/glitch/dekorasi lama), dan `styles-theme.css` (override untuk selector-selector tersebut). Tidak ada setter `sidebar-open`; `.bg-vignette` dasar tetap dipertahankan melalui `BackgroundFx.svelte`. Selector audio aktif `.audio-player`, `.ap-btn`, dan `.ap-vol*` tetap dipertahankan. |
| Empat custom property CSS tanpa referensi `var(...)` | `--glass-border-hover`, `--accent-warm`, `--accent-glow`, dan `--sidebar-w` dihapus dari `styles-base.css` setelah sweep exact-token lintas CSS/Svelte. |
| Custom property CSS `--radius` | Definisi di `styles-base.css` memiliki zero consumer `var(--radius)` pada seluruh source/style tree; dihapus. `--radius-sm` tetap dipertahankan karena dipakai oleh `styles-effects.css`. |
| TypeScript `LauncherUpdateRestartPayload` | Exact-token sweep menemukan hanya deklarasinya di `src/lib/types.ts`, tanpa import, type annotation, runtime access, test, atau config reference; interface dihapus. |

## Recheck setelah auditor

- Exact-token scan CSS menghitung 31 custom-property definitions dan menemukan zero definition tanpa consumer `var(--name)` setelah penghapusan `--radius`; `--radius-sm` memiliki consumer aktif.
- Exact-token scan TypeScript menemukan 31 exported declarations dan tidak menemukan declaration-only candidate setelah penghapusan `LauncherUpdateRestartPayload`.
- Heuristik public Rust menandai `install_patch_transaction` dan `is_safe_download_url` hanya ketika source production dihitung; keduanya memiliki caller eksplisit di `src-tauri/tests/` dan karena itu dipertahankan sebagai test contract, bukan dead code.
- Sweep dependency menemukan seluruh 9 nama dependency `package.json` memiliki token reference pada repo; dependency Rust feature-driven tetap diverifikasi melalui manifest, lockfile, source, dan gate Cargo.
- Recheck selector/CSS, exact tokens, LSP, dan gate tidak menemukan item terbukti-dead tambahan; kandidat yang ambigu tetap tidak dihapus.

## Rekonsiliasi dokumentasi

- `README.md:61` dan `README.md:96` sebelumnya mengklaim autodeteksi Windows Registry/jalur default, tetapi implementasi registry dan default-path detection telah dihapus setelah zero-caller review. Klaim tersebut diperbaiki menjadi perilaku aktual: pengguna memilih folder melalui dialog `browse_game_folder`, lalu launcher memvalidasi `Client-Win64-Shipping.exe`. Tidak ada fitur runtime yang dihapus dari perubahan dokumentasi ini; ini menghilangkan kontradiksi reader-facing.

## Kandidat deferred yang diverifikasi

Sweep awal menandai 12 custom property ber-use rendah/nihil. Empat yang benar-benar zero-reference sudah dihapus di atas. Delapan berikut sengaja dipertahankan karena memiliki consumer aktif lintas stylesheet dan komponen; bukan dead code: `--accent-peach`/`--accent-orange` pada countdown `RightPanel.svelte`, `--text-3` pada audio label/menu disabled, `--green`/`--red` pada `ToastHost.svelte` dan status/menu, `--blue` pada DX11/release/update UI, `--cream` pada header progress `RightPanel.svelte`, serta `--rp-w` pada layout `.right-panel`. Ini dicatat sebagai deferred hanya untuk menghindari penghapusan lintas-file yang tidak aman.

Wrapper public lain yang hanya dipakai oleh test atau runtime dynamic tetap dipertahankan bila bukti static tidak cukup; daftar generated schema, acceptance, dan test-only di bawah adalah contoh utamanya.

## Artifact lokal disposable

Artifact disposable aplikasi yang dibersihkan setelah validasi: `node_modules/`, `dist/`, dan `src-tauri/target/`.

`.pi-glla/` dan `.pi-subagents/` dipertahankan karena merupakan metadata/workspace agent aktif untuk goal dan audit ini, bukan artifact aplikasi; menghapusnya di tengah run dapat merusak state eksekusi.

## Dipertahankan sebagai ambigu/berisiko

- `src-tauri/gen/schemas/*.json`: generated schema tracked; `acl-manifests.json` dipakai langsung oleh acceptance tests dan `desktop-schema.json` direferensikan capability config.
- Semua `src-tauri/tests/*.rs`: ditemukan otomatis oleh Cargo meskipun tidak selalu memiliki referensi teks.
- Acceptance scripts dan workflow CI/release: executable contract, bukan dead file.
- Helper test-only di `src-tauri/src/engine/elevation.rs`: tetap ada karena menjadi cakupan smoke test; penghapusan membutuhkan keputusan perubahan cakupan test.
- Aset Tauri icons, `public/assets/logo.png`, dan `public/images/bg-default.jpg`: direferensikan oleh packaging/UI.

## Validasi

- `npm run check`
- `npm run build`
- `npm run test:patch-status`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1`
- `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `git diff --check`
- `cargo check --locked --features app --bin wuwaid-launcher` dengan frontend `dist/` sementara: lulus.

Catatan: bentuk Cargo yang valid meneruskan argumen test setelah separator `--`: `cargo test --locked --all-targets -- --test-threads=1`. Bentuk literal tanpa separator pada contract tidak dipakai karena ditolak Cargo.

Semua gate di atas lulus setelah perubahan; pemeriksaan akhir juga memastikan `DEAD_CODE_REPORT.md` non-kosong dan `node_modules/`, `dist/`, serta `src-tauri/target/` tidak ada.
