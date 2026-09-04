use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::engine::downloader::read_response_body_limited;

pub const GITHUB_API_LATEST_RELEASE: &str =
    "https://api.github.com/repos/TitoTFP/WuwaIDLauncher/releases/latest";
pub const OFFICIAL_GITHUB_OWNER: &str = "TitoTFP";
pub const OFFICIAL_GITHUB_REPOSITORY: &str = "WuwaIDLauncher";
pub const MAX_RELEASE_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_UPDATE_CHECKSUM_BYTES: u64 = 256 * 1024;
pub const MAX_UPDATE_ZIP_COMPRESSED_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_UPDATE_ZIP_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_UPDATE_ZIP_FILE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_UPDATE_ZIP_ENTRIES: usize = 64;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub version: String,
    pub title: String,
    pub date: String,
    pub author: String,
    pub body: String,
    pub zip_url: Option<String>,
    pub checksums_url: Option<String>,
}

pub const RELEASE_EXECUTABLE_NAME: &str = "WuwaIDLauncher.exe";

fn is_release_executable(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(RELEASE_EXECUTABLE_NAME))
}

pub fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn is_safe_download_url(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("https://")
        && trimmed.strip_prefix("https://").is_some_and(|rest| {
            !rest.is_empty() && rest.chars().all(|character| !character.is_whitespace())
        })
}

fn is_launcher_tag(tag: &str) -> bool {
    let tag = tag.trim();
    let Some(version) = tag.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

pub fn expected_zip_asset_name(tag: &str) -> Result<String, String> {
    if !is_launcher_tag(tag) {
        return Err("Tag release launcher tidak valid.".to_string());
    }
    Ok(format!("WuwaIDLauncher-{tag}.zip"))
}

pub fn expected_official_asset_url(tag: &str, asset_name: &str) -> Result<String, String> {
    let expected_zip = expected_zip_asset_name(tag)?;
    if asset_name != expected_zip && asset_name != "SHA256sums.txt" {
        return Err("Nama asset release launcher tidak diizinkan.".to_string());
    }
    Ok(format!(
        "https://github.com/{OFFICIAL_GITHUB_OWNER}/{OFFICIAL_GITHUB_REPOSITORY}/releases/download/{tag}/{asset_name}"
    ))
}

pub fn is_expected_official_asset_url(value: &str, tag: &str, asset_name: &str) -> bool {
    expected_official_asset_url(tag, asset_name).is_ok_and(|expected| value.trim() == expected)
}

pub fn validate_update_request(
    version: &str,
    tag: &str,
    zip_url: &str,
    checksums_url: Option<&str>,
) -> Result<(), String> {
    let expected_zip = expected_zip_asset_name(tag)?;
    let expected_zip_url = expected_official_asset_url(tag, &expected_zip)?;
    let expected_checksums_url = expected_official_asset_url(tag, "SHA256sums.txt")?;
    let expected_version = tag.trim_start_matches('v');
    if version.trim().trim_start_matches('v') != expected_version {
        return Err("Versi update tidak sesuai tag release resmi.".to_string());
    }
    if zip_url.trim() != expected_zip_url {
        return Err("URL ZIP update bukan asset release resmi yang diharapkan.".to_string());
    }
    if checksums_url.map(str::trim) != Some(expected_checksums_url.as_str()) {
        return Err("URL checksum update bukan asset release resmi yang diharapkan.".to_string());
    }
    Ok(())
}

fn is_allowed_github_response_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

pub fn parse_checksum_manifest(content: &str) -> HashMap<String, String> {
    content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }
            let hash = parts[0].trim().trim_start_matches('*').to_ascii_lowercase();
            let file = parts[1].trim().trim_start_matches('*');
            (is_valid_sha256(&hash) && !file.is_empty()).then(|| (file.to_string(), hash))
        })
        .collect()
}

pub fn validate_update_archive(zip_data: &[u8], expected_executable: &str) -> Result<(), String> {
    if zip_data.len() as u64 > MAX_UPDATE_ZIP_COMPRESSED_BYTES {
        return Err("ZIP update melebihi batas ukuran compressed.".to_string());
    }
    if !expected_executable.eq_ignore_ascii_case(RELEASE_EXECUTABLE_NAME)
        || Path::new(expected_executable).file_name().is_none()
    {
        return Err("Nama executable update tidak valid.".to_string());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_data))
        .map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    validate_archive_limits(&mut archive)?;
    let mut found_expected = false;
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| format!("Gagal membaca entry ZIP: {error}"))?;
        let Some(path) = file.enclosed_name() else {
            return Err("ZIP update memiliki path traversal atau path absolut.".to_string());
        };
        if !names.insert(path.to_path_buf()) {
            return Err(format!("ZIP update memiliki entry duplikat: {path:?}"));
        }
        if !file.is_dir()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            if path.file_name().and_then(|name| name.to_str()) == Some(expected_executable) {
                found_expected = true;
            } else {
                return Err(format!("ZIP memuat executable tak dikenal: {:?}", path));
            }
        }
    }
    if !found_expected {
        return Err(format!(
            "ZIP update tidak memuat executable {}.",
            expected_executable
        ));
    }
    Ok(())
}

fn validate_archive_limits<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Result<(), String> {
    if archive.len() > MAX_UPDATE_ZIP_ENTRIES {
        return Err(format!(
            "ZIP update memiliki terlalu banyak entry (maksimum {}).",
            MAX_UPDATE_ZIP_ENTRIES
        ));
    }
    let mut expanded_total = 0u64;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| format!("Gagal membaca metadata entry ZIP: {error}"))?;
        if file.compressed_size() > MAX_UPDATE_ZIP_COMPRESSED_BYTES {
            return Err("Entry ZIP update melebihi batas compressed.".to_string());
        }
        if file.size() > MAX_UPDATE_ZIP_FILE_BYTES {
            return Err("Entry ZIP update melebihi batas ukuran file.".to_string());
        }
        expanded_total = expanded_total
            .checked_add(file.size())
            .ok_or_else(|| "Ukuran expanded ZIP update overflow.".to_string())?;
        if expanded_total > MAX_UPDATE_ZIP_EXPANDED_BYTES {
            return Err("ZIP update melebihi batas ukuran expanded.".to_string());
        }
    }
    Ok(())
}

