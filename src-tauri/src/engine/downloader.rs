use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Instant;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub percent: u8,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_mbps: f64,
    pub status: String,
}

pub fn parse_sha256sums(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let hash = parts[0].trim().to_lowercase();
            let mut filename = parts[1].trim();
            if filename.starts_with('*') {
                filename = &filename[1..];
            }
            map.insert(filename.to_string(), hash);
        }
    }
    map
}

pub fn verify_sha256(file_path: &Path, expected_hash: &str) -> Result<bool, std::io::Error> {
    if !file_path.exists() {
        return Ok(false);
    }
    let mut file = File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    let calculated = hex::encode(hasher.finalize()).to_lowercase();
    Ok(calculated == expected_hash.trim().to_lowercase())
}

pub async fn get_asset_content_length(url: &str) -> Result<u64, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let resp = client
        .head(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .send()
        .await
        .map_err(|e| format!("HEAD request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HEAD request returned status: {}", resp.status()));
    }

    let len = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| resp.content_length())
        .ok_or_else(|| "Header Content-Length tidak ditemukan pada HEAD response.".to_string())?;

    if len == 0 {
        return Err("Content-Length bernilai 0 (kosong).".to_string());
    }

    Ok(len)
}

pub async fn download_file_with_expected_size<F>(
    url: &str,
    dest_path: &Path,
    expected_size: Option<u64>,
    on_progress: F,
) -> Result<u64, String>
where
    F: Fn(DownloadProgress) + Send + 'static,
{
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut response = client
        .get(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .send()
        .await
        .map_err(|e| format!("Failed to send GET request: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }

    let get_content_len = response
        .content_length()
        .ok_or_else(|| "Header Content-Length tidak ditemukan pada GET response.".to_string())?;

    if get_content_len == 0 {
        return Err("Content-Length bernilai 0 pada server.".to_string());
    }

    if let Some(exp) = expected_size {
        if exp > 0 && exp != get_content_len {
            return Err(format!(
                "Content-Length GET ({} bytes) tidak sesuai dengan HEAD metadata ({} bytes).",
                get_content_len, exp
            ));
        }
    }

    let total_size = get_content_len;

    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create target directory: {}", e))?;
    }

    let temp_dest = dest_path.with_extension("tmp_download");
    let mut file = tokio::fs::File::create(&temp_dest)
        .await
        .map_err(|e| format!("Failed to create temporary file: {}", e))?;

    let mut downloaded: u64 = 0;
    let start_time = Instant::now();
    let mut last_emit = Instant::now();

    let mut stream_err = None;
    while let Ok(Some(chunk)) = response.chunk().await {
        if let Err(e) = file.write_all(&chunk).await {
            stream_err = Some(format!("Write file error: {}", e));
            break;
        }

        downloaded += chunk.len() as u64;

        if last_emit.elapsed().as_millis() >= 350 || downloaded == total_size {
            let elapsed_secs = start_time.elapsed().as_secs_f64().max(0.001);
            let speed_mbps = (downloaded as f64 / (1024.0 * 1024.0)) / elapsed_secs;
            let percent = if total_size > 0 {
                ((downloaded as f64 / total_size as f64) * 100.0).min(100.0) as u8
            } else {
                0
            };

            on_progress(DownloadProgress {
                percent,
                downloaded_bytes: downloaded,
                total_bytes: total_size,
                speed_mbps,
                status: format!(
                    "{:.1} / {:.1} MB ({:.1} MB/s)",
                    downloaded as f64 / 1_048_576.0,
                    total_size as f64 / 1_048_576.0,
                    speed_mbps
                ),
            });

            last_emit = Instant::now();
        }
    }

    if let Some(err) = stream_err {
        let _ = tokio::fs::remove_file(&temp_dest).await;
        return Err(err);
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush downloaded file: {}", e))?;
    drop(file);

    if downloaded != total_size {
        let _ = tokio::fs::remove_file(&temp_dest).await;
        return Err(format!(
            "Ukuran file yang diunduh ({} bytes) tidak sesuai dengan total Content-Length ({} bytes). Unduhan dibatalkan.",
            downloaded, total_size
        ));
    }

    if dest_path.exists() {
        let _ = tokio::fs::remove_file(dest_path).await;
    }

    tokio::fs::rename(&temp_dest, dest_path)
        .await
        .map_err(|e| format!("Failed to move temp download to target: {}", e))?;

    Ok(downloaded)
}

pub async fn download_file<F>(
    url: &str,
    dest_path: &Path,
    on_progress: F,
) -> Result<(), String>
where
    F: Fn(DownloadProgress) + Send + 'static,
{
    download_file_with_expected_size(url, dest_path, None, on_progress)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_sha256sums() {
        let content = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  empty.pak\n\
                       a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e *test.dll";
        let parsed = parse_sha256sums(content);
        assert_eq!(
            parsed.get("empty.pak").unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            parsed.get("test.dll").unwrap(),
            "a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e"
        );
    }

    #[test]
    fn test_verify_sha256_valid() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"hello world").unwrap();
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_sha256(file.path(), expected).unwrap());
    }

    #[test]
    fn test_verify_sha256_mismatch() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"hello world").unwrap();
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(!verify_sha256(file.path(), expected).unwrap());
    }
}
