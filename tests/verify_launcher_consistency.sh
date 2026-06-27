#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

if ! command -v rg &>/dev/null; then
  rg() {
    local python_cmd
    if command -v python3 &>/dev/null; then
      python_cmd="python3"
    elif command -v python &>/dev/null; then
      python_cmd="python"
    else
      echo "FAIL: neither rg nor python found in path" >&2
      exit 1
    fi
    "$python_cmd" "$(dirname "$0")/rg_fallback.py" "$@"
  }
fi

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

if ! rg -n 'PakFolderRelativePath = @"Client\\Content\\Paks"' MainWindow.xaml.cs Helpers.cs >/dev/null; then
  fail "installer must target Client\\Content\\Paks"
fi

if ! rg -n 'Method2PakFolderPath\(string gamePath\)' Helpers.cs >/dev/null; then
  fail "method 2 must target Client\\Binaries\\Win64\\wuwaIndonesia"
fi

if ! rg -n 'PakFileName = "pakchunk0-ID-WindowsNoEditor_1000_P\.pak"' MainWindow.xaml.cs Helpers.cs >/dev/null; then
  fail "method 1 installer must use pakchunk0-ID-WindowsNoEditor_1000_P.pak"
fi

if rg -n 'internal const string PakFileName = "WuWaID_99_P\.pak"' MainWindow.xaml.cs >/dev/null; then
  fail "method 1 pak name must not be WuWaID_99_P.pak"
fi

if ! rg -n 'ManualPakFileName = "WuWa_ID_99_P\.pak"' MainWindow.xaml.cs Helpers.cs >/dev/null; then
  fail "method 2 installer must use WuWa_ID_99_P.pak"
fi

if ! rg -n 'LegacyPakFileName = "WuWaID_99_P\.pak"' MainWindow.xaml.cs Helpers.cs >/dev/null; then
  fail "launcher must keep old WuWaID_99_P.pak only for cleanup"
fi

if ! rg -n 'WinHttpLoaderFileName = "winhttp\.dll"' MainWindow.xaml.cs Helpers.cs >/dev/null; then
  fail "method 2 installer must use winhttp.dll"
fi

if ! rg -n 'UsesManualLoaderMethod\(method\)' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must branch install and launch behavior by selected method"
fi

if ! rg -n 'SigFileName = "pakchunk7-WindowsNoEditor\.sig"' MainWindow.xaml.cs Helpers.cs >/dev/null; then
  fail "launcher must use pakchunk7 signature file"
fi

if ! rg -n 'SigBackupFileName = "pakchunk7-WindowsNoEditor_backup\.sig"' MainWindow.xaml.cs Helpers.cs >/dev/null; then
  fail "launcher must use pakchunk7 signature backup file"
fi

if ! rg -n 'SigRestoreDelay = TimeSpan\.FromSeconds\(150\)' MainWindow.xaml.cs Helpers.cs >/dev/null; then
  fail "launcher must restore signature after 150 seconds"
fi

if ! rg -n 'RestoreSigBackup\(gamePath\)' MainWindow.xaml.cs Helpers.cs >/dev/null; then
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

if rg -n '`VH \$\{vhVer\}`' Resources/Web/script-home.js >/dev/null; then
  fail "version label must use ID prefix, not VH"
fi

if ! rg -n '`ID \$\{vhVer\}`' Resources/Web/script-home.js >/dev/null; then
  fail "version label must show ID prefix"
fi

if rg -n 'trbtnOverlay|trbtn-overlay\.min\.js|edge-cdn\.trakteer\.id' Resources/Web MainWindow.xaml.cs >/dev/null; then
  fail "Trakteer custom button must not load the hosted overlay widget or CDN assets"
fi

if rg -n 'id="trakteerModal"' Resources/Web/index.html >/dev/null; then
  fail "Trakteer button must open an external link, not an internal modal"
fi

if rg -n 'TRAKTEER_URL|initTrakteerModal|trakteerFrame' Resources/Web/script-home.js >/dev/null; then
  fail "Trakteer must not use modal/iframe — open external link directly"
fi

if rg -n 'frame-src https://trakteer\.id' MainWindow.xaml.cs >/dev/null; then
  fail "CSP must not allow Trakteer iframe (no iframe used)"
fi

if ! rg -n -- '--rp-w: 400px' Resources/Web/styles-base.css >/dev/null; then
  fail "right action panel must stay wide enough for Trakteer and install buttons"
fi

if ! rg -n 'min-width: 210px' Resources/Web/styles-effects.css >/dev/null; then
  fail "install button must keep a readable minimum width"
fi

if [ ! -f AppLogger.cs ]; then
  fail "launcher must include AppLogger.cs"
fi

if ! rg -n 'Path\.Combine\(appDataFolder, "Logs"\)' AppLogger.cs >/dev/null; then
  fail "logger must write under %LOCALAPPDATA%\\WuwaIDLauncher\\Logs"
fi

if ! rg -n 'launcher-\{DateTime\.Now:yyyyMMdd\}\.log' AppLogger.cs >/dev/null; then
  fail "logger must use daily launcher-YYYYMMDD.log files"
fi

if ! rg -n 'AddRedaction\(gamePath, "<GAME_PATH>"\)' AppLogger.cs >/dev/null; then
  fail "logger must redact active game path"
fi

