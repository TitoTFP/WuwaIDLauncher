# ADR-0001: End-to-end test suite via in-app `--e2e` mode

- Status: Accepted
- Date: 2025-08-06
- Area: Testing / CI

## Context

Unit tests (xunit) menutup helper & status evaluator, dan `verify_launcher_consistency.sh` memeriksa kebijakan statis, tapi tidak ada yang menguji **pipeline asli** launcher: unduh → verifikasi SHA-256 → tempatkan file → tulis version cache → bersihkan artefak metode lama → self-update zip. Regresi di jalur ini (URL salah, checksum reject lemah, cleanup antar-metode) hanya ketahuan setelah rilis.

Opsi yang ditolak:
- **GUI automation** (WinAppDriver/Appium atas WebView2): flaky, butuh desktop session, nilai rendah untuk pipeline.
- **Test project terpisah** yang memanggil method internal: memaksa banyak `private` menjadi `internal` tanpa manfaat nyata dibanding mode in-app.

## Decision

Pipeline E2E berjalan **di dalam proses asli** lewat flag CLI `--e2e`:

1. `App.OnStartup` mendeteksi `--e2e`, mem-bypass WebView2/debugger/mutex, menjalankan `E2eRunner.RunCoreAsync()`, lalu `Shutdown(exitCode)`.
2. `E2eConfig.BaseUrlOverride` mengarahkan seluruh download release ke `E2eStubServer` (HttpListener in-proses) sehingga deterministik dan offline.
3. `MainWindow` di-instansiasi tanpa `Show()`; `RunScript` no-op saat E2E; alur instalasi/update nyata (`RunInstallation`, `RunResourceMountInstallation`, `PerformLauncherUpdate`) dijalankan apa adanya.
4. Assertion pada **side effect di disk** (lokasi file + SHA-256, `versions.json`, mount artifacts, hasil cleanup) — bukan pada nilai return, karena flow menelan exception dan hanya lapor via bridge.
5. AppData diarahkan ke temp via env `WUWAID_E2E_APPDATA`.
6. Skenario: method1, method2 (termasuk cleanup method1), reject checksum PAK keliru, method3 resource mount, self-update zip sukses, reject checksum zip keliru.
7. CI: `release.yml` dan `testing.yml` menjalankan `publish/WuwaIDLauncher.exe --e2e` dan gate pada exit code.

Keputusan sengaja ditunda (bukan v1): E2E frontend web (Playwright + stub bridge), media download, dan smoke "live GitHub".

## Consequences

- **Positif**: jalur kritis teruji pada biner ter-publish sebelum rilis; deterministik; murah (in-proses, ~detik).
- **Negatif**: tidak menguji lapisan WebView2/UI; URL produksi kini properti statis yang bisa di-override (risiko kecil — default tidak berubah).
- **Ganti jika**: bila UI/web-frontend butuh jaminan, tambah suite Playwright terpisah, bukan memperluas mode ini.