pub fn create_update_handoff(
    staging_dir: &Path,
    current_executable: &Path,
    handoff_path: &Path,
) -> Result<PathBuf, String> {
    create_update_handoff_impl(staging_dir, current_executable, handoff_path, None)
}

/// Creates a handoff which commits the release-notes transaction only after
/// the replacement executable is running and passes the process health check.
/// The release-state handoff is a tiny CMD bootstrap plus a sibling PowerShell
/// transaction. PowerShell retains the launched process identity, owns all PID
/// and readiness-marker reads, and performs identity-checked tree cleanup before
/// rollback. Failed copy, rollback, launch, or health-check paths remove all
/// release-note state so a failed update cannot surface as What's New later.
pub fn create_update_handoff_with_release_state(
    staging_dir: &Path,
    current_executable: &Path,
    handoff_path: &Path,
    transaction_path: &Path,
    pending_path: &Path,
    ready_marker_path: &Path,
    release_tag: &str,
) -> Result<PathBuf, String> {
    create_update_handoff_impl(
        staging_dir,
        current_executable,
        handoff_path,
        Some((
            transaction_path,
            pending_path,
            ready_marker_path,
            release_tag,
        )),
    )
}

fn release_note_marker_temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("launcher-whats-new-ready.tag");
    path.with_file_name(format!("{name}.tmp"))
}

const RELEASE_HANDOFF_POWERSHELL: &str = r#"
# WUWAID_HANDOFF_TOKEN=__HANDOFF_TOKEN__
$ErrorActionPreference = 'Stop'
$staging = __STAGING__
$staged = __STAGED__
$current = __CURRENT__
$backup = __BACKUP__
$replacement = __REPLACEMENT__
$directory = __DIRECTORY__
$transaction = __TRANSACTION__
$pending = __PENDING__
$ready = __READY__
$readyTemp = __READY_TEMP__
$pidFile = __PID_FILE__
$handoffCmd = __HANDOFF_CMD__
$handoffPs1 = __HANDOFF_PS1__
$handoffToken = __HANDOFF_TOKEN_LITERAL__
$powerShell = $env:WUWAID_POWERSHELL_PATH
$rootProcess = $null
$rootIdentity = $null
$pidTemp = $null
$backupCreated = $false
$rollbackSucceeded = $false
$committed = $false
$exitCode = 1

function Read-StrictPid([string]$Path) {
    if (-not [IO.File]::Exists($Path)) { throw 'PID sidecar missing' }
    $text = [IO.File]::ReadAllText($Path).Trim()
    if ($text -notmatch '\A[1-9][0-9]{0,9}\z') { throw 'PID sidecar invalid' }
    try { return [Convert]::ToUInt32($text, 10) } catch { throw 'PID sidecar out of range' }
}

function Capture-ProcessIdentity([Diagnostics.Process]$Process, [switch]$NeedPath) {
    $Process.Refresh()
    $null = $Process.Handle
    $startTime = $Process.StartTime.ToUniversalTime()
    $path = $null
    try { $path = [IO.Path]::GetFullPath($Process.MainModule.FileName) }
    catch { if ($NeedPath) { throw } }
    [pscustomobject]@{
        Pid = [uint32]$Process.Id
        StartTimeUtc = $startTime
        Path = $path
        Process = $Process
    }
}

function Test-SameProcessIdentity($Expected, $Actual, [switch]$NeedPath) {
    if ($null -eq $Expected -or $null -eq $Actual) { return $false }
    if ($Expected.Pid -ne $Actual.Pid -or $Expected.StartTimeUtc -ne $Actual.StartTimeUtc) { return $false }
    if ($NeedPath -and -not [String]::Equals($Expected.Path, $Actual.Path, [StringComparison]::OrdinalIgnoreCase)) { return $false }
    return $true
}

function Get-ProcessTreePids([uint32]$RootPid) {
    $snapshot = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop)
    $pids = @([uint32]$RootPid)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($item in $snapshot) {
            [uint32]$childPid = $item.ProcessId
            if (($pids -contains [uint32]$item.ParentProcessId) -and -not ($pids -contains $childPid)) {
                $pids += $childPid
                $changed = $true
            }
        }
    }
    return $pids
}