if ! rg -n 'AddRedaction\(Environment\.GetFolderPath\(Environment\.SpecialFolder\.UserProfile\), "<USERPROFILE>"\)' AppLogger.cs >/dev/null; then
  fail "logger must redact user profile paths"
fi

if ! rg -n 'DeleteOldLogs\(TimeSpan\.FromDays\(14\)\)' AppLogger.cs >/dev/null; then
  fail "logger must delete log files older than 14 days"
fi

if ! rg -n 'AppLogger\.Initialize\(WuwaIDLauncher\.MainWindow\.AppDataFolder\)' App.xaml.cs >/dev/null; then
  fail "app startup must initialize file logger"
fi

if ! rg -n 'AppLogger\.SetGamePath\(gamePath\)' MainWindow.xaml.cs >/dev/null; then
  fail "launcher must register active game path for redaction"
fi

if ! rg -n 'AppLogger\.Exception\(ex, "Launcher update failed"\)' MainWindow.xaml.cs >/dev/null; then
  fail "launcher update failures must be logged"
fi

if [ ! -f LogUploadService.cs ]; then
  fail "launcher must include LogUploadService.cs"
fi

if ! rg -n 'internal const string LogUploadEndpoint = "https://logs\.titotfp\.my\.id/api/logs"' LogUploadService.cs >/dev/null; then
  fail "log upload endpoint must point to configured backend"
fi

if rg -n '(token|secret|bearer|authorization|github_pat|ghp_|AKIA|aws_access_key)' LogUploadService.cs >/dev/null; then
  fail "log upload must not embed storage tokens or credentials"
fi

if ! rg -n 'const int MaxLogFiles = 3' LogUploadService.cs >/dev/null; then
  fail "log upload must limit to latest 3 log files"
fi

if ! rg -n 'const long MaxLogBytes = 2 \* 1024 \* 1024' LogUploadService.cs >/dev/null; then
  fail "log upload must cap raw log payload at 2 MB"
fi

if ! rg -n 'MultipartFormDataContent' LogUploadService.cs >/dev/null; then
  fail "log upload must use multipart/form-data"
fi

if ! rg -n 'GetLogUploadEnabled\(\)' MainWindow.xaml.cs >/dev/null; then
  fail "launcher bridge must expose log upload enabled state"
fi

if ! rg -n 'UploadLogs\(string gamePath\)' MainWindow.xaml.cs >/dev/null; then
  fail "launcher bridge must expose manual log upload"
fi

if ! rg -n 'menuUploadLogs' Resources/Web/index.html Resources/Web/script-misc.js >/dev/null; then
  fail "web UI must include manual log upload action"
fi

if ! rg -n 'Log upload belum dikonfigurasi' Resources/Web/script-misc.js MainWindow.xaml.cs LogUploadService.cs >/dev/null; then
  fail "log upload must show disabled/unconfigured state"
fi

if rg -n 'AKIA|SECRET|TOKEN|Bearer |x-amz-credential|github_pat|ghp_' LogUploadService.cs MainWindow.xaml.cs Resources/Web >/dev/null; then
  fail "launcher must not embed storage or GitHub credentials for log upload"
fi

if [ ! -f GameLogCollector.cs ]; then
  fail "launcher must include GameLogCollector.cs"
fi

if ! rg -n 'Client", "Saved", "Logs"' GameLogCollector.cs >/dev/null; then
  fail "game log collector must read Client\\Saved\\Logs"
fi

if ! rg -n 'Client-backup-\*\.log' GameLogCollector.cs >/dev/null; then
  fail "game log collector must include latest Client backup logs"
fi

if ! rg -n 'Client", "Saved", "Crashes"' GameLogCollector.cs >/dev/null; then
  fail "game log collector must inspect Client\\Saved\\Crashes"
fi

if ! rg -n 'CrashContext\.runtime-xml|CrashReportClient\.ini' GameLogCollector.cs >/dev/null; then
  fail "game log collector must include crash text metadata"
fi

if ! rg -n 'CrashSightLog|pipe_client|cgsdk_\.log' GameLogCollector.cs >/dev/null; then
  fail "game log collector must include small auxiliary game logs"
fi

if rg -n '\.dmp|SaveGames|LocalStorage|ManifestResource|ManifestLauncher|ManifestLang|VideoManifest|launcherDownload' GameLogCollector.cs LogUploadService.cs >/dev/null; then
  fail "game log upload must not include dumps, save data, local storage, manifests, or launcher downloads"
fi

if ! rg -n 'SanitizeTextLog' GameLogCollector.cs LogUploadService.cs >/dev/null; then
  fail "game text logs must be sanitized before upload"
fi

if ! rg -n 'Computer|UserName|MachineId|DeviceId|LoginId' GameLogCollector.cs >/dev/null; then
  fail "game log sanitizer must redact machine/user/device/login fields"
fi

if ! rg -n 'GamePathRegex' GameLogCollector.cs >/dev/null; then
  fail "game log sanitizer must redact mapped Wuthering Waves paths"
fi

if ! rg -n 'game/' LogUploadService.cs >/dev/null; then
  fail "game logs must be placed under game/ in upload archive"
fi

if ! rg -n 'launcher/' LogUploadService.cs >/dev/null; then
  fail "launcher logs must be placed under launcher/ in upload archive"
fi

if ! rg -n 'UploadLogs\(S\.gamePath' Resources/Web/script-misc.js >/dev/null; then
  fail "manual log upload must pass selected game path"
fi

echo "launcher consistency checks passed"
