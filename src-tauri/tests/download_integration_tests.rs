use sha2::Digest;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wuwaid_launcher_lib::engine::downloader::{
    download_file_with_expected_size, get_asset_content_length, verify_sha256,
};
use wuwaid_launcher_lib::engine::media::{sync_media, AssetEntry, AssetManifest};

/// Spawns an in-process mock HTTP router that serves path-mapped payloads.
async fn spawn_mock_router(
    routes: HashMap<String, (Vec<u8>, Option<usize>)>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 2048];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);

            let is_head = req.starts_with("HEAD");
            eprintln!("DEBUG_MOCK_REQ:\n{}\nis_head: {}", req, is_head);
            let mut path = "/".to_string();
            if let Some(first_line) = req.lines().next() {
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() >= 2 {
                    path = parts[1].to_string();
                }
            }

            if let Some((body, content_len_override)) = routes.get(&path) {
                let len = content_len_override.unwrap_or(body.len());
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    len
                );
                let _ = socket.write_all(response.as_bytes()).await;
                if !is_head {
                    let _ = socket.write_all(body).await;
                }
                let _ = socket.flush().await;
                let _ = socket.shutdown().await;
            } else {
                let not_found =
                    "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(not_found.as_bytes()).await;
                let _ = socket.flush().await;
                let _ = socket.shutdown().await;
            }
        }
    });

    (url, handle)
}

#[derive(Clone, Copy)]
enum ResumeMode {
    Resume,
    AlwaysFull,
    InvalidRange,
    ChangedValidator,
    MissingRangeContentLength,
    RangeNotSatisfiable,
}

async fn read_http_request(socket: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    while request.len() < 8192 {
        let read = socket.read(&mut buffer).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

fn range_start(request: &str) -> Option<u64> {
    request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.eq_ignore_ascii_case("range") {
            return None;
        }
        value
            .trim()
            .strip_prefix("bytes=")?
            .strip_suffix('-')?
            .parse()
            .ok()
    })
}

async fn spawn_resumable_router(
    payload: Vec<u8>,
    drop_first: usize,
    mode: ResumeMode,
) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);
    let requests = Arc::new(Mutex::new(Vec::new()));
    let request_log = Arc::clone(&requests);
    let get_count = Arc::new(AtomicUsize::new(0));
    let request_count = Arc::clone(&get_count);

    let handle = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let request = read_http_request(&mut socket).await;
            request_log.lock().unwrap().push(request.clone());
            let is_head = request.starts_with("HEAD ");
            let requested_start = range_start(&request);
            let should_drop = !is_head && request_count.fetch_add(1, Ordering::SeqCst) < drop_first;
            let total = payload.len();

            let (status, body_start, body_end, content_length, content_range) = if is_head {
                ("200 OK", 0, 0, total, None)
            } else if should_drop {
                let start = requested_start.unwrap_or(0) as usize;
                let end = (start + 4).min(total);
                if requested_start.is_some() {
                    (
                        "206 Partial Content",
                        start,
                        end,
                        end.saturating_sub(start),
                        Some(format!("bytes {}-{}/{}", start, end - 1, total)),
                    )
                } else {
                    ("200 OK", 0, end, total, None)
                }
            } else if let Some(start) = requested_start {
                let start = start as usize;
                match mode {
                    ResumeMode::Resume
                    | ResumeMode::ChangedValidator
                    | ResumeMode::MissingRangeContentLength => (
                        "206 Partial Content",
                        start,
                        total,
                        total.saturating_sub(start),
                        Some(format!("bytes {}-{}/{}", start, total - 1, total)),
                    ),
                    ResumeMode::AlwaysFull => ("200 OK", 0, total, total, None),
                    ResumeMode::InvalidRange => {
                        let wrong_start = (start + 1).min(total - 1);
                        (
                            "206 Partial Content",
                            start,
                            total,
                            total.saturating_sub(start),
                            Some(format!("bytes {}-{}/{}", wrong_start, total - 1, total)),
                        )
                    }
                    ResumeMode::RangeNotSatisfiable => ("416 Range Not Satisfiable", 0, 0, 0, None),
                }
            } else {
                ("200 OK", 0, total, total, None)
            };

            let response_etag = if matches!(mode, ResumeMode::ChangedValidator)
                && requested_start.is_some()
                && !should_drop
            {
                "\"changed\""
            } else {
                "\"resume-test\""
            };
            let omit_range_content_length = matches!(mode, ResumeMode::MissingRangeContentLength)
                && requested_start.is_some()
                && !should_drop;
            let mut response = if omit_range_content_length {
                format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nETag: {response_etag}\r\nConnection: close\r\n"
                )
            } else {
                format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {content_length}\r\nContent-Type: application/octet-stream\r\nETag: {response_etag}\r\nConnection: close\r\n"
                )
            };
            if let Some(content_range) = content_range {
                response.push_str(&format!("Content-Range: {content_range}\r\n"));
            }
            response.push_str("\r\n");
            let _ = socket.write_all(response.as_bytes()).await;
            if !is_head && body_start < body_end {
                let _ = socket.write_all(&payload[body_start..body_end]).await;
            }
            let _ = socket.flush().await;
            let _ = socket.shutdown().await;
        }
    });

    (url, requests, handle)
}

