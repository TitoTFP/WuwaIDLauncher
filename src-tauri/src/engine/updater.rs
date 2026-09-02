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
/// Failed copy, rollback, launch, or health-check paths remove all release-note
/// state so a failed update cannot surface as What's New later.
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

fn normalize_batch_fragment(fragment: &str) -> String {
    let normalized = fragment.replace("\\\n", "").replace('\r', "");
    let lines = normalized.lines().map(str::trim_start).collect::<Vec<_>>();
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\r\n", lines.join("\r\n"))
    }
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
        value.contains('"') || value.contains('\r') || value.contains('\n')
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
            value.contains('"') || value.contains('\r') || value.contains('\n')
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
    let path_value = |path: &Path| path.to_string_lossy().replace('%', "%%");
    let quote = |path: &Path| format!("\"{}\"", path_value(path));
    let sleep_one = "%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 1\" >nul 2>nul\r\n";
    let sleep_two = "%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 2\" >nul 2>nul\r\n";
    let delete_self = "start \"\" /b \"%ComSpec%\" /d /c del /q \"%~f0\" >nul 2>nul\r\n";
    let release_setup = release_state
        .map(
            |(transaction_path, pending_path, ready_marker_path, release_tag)| {
                let ready_marker_temp = release_note_marker_temp_path(ready_marker_path);
                format!(
                    "         set \"release_transaction={}\"\r\n\\
                 set \"release_pending={}\"\r\n\\
                 set \"release_ready={}\"\r\n\\
                 set \"release_ready_temp={}\"\r\n\\
                 set \"release_started_pid_file={}\"\r\n\\
                 set \"release_tag={}\"\r\n\\
                 del /Q \"%release_ready_temp%\" >nul 2>nul\r\n\\
                 del /Q \"%release_started_pid_file%\" >nul 2>nul\r\n\\
                 if exist \"%release_ready_temp%\" goto fail_12\r\n\\
                 set \"WUWAID_LAUNCHER_UPDATE_READY=%release_ready_temp%\"\r\n",
                    path_value(transaction_path),
                    path_value(pending_path),
                    path_value(ready_marker_path),
                    path_value(ready_marker_temp.as_path()),
                    path_value(started_pid_path.as_path()),
                    release_tag,
                )
            },
        )
        .unwrap_or_default();
    let release_precondition = release_state
        .map(|(_transaction_path, _, _, _)| {
            "         if not exist \"%release_transaction%\" goto fail_11\r\n".to_string()
        })
        .unwrap_or_default();
    let release_health_check = release_state
        .map(|_| {
            "         if not exist \"%release_ready_temp%\" goto release_health_failure\r\n\\
                 set \"release_marker_pid=\"\r\n\\
                 set /p \"release_marker_pid=\"<\"%release_ready_temp%\"\r\n\\
                 if not defined release_marker_pid goto release_health_failure\r\n\\
                 for /f \"delims=0123456789\" %%A in (\"%release_marker_pid%\") do goto release_health_failure\r\n\\
                 set \"release_pid=%release_marker_pid%\"\r\n\\
                 set \"release_pid_valid=1\"\r\n\\
                 %SystemRoot%\\System32\\tasklist.exe /FI \"PID eq %release_pid%\" /FI \"IMAGENAME eq WuwaIDLauncher.exe\" | %SystemRoot%\\System32\\findstr.exe /I /C:\"WuwaIDLauncher.exe\" >nul\r\n\\
                 if errorlevel 1 goto release_health_failure\r\n"
                .to_string()
        })
        .unwrap_or_default();
    let release_start = release_state
        .map(|_| {
            format!(
                "         set \"WUWAID_UPDATE_EXECUTABLE={}\"\r\n\\
                 set \"WUWAID_UPDATE_DIRECTORY={}\"\r\n\\
                 set \"WUWAID_UPDATE_PID_FILE={}\"\r\n\\
                 %SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -NoProfile -NonInteractive -Command \"$process = Start-Process -FilePath $env:WUWAID_UPDATE_EXECUTABLE -WorkingDirectory $env:WUWAID_UPDATE_DIRECTORY -PassThru; [IO.File]::WriteAllText($env:WUWAID_UPDATE_PID_FILE, [string]$process.Id)\"\r\n\\
                 if errorlevel 1 goto fail_6\r\n\\
                 if not exist \"%release_started_pid_file%\" goto fail_6\r\n\\
                 set \"release_started_pid=\"\r\n\\
                 set /p \"release_started_pid=\"<\"%release_started_pid_file%\"\r\n\\
                 if not defined release_started_pid goto fail_6\r\n\\
                 for /f \"delims=0123456789\" %%A in (\"%release_started_pid%\") do goto fail_6\r\n\\
                 set \"release_pid=%release_started_pid%\"\r\n\\
                 set \"release_pid_valid=1\"\r\n\\
                 cmd /c exit 0\r\n",
                path_value(current_executable),
                path_value(current_directory),
                path_value(started_pid_path.as_path()),
            )
        })
        .unwrap_or_default();
    let release_rollback = |failure: &str| {
        format!(
            "                 if defined release_pid_valid call :stop_released_launcher\r\n\\
                 copy /Y {backup} {current} >nul\r\n\\
                 if errorlevel 1 goto fail_8\r\n\\
                 goto {failure}\r\n",
            current = quote(current_executable),
            backup = quote(&backup_executable),
            failure = failure,
        )
    };
    let release_commit = release_state
        .map(|(_, _, _, _)| {
            let pending_rollback = release_rollback("fail_11");
            let marker_rollback = release_rollback("fail_12");
            let ready_rollback = release_rollback("fail_13");
            format!(
                "         move /Y \"%release_transaction%\" \"%release_pending%\" >nul\r\n\\
                 if errorlevel 1 (\r\n\\
                 {pending_rollback}                 )\r\n\\
                 del /Q \"%release_ready_temp%\" >nul 2>nul\r\n\\
                 >\"%release_ready_temp%\" echo %release_tag%\r\n\\
                 if not exist \"%release_ready_temp%\" (\r\n\\
                 {marker_rollback}                 )\r\n\\
                 move /Y \"%release_ready_temp%\" \"%release_ready%\" >nul\r\n\\
                 if errorlevel 1 (\r\n\\
                 {ready_rollback}                 )\r\n",
                pending_rollback = pending_rollback,
                marker_rollback = marker_rollback,
                ready_rollback = ready_rollback,
            )
        })
        .unwrap_or_default();
    let failure_state_cleanup = if release_state.is_some() {
        "         call :cleanup_release_state\r\n"
    } else {
        ""
    };
    let release_process_cleanup = release_state
        .map(|_| {
            format!(
                "         :stop_released_launcher\r\n\\
                 %SystemRoot%\\System32\\taskkill.exe /PID %release_pid% /T /F >nul 2>nul\r\n\\
                 for /L %%W in (1,1,10) do (\r\n\\
                     %SystemRoot%\\System32\\tasklist.exe /FI \"PID eq %release_pid%\" /FO CSV /NH | %SystemRoot%\\System32\\findstr.exe /I /C:\"%release_pid%\" >nul\r\n\\
                     if errorlevel 1 exit /b 0\r\n\\
                     {sleep_one}                 )\r\n\\
                 exit /b 0\r\n",
                sleep_one = sleep_one,
            )
        })
        .unwrap_or_default();
    let release_health_failure = release_state
        .map(|_| {
            format!(
                ":release_health_failure\r\n{rollback}",
                rollback = release_rollback("fail_7"),
            )
        })
        .unwrap_or_default();
    let failure_labels = format!(
        "{release_process_cleanup}{release_health_failure}:fail_1\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 1\r\n\\
         :fail_2\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 2\r\n\\
         :fail_3\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 3\r\n\\
         :fail_4\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 4\r\n\\
         :fail_5\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 5\r\n\\
         :fail_6\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 6\r\n\\
         :fail_7\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 7\r\n\\
         :fail_8\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 8\r\n\\
         :fail_9\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 9\r\n\\
         :fail_10\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 10\r\n\\
         :fail_11\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 11\r\n\\
         :fail_12\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 12\r\n\\
         :fail_13\r\n{failure_state_cleanup}         call :cleanup_update_files\r\n         exit /b 13\r\n",
    );
    let release_cleanup = release_state
        .map(|(transaction_path, pending_path, ready_marker_path, _)| {
            let ready_marker_temp = release_note_marker_temp_path(ready_marker_path);
            format!(
                ":cleanup_release_state\r\n\\
                 rem Remove the visibility marker before payload files.\r\n\\
                 del /Q {} >nul 2>nul\r\n\\
                 del /Q {} >nul 2>nul\r\n\\
                 del /Q {} >nul 2>nul\r\n\\
                 del /Q {} >nul 2>nul\r\n\\
                 del /Q {} >nul 2>nul\r\n\\
                 exit /b 0\r\n",
                quote(ready_marker_path),
                quote(ready_marker_temp.as_path()),
                quote(started_pid_path.as_path()),
                quote(transaction_path),
                quote(pending_path),
            )
        })
        .unwrap_or_default();
    let update_cleanup = format!(
        ":cleanup_update_files\r\n\\
         del /Q {replacement} >nul 2>nul\r\n\\
         del /Q {backup} >nul 2>nul\r\n\\
         rmdir /S /Q {staging} >nul 2>nul\r\n\\
         {delete_self}\\
         exit /b 0\r\n",
        delete_self = delete_self,
        replacement = quote(&replacement_executable),
        backup = quote(&backup_executable),
        staging = quote(staging_dir),
    );
    let script = format!(
        "@echo off\r\n\
         setlocal\r\n\
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
    let script = if release_state.is_some() {
        let release_setup = normalize_batch_fragment(&release_setup);
        let release_precondition = normalize_batch_fragment(&release_precondition);
        let release_health_check = normalize_batch_fragment(&release_health_check);
        let release_start = normalize_batch_fragment(&release_start);
        let release_commit = normalize_batch_fragment(&release_commit);
        let failure_labels = normalize_batch_fragment(&failure_labels);
        let release_cleanup = normalize_batch_fragment(&release_cleanup);
        let update_cleanup = normalize_batch_fragment(&update_cleanup);
        let mut script = script;
        script = script.replace("setlocal\r\n", &format!("setlocal\r\n{release_setup}"));
        for code in 1..=8 {
            script = script.replace(
                &format!("exit /b {code}\r\n"),
                &format!("goto fail_{code}\r\n"),
            );
        }
        let replacement = quote(&replacement_executable);
        let replacement_anchor =
            format!("if exist {replacement} del /Q {replacement} >nul 2>nul\r\n");
        script = script.replace(
            &replacement_anchor,
            &format!("{release_precondition}{replacement_anchor}"),
        );
        let start_anchor = format!("start \"\" {}\r\n", quote(current_executable));
        if !script.contains(&start_anchor) {
            return Err(
                "Template handoff update tidak memiliki awal proses yang diharapkan.".to_string(),
            );
        }
        script = script.replace(&start_anchor, &release_start);
        let backup = quote(&backup_executable);
        let health_anchor =
            "%SystemRoot%\\System32\\tasklist.exe /FI \"IMAGENAME eq WuwaIDLauncher.exe\" | %SystemRoot%\\System32\\findstr.exe /I /C:\"WuwaIDLauncher.exe\" >nul\r\n";
        if !script.contains(health_anchor) {
            return Err(
                "Template handoff update tidak memiliki pemeriksaan kesehatan yang diharapkan."
                    .to_string(),
            );
        }
        script = script.replace(health_anchor, &release_health_check);
        let staging = quote(staging_dir);
        let success_anchor = format!(
            "del /Q {backup} >nul 2>nul\r\nrmdir /S /Q {staging} >nul 2>nul\r\nstart \"\" /b \"%ComSpec%\" /d /c del /q \"%~f0\" >nul 2>nul\r\nexit /b 0\r\n",
            backup = backup,
            staging = staging,
        );
        if !script.contains(&success_anchor) {
            return Err(
                "Template handoff update tidak memiliki akhir sukses yang diharapkan.".to_string(),
            );
        }
        let started_pid = quote(&started_pid_path);
        let success = format!(
            "{release_commit}del /Q {started_pid} >nul 2>nul\r\ndel /Q {backup} >nul 2>nul\r\nrmdir /S /Q {staging} >nul 2>nul\r\n{delete_self}exit /b 0\r\n{failure_labels}{release_cleanup}{update_cleanup}",
            release_commit = release_commit,
            started_pid = started_pid,
            backup = backup,
            staging = staging,
            delete_self = delete_self,
            failure_labels = failure_labels,
            release_cleanup = release_cleanup,
            update_cleanup = update_cleanup,
        );
        script.replace(&success_anchor, &success)
    } else {
        script
    };
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
        let script = std::fs::read_to_string(handoff).unwrap();

        assert!(script.contains("if not exist \"%release_transaction%\" goto fail_11"));
        assert!(script.contains("move /Y \"%release_transaction%\" \"%release_pending%\""));
        assert!(script.contains("set \"release_tag=v2.10.0\""));
        assert!(script.contains("Start-Sleep -Seconds 2"));
        assert!(!script.contains("timeout.exe"));
        assert!(script.contains("start \"\" /b \"%ComSpec%\" /d /c del /q \"%~f0\""));
        assert!(script.contains("set \"WUWAID_LAUNCHER_UPDATE_READY=%release_ready_temp%\""));
        assert!(script.contains("Start-Process -FilePath $env:WUWAID_UPDATE_EXECUTABLE"));
        assert!(script.contains("[IO.File]::WriteAllText($env:WUWAID_UPDATE_PID_FILE"));
        assert!(script.contains("set /p \"release_started_pid=\"<\"%release_started_pid_file%\""));
        assert!(script.contains("if exist \"%release_ready_temp%\" goto fail_12"));
        assert!(script.contains("PID eq %release_pid%"));
        assert!(script.contains(":stop_released_launcher"));
        assert!(script.contains("taskkill.exe /PID %release_pid% /T /F"));
        let health_failure = &script[script.find(":release_health_failure").unwrap()..];
        assert!(
            health_failure.find("call :stop_released_launcher").unwrap()
                < health_failure.find("copy /Y").unwrap()
        );
        assert!(!script.contains("tasklist.exe /FI \"IMAGENAME eq WuwaIDLauncher.exe\" |"));
        assert!(script.contains("goto fail_7"));
        assert!(script.contains("if not exist \"%release_ready_temp%\""));
        assert!(script.contains(":cleanup_release_state"));
        assert!(!script.lines().any(|line| line.trim_end().ends_with('\\')));
        assert!(script.contains(&format!("del /Q \"{}\"", pending.to_string_lossy())));
    }
}
