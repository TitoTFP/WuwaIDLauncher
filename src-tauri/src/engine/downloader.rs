use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

pub const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum DownloadRedirectPolicy {
    AnyHttps,
    OfficialGithubAsset { expected_url: String },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub percent: u8,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_mbps: f64,
    pub status: String,
}

pub async fn read_response_body_limited(
    mut response: reqwest::Response,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Gagal membaca HTTP response: {error}"))?
    {
        if body.len() as u64 > max_bytes
            || chunk.len() as u64 > max_bytes.saturating_sub(body.len() as u64)
        {
            return Err(format!("HTTP response melebihi batas {} bytes.", max_bytes));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
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
    Ok(compute_sha256(file_path)? == expected_hash.trim().to_lowercase())
}

pub fn compute_sha256(file_path: &Path) -> Result<String, std::io::Error> {
    if !file_path.is_file() {
        return Ok(String::new());
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

    Ok(hex::encode(hasher.finalize()).to_lowercase())
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
    if len > MAX_DOWNLOAD_BYTES {
        return Err(format!(
            "Content-Length melebihi batas download {} bytes.",
            MAX_DOWNLOAD_BYTES
        ));
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
    download_file_with_expected_size_limited_policy(
        url,
        dest_path,
        expected_size,
        MAX_DOWNLOAD_BYTES,
        DownloadRedirectPolicy::AnyHttps,
        on_progress,
    )
    .await
}

pub async fn download_file_with_expected_size_limited<F>(
    url: &str,
    dest_path: &Path,
    expected_size: Option<u64>,
    max_bytes: u64,
    on_progress: F,
) -> Result<u64, String>
where
    F: Fn(DownloadProgress) + Send + 'static,
{
    download_file_with_expected_size_limited_policy(
        url,
        dest_path,
        expected_size,
        max_bytes,
        DownloadRedirectPolicy::AnyHttps,
        on_progress,
    )
    .await
}

pub async fn download_file_with_expected_size_limited_policy<F>(
    url: &str,
    dest_path: &Path,
    expected_size: Option<u64>,
    max_bytes: u64,
    redirect_policy: DownloadRedirectPolicy,
    on_progress: F,
) -> Result<u64, String>
where
    F: Fn(DownloadProgress) + Send + 'static,
{
    if max_bytes == 0 {
        return Err("Batas download tidak boleh nol.".to_string());
    }
    if expected_size.is_some_and(|size| size == 0 || size > max_bytes) {
        return Err(format!(
            "Ukuran download melebihi batas {} bytes.",
            max_bytes
        ));
    }

    let mut client_builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(60));
    if let DownloadRedirectPolicy::OfficialGithubAsset { expected_url } = &redirect_policy {
        if url != expected_url {
            return Err("URL asset GitHub tidak sesuai URL resmi yang diharapkan.".to_string());
        }
        client_builder =
            client_builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if is_allowed_github_redirect(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("redirect asset GitHub menuju host yang tidak diizinkan")
                }
            }));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let client = if matches!(
        &redirect_policy,
        DownloadRedirectPolicy::OfficialGithubAsset { .. }
    ) {
        client_builder
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?
    } else {
        client
    };

    let mut response = client
        .get(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .send()
        .await
        .map_err(|e| format!("Failed to send GET request: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }

    if matches!(
        &redirect_policy,
        DownloadRedirectPolicy::OfficialGithubAsset { .. }
    ) && !is_allowed_github_redirect(response.url())
    {
        return Err("Redirect asset GitHub menuju URL yang tidak diizinkan.".to_string());
    }

    let get_content_len = response
        .content_length()
        .ok_or_else(|| "Header Content-Length tidak ditemukan pada GET response.".to_string())?;

    if get_content_len == 0 {
        return Err("Content-Length bernilai 0 pada server.".to_string());
    }
    if get_content_len > max_bytes {
        return Err(format!("Download melebihi batas {} bytes.", max_bytes));
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

    let temp_dest = unique_temp_path(dest_path, "download");
    let mut file = tokio::fs::File::create(&temp_dest)
        .await
        .map_err(|e| format!("Failed to create temporary file: {}", e))?;

    let mut downloaded: u64 = 0;
    let start_time = Instant::now();
    let mut last_emit = Instant::now();

    let mut stream_err = None;
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                stream_err = Some(format!("Read download stream error: {error}"));
                break;
            }
        };
        if downloaded > max_bytes
            || chunk.len() as u64 > max_bytes.saturating_sub(downloaded)
            || chunk.len() as u64 > total_size.saturating_sub(downloaded)
        {
            stream_err = Some(format!(
                "Download melebihi ukuran yang diharapkan atau batas {} bytes.",
                max_bytes
            ));
            break;
        }
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

    promote_download(&temp_dest, dest_path).await?;

    Ok(downloaded)
}

fn unique_temp_path(path: &Path, suffix: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    path.with_file_name(format!(".{name}.{suffix}-{}-{stamp}", std::process::id()))
}

async fn promote_download(temp: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() && !destination.is_file() {
        let _ = tokio::fs::remove_file(temp).await;
        return Err(format!("Target download bukan file: {destination:?}"));
    }
    let backup = unique_temp_path(destination, "previous");
    let had_destination = destination.is_file();
    if had_destination {
        tokio::fs::rename(destination, &backup)
            .await
            .map_err(|error| format!("Gagal mencadangkan download lama: {error}"))?;
    }
    if let Err(error) = tokio::fs::rename(temp, destination).await {
        if had_destination {
            let _ = tokio::fs::rename(&backup, destination).await;
        }
        let _ = tokio::fs::remove_file(temp).await;
        return Err(format!("Failed to move temp download to target: {error}"));
    }
    let _ = tokio::fs::remove_file(&backup).await;
    Ok(())
}

fn is_allowed_github_redirect(url: &reqwest::Url) -> bool {
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

pub fn official_github_client(timeout: std::time::Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if is_allowed_github_redirect(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("redirect asset GitHub menuju host yang tidak diizinkan")
            }
        }))
        .build()
        .map_err(|error| format!("Gagal membuat client GitHub resmi: {error}"))
}

pub async fn download_file<F>(url: &str, dest_path: &Path, on_progress: F) -> Result<(), String>
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
        let content =
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  empty.pak\n\
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
