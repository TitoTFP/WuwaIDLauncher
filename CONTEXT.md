# Konteks — WuwaIDLauncher

Launcher desktop Windows (WPF + WebView2) untuk patch terjemahan Indonesia game Wuthering Waves (WuwaID).

## Glosarium

- **Install Method** — cara patch dipasang ke folder game: `method1` (PAK canonical + signature bypass, UI Metode 3), `method2` (manual loader `winhttp.dll`, UI Metode 2), `method3` (Resource Mount tanpa signature bypass, UI Metode 1). Dikenali lewat `InstallMethods.Normalize`.
- **Resource Mount** — (internal `method3`, UI Metode 1) menulis pak + sig + mount-file ke folder resource versi game yang aktif (`Client/Saved/Resources/<versi>/`), diverifikasi via owner marker `.wuwaid-resource-mount` dan konsistensi SHA-1 mount. Dibedakan dari instalasi metode lain lewat `ResourceMountInstaller.IsManaged`.
- **Resource-ready** — kondisi folder resource game yang siap dipatch internal `method3` (UI Metode 1): ada `ResManifest`, folder `Mount`, dan signature resmi (`Resource/Base/*.sig` + pak berpasangan + entri mount yang cocok SHA-1).
- **Version Cache** — `versions.json` di AppData; menyimpan fingerprint SHA-256 tiap asset terpasang plus `_installMethod` dan `_vhVersion`. Sumber kebenaran status patch.
- **SHA256sums.txt** — manifest checksum release; format `<sha256>  <nama-asset>` per baris. Dipakai untuk verifikasi PAK, loader, dan zip self-update sebelum dieksekusi.
- **Stub Server** — HTTP server tiruan dalam proses (`E2eStubServer`) yang menggantikan GitHub saat mode E2E; bisa mengeluarkan checksum keliru untuk menguji jalur penolakan.
- **E2E mode** — mode headless `--e2e` (lihat ADR 0001): proses menjalankan pipeline nyata tanpa WebView2, mengembalikan exit code 0/1 untuk gate CI.

## Keputusan arsitektur

Lihat `docs/adr/` — terkait pengujian: `0001-e2e-test-suite.md`.
