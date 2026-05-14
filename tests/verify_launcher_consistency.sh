#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

if rg -n "wuwaVietHoa" MainWindow.xaml.cs Resources/Web | rg -v "LegacyModFolderName"; then
  fail "legacy mod folder name must only remain as migration fallback"
fi

if rg -n "Lỗi|Đang|Hoàn|Bạn đang|Thư mục|Quyền|Khởi động|Cập nhật|Không|Xoá|Chưa|Đã|Huỷ|Xác nhận|tuỳ|gốc|rỗng" MainWindow.xaml.cs Resources/Web; then
  fail "Vietnamese UI/status text must be translated to Indonesian"
fi

if rg -n "[ÀÁÂÃÈÉÊÌÍÒÓÔÕÙÚĂĐĨŨƠƯàáâãèéêìíòóôõùúăđĩũơưẠ-ỹ]" MainWindow.xaml.cs Resources/Web; then
  fail "Vietnamese diacritics must not remain in launcher UI/status files"
fi

if ! rg -n "VerifySha256\(destPath, hash\)" MainWindow.xaml.cs >/dev/null; then
  fail "installer must verify local files against release SHA256"
fi

if ! rg -n "Hash file .* tidak cocok" MainWindow.xaml.cs >/dev/null; then
  fail "installer must reject mismatched downloaded files"
fi

if rg -n 'name == "version\.dll"|name == "version\.dll" \?' MainWindow.xaml.cs >/dev/null; then
  fail "installer must not download or route version.dll"
fi

if ! rg -n 'PakFolderRelativePath = @"Client\\Content\\Paks"' MainWindow.xaml.cs >/dev/null; then
  fail "installer must target Client\\Content\\Paks"
fi

if rg -n 'Path\.Combine\(gamePath, @"Client\\Binaries\\Win64", MainWindow\.ModFolderName\)' MainWindow.xaml.cs >/dev/null; then
  fail "mod pak operations must not target Client\\Binaries\\Win64\\wuwaIndonesia"
fi

if ! rg -n 'PakFileName = "pakchunk0-ID-WindowsNoEditor_1000_P\.pak"' MainWindow.xaml.cs >/dev/null; then
  fail "installer must use pakchunk0-ID-WindowsNoEditor_1000_P.pak"
fi

if rg -n 'internal const string PakFileName = "WuWaID_99_P\.pak"' MainWindow.xaml.cs >/dev/null; then
  fail "installer must not use legacy WuWaID_99_P.pak as primary pak"
fi

if ! rg -n 'LegacyPakFileName = "WuWaID_99_P\.pak"' MainWindow.xaml.cs >/dev/null; then
  fail "installer must keep legacy pak name only for cleanup"
fi

if ! rg -n 'SigFileName = "pakchunk7-WindowsNoEditor\.sig"' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must use pakchunk7 signature file"
fi

if ! rg -n 'SigBackupFileName = "pakchunk7-WindowsNoEditor_backup\.sig"' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must use pakchunk7 signature backup file"
fi

if ! rg -n 'SigRestoreDelay = TimeSpan\.FromSeconds\(150\)' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must restore signature after 150 seconds"
fi

if ! rg -n 'RestoreSigBackup\(gamePath\)' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must restore stale signature backups"
fi

if ! rg -n 'Verb = "runas"' MainWindow.xaml.cs >/dev/null; then
  fail "game launch must use runas like PT-BR launcher"
fi

if ! rg -n 'Arguments = dx11 \? "-dx11" : ""' MainWindow.xaml.cs >/dev/null; then
  fail "game launch must only pass -dx11 when selected and no args otherwise"
fi

if rg -n -- '-SkipSplash|-dx12' MainWindow.xaml.cs >/dev/null; then
  fail "game launch must not force -SkipSplash or -dx12"
fi

if ! rg -n '_launchInProgress' MainWindow.xaml.cs >/dev/null; then
  fail "game launch must block second launch while signature restore timer is active"
fi

if rg -n 'Application\.Current\.Shutdown\(\)' MainWindow.xaml.cs | rg -n 'RestoreSigBackupAfterDelay|MonitorLaunchStateAsync' >/dev/null; then
  fail "launcher must not exit after launch signature restore"
