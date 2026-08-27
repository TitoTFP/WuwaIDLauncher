use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, IF_RANGE, RANGE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

pub const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DOWNLOAD_ATTEMPTS: usize = 4;
const RETRY_BACKOFF_SECONDS: [u64; 3] = [1, 2, 4];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadResumeMetadata {
    url: String,
    expected_size: Option<u64>,
    total_size: u64,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug)]
struct DownloadAttemptError {
    message: String,
    retryable: bool,
    reset_partial: bool,
}

impl DownloadAttemptError {
    fn new(message: impl Into<String>, retryable: bool, reset_partial: bool) -> Self {
        Self {
            message: message.into(),
            retryable,
            reset_partial,
        }
    }
}

struct DownloadAttempt<'a, F> {
    client: &'a reqwest::Client,
    redirect_policy: &'a DownloadRedirectPolicy,
    url: &'a str,
    partial_path: &'a Path,
    metadata_path: &'a Path,
    offset: u64,
    resume: Option<&'a DownloadResumeMetadata>,
    expected_size: Option<u64>,
    max_bytes: u64,
    on_progress: &'a F,
}

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
        .header(ACCEPT_ENCODING, "identity")
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

    let mut client_builder = reqwest::Client::builder().timeout(Duration::from_secs(60));
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
    let client = client_builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create target directory: {}", e))?;
    }

    let partial_path = resumable_temp_path(dest_path);
    let metadata_path = resume_metadata_path(&partial_path);
    let mut last_error = None;

    for attempt in 0..MAX_DOWNLOAD_ATTEMPTS {
        let resume_state =
            load_resume_state(&partial_path, &metadata_path, url, expected_size, max_bytes).await?;
        let (offset, resume_metadata) = match resume_state {
            Some((metadata, offset)) if offset > 0 && resumable_validator(&metadata).is_none() => {
                // A byte range without an entity validator could combine two
                // different versions of a mutable URL. Start safely instead.
                reset_partial(&partial_path, &metadata_path).await?;
                (0, None)
            }
            Some((metadata, offset)) => (offset, Some(metadata)),
            None => (0, None),
        };

        if let Some(metadata) = resume_metadata.as_ref() {
            if offset == metadata.total_size {
                promote_download(&partial_path, dest_path).await?;
                let _ = remove_file_if_exists(&metadata_path).await;
                return Ok(offset);
            }
            if offset > 0 {
                on_progress(DownloadProgress {
                    percent: ((offset as f64 / metadata.total_size as f64) * 100.0).min(100.0)
                        as u8,
                    downloaded_bytes: offset,
                    total_bytes: metadata.total_size,
                    speed_mbps: 0.0,
                    status: format!(
                        "Melanjutkan {:.1} / {:.1} MB",
                        offset as f64 / 1_048_576.0,
                        metadata.total_size as f64 / 1_048_576.0
                    ),
                });
            }
        }

        match download_attempt(DownloadAttempt {
            client: &client,
            redirect_policy: &redirect_policy,
            url,
            partial_path: &partial_path,
            metadata_path: &metadata_path,
            offset,
            resume: resume_metadata.as_ref(),
            expected_size,
            max_bytes,
            on_progress: &on_progress,
        })
        .await
        {
            Ok(downloaded) => {
                promote_download(&partial_path, dest_path).await?;
                let _ = remove_file_if_exists(&metadata_path).await;
                return Ok(downloaded);
            }
            Err(error) => {
                last_error = Some(error.message.clone());
                if error.reset_partial {
                    reset_partial(&partial_path, &metadata_path).await?;
                }
                if !error.retryable || attempt + 1 == MAX_DOWNLOAD_ATTEMPTS {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(
                    RETRY_BACKOFF_SECONDS[attempt.min(RETRY_BACKOFF_SECONDS.len() - 1)],
                ))
                .await;
            }
        }
    }

    let error = last_error.unwrap_or_else(|| "Download gagal.".to_string());
    if tokio::fs::metadata(&partial_path).await.is_ok() {
        Err(format!(
            "{error}. Data sebagian disimpan untuk dilanjutkan: {}",
            partial_path.display()
        ))
    } else {
        Err(error)
    }
}