function Stop-VerifiedTree($Identity) {
    if ($null -eq $Identity) { return $true }
    $root = $Identity.Process
    try {
        $live = Capture-ProcessIdentity $root -NeedPath
        if (-not (Test-SameProcessIdentity $Identity $live -NeedPath)) { return $false }
    } catch {
        try {
            if (-not $root.HasExited) { return $false }
            $replacement = Get-Process -Id $Identity.Pid -ErrorAction SilentlyContinue
            if ($null -ne $replacement) { return $false }
        } catch { return $false }
    }

    $killTreeMethod = [Diagnostics.Process].GetMethod('Kill', [Type[]]@([bool]))
    $treeKillIssued = $false
    if ($null -ne $killTreeMethod) {
        try {
            [void]$killTreeMethod.Invoke($root, [object[]]@($true))
            $treeKillIssued = $true
        } catch { }
    }

    if (-not $treeKillIssued) {
        $snapshot = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop)
        $records = @()
        $known = @([uint32]$Identity.Pid)
        $depth = 0
        $changed = $true
        while ($changed -and $depth -lt $snapshot.Count) {
            $changed = $false
            $depth++
            foreach ($item in $snapshot) {
                [uint32]$childPid = $item.ProcessId
                if (($known -contains [uint32]$item.ParentProcessId) -and -not ($known -contains $childPid)) {
                    $known += $childPid
                    try {
                        $child = Get-Process -Id $childPid -ErrorAction Stop
                        $childIdentity = Capture-ProcessIdentity $child
                        $records += [pscustomobject]@{ Depth = $depth; Identity = $childIdentity }
                    } catch { }
                    $changed = $true
                }
            }
        }
        foreach ($record in ($records | Sort-Object Depth -Descending)) {
            try {
                $liveChild = Capture-ProcessIdentity $record.Identity.Process
                if (Test-SameProcessIdentity $record.Identity $liveChild) { $record.Identity.Process.Kill() }
            } catch { }
        }
        if (-not $root.HasExited) {
            try {
                $live = Capture-ProcessIdentity $root -NeedPath
                if (-not (Test-SameProcessIdentity $Identity $live -NeedPath)) { return $false }
                $root.Kill()
            } catch { return $false }
        }
    }

    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $currentTreePids = @(Get-ProcessTreePids $Identity.Pid)
        $remaining = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop | Where-Object { $currentTreePids -contains [uint32]$_.ProcessId })
        if ($remaining.Count -eq 0) { return $true }
        if ($null -ne $killTreeMethod) {
            $rootRemaining = @($remaining | Where-Object { [uint32]$_.ProcessId -eq $Identity.Pid }).Count -gt 0
            if ($rootRemaining) {
                try {
                    $live = Capture-ProcessIdentity $root -NeedPath
                    if (-not (Test-SameProcessIdentity $Identity $live -NeedPath)) { return $false }
                    [void]$killTreeMethod.Invoke($root, [object[]]@($true))
                } catch { return $false }
            }
        } else {
            foreach ($record in ($records | Sort-Object Depth -Descending)) {
                try {
                    $liveChild = Capture-ProcessIdentity $record.Identity.Process
                    if (Test-SameProcessIdentity $record.Identity $liveChild) { $record.Identity.Process.Kill() }
                } catch { }
            }
            $rootRemaining = @($remaining | Where-Object { [uint32]$_.ProcessId -eq $Identity.Pid }).Count -gt 0
            if ($rootRemaining) {
                try {
                    $live = Capture-ProcessIdentity $root -NeedPath
                    if (-not (Test-SameProcessIdentity $Identity $live -NeedPath)) { return $false }
                    $root.Kill()
                } catch { return $false }
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}

function Remove-FileWithRetry([string]$Path) {
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        if (-not (Test-Path -LiteralPath $Path)) { return $true }
        try {
            $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
            if ($item.PSIsContainer) { return $false }
            Remove-Item -LiteralPath $Path -Force -ErrorAction Stop
            if (-not (Test-Path -LiteralPath $Path)) { return $true }
        } catch { }
        Start-Sleep -Milliseconds 100
    }
    return (-not (Test-Path -LiteralPath $Path))
}

function Remove-DirectoryWithRetry([string]$Path) {
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        if (-not (Test-Path -LiteralPath $Path)) { return $true }
        try { Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop } catch { }
        if (-not (Test-Path -LiteralPath $Path)) { return $true }
        Start-Sleep -Milliseconds 100
    }
    return (-not (Test-Path -LiteralPath $Path))
}

function Start-DeferredSelfCleanup {
    $cleanup = @'
$ErrorActionPreference = 'SilentlyContinue'
$targets = @(__HANDOFF_CMD__, __HANDOFF_PS1__)
$token = __HANDOFF_TOKEN_LITERAL__
$until = [DateTime]::UtcNow.AddSeconds(30)
do {
    foreach ($target in $targets) {
        try {
            $item = Get-Item -LiteralPath $target -Force -ErrorAction Stop
            if (-not $item.PSIsContainer -and [IO.File]::ReadAllText($target).Contains($token)) {
                Remove-Item -LiteralPath $target -Force -ErrorAction Stop
            }
        } catch { }
    }
    if (@($targets | Where-Object { Test-Path -LiteralPath $_ }).Count -eq 0) { exit 0 }
    Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $until)
exit 1
'@
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($cleanup))
    Start-Process -FilePath $powerShell -WindowStyle Hidden -ArgumentList @('-NoLogo', '-NoProfile', '-NonInteractive', '-EncodedCommand', $encoded) | Out-Null
}

