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
