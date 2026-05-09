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

if ! rg -n 'Application\.Current\.Shutdown\(\)' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must exit after restoring signature"
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

echo "launcher consistency checks passed"