fi

if ! rg -n 'WindowState = WindowState\.Minimized' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must minimize during launch flow"
fi

if ! rg -n 'Signature file tidak terdeteksi, jalankan Wuthering Waves dulu tanpa mod atau launcher ini\.' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must block launch when signature and backup are missing"
fi

if ! rg -n 'window\.onGameLaunchStarted\(\)' MainWindow.xaml.cs Resources/Web/script-home.js >/dev/null; then
  fail "launcher must notify UI when game launch starts"
fi

if ! rg -n 'window\.onGameLaunchWaitingRestore\(\)' MainWindow.xaml.cs Resources/Web/script-home.js >/dev/null; then
  fail "launcher must notify UI when waiting for signature restore"
fi

if ! rg -n 'window\.onGameLaunchFinished\(\)' MainWindow.xaml.cs Resources/Web/script-home.js >/dev/null; then
  fail "launcher must notify UI when game launch lock ends"
fi

if ! rg -n 'Game sedang berjalan' Resources/Web/script-home.js >/dev/null; then
  fail "launch button must show game running state"
fi

if ! rg -n 'Memulihkan signature' Resources/Web/script-home.js >/dev/null; then
  fail "launch button must show signature restore state"
fi

if ! rg -n 'S\.launching' Resources/Web/script-core.js Resources/Web/script-home.js >/dev/null; then
  fail "web UI must track launch lock state"
fi

if ! rg -n 'e\.Cancel = true' MainWindow.xaml.cs >/dev/null; then
  fail "launcher close must be cancellable during pending signature restore"
fi

if ! rg -n 'RequestCloseWindow' MainWindow.xaml.cs >/dev/null; then
  fail "launcher close button must respect launch lock"
fi

if rg -n 'if \(name == "UTMAlexander_100_P\.pak"\)' MainWindow.xaml.cs >/dev/null; then
  fail "installer must not special-case the repository font pak"
fi

if rg -n 'name == "UTMAlexander_100_P\.pak"\s*&&' MainWindow.xaml.cs >/dev/null; then
  fail "repository font skip must not depend on custom font detection"
fi

if rg -n '\|\|\s*name == "UTMAlexander_100_P\.pak"' MainWindow.xaml.cs >/dev/null; then
  fail "repository font pak must not be part of installer download whitelist"
fi

if rg -n '`VH \$\{vhVer\}`' Resources/Web/script.js Resources/Web/script-home.js >/dev/null; then
  fail "version label must use ID prefix, not VH"
fi

if ! rg -n '`ID \$\{vhVer\}`' Resources/Web/script.js Resources/Web/script-home.js >/dev/null; then
  fail "version label must show ID prefix"
fi

if rg -n 'trbtnOverlay|trbtn-overlay\.min\.js|edge-cdn\.trakteer\.id' Resources/Web MainWindow.xaml.cs >/dev/null; then
  fail "Trakteer custom button must not load the hosted overlay widget or CDN assets"
fi

if ! rg -n 'id="trakteerModal"' Resources/Web/index.html >/dev/null; then
  fail "Trakteer button must open an internal launcher modal"
fi

if ! rg -n 'TRAKTEER_URL = .https://trakteer\.id/v1/TitoTFP/tip/embed/modal' Resources/Web/script-home.js >/dev/null; then
  fail "Trakteer modal must use the embed modal URL"
fi

if ! rg -n 'trakteerFrame\.src = TRAKTEER_URL' Resources/Web/script-home.js >/dev/null; then
  fail "Trakteer modal must load the iframe from the launcher click handler"
fi

if ! rg -n 'frame-src https://trakteer\.id' MainWindow.xaml.cs >/dev/null; then
  fail "CSP must allow Trakteer iframe content"
fi

if ! rg -n -- '--rp-w: 400px' Resources/Web/styles-base.css Resources/Web/styles.css >/dev/null; then
  fail "right action panel must stay wide enough for Trakteer and install buttons"
fi

if ! rg -n 'min-width: 210px' Resources/Web/styles-effects.css Resources/Web/styles.css >/dev/null; then
  fail "install button must keep a readable minimum width"
fi

echo "launcher consistency checks passed"