fn resumable_test_payload() -> Vec<u8> {
    (0..64).map(|value| value as u8).collect()
}

#[tokio::test]
async fn test_mock_http_download_success_and_sha_verification() {
    let payload = b"WUWA_ID_TEST_PAYLOAD_CONTENT_OK";
    let hash = hex::encode(sha2::Sha256::digest(payload)).to_lowercase();

    let mut routes = HashMap::new();
    routes.insert("/asset.pak".to_string(), (payload.to_vec(), None));

    let (base_url, _server) = spawn_mock_router(routes).await;
    let download_url = format!("{}/asset.pak", base_url);

    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");

    // 1. Verify HEAD metadata query
    let meta_len = get_asset_content_length(&download_url).await.unwrap();
    assert_eq!(meta_len, payload.len() as u64);

    // 2. Stream download with exact expected size
    let downloaded_bytes =
        download_file_with_expected_size(&download_url, &dest, Some(meta_len), |p| {
            assert!(p.percent <= 100);
        })
        .await
        .unwrap();

    assert_eq!(downloaded_bytes, payload.len() as u64);
    assert!(dest.exists());

    // 3. Verify SHA-256
    assert!(verify_sha256(&dest, &hash).unwrap());
}

#[tokio::test]
async fn test_mock_http_content_length_mismatch_fails_and_preserves_partial() {
    let payload = b"SHORT_DATA";
    let mut routes = HashMap::new();
    // Mock server advertises 1000 bytes, but sends only 10 bytes
    routes.insert("/mismatch.pak".to_string(), (payload.to_vec(), Some(1000)));

    let (base_url, _server) = spawn_mock_router(routes).await;
    let download_url = format!("{}/mismatch.pak", base_url);

    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("mismatch.pak");

    let res = download_file_with_expected_size(&download_url, &dest, Some(1000), |_| {}).await;
    assert!(res.is_err());
    assert!(!dest.exists());
    assert!(tmp.path().join(".mismatch.pak.part").is_file());
    assert!(tmp.path().join(".mismatch.pak.part.json").is_file());
    assert!(!dest.with_extension("tmp_download").exists());
}

