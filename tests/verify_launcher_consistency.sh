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

echo "launcher consistency checks passed"