try {
    if (Test-Path -LiteralPath $pidFile -PathType Container) { throw 'PID sidecar path is a directory' }
    if (Test-Path -LiteralPath ($pidFile + '.tmp') -PathType Container) { throw 'PID temp path is a directory' }
    if (-not (Remove-FileWithRetry $pidFile) -or -not (Remove-FileWithRetry ($pidFile + '.tmp'))) { throw 'PID sidecar cleanup failed' }
    if (-not (Remove-FileWithRetry $readyTemp)) { throw 'readiness marker cleanup failed' }
    if (-not [IO.File]::Exists($staged)) { throw 'staging executable missing' }
    if (-not [IO.File]::Exists($current)) { throw 'current executable missing' }
    if (-not [IO.File]::Exists($transaction)) { throw 'release transaction missing' }

    if (-not (Remove-FileWithRetry $replacement)) { throw 'replacement cleanup failed' }
    if (-not (Remove-FileWithRetry $backup)) { throw 'backup cleanup failed' }
    Copy-Item -LiteralPath $current -Destination $backup -Force
    $backupCreated = $true
    Copy-Item -LiteralPath $staged -Destination $replacement -Force
    Move-Item -LiteralPath $replacement -Destination $current -Force
    $stagedHash = (Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash
    $currentHash = (Get-FileHash -LiteralPath $current -Algorithm SHA256).Hash
    if (-not [String]::Equals($stagedHash, $currentHash, [StringComparison]::OrdinalIgnoreCase)) { throw 'replacement verification failed' }

    $env:WUWAID_LAUNCHER_UPDATE_READY = $readyTemp
    $rootProcess = Start-Process -FilePath $current -WorkingDirectory $directory -NoNewWindow -PassThru
    $rootIdentity = Capture-ProcessIdentity $rootProcess -NeedPath
    $pidTemp = $pidFile + '.tmp.' + [Guid]::NewGuid().ToString('N')
    [IO.File]::WriteAllText($pidTemp, "$($rootIdentity.Pid)`r`n")
    Move-Item -LiteralPath $pidTemp -Destination $pidFile -Force
    if ((Read-StrictPid $pidFile) -ne $rootIdentity.Pid) { throw 'PID sidecar verification failed' }

    $healthy = $false
    $readyDeadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if ([IO.File]::Exists($readyTemp)) {
            if ((Read-StrictPid $readyTemp) -ne $rootIdentity.Pid) { throw 'readiness PID verification failed' }
            $healthy = $true
            break
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $readyDeadline)
    if (-not $healthy) { throw 'readiness marker timeout' }
    if ((Read-StrictPid $pidFile) -ne $rootIdentity.Pid) { throw 'PID sidecar changed' }
    $live = Capture-ProcessIdentity (Get-Process -Id $rootIdentity.Pid -ErrorAction Stop) -NeedPath
    if (-not (Test-SameProcessIdentity $rootIdentity $live -NeedPath)) { throw 'update process identity changed' }

    if (Test-Path -LiteralPath $pending -PathType Container) { throw 'pending release path is a directory' }
    Move-Item -LiteralPath $transaction -Destination $pending -Force
    if (-not [IO.File]::Exists($pending)) { throw 'pending release move failed' }
    if (-not (Remove-FileWithRetry $readyTemp)) { throw 'ready marker temp cleanup failed' }
    [IO.File]::WriteAllText($readyTemp, __RELEASE_TAG__ + "`r`n")
    Move-Item -LiteralPath $readyTemp -Destination $ready -Force
    if (-not [IO.File]::Exists($ready)) { throw 'ready marker move failed' }
    $committed = $true
    $exitCode = 0
} catch {
    $stopped = $true
    if ($null -ne $rootIdentity) { $stopped = Stop-VerifiedTree $rootIdentity }
    elseif ($null -ne $rootProcess) {
        try {
            $killTreeMethod = [Diagnostics.Process].GetMethod('Kill', [Type[]]@([bool]))
            if ($null -eq $killTreeMethod) {
                $stopped = $false
            } else {
                [void]$killTreeMethod.Invoke($rootProcess, [object[]]@($true))
                $rootProcess.WaitForExit(10000)
                $stopped = $rootProcess.HasExited
            }
        } catch { $stopped = $false }
    }
    if ($backupCreated -and $stopped) {
        try {
            Copy-Item -LiteralPath $backup -Destination $current -Force
            $rollbackSucceeded = $true
        } catch { $stopped = $false }
    }
    if (-not $stopped) { $exitCode = 98 }
} finally {
    if ($null -ne $pidTemp) { [void](Remove-FileWithRetry $pidTemp) }
    [void](Remove-FileWithRetry ($pidFile + '.tmp'))
    [void](Remove-FileWithRetry $pidFile)
    [void](Remove-FileWithRetry $replacement)
    if ($committed) {
        [void](Remove-FileWithRetry $backup)
        [void](Remove-DirectoryWithRetry $staging)
    } else {
        [void](Remove-FileWithRetry $readyTemp)
        [void](Remove-FileWithRetry $transaction)
        [void](Remove-FileWithRetry $pending)
        [void](Remove-FileWithRetry $ready)
        if ($rollbackSucceeded -or -not $backupCreated) { [void](Remove-FileWithRetry $backup) }
        [void](Remove-DirectoryWithRetry $staging)
    }
    Start-DeferredSelfCleanup
}
exit $exitCode
"#;

fn powershell_literal(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

fn handoff_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("WUWAID-HANDOFF-{}-{nanos}", std::process::id())
}

struct ReleaseHandoffSpec<'a> {
    staging_dir: &'a Path,
    staged_executable: &'a Path,
    current_executable: &'a Path,
    handoff_path: &'a Path,
    transaction_path: &'a Path,
    pending_path: &'a Path,
    ready_marker_path: &'a Path,
    release_tag: &'a str,
    backup_executable: &'a Path,
    replacement_executable: &'a Path,
    current_directory: &'a Path,
    started_pid_path: &'a Path,
}

fn create_release_update_handoff(spec: ReleaseHandoffSpec<'_>) -> Result<PathBuf, String> {
    let ReleaseHandoffSpec {
        staging_dir,
        staged_executable,
        current_executable,
        handoff_path,
        transaction_path,
        pending_path,
        ready_marker_path,
        release_tag,
        backup_executable,
        replacement_executable,
        current_directory,
        started_pid_path,
    } = spec;
    let handoff_ps1 = handoff_path.with_extension("ps1");
    let paths = [
        staging_dir,
        staged_executable,
        current_executable,
        handoff_path,
        &handoff_ps1,
        transaction_path,
        pending_path,
        ready_marker_path,
        &release_note_marker_temp_path(ready_marker_path),
        backup_executable,
        replacement_executable,
        current_directory,
        started_pid_path,
    ];
    if paths.iter().any(|path| {
        let value = path.to_string_lossy();
        value.contains('\0') || value.contains('\r') || value.contains('\n')
    }) {
        return Err("Path handoff update mengandung karakter yang tidak valid.".to_string());
    }
    let token = handoff_token();
    let mut powershell = RELEASE_HANDOFF_POWERSHELL.to_string();
    let replacements = [
        ("__HANDOFF_TOKEN__", format!("'{}'", token)),
        ("__HANDOFF_TOKEN_LITERAL__", format!("'{}'", token)),
        ("__STAGING__", powershell_literal(staging_dir)),
        ("__STAGED__", powershell_literal(staged_executable)),
        ("__CURRENT__", powershell_literal(current_executable)),
        ("__BACKUP__", powershell_literal(backup_executable)),
        (
            "__REPLACEMENT__",
            powershell_literal(replacement_executable),
        ),
        ("__DIRECTORY__", powershell_literal(current_directory)),
        ("__TRANSACTION__", powershell_literal(transaction_path)),
        ("__PENDING__", powershell_literal(pending_path)),
        ("__READY__", powershell_literal(ready_marker_path)),
        (
            "__READY_TEMP__",
            powershell_literal(&release_note_marker_temp_path(ready_marker_path)),
        ),
        ("__PID_FILE__", powershell_literal(started_pid_path)),
        ("__HANDOFF_CMD__", powershell_literal(handoff_path)),
        ("__HANDOFF_PS1__", powershell_literal(&handoff_ps1)),
        (
            "__RELEASE_TAG__",
            powershell_literal(Path::new(release_tag)),
        ),
    ];
    for (placeholder, value) in replacements {
        powershell = powershell.replace(placeholder, &value);
    }
    let bootstrap = format!(
        "@echo off\r\nrem WUWAID_HANDOFF_TOKEN={token}\r\nsetlocal DisableDelayedExpansion\r\nset \"WUWAID_POWERSHELL_PATH=%ProgramW6432%\\PowerShell\\7\\pwsh.exe\"\r\nif not exist \"%WUWAID_POWERSHELL_PATH%\" set \"WUWAID_POWERSHELL_PATH=%ProgramFiles%\\PowerShell\\7\\pwsh.exe\"\r\nif not exist \"%WUWAID_POWERSHELL_PATH%\" set \"WUWAID_POWERSHELL_PATH=%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\"\r\nif not exist \"%WUWAID_POWERSHELL_PATH%\" exit /b 90\r\n\"%WUWAID_POWERSHELL_PATH%\" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"%~dpn0.ps1\"\r\nset \"WUWAID_RC=%ERRORLEVEL%\"\r\nexit /b %WUWAID_RC%\r\n",
    );
    if let Some(parent) = handoff_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Gagal membuat folder handoff update: {error}"))?;
    }
    fs::write(&handoff_ps1, powershell)
        .map_err(|error| format!("Gagal menulis PowerShell handoff update: {error}"))?;
    fs::write(handoff_path, bootstrap)
        .map_err(|error| format!("Gagal menulis handoff update: {error}"))?;
    Ok(handoff_path.to_path_buf())
}