#[tokio::test]
async fn test_mock_http_media_sync_with_full_hash_validation() {
    let audio_data = b"MOCK_BGM_AUDIO_DATA_VALID";
    let video_data = b"MOCK_BG_VIDEO_DATA_VALID";

    let audio_hash = hex::encode(sha2::Sha256::digest(audio_data)).to_lowercase();
    let video_hash = hex::encode(sha2::Sha256::digest(video_data)).to_lowercase();

    let mut routes = HashMap::new();
    routes.insert("/bgm.mp3".to_string(), (audio_data.to_vec(), None));
    routes.insert("/bg-video.mp4".to_string(), (video_data.to_vec(), None));

    let (base_url, _server) = spawn_mock_router(routes).await;
    let audio_url = format!("{}/bgm.mp3", base_url);
    let video_url = format!("{}/bg-video.mp4", base_url);

    let tmp = tempdir().unwrap();
    let cache_dir = tmp.path();

    let manifest = AssetManifest {
        update_date: Some("2026-08-28T04:00:00Z".to_string()),
        assets: vec![
            AssetEntry {
                name: "bgm.mp3".to_string(),
                url: audio_url,
                sha256: audio_hash,
            },
            AssetEntry {
                name: "bg-video.mp4".to_string(),
                url: video_url,
                sha256: video_hash,
            },
        ],
    };

    let sync_res = sync_media(cache_dir, &manifest, |_, _| {}).await.unwrap();
    assert!(PathBuf::from(&sync_res.bgm_url).exists());
    assert!(PathBuf::from(&sync_res.video_url).exists());
}

#[tokio::test]
async fn test_corrupted_cached_media_is_rejected_and_re_downloaded_before_ready() {
    let audio_data = b"MOCK_GENUINE_BGM_AUDIO";
    let video_data = b"MOCK_GENUINE_BG_VIDEO";

    let audio_hash = hex::encode(sha2::Sha256::digest(audio_data)).to_lowercase();
    let video_hash = hex::encode(sha2::Sha256::digest(video_data)).to_lowercase();

    let mut routes = HashMap::new();
    routes.insert("/bgm.mp3".to_string(), (audio_data.to_vec(), None));
    routes.insert("/bg-video.mp4".to_string(), (video_data.to_vec(), None));

    let (base_url, _server) = spawn_mock_router(routes).await;
    let audio_url = format!("{}/bgm.mp3", base_url);
    let video_url = format!("{}/bg-video.mp4", base_url);

    let tmp = tempdir().unwrap();
    let cache_dir = tmp.path();

    // Create a corrupted bgm.mp3 in the cache directory
    let corrupted_bgm = cache_dir.join("bgm.mp3");
    std::fs::write(&corrupted_bgm, b"CORRUPTED_GARBAGE_DATA").unwrap();
    assert!(corrupted_bgm.exists());

    let manifest = AssetManifest {
        update_date: None,
        assets: vec![
            AssetEntry {
                name: "bgm.mp3".to_string(),
                url: audio_url,
                sha256: audio_hash.clone(),
            },
            AssetEntry {
                name: "bg-video.mp4".to_string(),
                url: video_url,
                sha256: video_hash.clone(),
            },
        ],
    };

    // sync_media must reject/remove the corrupted file and re-download the genuine file
    let sync_res = sync_media(cache_dir, &manifest, |_, _| {}).await.unwrap();
    assert!(PathBuf::from(&sync_res.bgm_url).exists());
    assert!(verify_sha256(&PathBuf::from(&sync_res.bgm_url), &audio_hash).unwrap());
    assert!(verify_sha256(&PathBuf::from(&sync_res.video_url), &video_hash).unwrap());
}

