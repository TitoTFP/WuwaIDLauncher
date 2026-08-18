use std::fs::{self, File};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

pub const GITHUB_API_LATEST_RELEASE: &str = "https://api.github.com/repos/TitoTFP/WuwaIDLauncher/releases/latest";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub version: String,
    pub body: String,
    pub zip_url: Option<String>,
    pub checksums_url: Option<String>,
}

pub const RELEASE_EXECUTABLE_NAME: &str = "wuwaid-launcher.exe";

fn is_release_executable(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case(RELEASE_EXECUTABLE_NAME)
                || name == "WuwaIDLauncher.exe"
        })
}

pub fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn is_safe_download_url(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("https://")
        && trimmed
            .strip_prefix("https://")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|character| !character.is_whitespace()))
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
    if expected_executable.is_empty() || Path::new(expected_executable).file_name().is_none() {
        return Err("Nama executable update tidak valid.".to_string());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_data))
        .map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    let mut found_expected = false;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| format!("Gagal membaca entry ZIP: {error}"))?;
        let Some(path) = file.enclosed_name() else {
            return Err("ZIP update memiliki path traversal atau path absolut.".to_string());
        };
        if !file.is_dir() && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("exe")) {
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

pub fn create_update_handoff(
    staging_dir: &Path,
    current_executable: &Path,
    handoff_path: &Path,
) -> Result<PathBuf, String> {
    let staged_executable = staging_dir.join(
        current_executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("WuwaIDLauncher.exe"),
    );
    let backup = current_executable.with_extension("old");
    let quote = |path: &Path| format!("\"{}\"", path.to_string_lossy());
    let script = format!(
        "@echo off\r\n\
         setlocal\r\n\
         rem WuwaID updater handoff with rollback\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         move /Y {current} {backup} >nul\r\n\
         move /Y {staged} {current} >nul\r\n\
         if errorlevel 1 (\r\n\
           rem rollback\r\n\
           move /Y {backup} {current} >nul\r\n\
           exit /b 1\r\n\
         )\r\n\
         start \"\" {current}\r\n\
         rmdir /S /Q {staging} >nul 2>nul\r\n\
         del \"%~f0\"\r\n",
        current = quote(current_executable),
        backup = quote(&backup),
        staged = quote(&staged_executable),
        staging = quote(staging_dir),
    );
    if let Some(parent) = handoff_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Gagal membuat folder handoff update: {error}"))?;
    }
    fs::write(handoff_path, script)
        .map_err(|error| format!("Gagal menulis handoff update: {error}"))?;
    Ok(handoff_path.to_path_buf())
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

pub async fn check_latest_release(current_version: &str) -> Result<Option<ReleaseInfo>, String> {
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

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse release JSON: {}", e))?;

    let tag_name = json.get("tag_name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let body = json.get("body").and_then(|v| v.as_str()).unwrap_or_default().to_string();

    let mut zip_url = None;
    let mut checksums_url = None;

    if let Some(assets) = json.get("assets").and_then(|a| a.as_array()) {
        for asset in assets {
            let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            let url = asset.get("browser_download_url").and_then(|u| u.as_str()).map(|s| s.to_string());
            if name.ends_with(".zip") && name.contains("Launcher") {
                zip_url = url.clone();
            }
            if name.eq_ignore_ascii_case("SHA256sums.txt") {
                checksums_url = url;
            }
        }
    }

    let is_newer = is_newer_version(current_version, &tag_name);

    if is_newer {
        Ok(Some(ReleaseInfo {
            tag_name: tag_name.clone(),
            version: tag_name.trim_start_matches('v').to_string(),
            body,
            zip_url,
            checksums_url,
        }))
    } else {
        Ok(None)
    }
}

pub fn extract_zip_update(zip_data: &[u8], target_dir: &Path) -> Result<PathBuf, String> {
    let reader = Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("Invalid ZIP archive: {}", e))?;

    fs::create_dir_all(target_dir).map_err(|e| format!("Failed to create staging dir: {}", e))?;

    let mut main_exe_path = None;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Failed to read entry: {}", e))?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        if !file.is_dir()
            && outpath.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            && !is_release_executable(&outpath)
        {
            return Err(format!("ZIP memuat executable tak dikenal: {:?}", outpath));
        }
        let release_executable = is_release_executable(&outpath);

        if file.is_dir() {
            fs::create_dir_all(&outpath).map_err(|e| format!("Failed to create dir: {}", e))?;
        } else {
            let normalized_outpath = if release_executable {
                target_dir.join(RELEASE_EXECUTABLE_NAME)
            } else {
                outpath
            };
            if let Some(p) = normalized_outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| format!("Failed to create parent dir: {}", e))?;
                }
            }
            let mut outfile = File::create(&normalized_outpath).map_err(|e| format!("Failed to create file: {}", e))?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| format!("Failed to extract file: {}", e))?;

            if release_executable {
                main_exe_path = Some(normalized_outpath);
            }
        }
    }

    main_exe_path.ok_or_else(|| format!("No executable {} found in update ZIP", RELEASE_EXECUTABLE_NAME))
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
    fn test_extract_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("extracted");

        // Build mock zip
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            zip.start_file("WuwaIDLauncher.exe", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut zip, b"MOCK_EXE_DATA").unwrap();
            zip.finish().unwrap();
        }

        let exe_path = extract_zip_update(&buf.into_inner(), &target).unwrap();
        assert!(exe_path.exists());
        assert_eq!(exe_path.file_name().unwrap(), RELEASE_EXECUTABLE_NAME);
    }
}
