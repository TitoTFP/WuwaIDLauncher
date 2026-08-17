use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub const DEFAULT_LOG_UPLOAD_ENDPOINT: &str = "https://wuwa-logs.titofp.workers.dev/upload";

pub fn get_game_logs_dir(game_path: &Path) -> PathBuf {
    game_path.join("Client").join("Saved").join("Logs")
}

pub fn collect_logs_to_zip(
    game_path: &Path,
    appdata_dir: &Path,
) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buf);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // 1. Collect game logs
    let game_logs = get_game_logs_dir(game_path);
    if game_logs.exists() {
        if let Ok(entries) = fs::read_dir(&game_logs) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "log") {
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                    let zip_name = format!("game_logs/{}", file_name);
                    if let Ok(content) = fs::read(&path) {
                        let _ = zip.start_file(zip_name, options);
                        let _ = zip.write_all(&content);
                    }
                }
            }
        }
    }

    // 2. Collect launcher logs & settings
    let settings_file = appdata_dir.join("settings.json");
    if settings_file.exists() {
        if let Ok(content) = fs::read(&settings_file) {
            let _ = zip.start_file("launcher/settings.json", options);
            let _ = zip.write_all(&content);
        }
    }

    let versions_file = appdata_dir.join("versions.json");
    if versions_file.exists() {
        if let Ok(content) = fs::read(&versions_file) {
            let _ = zip.start_file("launcher/versions.json", options);
            let _ = zip.write_all(&content);
        }
    }

    zip.finish().map_err(|e| format!("Failed to build ZIP: {}", e))?;
    Ok(buf.into_inner())
}

pub async fn upload_logs_zip(
    zip_data: Vec<u8>,
    client_id: &str,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let part = reqwest::multipart::Part::bytes(zip_data)
        .file_name(format!("wuwa_logs_{}.zip", client_id))
        .mime_str("application/zip")
        .map_err(|e| format!("Failed to create multipart part: {}", e))?;

    let form = reqwest::multipart::Form::new()
        .text("client_id", client_id.to_string())
        .part("file", part);

    let response = client
        .post(DEFAULT_LOG_UPLOAD_ENDPOINT)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Upload request failed: {}", e))?;

    if response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        Ok(text)
    } else {
        Err(format!("Upload failed with HTTP status {}", response.status()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_collect_logs_to_zip() {
        let tmp = tempdir().unwrap();
        let game_dir = tmp.path().join("game");
        let appdata_dir = tmp.path().join("appdata");

        let logs_dir = game_dir.join("Client").join("Saved").join("Logs");
        fs::create_dir_all(&logs_dir).unwrap();
        fs::create_dir_all(&appdata_dir).unwrap();

        fs::write(logs_dir.join("Client.log"), b"Game log entry").unwrap();
        fs::write(appdata_dir.join("settings.json"), b"{\"gamePath\":\"\"}").unwrap();

        let zip_bytes = collect_logs_to_zip(&game_dir, &appdata_dir).unwrap();
        assert!(!zip_bytes.is_empty());

        // Verify ZIP content
        let reader = Cursor::new(zip_bytes);
        let archive = zip::ZipArchive::new(reader).unwrap();
        assert!(archive.len() >= 2);
    }
}