#[tokio::test]
async fn test_download_resumes_after_stream_error() {
    let payload = resumable_test_payload();
    let (base_url, requests, _server) =
        spawn_resumable_router(payload.clone(), 1, ResumeMode::Resume).await;
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");

    let downloaded = download_file_with_expected_size(
        &format!("{base_url}/asset.pak"),
        &dest,
        Some(payload.len() as u64),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(downloaded, payload.len() as u64);
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    let expected_hash = hex::encode(sha2::Sha256::digest(&payload));
    assert!(verify_sha256(&dest, &expected_hash).unwrap());
    let requests = requests.lock().unwrap().clone();
    assert!(requests.len() >= 2);
    assert!(requests[0].to_ascii_lowercase().contains("get /asset.pak"));
    assert!(requests[0]
        .to_ascii_lowercase()
        .contains("accept-encoding: identity"));
    assert!(requests[1].to_ascii_lowercase().contains("range: bytes=4-"));
    assert!(!tmp.path().join(".asset.pak.part").exists());
    assert!(!tmp.path().join(".asset.pak.part.json").exists());
}

#[tokio::test]
async fn test_download_resets_partial_when_range_returns_full_response() {
    let payload = resumable_test_payload();
    let (base_url, requests, _server) =
        spawn_resumable_router(payload.clone(), 1, ResumeMode::AlwaysFull).await;
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");

    download_file_with_expected_size(
        &format!("{base_url}/asset.pak"),
        &dest,
        Some(payload.len() as u64),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    let requests = requests.lock().unwrap().clone();
    assert!(requests.len() >= 2);
    assert!(requests[1].to_ascii_lowercase().contains("range: bytes=4-"));
}

#[tokio::test]
async fn test_download_restarts_after_invalid_content_range() {
    let payload = resumable_test_payload();
    let (base_url, requests, _server) =
        spawn_resumable_router(payload.clone(), 1, ResumeMode::InvalidRange).await;
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");

    download_file_with_expected_size(
        &format!("{base_url}/asset.pak"),
        &dest,
        Some(payload.len() as u64),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    let requests = requests.lock().unwrap().clone();
    assert!(requests.len() >= 3);
    assert!(requests[1].to_ascii_lowercase().contains("range: bytes=4-"));
    assert!(!requests[2].to_ascii_lowercase().contains("range:"));
}

#[tokio::test]
async fn test_download_restarts_after_changed_validator() {
    let payload = resumable_test_payload();
    let (base_url, requests, _server) =
        spawn_resumable_router(payload.clone(), 1, ResumeMode::ChangedValidator).await;
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");

    download_file_with_expected_size(
        &format!("{base_url}/asset.pak"),
        &dest,
        Some(payload.len() as u64),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    let requests = requests.lock().unwrap().clone();
    assert!(requests.len() >= 3);
    assert!(requests[1].to_ascii_lowercase().contains("range: bytes=4-"));
    assert!(!requests[2].to_ascii_lowercase().contains("range:"));
}

#[tokio::test]
async fn test_download_restarts_when_range_content_length_is_missing() {
    let payload = resumable_test_payload();
    let (base_url, requests, _server) =
        spawn_resumable_router(payload.clone(), 1, ResumeMode::MissingRangeContentLength).await;
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");

    download_file_with_expected_size(
        &format!("{base_url}/asset.pak"),
        &dest,
        Some(payload.len() as u64),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    let requests = requests.lock().unwrap().clone();
    assert!(requests.len() >= 3);
    assert!(requests[1].to_ascii_lowercase().contains("range: bytes=4-"));
    assert!(!requests[2].to_ascii_lowercase().contains("range:"));
}

#[tokio::test]
async fn test_download_restarts_after_range_not_satisfiable() {
    let payload = resumable_test_payload();
    let (base_url, requests, _server) =
        spawn_resumable_router(payload.clone(), 1, ResumeMode::RangeNotSatisfiable).await;
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");

    download_file_with_expected_size(
        &format!("{base_url}/asset.pak"),
        &dest,
        Some(payload.len() as u64),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    let requests = requests.lock().unwrap().clone();
    assert!(requests.len() >= 3);
    assert!(requests[1].to_ascii_lowercase().contains("range: bytes=4-"));
    assert!(!requests[2].to_ascii_lowercase().contains("range:"));
}

#[tokio::test]
async fn test_download_preserves_and_reuses_partial_after_retry_exhaustion() {
    let payload = resumable_test_payload();
    let (base_url, requests, _server) =
        spawn_resumable_router(payload.clone(), 4, ResumeMode::Resume).await;
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");
    let url = format!("{base_url}/asset.pak");
    let partial = tmp.path().join(".asset.pak.part");
    let metadata = tmp.path().join(".asset.pak.part.json");

    let first =
        download_file_with_expected_size(&url, &dest, Some(payload.len() as u64), |_| {}).await;
    assert!(first.is_err());
    assert!(!dest.exists());
    assert!(partial.is_file());
    assert!(metadata.is_file());

    download_file_with_expected_size(&url, &dest, Some(payload.len() as u64), |_| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    assert!(!partial.exists());
    assert!(!metadata.exists());

    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 5);
    assert!(requests[4]
        .to_ascii_lowercase()
        .contains("range: bytes=16-"));
}