async fn download_attempt<F>(attempt: DownloadAttempt<'_, F>) -> Result<u64, DownloadAttemptError>
where
    F: Fn(DownloadProgress),
{
    let DownloadAttempt {
        client,
        redirect_policy,
        url,
        partial_path,
        metadata_path,
        offset,
        resume,
        expected_size,
        max_bytes,
        on_progress,
    } = attempt;
    let mut request = client
        .get(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .header(ACCEPT_ENCODING, "identity");
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
        if let Some(validator) = resume.and_then(resumable_validator) {
            request = request.header(IF_RANGE, validator);
        }
    }

    let mut response = request.send().await.map_err(|error| {
        DownloadAttemptError::new(format!("Failed to send GET request: {error}"), true, false)
    })?;

    let status = response.status();
    if !status.is_success() {
        if offset > 0 && status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            return Err(DownloadAttemptError::new(
                "Server menolak Range resume; file sebagian akan diulang dari awal.",
                true,
                true,
            ));
        }
        let retryable = status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error();
        return Err(DownloadAttemptError::new(
            format!("Download failed with status: {status}"),
            retryable,
            false,
        ));
    }

    if matches!(
        redirect_policy,
        DownloadRedirectPolicy::OfficialGithubAsset { .. }
    ) && !is_allowed_github_redirect(response.url())
    {
        return Err(DownloadAttemptError::new(
            "Redirect asset GitHub menuju URL yang tidak diizinkan.",
            false,
            false,
        ));
    }

    let (write_offset, total_size, metadata, append) = if status == reqwest::StatusCode::OK {
        let total_size = response.content_length().ok_or_else(|| {
            DownloadAttemptError::new(
                "Header Content-Length tidak ditemukan pada GET response.",
                false,
                offset > 0,
            )
        })?;
        validate_download_size(total_size, expected_size, max_bytes)
            .map_err(|message| DownloadAttemptError::new(message, false, true))?;
        (
            0,
            total_size,
            metadata_from_response(url, expected_size, total_size, &response),
            false,
        )
    } else if offset == 0 {
        return Err(DownloadAttemptError::new(
            format!("Response awal download tidak valid: {status}"),
            true,
            true,
        ));
    } else {
        let Some(resume) = resume else {
            return Err(DownloadAttemptError::new(
                "Metadata resume tidak ditemukan untuk file sebagian.",
                false,
                true,
            ));
        };

        match status {
            reqwest::StatusCode::PARTIAL_CONTENT => {
                let raw_range = response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|value| value.to_str().ok());
                let (start, end, total_size) = parse_content_range(raw_range)
                    .map_err(|error| DownloadAttemptError::new(error, true, true))?;
                if start != offset || total_size != resume.total_size {
                    return Err(DownloadAttemptError::new(
                        format!(
                            "Content-Range tidak sesuai: diminta mulai {offset}, diterima {start}-{end}/{total_size}."
                        ),
                        true,
                        true,
                    ));
                }
                validate_download_size(total_size, expected_size, max_bytes)
                    .map_err(|message| DownloadAttemptError::new(message, false, true))?;
                let expected_body_size = end - start + 1;
                if response.content_length() != Some(expected_body_size) {
                    return Err(DownloadAttemptError::new(
                        "Content-Length response Range tidak sesuai dengan Content-Range.",
                        true,
                        true,
                    ));
                }
                if !response_validator_matches(resume, &response) {
                    return Err(DownloadAttemptError::new(
                        "Validator file berubah atau hilang saat melanjutkan download.",
                        true,
                        true,
                    ));
                }
                (offset, total_size, resume.clone(), true)
            }
            _ => {
                return Err(DownloadAttemptError::new(
                    format!("Response resume tidak valid: {status}"),
                    true,
                    true,
                ));
            }
        }
    };

    let mut file = if append {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(partial_path)
            .await
            .map_err(|error| {
                DownloadAttemptError::new(
                    format!("Failed to open partial download: {error}"),
                    false,
                    false,
                )
            })?
    } else {
        open_partial_for_full_response(partial_path, metadata_path, &metadata).await?
    };

    let start_time = Instant::now();
    let mut last_emit = Instant::now();
    let mut downloaded = write_offset;
    let mut stream_error = None;
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                stream_error = Some(DownloadAttemptError::new(
                    format!("Read download stream error: {error}"),
                    true,
                    false,
                ));
                break;
            }
        };
        if downloaded > max_bytes
            || chunk.len() as u64 > max_bytes.saturating_sub(downloaded)
            || chunk.len() as u64 > total_size.saturating_sub(downloaded)
        {
            stream_error = Some(DownloadAttemptError::new(
                format!(
                    "Download melebihi ukuran yang diharapkan atau batas {} bytes.",
                    max_bytes
                ),
                false,
                true,
            ));
            break;
        }
        if let Err(error) = file.write_all(&chunk).await {
            stream_error = Some(DownloadAttemptError::new(
                format!("Write file error: {error}"),
                false,
                false,
            ));
            break;
        }

        downloaded += chunk.len() as u64;
        if last_emit.elapsed().as_millis() >= 350 || downloaded == total_size {
            let elapsed_secs = start_time.elapsed().as_secs_f64().max(0.001);
            let transferred = downloaded.saturating_sub(write_offset);
            let speed_mbps = (transferred as f64 / (1024.0 * 1024.0)) / elapsed_secs;
            let percent = ((downloaded as f64 / total_size as f64) * 100.0).min(100.0) as u8;
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

    if let Some(error) = stream_error {
        let _ = file.flush().await;
        drop(file);
        return Err(error);
    }
    if let Err(error) = file.flush().await {
        drop(file);
        return Err(DownloadAttemptError::new(
            format!("Failed to flush downloaded file: {error}"),
            false,
            false,
        ));
    }
    drop(file);

    if downloaded != total_size {
        return Err(DownloadAttemptError::new(
            format!(
                "Ukuran file sebagian ({} bytes) belum mencapai total Content-Length ({} bytes).",
                downloaded, total_size
            ),
            true,
            false,
        ));
    }

    Ok(downloaded)
}

