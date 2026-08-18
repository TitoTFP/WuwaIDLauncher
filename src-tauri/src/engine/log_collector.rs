use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub const DEFAULT_LOG_UPLOAD_ENDPOINT: &str = "";
pub const MAX_UPLOAD_ATTEMPTS: usize = 2;

pub fn configured_upload_endpoint() -> Option<String> {
    let endpoint = std::env::var("WUWAID_LOG_UPLOAD_ENDPOINT")
        .unwrap_or_else(|_| DEFAULT_LOG_UPLOAD_ENDPOINT.to_string());
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return None;
    }
    let Some(host) = endpoint.strip_prefix("https://") else {
        return None;
    };
    if host.is_empty() || host.chars().any(char::is_whitespace) {
        return None;
    }
    Some(endpoint.to_string())
}

pub fn max_upload_attempts() -> usize {
    MAX_UPLOAD_ATTEMPTS
}

pub fn redact_json_document(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut value: serde_json::Value = serde_json::from_slice(data)
        .map_err(|error| format!("Diagnostic JSON tidak valid: {error}"))?;
    redact_value(&mut value);
    serde_json::to_vec(&value).map_err(|error| format!("Diagnostic JSON gagal diserialisasi: {error}"))
}

fn redact_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.retain(|key, _| {
                !matches!(
                    key.to_ascii_lowercase().as_str(),
                    "gamepath" | "installpath" | "clientid" | "client_id" | "username"
                )
            });
            for child in object.values_mut() {
                redact_value(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_value(item);
            }
        }
        _ => {}
    }
}

pub fn get_game_logs_dir(game_path: &Path) -> PathBuf {
    game_path.join("Client").join("Saved").join("Logs")
}

pub fn save_logs_bundle(zip_data: &[u8], appdata_dir: &Path) -> Result<PathBuf, String> {
    let diagnostics_dir = appdata_dir.join("Diagnostics");
    fs::create_dir_all(&diagnostics_dir)
        .map_err(|error| format!("Gagal membuat folder diagnostics lokal: {error}"))?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Waktu sistem tidak valid: {error}"))?
        .as_millis();
    let stem = format!("wuwa_logs_{}_{}", std::process::id(), timestamp);
    let final_path = diagnostics_dir.join(format!("{stem}.zip"));
    let temp_path = diagnostics_dir.join(format!(".{stem}.part"));

    fs::write(&temp_path, zip_data)
        .map_err(|error| format!("Gagal menyimpan bundle diagnostics sementara: {error}"))?;
    if let Err(error) = fs::rename(&temp_path, &final_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("Gagal menyimpan bundle diagnostics: {error}"));
    }
    Ok(final_path)
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
            let redacted = redact_json_document(&content)
                .unwrap_or_else(|_| br#"{"redacted":true}"#.to_vec());
            let _ = zip.write_all(&redacted);
        }
    }

    let versions_file = appdata_dir.join("versions.json");
    if versions_file.exists() {
        if let Ok(content) = fs::read(&versions_file) {
            let _ = zip.start_file("launcher/versions.json", options);
            let redacted = redact_json_document(&content)
                .unwrap_or_else(|_| br#"{"redacted":true}"#.to_vec());
            let _ = zip.write_all(&redacted);
        }
    }

    zip.finish().map_err(|e| format!("Failed to build ZIP: {}", e))?;
    Ok(buf.into_inner())
}

pub async fn upload_logs_zip(
    zip_data: Vec<u8>,
    client_id: &str,
) -> Result<String, String> {
    let endpoint = configured_upload_endpoint().ok_or_else(|| {
        "Endpoint upload diagnostics belum dikonfigurasi; bundle lokal tetap tersedia.".to_string()
    })?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let mut last_error = "Upload diagnostics gagal.".to_string();
    for attempt in 1..=MAX_UPLOAD_ATTEMPTS {
        let part = reqwest::multipart::Part::bytes(zip_data.clone())
            .file_name(format!("wuwa_logs_{}.zip", client_id))
            .mime_str("application/zip")
            .map_err(|error| format!("Gagal membuat multipart upload: {error}"))?;
        let form = reqwest::multipart::Form::new()
            .text("client_id", client_id.to_string())
            .part("file", part);

        match client
            .post(&endpoint)
            .multipart(form)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                return Ok(response.text().await.unwrap_or_default());
            }
            Ok(response) => {
                last_error = format!("Upload gagal dengan HTTP status {}", response.status());
            }
            Err(error) => {
                last_error = format!("Request upload gagal: {error}");
            }
        }

        if attempt < MAX_UPLOAD_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
    Err(last_error)
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

    #[test]
    fn upload_endpoint_requires_explicit_https_configuration() {
        assert!(configured_upload_endpoint().is_none());
    }
}