fn create_update_handoff_impl(
    staging_dir: &Path,
    current_executable: &Path,
    handoff_path: &Path,
    release_state: Option<(&Path, &Path, &Path, &str)>,
) -> Result<PathBuf, String> {
    let staged_executable = staging_dir.join(
        current_executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("WuwaIDLauncher.exe"),
    );
    let backup_executable = current_executable.with_file_name(format!(
        "{}.wuwaid-backup",
        current_executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(RELEASE_EXECUTABLE_NAME)
    ));
    let replacement_executable = current_executable.with_file_name(format!(
        "{}.wuwaid-new",
        current_executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(RELEASE_EXECUTABLE_NAME)
    ));
    let current_directory = current_executable
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let started_pid_path = handoff_path.with_file_name(format!(
        "{}.wuwaid-started.pid",
        handoff_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("update-handoff.cmd")
    ));
    let paths = [
        staging_dir,
        &staged_executable,
        current_executable,
        &backup_executable,
        &replacement_executable,
        handoff_path,
        current_directory,
        &started_pid_path,
    ];
    if paths.iter().any(|path| {
        let value = path.to_string_lossy();
        value.contains('\0') || value.contains('"') || value.contains('\r') || value.contains('\n')
    }) {
        return Err("Path handoff update mengandung karakter yang tidak valid.".to_string());
    }
    if let Some((transaction_path, pending_path, ready_marker_path, release_tag)) = release_state {
        let ready_marker_temp = release_note_marker_temp_path(ready_marker_path);
        let release_paths = [
            transaction_path,
            pending_path,
            ready_marker_path,
            ready_marker_temp.as_path(),
            started_pid_path.as_path(),
        ];
        if release_paths.iter().any(|path| {
            let value = path.to_string_lossy();
            value.contains('\0')
                || value.contains('"')
                || value.contains('\r')
                || value.contains('\n')
        }) || release_tag.is_empty()
            || !release_tag
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        {
            return Err(
                "State release notes handoff mengandung nilai yang tidak valid.".to_string(),
            );
        }
    }
    if let Some((transaction_path, pending_path, ready_marker_path, release_tag)) = release_state {
        return create_release_update_handoff(ReleaseHandoffSpec {
            staging_dir,
            staged_executable: &staged_executable,
            current_executable,
            handoff_path,
            transaction_path,
            pending_path,
            ready_marker_path,
            release_tag,
            backup_executable: &backup_executable,
            replacement_executable: &replacement_executable,
            current_directory,
            started_pid_path: &started_pid_path,
        });
    }
    let path_value = |path: &Path| path.to_string_lossy().replace('%', "%%");
    let quote = |path: &Path| format!("\"{}\"", path_value(path));
    let sleep_one = "start \"\" /wait /b \"%WUWAID_POWERSHELL_PATH%\" -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 1\" >nul 2>nul\r\n";
    let sleep_two = "start \"\" /wait /b \"%WUWAID_POWERSHELL_PATH%\" -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 2\" >nul 2>nul\r\n";
    let delete_self = "set \"WUWAID_HANDOFF_DELETE=%~f0.wuwaid-delete-%RANDOM%-%RANDOM%\"\r\n\
                  if exist \"%WUWAID_HANDOFF_DELETE%\" exit /b 0\r\n\
                  move /Y \"%~f0\" \"%WUWAID_HANDOFF_DELETE%\" >nul 2>nul\r\n\
                  if errorlevel 1 exit /b 0\r\n\
                  start \"\" /b \"%ComSpec%\" /d /c del /q \"%WUWAID_HANDOFF_DELETE%\" >nul 2>nul\r\n";
    let script = format!(
        "@echo off\r\n\
         set \"WUWAID_POWERSHELL_PATH=%ProgramW6432%\\PowerShell\\7\\pwsh.exe\"\r\n\
         if not exist \"%WUWAID_POWERSHELL_PATH%\" set \"WUWAID_POWERSHELL_PATH=%ProgramFiles%\\PowerShell\\7\\pwsh.exe\"\r\n\
         if not exist \"%WUWAID_POWERSHELL_PATH%\" set \"WUWAID_POWERSHELL_PATH=%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\"\r\n\
         setlocal DisableDelayedExpansion\r\n\
         rem WuwaID updater handoff with a verified backup and rollback\r\n\
         {sleep_one}\r\n\
         if not exist {staged} exit /b 1\r\n\
         if not exist {current} exit /b 1\r\n\
         if exist {replacement} del /Q {replacement} >nul 2>nul\r\n\
         copy /Y {current} {backup} >nul\r\n\
         if errorlevel 1 (\r\n\
            exit /b 2\r\n\
         )\r\n\
         copy /Y {staged} {replacement} >nul\r\n\
         if errorlevel 1 (\r\n\
            exit /b 3\r\n\
         )\r\n\
         move /Y {replacement} {current} >nul\r\n\
         if errorlevel 1 (\r\n\
             copy /Y {backup} {current} >nul\r\n\
             if errorlevel 1 exit /b 8\r\n\
             exit /b 4\r\n\
         )\r\n\
         %SystemRoot%\\System32\\fc.exe /B {staged} {current} >nul\r\n\
         if errorlevel 1 (\r\n\
             copy /Y {backup} {current} >nul\r\n\
             if errorlevel 1 exit /b 8\r\n\
             exit /b 5\r\n\
         )\r\n\
         start \"\" {current}\r\n\
         if errorlevel 1 (\r\n\
             copy /Y {backup} {current} >nul\r\n\
             if errorlevel 1 exit /b 8\r\n\
             exit /b 6\r\n\
         )\r\n\
          {sleep_two}\r\n\
          %SystemRoot%\\System32\\tasklist.exe /FI \"IMAGENAME eq WuwaIDLauncher.exe\" | %SystemRoot%\\System32\\findstr.exe /I /C:\"WuwaIDLauncher.exe\" >nul\r\n\
          if errorlevel 1 (\r\n\
             copy /Y {backup} {current} >nul\r\n\
             if errorlevel 1 exit /b 8\r\n\
             exit /b 7\r\n\
          )\r\n\
          del /Q {backup} >nul 2>nul\r\n\
          rmdir /S /Q {staging} >nul 2>nul\r\n\
         {delete_self}\
         exit /b 0\r\n",
        current = quote(current_executable),
        staged = quote(&staged_executable),
        backup = quote(&backup_executable),
        replacement = quote(&replacement_executable),
        staging = quote(staging_dir),
        sleep_one = sleep_one,
        sleep_two = sleep_two,
    );
    if let Some(parent) = handoff_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Gagal membuat folder handoff update: {error}"))?;
    }
    fs::write(handoff_path, script)
        .map_err(|error| format!("Gagal menulis handoff update: {error}"))?;
    Ok(handoff_path.to_path_buf())
}