fn validate_download_size(
    total_size: u64,
    expected_size: Option<u64>,
    max_bytes: u64,
) -> Result<(), String> {
    if total_size == 0 {
        return Err("Content-Length bernilai 0 pada server.".to_string());
    }
    if total_size > max_bytes {
        return Err(format!("Download melebihi batas {} bytes.", max_bytes));
    }
    if expected_size.is_some_and(|expected| expected != total_size) {
        return Err(format!(
            "Content-Length GET ({} bytes) tidak sesuai dengan metadata ({} bytes).",
            total_size,
            expected_size.unwrap_or_default()
        ));
    }
    Ok(())
}

fn parse_content_range(value: Option<&str>) -> Result<(u64, u64, u64), String> {
    let value = value
        .ok_or_else(|| "Header Content-Range tidak ditemukan pada response Range.".to_string())?;
    let mut parts = value.split_whitespace();
    let unit = parts
        .next()
        .ok_or_else(|| "Format Content-Range tidak valid.".to_string())?;
    let value = parts
        .next()
        .ok_or_else(|| "Format Content-Range tidak valid.".to_string())?;
    if parts.next().is_some() {
        return Err("Format Content-Range tidak valid.".to_string());
    }
    if unit != "bytes" {
        return Err("Unit Content-Range bukan bytes.".to_string());
    }
    let (range, total) = value
        .split_once('/')
        .ok_or_else(|| "Total Content-Range tidak ditemukan.".to_string())?;
    let total = total
        .parse::<u64>()
        .map_err(|_| "Total Content-Range tidak valid.".to_string())?;
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| "Rentang Content-Range tidak valid.".to_string())?;
    let start = start
        .parse::<u64>()
        .map_err(|_| "Awal Content-Range tidak valid.".to_string())?;
    let end = end
        .parse::<u64>()
        .map_err(|_| "Akhir Content-Range tidak valid.".to_string())?;
    if start > end || end >= total {
        return Err("Rentang Content-Range berada di luar total file.".to_string());
    }
    Ok((start, end, total))
}

fn metadata_from_response(
    url: &str,
    expected_size: Option<u64>,
    total_size: u64,
    response: &reqwest::Response,
) -> DownloadResumeMetadata {
    DownloadResumeMetadata {
        url: url.to_string(),
        expected_size,
        total_size,
        etag: response_header(response, "etag"),
        last_modified: response_header(response, "last-modified"),
    }
}

fn response_header(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn resumable_validator(metadata: &DownloadResumeMetadata) -> Option<&str> {
    metadata
        .etag
        .as_deref()
        .filter(|value| !value.is_empty() && !value.starts_with("W/"))
        .or_else(|| {
            metadata
                .last_modified
                .as_deref()
                .filter(|value| !value.is_empty())
        })
}

fn response_validator_matches(
    metadata: &DownloadResumeMetadata,
    response: &reqwest::Response,
) -> bool {
    if let Some(etag) = metadata
        .etag
        .as_deref()
        .filter(|value| !value.is_empty() && !value.starts_with("W/"))
    {
        return response_header(response, "etag").as_deref() == Some(etag);
    }
    metadata
        .last_modified
        .as_deref()
        .filter(|value| !value.is_empty())
        .is_some_and(|last_modified| {
            response_header(response, "last-modified").as_deref() == Some(last_modified)
        })
}

fn resumable_temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    path.with_file_name(format!(".{name}.part"))
}

fn resume_metadata_path(partial_path: &Path) -> PathBuf {
    let name = partial_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("download.part");
    partial_path.with_file_name(format!("{name}.json"))
}

async fn load_resume_state(
    partial_path: &Path,
    metadata_path: &Path,
    url: &str,
    expected_size: Option<u64>,
    max_bytes: u64,
) -> Result<Option<(DownloadResumeMetadata, u64)>, String> {
    let partial_stat = match tokio::fs::metadata(partial_path).await {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            remove_file_if_exists(metadata_path).await?;
            return Ok(None);
        }
        Err(error) => {
            return Err(format!("Gagal membaca file sebagian: {error}"));
        }
    };
    if !partial_stat.is_file() {
        return Err(format!(
            "Target partial download bukan file: {partial_path:?}"
        ));
    }

    let raw_metadata = match tokio::fs::read(metadata_path).await {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            reset_partial(partial_path, metadata_path).await?;
            return Ok(None);
        }
        Err(error) => {
            return Err(format!("Gagal membaca metadata download sebagian: {error}"));
        }
    };
    let metadata: DownloadResumeMetadata = match serde_json::from_slice(&raw_metadata) {
        Ok(metadata) => metadata,
        Err(_) => {
            reset_partial(partial_path, metadata_path).await?;
            return Ok(None);
        }
    };
    let valid = metadata.url == url
        && metadata.total_size > 0
        && metadata.total_size <= max_bytes
        && metadata.expected_size == expected_size
        && !metadata
            .expected_size
            .is_some_and(|size| size != metadata.total_size)
        && partial_stat.len() <= metadata.total_size;
    if !valid {
        reset_partial(partial_path, metadata_path).await?;
        return Ok(None);
    }
    Ok(Some((metadata, partial_stat.len())))
}

async fn write_resume_metadata(
    metadata_path: &Path,
    metadata: &DownloadResumeMetadata,
) -> Result<(), String> {
    let data = serde_json::to_vec(metadata)
        .map_err(|error| format!("Gagal menyusun metadata download sebagian: {error}"))?;
    let temporary = unique_temp_path(metadata_path, "tmp");
    if let Err(error) = tokio::fs::write(&temporary, data).await {
        return Err(format!("Gagal menulis metadata download sebagian: {error}"));
    }
    if let Err(error) = replace_file_atomically(&temporary, metadata_path) {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    Ok(())
}

async fn open_partial_for_full_response(
    partial_path: &Path,
    metadata_path: &Path,
    metadata: &DownloadResumeMetadata,
) -> Result<tokio::fs::File, DownloadAttemptError> {
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(partial_path)
        .await
        .map_err(|error| {
            DownloadAttemptError::new(
                format!("Failed to create partial download: {error}"),
                false,
                false,
            )
        })?;
    write_resume_metadata(metadata_path, metadata)
        .await
        .map_err(|error| DownloadAttemptError::new(error, false, false))?;
    Ok(file)
}

async fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Gagal menghapus file sementara {path:?}: {error}")),
    }
}

async fn reset_partial(partial_path: &Path, metadata_path: &Path) -> Result<(), String> {
    remove_file_if_exists(partial_path).await?;
    remove_file_if_exists(metadata_path).await
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
        return Err(format!("Target download bukan file: {destination:?}"));
    }
    replace_file_atomically(temp, destination)
}

pub(crate) fn replace_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };

        let source: Vec<u16> = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: both UTF-16 buffers are NUL-terminated, remain owned for the
        // entire call, and the Windows API only reads these immutable paths.
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|error| format!("Gagal mengganti file secara atomik: {error}"))
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(source, destination)
            .map_err(|error| format!("Gagal mengganti file secara atomik: {error}"))
    }
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
            parsed.get("empty.pak").map(|value| value.as_str()),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert_eq!(
            parsed.get("test.dll").map(|value| value.as_str()),
            Some("a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e")
        );
    }

    #[test]
    fn test_verify_sha256_valid() -> Result<(), Box<dyn std::error::Error>> {
        let file = NamedTempFile::new()?;
        std::fs::write(file.path(), b"hello world")?;
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(matches!(verify_sha256(file.path(), expected), Ok(true)));
        Ok(())
    }

    #[tokio::test]
    async fn test_full_response_truncates_before_metadata_activation_failure(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let partial_path = temp.path().join(".asset.pak.part");
        let metadata_path = temp.path().join(".asset.pak.part.json");
        tokio::fs::write(&partial_path, b"stale-prefix").await?;
        tokio::fs::create_dir(&metadata_path).await?;

        let metadata = DownloadResumeMetadata {
            url: "https://example.com/asset.pak".to_string(),
            expected_size: Some(64),
            total_size: 64,
            etag: Some("\"new-validator\"".to_string()),
            last_modified: None,
        };

        let result = open_partial_for_full_response(&partial_path, &metadata_path, &metadata).await;
        assert!(result.is_err());
        assert_eq!(tokio::fs::metadata(&partial_path).await?.len(), 0);
        assert!(tokio::fs::metadata(&metadata_path).await?.is_dir());
        Ok(())
    }

    #[test]
    fn test_verify_sha256_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let file = NamedTempFile::new()?;
        std::fs::write(file.path(), b"hello world")?;
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(matches!(verify_sha256(file.path(), expected), Ok(false)));
        Ok(())
    }
}