/// Replaces a file while retaining enough state to restore the previous file
/// when staging, activation, or post-copy verification fails.  The Windows
/// handoff script implements the same sequence after the running process exits;
/// this synchronous helper keeps the transaction testable on every platform.
pub fn replace_file_recoverable(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("update_source_missing: {source:?}"));
    }
    if target.exists() && !target.is_file() {
        return Err(format!("update_target_not_file: {target:?}"));
    }
    let parent = target
        .parent()
        .ok_or_else(|| "update_target_parent_missing".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("update_parent_failed: {error}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp = parent.join(format!(".wuwaid-update-new-{}-{stamp}", std::process::id()));
    let backup = parent.join(format!(
        ".wuwaid-update-backup-{}-{stamp}",
        std::process::id()
    ));
    let had_target = target.is_file();
    let result = (|| -> Result<(), String> {
        fs::copy(source, &temp).map_err(|error| format!("update_stage_failed: {error}"))?;
        if had_target {
            fs::copy(target, &backup).map_err(|error| format!("update_backup_failed: {error}"))?;
        }
        if let Err(error) = fs::rename(&temp, target) {
            if had_target {
                fs::copy(&backup, target)
                    .map_err(|restore| format!("update_restore_failed: {restore}"))?;
            }
            return Err(format!("update_activate_failed: {error}"));
        }
        let source_hash = crate::engine::downloader::compute_sha256(source)
            .map_err(|error| format!("update_source_verify_failed: {error}"))?;
        let target_hash = crate::engine::downloader::compute_sha256(target)
            .map_err(|error| format!("update_target_verify_failed: {error}"))?;
        if source_hash != target_hash {
            if had_target {
                fs::copy(&backup, target)
                    .map_err(|error| format!("update_restore_failed: {error}"))?;
            } else {
                fs::remove_file(target)
                    .map_err(|error| format!("update_remove_invalid_target_failed: {error}"))?;
            }
            return Err("update_post_copy_verification_failed".to_string());
        }
        Ok(())
    })();
    let _ = fs::remove_file(&temp);
    if result.is_ok() && had_target {
        if let Err(error) = fs::remove_file(&backup) {
            return Err(format!("update_backup_cleanup_failed: {error}"));
        }
    }
    result
}

pub fn parse_version(tag: &str) -> Vec<u32> {
    let clean = tag.trim_start_matches('v').trim();
    clean
        .split('.')
        .filter_map(|p| p.parse::<u32>().ok())
        .collect()
}

pub fn is_newer_version(current: &str, latest: &str) -> bool {
    let cur_v = parse_version(current);
    let lat_v = parse_version(latest);

    let max_len = cur_v.len().max(lat_v.len());
    for i in 0..max_len {
        let c = cur_v.get(i).copied().unwrap_or(0);
        let l = lat_v.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}

pub fn parse_latest_release_json(json: &serde_json::Value) -> Result<ReleaseInfo, String> {
    let tag_name = json
        .get("tag_name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "GitHub release tidak memiliki tag launcher.".to_string())?
        .to_string();
    let version = tag_name.trim_start_matches('v').to_string();
    let expected_zip_name = expected_zip_asset_name(&tag_name)?;
    let title = json
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("WuwaID Launcher {tag_name}"));
    let date = json
        .get("published_at")
        .or_else(|| json.get("created_at"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let author = json
        .get("author")
        .and_then(|value| value.get("login"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("WuwaID Team")
        .to_string();
    let body = json
        .get("body")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();

    let mut zip_url = None;
    let mut checksums_url = None;
    if let Some(assets) = json.get("assets").and_then(|value| value.as_array()) {
        for asset in assets {
            let name = asset
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let url = asset
                .get("browser_download_url")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            if name == expected_zip_name {
                let url = url
                    .as_deref()
                    .ok_or_else(|| "Asset ZIP release launcher tidak memiliki URL.".to_string())?;
                if !is_expected_official_asset_url(url, &tag_name, &expected_zip_name) {
                    return Err(
                        "Asset ZIP release launcher bukan URL resmi yang diharapkan.".to_string(),
                    );
                }
                zip_url = Some(url.to_string());
            }
            if name.eq_ignore_ascii_case("SHA256sums.txt") {
                let url = url.as_deref().ok_or_else(|| {
                    "Asset checksum release launcher tidak memiliki URL.".to_string()
                })?;
                if !is_expected_official_asset_url(url, &tag_name, "SHA256sums.txt") {
                    return Err(
                        "Asset checksum release launcher bukan URL resmi yang diharapkan."
                            .to_string(),
                    );
                }
                checksums_url = Some(url.to_string());
            }
        }
    }

    Ok(ReleaseInfo {
        tag_name,
        version,
        title,
        date,
        author,
        body,
        zip_url,
        checksums_url,
    })
}

pub async fn fetch_latest_release() -> Result<ReleaseInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent("WuwaIDLauncher-Tauri")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(GITHUB_API_LATEST_RELEASE)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch latest release: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("GitHub API returned status: {}", response.status()));
    }

    if response
        .content_length()
        .is_some_and(|length| length > MAX_RELEASE_RESPONSE_BYTES)
    {
        return Err("Response release launcher terlalu besar.".to_string());
    }
    if response.url().as_str() != GITHUB_API_LATEST_RELEASE {
        return Err("Redirect GitHub API release launcher tidak diizinkan.".to_string());
    }
    let body = read_response_body_limited(response, MAX_RELEASE_RESPONSE_BYTES)
        .await
        .map_err(|error| format!("Failed to read release JSON: {error}"))?;
    let json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| format!("Failed to parse release JSON: {}", e))?;

    parse_latest_release_json(&json)
}

pub async fn fetch_official_asset_body(
    url: &str,
    tag: &str,
    asset_name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    if !is_expected_official_asset_url(url, tag, asset_name) {
        return Err("URL asset bukan asset release resmi yang diharapkan.".to_string());
    }
    let client = crate::engine::downloader::official_github_client(Duration::from_secs(15))?;
    let response = client
        .get(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .send()
        .await
        .map_err(|error| format!("Gagal mengambil asset release resmi: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Asset release resmi returned status: {}",
            response.status()
        ));
    }
    if !is_allowed_github_response_url(response.url()) {
        return Err("Redirect asset release resmi tidak diizinkan.".to_string());
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err(format!("Asset release melebihi batas {max_bytes} bytes."));
    }
    read_response_body_limited(response, max_bytes)
        .await
        .map_err(|error| format!("Gagal membaca asset release resmi: {error}"))
}

pub async fn check_latest_release(current_version: &str) -> Result<Option<ReleaseInfo>, String> {
    let release = fetch_latest_release().await?;
    if is_newer_version(current_version, &release.tag_name) {
        Ok(Some(release))
    } else {
        Ok(None)
    }
}

pub fn extract_zip_update(zip_data: &[u8], target_dir: &Path) -> Result<PathBuf, String> {
    validate_update_archive(zip_data, RELEASE_EXECUTABLE_NAME)?;
    let reader = Cursor::new(zip_data);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("Invalid ZIP archive: {}", e))?;

    fs::create_dir_all(target_dir).map_err(|e| format!("Failed to create staging dir: {}", e))?;
    let staging_root = fs::canonicalize(target_dir)
        .map_err(|error| format!("Gagal canonicalize staging update: {error}"))?;

    let mut main_exe_path = None;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read entry: {}", e))?;
        let outpath = match file.enclosed_name() {
            Some(path) => staging_root.join(path),
            None => continue,
        };
        if outpath
            .parent()
            .is_some_and(|parent| !parent.starts_with(&staging_root))
            || !outpath.starts_with(&staging_root)
        {
            return Err("ZIP update memiliki path di luar staging directory.".to_string());
        }

        if !file.is_dir()
            && outpath
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            && !is_release_executable(&outpath)
        {
            return Err(format!("ZIP memuat executable tak dikenal: {:?}", outpath));
        }
        let release_executable = is_release_executable(&outpath);

        if file.is_dir() {
            fs::create_dir_all(&outpath).map_err(|e| format!("Failed to create dir: {}", e))?;
        } else {
            let normalized_outpath = if release_executable {
                staging_root.join(RELEASE_EXECUTABLE_NAME)
            } else {
                outpath.clone()
            };
            if let Some(p) = normalized_outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)
                        .map_err(|e| format!("Failed to create parent dir: {}", e))?;
                }
            }
            let mut outfile = File::create(&normalized_outpath)
                .map_err(|e| format!("Failed to create file: {}", e))?;
            let copied = std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to extract file: {}", e))?;
            if copied != file.size() {
                return Err(format!(
                    "Ukuran entry ZIP berubah saat ekstraksi: {outpath:?}"
                ));
            }

            if release_executable {
                main_exe_path = Some(normalized_outpath);
            }
        }
    }

    main_exe_path.ok_or_else(|| {
        format!(
            "No executable {} found in update ZIP",
            RELEASE_EXECUTABLE_NAME
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(is_newer_version("2.6.1", "2.6.2"));
        assert!(is_newer_version("2.6.1", "v2.7.0"));
        assert!(is_newer_version("2.6.1", "3.0.0"));
        assert!(!is_newer_version("2.6.1", "2.6.1"));
        assert!(!is_newer_version("2.6.2", "2.6.1"));
        assert!(!is_newer_version("3.0.0", "2.6.1"));
    }

    #[test]
    fn latest_release_endpoint_targets_launcher_repository() {
        assert!(GITHUB_API_LATEST_RELEASE.contains("/repos/TitoTFP/WuwaIDLauncher/"));
        assert!(!GITHUB_API_LATEST_RELEASE.contains("/repos/TitoTFP/WuwaID/releases"));
    }

    #[test]
    fn latest_release_json_maps_launcher_notes() {
        let json = serde_json::json!({
            "tag_name": "v2.6.2",
            "name": "WuwaID Launcher v2.6.2",
            "body": "## Perubahan\n- Perbaikan launcher",
            "published_at": "2026-08-18T12:00:00Z",
            "author": { "login": "TitoTFP" },
            "assets": [
                {
                    "name": "WuwaIDLauncher-v2.6.2.zip",
                    "browser_download_url": "https://github.com/TitoTFP/WuwaIDLauncher/releases/download/v2.6.2/WuwaIDLauncher-v2.6.2.zip"
                },
                {
                    "name": "SHA256sums.txt",
                    "browser_download_url": "https://github.com/TitoTFP/WuwaIDLauncher/releases/download/v2.6.2/SHA256sums.txt"
                }
            ]
        });

        let release = parse_latest_release_json(&json).unwrap();
        assert_eq!(release.tag_name, "v2.6.2");
        assert_eq!(release.version, "2.6.2");
        assert_eq!(release.title, "WuwaID Launcher v2.6.2");
        assert_eq!(release.body, "## Perubahan\n- Perbaikan launcher");
        assert_eq!(release.date, "2026-08-18T12:00:00Z");
        assert_eq!(release.author, "TitoTFP");
        assert!(release
            .zip_url
            .unwrap()
            .ends_with("WuwaIDLauncher-v2.6.2.zip"));
        assert!(release.checksums_url.unwrap().ends_with("SHA256sums.txt"));
    }

    #[test]
    fn test_extract_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("extracted");

        // Build mock zip
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            zip.start_file(
                "WuwaIDLauncher.exe",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            std::io::Write::write_all(&mut zip, b"MOCK_EXE_DATA").unwrap();
            zip.finish().unwrap();
        }

        let exe_path = extract_zip_update(&buf.into_inner(), &target).unwrap();
        assert!(exe_path.exists());
        assert_eq!(exe_path.file_name().unwrap(), RELEASE_EXECUTABLE_NAME);
    }

    #[test]
    fn recoverable_replacement_rejects_missing_source_without_touching_target() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("missing.exe");
        let target = temp.path().join(RELEASE_EXECUTABLE_NAME);
        std::fs::write(&target, b"current").unwrap();

        assert!(replace_file_recoverable(&source, &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"current");
    }

    #[test]
    fn release_note_handoff_script_commits_only_after_health_check() {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join("staging");
        let current = temp.path().join(RELEASE_EXECUTABLE_NAME);
        let handoff = temp.path().join("handoff.cmd");
        let transaction = temp.path().join("launcher-whats-new-transaction.json");
        let pending = temp.path().join("launcher-whats-new-pending.json");
        let ready = temp.path().join("launcher-whats-new-ready.tag");
        std::fs::create_dir_all(&staging).unwrap();

        create_update_handoff_with_release_state(
            &staging,
            &current,
            &handoff,
            &transaction,
            &pending,
            &ready,
            "v2.10.0",
        )
        .unwrap();
        let script = std::fs::read_to_string(&handoff).unwrap();
        let powershell_path = handoff.with_extension("ps1");
        let powershell = std::fs::read_to_string(&powershell_path).unwrap();

        assert!(script.contains("@echo off"));
        assert!(script.contains("setlocal DisableDelayedExpansion"));
        assert!(script
            .contains("set \"WUWAID_POWERSHELL_PATH=%ProgramW6432%\\PowerShell\\7\\pwsh.exe\""));
        assert!(script.contains(
            "if not exist \"%WUWAID_POWERSHELL_PATH%\" set \"WUWAID_POWERSHELL_PATH=%ProgramFiles%\\PowerShell\\7\\pwsh.exe\""
        ));
        assert!(script.contains(
            "if not exist \"%WUWAID_POWERSHELL_PATH%\" set \"WUWAID_POWERSHELL_PATH=%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\""
        ));
        assert!(script.contains("-ExecutionPolicy Bypass -File \"%~dpn0.ps1\""));
        assert!(!script.contains("type "));
        assert!(!script.contains("for /f"));
        assert!(!script.contains("set /p "));
        assert!(!script.contains("taskkill"));
        assert!(!script.contains("release_pid"));
        assert!(!script.contains("del /q"));
        assert!(!script.contains("rmdir"));

        assert!(powershell.contains("function Read-StrictPid"));
        assert!(powershell.contains("[IO.File]::ReadAllText($Path).Trim()"));
        assert!(powershell.contains("'\\A[1-9][0-9]{0,9}\\z'"));
        assert!(powershell.contains("[Convert]::ToUInt32($text, 10)"));
        assert!(powershell.contains("Start-Process -FilePath $current"));
        assert!(powershell.contains("if (-not [IO.File]::Exists($staged))"));
        assert!(powershell.contains("WriteAllText($readyTemp, 'v2.10.0' +"));
        assert!(powershell.contains("-NoNewWindow -PassThru"));
        assert!(powershell.contains("$null = $Process.Handle"));
        assert!(powershell.contains("StartTimeUtc"));
        assert!(powershell.contains("function Stop-VerifiedTree"));
        assert!(powershell.contains("ParentProcessId"));
        assert!(powershell.contains("Kill', [Type[]]@([bool])"));
        assert!(powershell.contains("$Identity.Process"));
        assert!(powershell.contains("(Read-StrictPid $readyTemp) -ne $rootIdentity.Pid"));
        assert!(powershell.contains("Test-SameProcessIdentity $rootIdentity $live -NeedPath"));
        assert!(powershell.contains("Move-Item -LiteralPath $pidTemp -Destination $pidFile -Force"));
        assert!(powershell.contains("Remove-FileWithRetry ($pidFile + '.tmp')"));
        assert!(powershell.contains("function Start-DeferredSelfCleanup"));
        assert!(powershell.contains("$encoded = [Convert]::ToBase64String"));
        assert!(powershell.contains("$handoffToken"));
        assert!(!powershell.contains("RedirectStandardOutput"));
        assert!(!powershell.contains("$process.Kill()"));
        assert!(powershell.contains("$root.Kill()"));
        assert!(powershell.contains("Copy-Item -LiteralPath $backup -Destination $current -Force"));
        assert!(powershell.contains("$rollbackSucceeded = $false"));
        assert!(powershell.contains("$rollbackSucceeded -or -not $backupCreated"));
        assert!(powershell.contains("$committed = $true"));
        assert!(!powershell.contains("__HANDOFF_TOKEN__"));
        assert!(!powershell
            .lines()
            .any(|line| line.trim_end().ends_with('\\')));
    }
}
